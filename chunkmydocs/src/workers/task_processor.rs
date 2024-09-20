use chrono::{ DateTime, Utc };
use tempfile::NamedTempFile;
use std::io::Write;
use chunkmydocs::task::pdla::pdla_extraction;
use chunkmydocs::task::process::process_segments;
use chunkmydocs::models::rrq::queue::QueuePayload;
use chunkmydocs::models::server::extract::ExtractionPayload;
use chunkmydocs::models::server::segment::Segment;
use chunkmydocs::models::server::task::Status;
use chunkmydocs::utils::rrq::consumer::consumer;
use chunkmydocs::utils::configs::extraction_config;
use chunkmydocs::utils::db::deadpool_postgres::{ Client, Pool, create_pool };
use chunkmydocs::utils::json2mkd::json_2_mkd::hierarchical_chunk_and_add_markdown;
use chunkmydocs::utils::storage::config_s3::create_client;
use chunkmydocs::utils::storage::services::{ download_to_tempfile, upload_to_s3 };

pub async fn log_task(
    task_id: String,
    status: Status,
    message: Option<String>,
    finished_at: Option<DateTime<Utc>>,
    pool: &Pool
) -> Result<(), Box<dyn std::error::Error>> {
    let client: Client = pool.get().await?;

    let task_query = format!(
        "UPDATE tasks SET status = '{:?}', message = '{}', finished_at = '{:?}' WHERE task_id = '{}'",
        status,
        message.unwrap_or_default(),
        finished_at.unwrap_or_default(),
        task_id
    );

    client.execute(&task_query, &[]).await?;

    Ok(())
}

async fn process(payload: QueuePayload) -> Result<(), Box<dyn std::error::Error>> {
    println!("Processing task");
    let s3_client = create_client().await?;
    let reqwest_client = reqwest::Client::new();
    let extraction_item: ExtractionPayload = serde_json::from_value(payload.payload)?;
    let task_id = extraction_item.task_id.clone();

    let pg_pool = create_pool();

    log_task(
        task_id.clone(),
        Status::Processing,
        Some(format!("Task processing | Tries ({}/{})", payload.attempt, payload.max_attempts)),
        None,
        &pg_pool
    ).await?;

    let result: Result<(), Box<dyn std::error::Error>> = (async {
        let temp_file = download_to_tempfile(
            &s3_client,
            &reqwest_client,
            &extraction_item.input_location,
            None
        ).await?;

        let pdla_response = pdla_extraction(
            temp_file.path(),
            extraction_item.model,
            extraction_item.batch_size
        ).await?;

        let base_segments: Vec<Segment> = serde_json::from_str(&pdla_response)?;

        let segments: Vec<Segment> = process_segments(
            temp_file.path(),
            &base_segments
        ).await?;

        let chunks = hierarchical_chunk_and_add_markdown(
            segments,
            extraction_item.target_chunk_length
        ).await?;

        let mut output_temp_file = NamedTempFile::new()?;
        output_temp_file.write_all(serde_json::to_string(&chunks)?.as_bytes())?;
    
        upload_to_s3(&s3_client, &extraction_item.output_location, &output_temp_file.path()).await?;

        if temp_file.path().exists() {
            if let Err(e) = std::fs::remove_file(temp_file.path()) {
                eprintln!("Error deleting temporary file: {:?}", e);
            }
        }
        if output_temp_file.path().exists() {
            if let Err(e) = std::fs::remove_file(output_temp_file.path()) {
                eprintln!("Error deleting temporary file: {:?}", e);
            }
        }

        Ok(())
    }).await;

    match result {
        Ok(_) => {
            println!("Task succeeded");
            log_task(
                task_id.clone(),
                Status::Succeeded,
                Some("Task succeeded".to_string()),
                Some(Utc::now()),
                &pg_pool
            ).await?;
            Ok(())
        }
        Err(e) => {
            eprintln!("Error processing task: {:?}", e);
            if payload.attempt >= payload.max_attempts {
                println!("Task failed");
                log_task(
                    task_id.clone(),
                    Status::Failed,
                    Some("Task failed".to_string()),
                    Some(Utc::now()),
                    &pg_pool
                ).await?;
            }
            Err(e)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = extraction_config::Config::from_env()?;
    println!("Starting task processor");
    consumer(process, config.extraction_queue, 1, 600).await?;
    Ok(())
}
