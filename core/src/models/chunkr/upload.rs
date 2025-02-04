use crate::configs::expiration_config;
use crate::models::chunkr::chunk_processing::{
    default_ignore_headers_and_footers, ChunkProcessing,
};
use crate::models::chunkr::segment_processing::SegmentProcessing;
use crate::models::chunkr::structured_extraction::JsonSchema;
use crate::models::chunkr::task::Configuration;
use actix_multipart::form::json::Json as MPJson;
use actix_multipart::form::{tempfile::TempFile, text::Text, MultipartForm};
use postgres_types::{FromSql, ToSql};
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumString};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, MultipartForm, ToSchema, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CreateForm {
    #[param(style = Form, value_type = Option<ChunkProcessing>)]
    #[schema(value_type = Option<ChunkProcessing>)]
    pub chunk_processing: Option<MPJson<ChunkProcessing>>,
    #[param(style = Form, value_type = Option<i32>)]
    #[schema(value_type = Option<i32>)]
    /// The number of seconds until task is deleted.
    /// Expried tasks can **not** be updated, polled or accessed via web interface.
    pub expires_in: Option<MPJson<i32>>,
    #[param(style = Form, value_type = String, format = "binary")]
    #[schema(value_type = String, format = "binary")]
    /// The file to be uploaded.
    pub file: TempFile,
    #[param(style = Form, value_type = Option<bool>)]
    #[schema(value_type = Option<bool>, default = false)]
    /// Whether to use high-resolution images for cropping and post-processing. (Latency penalty: ~7 seconds per page)
    pub high_resolution: Option<MPJson<bool>>,
    #[param(style = Form, value_type = Option<JsonSchema>)]
    #[schema(value_type = Option<JsonSchema>)]
    pub json_schema: Option<MPJson<JsonSchema>>,
    #[param(style = Form, value_type = Option<OcrStrategy>)]
    #[schema(value_type = Option<OcrStrategy>, default = "All")]
    pub ocr_strategy: Option<MPJson<OcrStrategy>>,
    #[param(style = Form, value_type = Option<SegmentProcessing>)]
    #[schema(value_type = Option<SegmentProcessing>)]
    pub segment_processing: Option<MPJson<SegmentProcessing>>,
    #[param(style = Form, value_type = Option<SegmentationStrategy>)]
    #[schema(value_type = Option<SegmentationStrategy>, default = "LayoutAnalysis")]
    pub segmentation_strategy: Option<MPJson<SegmentationStrategy>>,
    #[param(style = Form, value_type = Option<i32>)]
    #[schema(value_type = Option<i32>, default = 512)]
    #[deprecated = "Use `chunk_processing` instead"]
    /// Deprecated: Use `chunk_processing.target_length` instead.
    ///
    /// The target chunk length to be used for chunking.
    /// If 0, each chunk will contain a single segment.
    pub target_chunk_length: Option<Text<i32>>,
}

impl CreateForm {
    fn get_chunk_processing(&self) -> ChunkProcessing {
        self.chunk_processing
            .as_ref()
            .map(|mp_json| mp_json.0.clone())
            .or_else(|| {
                // For backwards compatibility: if chunk_processing is not set but target_chunk_length is,
                // create a ChunkProcessing with target_length as target_chunk_length
                self.target_chunk_length.as_ref().map(|length| {
                    let chunk_processing = ChunkProcessing {
                        ignore_headers_and_footers: default_ignore_headers_and_footers(),
                        target_length: length.0,
                    };
                    chunk_processing
                })
            })
            .unwrap_or_else(ChunkProcessing::default)
    }

    fn get_expires_in(&self) -> Option<i32> {
        let expiration_config = expiration_config::Config::from_env().unwrap();
        self.expires_in
            .as_ref()
            .map(|e| e.0)
            .or(expiration_config.time)
    }

    fn get_high_resolution(&self) -> bool {
        self.high_resolution.as_ref().map(|e| e.0).unwrap_or(false)
    }

