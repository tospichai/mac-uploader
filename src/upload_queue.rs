use std::path::PathBuf;
use std::collections::VecDeque;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UploadStatus {
    Queued,
    Uploading,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadItem {
    pub id: Uuid,
    pub file_path: PathBuf,
    pub file_name: String,
    pub status: UploadStatus,
    pub added_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub progress: f32, // 0.0 to 1.0
    pub thumbnail_data: Option<Vec<u8>>, // Small thumbnail for UI display
}

impl UploadItem {
    pub fn new(file_path: PathBuf) -> Self {
        let file_name = file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        Self {
            id: Uuid::new_v4(),
            file_path,
            file_name,
            status: UploadStatus::Queued,
            added_at: Utc::now(),
            started_at: None,
            completed_at: None,
            progress: 0.0,
            thumbnail_data: None,
        }
    }

    pub fn start_upload(&mut self) {
        self.status = UploadStatus::Uploading;
        self.started_at = Some(Utc::now());
        self.progress = 0.1;
    }

    pub fn update_progress(&mut self, progress: f32) {
        self.progress = progress.clamp(0.0, 1.0);
    }

    pub fn complete_upload(&mut self) {
        self.status = UploadStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.progress = 1.0;
    }

    pub fn fail_upload(&mut self, error: String) {
        self.status = UploadStatus::Failed(error);
        self.completed_at = Some(Utc::now());
    }
}

pub struct UploadQueue {
    items: VecDeque<UploadItem>,
    max_concurrent_uploads: usize,
    active_uploads: usize,
}

impl UploadQueue {
    pub fn new() -> Self {
        Self {
            items: VecDeque::new(),
            max_concurrent_uploads: 3, // Default to 3 concurrent uploads
            active_uploads: 0,
        }
    }

    pub fn set_max_concurrent_uploads(&mut self, max: usize) {
        self.max_concurrent_uploads = max;
    }

    pub async fn add_file(&mut self, file_path: PathBuf) -> Option<Uuid> {
        println!("📝 UploadQueue::add_file called for: {}", file_path.display());

        // Check if file already exists in the queue to prevent duplicates
        // This is important since the file watcher now processes all files regardless of modification time
        if self.items.iter().any(|item| item.file_path == file_path) {
            println!("⚠ File already exists in queue: {}", file_path.display());
            return None;
        }

        let mut item = UploadItem::new(file_path.clone());

        // Try to generate thumbnail
        if let Ok(thumbnail) = self.generate_thumbnail(&file_path).await {
            item.thumbnail_data = Some(thumbnail);
            println!("✅ Thumbnail generated for: {}", file_path.display());
        } else {
            println!("⚠ Failed to generate thumbnail for: {}", file_path.display());
        }

        let id = item.id;
        self.items.push_back(item);

        println!("➕ File added to queue with ID: {}", id);
        println!("📊 Total items in queue: {}", self.items.len());

        Some(id)
    }

    async fn generate_thumbnail(&self, file_path: &PathBuf) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Try to open the image
        let img = image::open(file_path)?;

        // Resize to a small thumbnail (e.g., 100x100)
        let thumbnail = img.thumbnail(100, 100);

        // Convert to RGB bytes
        let rgb_img = thumbnail.to_rgb8();
        let (_width, _height) = rgb_img.dimensions();
        let pixels = rgb_img.into_raw();

        // For now, we'll just return the raw RGB data
        // In a real implementation, you might want to encode this as PNG or JPEG
        Ok(pixels)
    }

    pub fn get_items(&self) -> Vec<&UploadItem> {
        self.items.iter().collect()
    }

    pub fn get_queued_items(&self) -> Vec<&UploadItem> {
        self.items
            .iter()
            .filter(|item| matches!(item.status, UploadStatus::Queued))
            .collect()
    }

    pub fn get_active_items(&self) -> Vec<&UploadItem> {
        self.items
            .iter()
            .filter(|item| matches!(item.status, UploadStatus::Uploading))
            .collect()
    }

    pub fn get_completed_items(&self) -> Vec<&UploadItem> {
        self.items
            .iter()
            .filter(|item| matches!(item.status, UploadStatus::Completed))
            .collect()
    }

    pub fn get_failed_items(&self) -> Vec<&UploadItem> {
        self.items
            .iter()
            .filter(|item| matches!(item.status, UploadStatus::Failed(_)))
            .collect()
    }

    pub fn get_item_by_id(&self, id: Uuid) -> Option<&UploadItem> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn get_item_mut_by_id(&mut self, id: Uuid) -> Option<&mut UploadItem> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    pub fn remove_item(&mut self, id: Uuid) -> Option<UploadItem> {
        let index = self.items.iter().position(|item| item.id == id)?;
        Some(self.items.remove(index).unwrap())
    }

    pub fn clear_completed(&mut self) {
        self.items.retain(|item| !matches!(item.status, UploadStatus::Completed));
    }

    pub fn clear_failed(&mut self) {
        self.items.retain(|item| !matches!(item.status, UploadStatus::Failed(_)));
    }

    pub fn clear_all(&mut self) {
        self.items.clear();
    }

    pub fn can_start_upload(&self) -> bool {
        self.active_uploads < self.max_concurrent_uploads
    }

    pub fn increment_active_uploads(&mut self) {
        self.active_uploads += 1;
    }

    pub fn decrement_active_uploads(&mut self) {
        if self.active_uploads > 0 {
            self.active_uploads -= 1;
        }
    }

    pub fn get_next_queued_item(&mut self) -> Option<&mut UploadItem> {
        self.items.iter_mut().find(|item| matches!(item.status, UploadStatus::Queued))
    }

    pub fn get_stats(&self) -> QueueStats {
        let total = self.items.len();
        let queued = self.get_queued_items().len();
        let active = self.get_active_items().len();
        let completed = self.get_completed_items().len();
        let failed = self.get_failed_items().len();

        QueueStats {
            total,
            queued,
            active,
            completed,
            failed,
        }
    }
}

#[derive(Debug, Clone)]
pub struct QueueStats {
    pub total: usize,
    pub queued: usize,
    pub active: usize,
    pub completed: usize,
    pub failed: usize,
}