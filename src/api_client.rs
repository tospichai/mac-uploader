use reqwest::multipart;
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("API returned error: {message}")]
    ApiError { message: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub success: bool,
    pub message: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadResponse {
    pub success: bool,
    pub message: String,
    pub photo_id: Option<String>,
    pub s3: Option<S3Info>,
    pub meta: Option<MetaInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct S3Info {
    pub original_key: String,
    pub thumb_key: Option<String>,
    pub bucket: String,
    pub region: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetaInfo {
    pub original_name: String,
    pub local_path: String,
    pub shot_at: String,
    pub checksum: Option<String>,
    pub event_code: String,
}

pub struct ApiClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl ApiClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        let client = reqwest::Client::new();
        Self {
            client,
            base_url,
            api_key,
        }
    }

    pub async fn test_connection(&self, api_key: &str) -> Result<HealthResponse, ApiError> {
        let url = format!("{}/check-api-key", self.base_url.trim_end_matches('/'));

        let response = self.client
            .get(&url)
            .query(&[("api_key", api_key)])
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ApiError::ApiError {
                message: format!("HTTP {}: {}", response.status(), response.text().await?),
            });
        }

        let health_response: HealthResponse = response.json().await?;

        if !health_response.success {
            return Err(ApiError::ApiError {
                message: health_response.message,
            });
        }

        Ok(health_response)
    }

    pub async fn upload_photo<F>(
        &self,
        event_code: &str,
        file_path: &Path,
        api_key: &str,
        on_progress: F,
    ) -> Result<UploadResponse, ApiError>
    where
        F: Fn(f32) + Send + Sync + 'static,
    {
        println!("🚀 ApiClient::upload_photo called");
        println!("📡 URL: {}/api/gallery/{}/photos", self.base_url.trim_end_matches('/'), event_code);
        println!("📁 File path: {}", file_path.display());
        println!("🔑 API key: {}...", &api_key[..api_key.len().min(10)]);

        let url = format!(
            "{}/api/gallery/{}/photos",
            self.base_url.trim_end_matches('/'),
            event_code
        );

        let file_name = file_path
            .file_name()
            .ok_or_else(|| ApiError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid file path"
            )))?
            .to_string_lossy()
            .to_string();

        let file_name_clone = file_name.clone();
        let file_path_str = file_path.to_string_lossy().to_string();

        println!("📖 Opening file: {}", file_name);
        let file = tokio::fs::File::open(file_path).await?;
        let metadata = file.metadata().await?;
        let total_size = metadata.len();
        println!("✅ File opened successfully, size: {} bytes", total_size);

        // Create a stream for the file
        let reader_stream = tokio_util::io::ReaderStream::new(file);
        
        // Wrap the stream to track progress
        let mut uploaded = 0u64;
        let async_stream = futures_util::stream::StreamExt::map(reader_stream, move |chunk| {
            if let Ok(bytes) = &chunk {
                uploaded += bytes.len() as u64;
                let progress = if total_size > 0 {
                    uploaded as f32 / total_size as f32
                } else {
                    0.0
                };
                on_progress(progress);
            }
            chunk
        });

        let file_part = multipart::Part::stream(reqwest::Body::wrap_stream(async_stream))
            .file_name(file_name)
            .mime_str("image/jpeg")?; // We'll assume JPEG for now, could be enhanced

        let form = multipart::Form::new()
            .part("original_file", file_part)
            .text("api_key", api_key.to_string())
            .text("original_name", file_name_clone)
            .text("local_path", file_path_str)
            .text("shot_at", chrono::Utc::now().to_rfc3339());

        println!("📤 Sending POST request to: {}", url);
        println!("📋 Form data includes: original_file, api_key ({}...), original_name, local_path, shot_at",
                 &api_key[..api_key.len().min(10)]);

        let response = self.client
            .post(&url)
            .multipart(form)
            .send()
            .await?;

        println!("📨 Response received with status: {}", response.status());

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await?;
            println!("❌ HTTP Error {}: {}", status, error_text);
            return Err(ApiError::ApiError {
                message: format!("HTTP {}: {}", status, error_text),
            });
        }

        println!("📄 Parsing JSON response...");
        let upload_response: UploadResponse = response.json().await?;
        println!("✅ Response parsed: success={}, message={}", upload_response.success, upload_response.message);

        if !upload_response.success {
            println!("❌ API returned error: {}", upload_response.message);
            return Err(ApiError::ApiError {
                message: upload_response.message,
            });
        }

        println!("🎉 Upload successful!");
        if let Some(ref photo_id) = upload_response.photo_id {
            println!("📸 Photo ID: {}", photo_id);
        }

        Ok(upload_response)
    }
}