    fn get_json_schema(&self) -> Option<JsonSchema> {
        self.json_schema.as_ref().map(|e| e.0.clone())
    }

    fn get_ocr_strategy(&self) -> OcrStrategy {
        self.ocr_strategy
            .as_ref()
            .map(|e| e.0.clone())
            .unwrap_or(OcrStrategy::All)
    }

    fn get_segment_processing(&self) -> SegmentProcessing {
        let user_config = self
            .segment_processing
            .as_ref()
            .map(|e| e.0.clone())
            .unwrap_or_default();

        SegmentProcessing {
            title: user_config
                .title
                .or_else(|| SegmentProcessing::default().title),
            section_header: user_config
                .section_header
                .or_else(|| SegmentProcessing::default().section_header),
            text: user_config
                .text
                .or_else(|| SegmentProcessing::default().text),
            list_item: user_config
                .list_item
                .or_else(|| SegmentProcessing::default().list_item),
            table: user_config
                .table
                .or_else(|| SegmentProcessing::default().table),
            picture: user_config
                .picture
                .or_else(|| SegmentProcessing::default().picture),
            caption: user_config
                .caption
                .or_else(|| SegmentProcessing::default().caption),
            formula: user_config
                .formula
                .or_else(|| SegmentProcessing::default().formula),
            footnote: user_config
                .footnote
                .or_else(|| SegmentProcessing::default().footnote),
            page_header: user_config
                .page_header
                .or_else(|| SegmentProcessing::default().page_header),
            page_footer: user_config
                .page_footer
                .or_else(|| SegmentProcessing::default().page_footer),
            page: user_config
                .page
                .or_else(|| SegmentProcessing::default().page),
        }
    }

    fn get_segmentation_strategy(&self) -> SegmentationStrategy {
        self.segmentation_strategy
            .as_ref()
            .map(|e| e.0.clone())
            .unwrap_or(SegmentationStrategy::LayoutAnalysis)
    }

    pub fn to_configuration(&self) -> Configuration {
        Configuration {
            chunk_processing: self.get_chunk_processing(),
            expires_in: self.get_expires_in(),
            high_resolution: self.get_high_resolution(),
            json_schema: self.get_json_schema(),
            model: None,
            ocr_strategy: self.get_ocr_strategy(),
            segment_processing: self.get_segment_processing(),
            segmentation_strategy: self.get_segmentation_strategy(),
            target_chunk_length: None,
        }
    }
}

#[derive(Debug, MultipartForm, ToSchema, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UpdateForm {
    #[param(style = Form, value_type = Option<ChunkProcessing>)]
    #[schema(value_type = Option<ChunkProcessing>)]
    pub chunk_processing: Option<MPJson<ChunkProcessing>>,
    #[param(style = Form, value_type = Option<i32>)]
    #[schema(value_type = Option<i32>)]
    /// The number of seconds until task is deleted.
    /// Expried tasks can **not** be updated, polled or accessed via web interface.
    pub expires_in: Option<MPJson<i32>>,
    #[param(style = Form, value_type = Option<bool>)]
    #[schema(value_type = Option<bool>)]
    /// Whether to use high-resolution images for cropping and post-processing. (Latency penalty: ~7 seconds per page)
    pub high_resolution: Option<MPJson<bool>>,
    #[param(style = Form, value_type = Option<JsonSchema>)]
    #[schema(value_type = Option<JsonSchema>)]
    pub json_schema: Option<MPJson<JsonSchema>>,
    #[param(style = Form, value_type = Option<OcrStrategy>)]
    #[schema(value_type = Option<OcrStrategy>)]
    pub ocr_strategy: Option<MPJson<OcrStrategy>>,
    #[param(style = Form, value_type = Option<SegmentProcessing>)]
    #[schema(value_type = Option<SegmentProcessing>)]
    pub segment_processing: Option<MPJson<SegmentProcessing>>,
    #[param(style = Form, value_type = Option<SegmentationStrategy>)]
    #[schema(value_type = Option<SegmentationStrategy>)]
    pub segmentation_strategy: Option<MPJson<SegmentationStrategy>>,
}

