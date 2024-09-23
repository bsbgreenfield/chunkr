use crate::models::server::extract::Configuration;
use crate::models::server::task::{ Status, TaskResponse };
use crate::utils::db::deadpool_postgres::{ Client, Pool };
use crate::utils::storage::services::generate_presigned_url;
use aws_sdk_s3::Client as S3Client;
use chrono::{ DateTime, Utc };

pub async fn get_tasks(
    pool: &Pool,
    s3_client: &S3Client,
    user_id: String,
    page: i64,
    limit: i64
) -> Result<Vec<TaskResponse>, Box<dyn std::error::Error>> {
    let client: Client = pool.get().await?;
    let offset = (page - 1) * limit;
    let tasks = client.query(
        "SELECT task_id, status, created_at, finished_at, expires_at, message, input_location, output_location, task_url, configuration, file_name, page_count
         FROM TASKS
         WHERE user_id = $1
         ORDER BY created_at DESC
         OFFSET $2 LIMIT $3",
        &[&user_id, &offset, &limit]
    ).await?;

    let mut task_responses = Vec::new();

    for row in tasks {
        match process_task_row(&row, s3_client).await {
            Ok(task_response) => task_responses.push(task_response),
            Err(e) => eprintln!("Error processing task row: {}", e),
        }
    }

    Ok(task_responses)
}

async fn process_task_row(
    row: &tokio_postgres::Row,
    s3_client: &S3Client
) -> Result<TaskResponse, Box<dyn std::error::Error>> {
    let task_id: String = row.try_get("task_id")?;
    let expires_at: Option<DateTime<Utc>> = row.try_get("expires_at")?;

    if expires_at.is_some() && expires_at.unwrap() < Utc::now() {
        return Err("Task expired".into());
    }

    let file_name: Option<String> = row.try_get("file_name")?;
    let page_count: Option<i32> = row.try_get("page_count")?;
    let status: Status = row
        .try_get::<_, Option<String>>("status")?
        .ok_or("Status is None")?
        .parse()?;
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let finished_at: Option<DateTime<Utc>> = row.try_get("finished_at")?;
    let message = row.try_get::<_, Option<String>>("message")?.unwrap_or_default();
    let input_location: String = row.try_get("input_location")?;
    let input_file_url = generate_presigned_url(s3_client, &input_location, None).await.ok();
    let task_url: Option<String> = row.try_get("task_url")?;
    let configuration: Configuration = serde_json::from_str(
        &row.try_get::<_, Option<String>>("configuration")?
            .ok_or("Configuration is None")?
    )?;

    Ok(TaskResponse {
        task_id,
        status,
        created_at,
        finished_at,
        expires_at,
        message,
        output: None,
        input_file_url,
        task_url,
        configuration,
        file_name,
        page_count,
    })
}
