use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, RwLock};
use uuid::Uuid;
use crate::upload_queue::UploadQueue;
use crate::api_client::ApiClient;
use std::fs;

pub struct UploadManager {
    queue: Arc<Mutex<UploadQueue>>,
    api_client: Arc<ApiClient>,
    event_code: Arc<RwLock<String>>,
    watch_folder: PathBuf,
    is_running: bool,
    log_sender: Option<mpsc::UnboundedSender<String>>,
    api_key: String,
}

impl UploadManager {
    pub fn new(
        queue: Arc<Mutex<UploadQueue>>,
        api_client: Arc<ApiClient>,
        event_code: String,
        watch_folder: PathBuf,
        log_sender: Option<mpsc::UnboundedSender<String>>,
        api_key: String,
    ) -> Self {
        Self {
            queue,
            api_client,
            event_code: Arc::new(RwLock::new(event_code)),
            watch_folder,
            is_running: false,
            log_sender,
            api_key,
        }
    }

    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_running {
            return Ok(());
        }

        self.is_running = true;

        // Log that upload manager is starting
        if let Some(ref sender) = self.log_sender {
            let event_code = self.event_code.read().await;
            let _ = sender.send("🚀 UploadManager starting...".to_string());
            let _ = sender.send(format!("📋 Event code: {}", *event_code));
            let _ = sender.send(format!("🔑 API key: {}...", &self.api_key[..self.api_key.len().min(10)]));
            let _ = sender.send(format!("📁 Watch folder: {}", self.watch_folder.display()));
        }

        // Create uploaded folder if it doesn't exist
        let uploaded_folder = self.watch_folder.join("uploaded");
        fs::create_dir_all(&uploaded_folder)?;

        // Start the upload loop
        let queue = self.queue.clone();
        let api_client = self.api_client.clone();
        let event_code = self.event_code.clone();
        let watch_folder = self.watch_folder.clone();
        let log_sender = self.log_sender.clone();
        let api_key = self.api_key.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));

            loop {
                interval.tick().await;

                let mut q = queue.lock().await;

                // Queue status logging removed to reduce log spam
                // let stats = q.get_stats();
                // if let Some(ref sender) = log_sender {
                //     let _ = sender.send(
                //         format!("📊 Queue status - Total: {}, Queued: {}, Active: {}, Completed: {}, Failed: {}",
                //         stats.total, stats.queued, stats.active, stats.completed, stats.failed)
                //     );
                // }

                // Get next queued item if available
                if let Some(item) = q.get_next_queued_item() {
                    let item_id = item.id;
                    let file_path = item.file_path.clone();
                    let api_client = api_client.clone();
                    let event_code = event_code.clone();
                    let watch_folder = watch_folder.clone();
                    let queue = queue.clone();
                    let log_sender_clone = log_sender.clone(); // Clone for the new task
                    let api_key_clone = api_key.clone(); // Clone API key for the new task

                    // Mark as uploading
                    item.start_upload();

                    // Log that upload is starting before dropping q
                    if let Some(ref sender) = log_sender {
                        let _ = sender.send(format!("⬆ Starting upload for: {}", item.file_name));
                    }

                    drop(q); // Release the lock before starting the upload

                    // Start upload in a separate task
                    tokio::spawn(async move {
                        // Get the current event code at upload time
                        let event_code_value = event_code.read().await;
                        let result = Self::upload_and_move_file(
                            &api_client,
                            &event_code_value,
                            &file_path,
                            &watch_folder,
                            item_id,
                            &queue,
                            log_sender_clone.clone(),
                            &api_key_clone, // Pass the API key clone
                        ).await;

                        // Prepare file name for logging after the upload attempt
                        let file_name = file_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown");

                        match result {
                            Ok(response) => {
                                // Upload succeeded
                                let mut q = queue.lock().await;
                                if let Some(item) = q.get_item_mut_by_id(item_id) {
                                    item.complete_upload();
                                }
                                drop(q); // Release lock before logging

                                // Log success with response details
                                let log_msg = format!(
                                    "✅ Upload successful: {} (Photo ID: {})",
                                    file_name,
                                    response.photo_id.unwrap_or_else(|| "N/A".to_string())
                                );

                                // Clone sender for this use
                                if let Some(sender) = log_sender_clone.clone() {
                                    if let Some(s3_info) = &response.s3 {
                                        let s3_msg = format!(
                                            "   S3: {} in bucket {} ({})",
                                            s3_info.original_key,
                                            s3_info.bucket,
                                            s3_info.region
                                        );
                                        let _ = sender.send(format!("{}\n   {}", log_msg, s3_msg));
                                    } else {
                                        let _ = sender.send(log_msg);
                                    }
                                }
                            }
                            Err(e) => {
                                // Upload failed
                                let mut q = queue.lock().await;
                                if let Some(item) = q.get_item_mut_by_id(item_id) {
                                    item.fail_upload(format!("Upload failed: {}", e));
                                }
                                drop(q); // Release lock before logging

                                // Log error
                                let log_msg = format!("❌ Upload failed for {}: {}", file_name, e);

                                // Clone sender for this use
                                if let Some(sender) = log_sender_clone.clone() {
                                    let _ = sender.send(log_msg);
                                }
                            }
                        }
                    });
                }
            }
        });

        Ok(())
    }

    async fn upload_and_move_file(
        api_client: &ApiClient,
        event_code: &str,
        file_path: &PathBuf,
        watch_folder: &PathBuf,
        item_id: Uuid,
        queue: &Arc<Mutex<UploadQueue>>,
        log_sender: Option<mpsc::UnboundedSender<String>>,
        api_key: &str,
    ) -> Result<crate::api_client::UploadResponse, String> {
        // Log the upload attempt
        if let Some(ref sender) = log_sender {
            let _ = sender.send(format!("📤 Attempting to upload: {}", file_path.display()));
            let _ = sender.send(format!("🔑 Using API key: {}...", &api_key[..api_key.len().min(10)]));
            let _ = sender.send(format!("🎯 Event code: {}", event_code));
        }

        // Perform the upload with the correct API key
        let response = api_client.upload_photo(event_code, file_path, api_key).await
            .map_err(|e| format!("API error: {}", e))?;

        // If upload succeeded, move the file to uploaded folder
        let uploaded_folder = watch_folder.join("uploaded");
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| "Invalid file name".to_string())?;

        let new_path = uploaded_folder.join(file_name);

        // If file already exists in uploaded folder, add a timestamp
        let final_path = if new_path.exists() {
            let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            let stem = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| "Invalid file stem".to_string())?
                .to_string();
            let extension = file_path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("");

            uploaded_folder.join(format!("{}_{}.{}", stem, timestamp, extension))
        } else {
            new_path
        };

        fs::rename(file_path, &final_path)
            .map_err(|e| format!("Failed to move file: {}", e))?;

        // Update the item with the new path
        let mut q = queue.lock().await;
        if let Some(item) = q.get_item_mut_by_id(item_id) {
            item.progress = 0.9; // Almost done
        }

        Ok(response)
    }

    pub fn stop(&mut self) {
        self.is_running = false;
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }

    pub async fn update_event_code(&self, new_event_code: String) {
        let mut event_code = self.event_code.write().await;
        if *event_code != new_event_code {
            let old_event_code = event_code.clone();
            *event_code = new_event_code.clone();

            // Log the change
            if let Some(ref sender) = self.log_sender {
                let _ = sender.send(format!("🔄 Event code updated: {} -> {}", old_event_code, new_event_code));
            }
        }
    }
}