impl UpdateForm {
    fn get_segment_processing(&self, current_config: &Configuration) -> SegmentProcessing {
        let user_config = self
            .segment_processing
            .as_ref()
            .map(|e| e.0.clone())
            .unwrap_or_default();

        SegmentProcessing {
            title: user_config
                .title
                .or(current_config.segment_processing.title.clone()),
            section_header: user_config
                .section_header
                .or(current_config.segment_processing.section_header.clone()),
            text: user_config
                .text
                .or(current_config.segment_processing.text.clone()),
            list_item: user_config
                .list_item
                .or(current_config.segment_processing.list_item.clone()),
            table: user_config
                .table
                .or(current_config.segment_processing.table.clone()),
            picture: user_config
                .picture
                .or(current_config.segment_processing.picture.clone()),
            caption: user_config
                .caption
                .or(current_config.segment_processing.caption.clone()),
            formula: user_config
                .formula
                .or(current_config.segment_processing.formula.clone()),
            footnote: user_config
                .footnote
                .or(current_config.segment_processing.footnote.clone()),
            page_header: user_config
                .page_header
                .or(current_config.segment_processing.page_header.clone()),
            page_footer: user_config
                .page_footer
                .or(current_config.segment_processing.page_footer.clone()),
            page: user_config
                .page
                .or(current_config.segment_processing.page.clone()),
        }
    }

    pub fn to_configuration(&self, current_config: &Configuration) -> Configuration {
        Configuration {
            chunk_processing: self
                .chunk_processing
                .as_ref()
                .map(|e| e.0.clone())
                .unwrap_or_else(|| current_config.chunk_processing.clone()),
            expires_in: self
                .expires_in
                .as_ref()
                .map(|e| e.0)
                .or(current_config.expires_in),
            high_resolution: self
                .high_resolution
                .as_ref()
                .map(|e| e.0)
                .unwrap_or(current_config.high_resolution),
            json_schema: self
                .json_schema
                .as_ref()
                .map(|e| e.0.clone())
                .or(current_config.json_schema.clone()),
            model: None,
            ocr_strategy: self
                .ocr_strategy
                .as_ref()
                .map(|e| e.0.clone())
                .unwrap_or(current_config.ocr_strategy.clone()),
            segment_processing: self.get_segment_processing(current_config),
            segmentation_strategy: self
                .segmentation_strategy
                .as_ref()
                .map(|e| e.0.clone())
                .unwrap_or(current_config.segmentation_strategy.clone()),
            target_chunk_length: None,
        }
    }
}

#[derive(
    Debug, Serialize, Deserialize, PartialEq, Clone, ToSql, FromSql, ToSchema, Display, EnumString,
)]
/// Controls the Optical Character Recognition (OCR) strategy.
/// - `All`: Processes all pages with OCR. (Latency penalty: ~0.5 seconds per page)
/// - `Auto`: Selectively applies OCR only to pages with missing or low-quality text. When text layer is present the bounding boxes from the text layer are used.
pub enum OcrStrategy {
    All,
    #[serde(alias = "Off")]
    Auto,
}

#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Display,
    EnumString,
    Eq,
    PartialEq,
    ToSql,
    FromSql,
    ToSchema,
)]
/// Controls the segmentation strategy:
/// - `LayoutAnalysis`: Analyzes pages for layout elements (e.g., `Table`, `Picture`, `Formula`, etc.) using bounding boxes. Provides fine-grained segmentation and better chunking. (Latency penalty: ~TBD seconds per page).
/// - `Page`: Treats each page as a single segment. Faster processing, but without layout element detection and only simple chunking.
pub enum SegmentationStrategy {
    LayoutAnalysis,
    Page,
}
