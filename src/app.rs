use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use serde::{Deserialize, Serialize};
use std::fs;
use crate::upload_queue::UploadQueue;
use crate::file_watcher::FileWatcher;
use crate::api_client::ApiClient;
use crate::upload_manager::UploadManager;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub api_endpoint: String,
    pub api_key: String,
    pub event_code: String,
    pub watch_folder: Option<String>,
}

pub struct MacUploaderApp {
    // Configuration
    api_endpoint: String,
    api_key: String,
    event_code: String,
    watch_folder: Option<PathBuf>,

    // UI state
    show_api_key: bool,
    connection_status: ConnectionStatus,
    logs: Vec<String>,
    is_watching: bool,
    new_logs_count: usize,

    // Core components
    upload_queue: Arc<Mutex<UploadQueue>>,
    file_watcher: Option<FileWatcher>,
    api_client: Option<Arc<ApiClient>>,
    upload_manager: Option<Arc<Mutex<UploadManager>>>,

    // Runtime
    runtime: Option<tokio::runtime::Runtime>,

    // Logging channel
    log_sender: Option<mpsc::UnboundedSender<String>>,
    log_receiver: Option<mpsc::UnboundedReceiver<String>>,

    // Config file path
    config_path: PathBuf,
}

#[derive(Debug, PartialEq, Default)]
pub enum ConnectionStatus {
    #[default]
    NotTested,
    Testing,
    Connected,
    Failed(String),
}

impl MacUploaderApp {
    pub fn new() -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        let (log_sender, log_receiver) = mpsc::unbounded_channel::<String>();

        // Determine config file path
        let config_path = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("config.json");

        // Load config if exists
        let config = Self::load_config(&config_path).unwrap_or_default();

        Self {
            api_endpoint: config.api_endpoint,
            api_key: config.api_key,
            event_code: config.event_code,
            watch_folder: config.watch_folder.and_then(|s| Some(PathBuf::from(s))),
            show_api_key: false,
            connection_status: ConnectionStatus::NotTested,
            logs: Vec::new(),
            is_watching: false,
            new_logs_count: 0,
            upload_queue: Arc::new(Mutex::new(UploadQueue::new())),
            file_watcher: None,
            api_client: None,
            upload_manager: None,
            runtime: Some(runtime),
            log_sender: Some(log_sender),
            log_receiver: Some(log_receiver),
            config_path,
        }
    }

    fn load_config(path: &PathBuf) -> Option<AppConfig> {
        if path.exists() {
            match fs::read_to_string(path) {
                Ok(content) => {
                    match serde_json::from_str::<AppConfig>(&content) {
                        Ok(config) => {
                            println!("Loaded config from {:?}", path);
                            Some(config)
                        }
                        Err(e) => {
                            eprintln!("Failed to parse config: {}", e);
                            None
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to read config file: {}", e);
                    None
                }
            }
        } else {
            None
        }
    }

    fn save_config(&self) {
        let config = AppConfig {
            api_endpoint: self.api_endpoint.clone(),
            api_key: self.api_key.clone(),
            event_code: self.event_code.clone(),
            watch_folder: self.watch_folder.as_ref().map(|p| p.to_string_lossy().to_string()),
        };

        match serde_json::to_string_pretty(&config) {
            Ok(json) => {
                if let Err(e) = fs::write(&self.config_path, json) {
                    eprintln!("Failed to save config: {}", e);
                } else {
                    println!("Saved config to {:?}", self.config_path);
                }
            }
            Err(e) => {
                eprintln!("Failed to serialize config: {}", e);
            }
        }
    }

    fn test_connection(&mut self) {
        if self.api_endpoint.is_empty() || self.api_key.is_empty() {
            self.logs.push("Please enter API endpoint and API key".to_string());
            return;
        }

        self.connection_status = ConnectionStatus::Testing;
        self.logs.push("Testing connection...".to_string());

        // Save config
        self.save_config();

        // Always recreate API client with current settings
        self.api_client = Some(Arc::new(ApiClient::new(
            self.api_endpoint.clone(),
            self.api_key.clone(),
        )));

        self.logs.push(format!("Created API client for endpoint: {}", self.api_endpoint));

        let api_client = self.api_client.as_ref().unwrap().clone();
        let api_key = self.api_key.clone();

        // Get the log sender
        let log_sender = self.log_sender.clone();

        if let Some(rt) = &self.runtime {
            let _ = rt.spawn(async move {
                match api_client.test_connection(&api_key).await {
                    Ok(response) => {
                        if let Some(sender) = log_sender {
                            let log_msg = format!(
                                "✅ Connection test successful: {} (Timestamp: {})",
                                response.message,
                                response.timestamp
                            );
                            let _ = sender.send(log_msg);
                            // Send status update
                            let _ = sender.send("STATUS:CONNECTED".to_string());
                        }
                    }
                    Err(e) => {
                        if let Some(sender) = log_sender {
                            let log_msg = format!("❌ Connection test failed: {}", e);
                            let _ = sender.send(log_msg);
                            // Send status update
                            let _ = sender.send(format!("STATUS:FAILED:{}", e));
                        }
                    }
                }
            });
        }
    }

    fn select_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .pick_folder()
        {
            self.watch_folder = Some(path.clone());
            self.logs.push(format!("Selected folder: {}", path.display()));

            // Save config
            self.save_config();

            // Start file watcher if all settings are configured
            if !self.api_endpoint.is_empty() && !self.api_key.is_empty() && !self.event_code.is_empty() {
                self.start_file_watcher();
            }
        }
    }

    fn start_file_watcher(&mut self) {
        if let Some(ref folder) = self.watch_folder {
            let upload_queue = self.upload_queue.clone();
            let folder_clone = folder.clone();
            let log_sender = self.log_sender.clone();

            // Log the attempt to start watching
            self.logs.push(format!("Attempting to start file watcher for: {}", folder.display()));

            // Create file watcher
            match FileWatcher::new(folder_clone.clone(), move |file_path| {
                let queue = upload_queue.clone();
                let file_path_clone = file_path.clone();
                let log_sender_clone = log_sender.clone();

                println!("🎯 File watcher callback triggered for: {}", file_path_clone.display());

                // We need to spawn a new runtime in the callback thread
                let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
                rt.block_on(async move {
                    let mut q = queue.lock().await;
                    let file_name = file_path_clone
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");

                    // Log file detection
                    if let Some(sender) = &log_sender_clone {
                        let _ = sender.send("🔔 FILE WATCHER CALLBACK TRIGGERED".to_string());
                        let _ = sender.send(format!("📁 Detected new file: {}", file_name));
                        let _ = sender.send(format!("📍 Full path: {}", file_path_clone.display()));
                        let _ = sender.send(format!("📊 Queue size before adding: {}", q.get_stats().total));
                    }

                    if let Some(item_id) = q.add_file(file_path).await {
                        // Log that file was added to queue
                        if let Some(sender) = &log_sender_clone {
                            let _ = sender.send(format!("➕ Added to upload queue: {} (ID: {})", file_name, item_id));
                            let _ = sender.send(format!("📊 Queue size after adding: {}", q.get_stats().total));
                        }
                    } else {
                        // Log that file was already in queue
                        if let Some(sender) = &log_sender_clone {
                            let _ = sender.send(format!("⚠ File already in queue: {}", file_name));
                        }
                    }
                });
            }) {
                Ok(watcher) => {
                    self.file_watcher = Some(watcher);
                    self.logs.push(format!("✅ Successfully started watching folder: {}", folder.display()));
                    self.logs.push("📡 File watcher is now active and monitoring for new image files...".to_string());
                }
                Err(e) => {
                    // Handle error with more detail
                    let error_msg = format!("❌ Failed to create file watcher: {}", e);
                    self.logs.push(error_msg.clone());
                    self.logs.push("💡 Possible solutions:".to_string());
                    self.logs.push("   • Check folder permissions".to_string());
                    self.logs.push("   • Try a different folder".to_string());
                    self.logs.push("   • Ensure the folder exists and is accessible".to_string());

                    // Also log to stderr for terminal visibility
                    eprintln!("{}", error_msg);
                }
            }
        }
    }

    fn start_watching(&mut self) {
        if self.watch_folder.is_none() {
            self.logs.push("Please select a folder to watch first".to_string());
            return;
        }

        if self.api_endpoint.is_empty() || self.api_key.is_empty() || self.event_code.is_empty() {
            self.logs.push("Please configure API settings first".to_string());
            return;
        }

        // Save config
        self.save_config();
        self.logs.push("Configuration saved".to_string());

        // Always create/update API client with current settings
        self.api_client = Some(Arc::new(ApiClient::new(
            self.api_endpoint.clone(),
            self.api_key.clone(),
        )));
        self.logs.push(format!("API client created for endpoint: {}", self.api_endpoint));

        // Create upload manager if not exists
        if self.upload_manager.is_none() {
            if let (Some(api_client), Some(folder)) = (self.api_client.as_ref(), self.watch_folder.as_ref()) {
                let manager = UploadManager::new(
                    self.upload_queue.clone(),
                    api_client.clone(),
                    self.event_code.clone(),
                    folder.clone(),
                    self.log_sender.clone(),
                    self.api_key.clone(), // Add the API key
                );
                self.upload_manager = Some(Arc::new(Mutex::new(manager)));
                self.logs.push("Upload manager created".to_string());
                self.logs.push(format!("🔑 API key configured: {}...", &self.api_key[..self.api_key.len().min(10)]));
            }
        }

        // Start the upload manager asynchronously
        if let Some(ref manager_arc) = self.upload_manager {
            let manager_clone = manager_arc.clone();
            let log_sender = self.log_sender.clone();

            if let Some(rt) = &self.runtime {
                rt.spawn(async move {
                    let mut manager = manager_clone.lock().await;
                    if let Err(e) = manager.start().await {
                        if let Some(sender) = log_sender {
                            let _ = sender.send(format!("❌ Failed to start upload manager: {}", e));
                        }
                    } else {
                        if let Some(sender) = log_sender {
                            let _ = sender.send("✅ Upload manager started successfully".to_string());
                        }
                    }
                });
                self.logs.push("Upload manager start command sent".to_string());
            }
        }

        // Start file watcher
        self.start_file_watcher();
        self.logs.push("File watching initialization complete".to_string());

        // Set the watching state to true
        self.is_watching = true;
    }

    fn stop_watching(&mut self) {
        // Stop the upload manager
        if let Some(ref manager_arc) = self.upload_manager {
            if let Some(rt) = &self.runtime {
                let manager_clone = manager_arc.clone();
                let log_sender = self.log_sender.clone();

                rt.spawn(async move {
                    let mut manager = manager_clone.lock().await;
                    manager.stop();
                    if let Some(sender) = log_sender {
                        let _ = sender.send("⏹ Upload manager stopped".to_string());
                    }
                });
            }
        }

        // Drop the file watcher to stop it
        self.file_watcher = None;
        self.logs.push("⏹ File watching stopped".to_string());

        // Set the watching state to false
        self.is_watching = false;
    }

    fn open_gallery(&self) {
        if !self.api_endpoint.is_empty() && !self.event_code.is_empty() {
            let url = format!("{}/{}/photos", self.api_endpoint.trim_end_matches('/'), self.event_code);
            match webbrowser::open(&url) {
                Ok(_) => {
                    if let Some(sender) = &self.log_sender {
                        let _ = sender.send(format!("🌐 Opening gallery in browser: {}", url));
                    }
                }
                Err(e) => {
                    if let Some(sender) = &self.log_sender {
                        let _ = sender.send(format!("❌ Failed to open browser: {}", e));
                    }
                }
            }
        } else {
            if let Some(sender) = &self.log_sender {
                let _ = sender.send("⚠️ Please configure API endpoint and event code first".to_string());
            }
        }
    }
}

impl eframe::App for MacUploaderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for new log messages
        if let Some(ref mut receiver) = self.log_receiver {
            while let Ok(log_msg) = receiver.try_recv() {
                // Check for status updates
                if log_msg.starts_with("STATUS:") {
                    let status_parts: Vec<&str> = log_msg.splitn(3, ':').collect();
                    if status_parts.len() >= 2 {
                        match status_parts[1] {
                            "CONNECTED" => {
                                self.connection_status = ConnectionStatus::Connected;
                            }
                            "FAILED" => {
                                let error_msg = if status_parts.len() > 2 {
                                    status_parts[2].to_string()
                                } else {
                                    "Unknown error".to_string()
                                };
                                self.connection_status = ConnectionStatus::Failed(error_msg);
                            }
                            _ => {}
                        }
                    }
                } else {
                    self.logs.push(log_msg);
                    self.new_logs_count += 1;
                }
            }
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Mac Photo Uploader");

            // Configuration section
            ui.separator();
            ui.heading("Configuration");

            // API Endpoint
            ui.horizontal(|ui| {
                ui.label("API Endpoint:");
                ui.text_edit_singleline(&mut self.api_endpoint);
            });

            // API Key
            ui.horizontal(|ui| {
                ui.label("API Key:");
                if self.show_api_key {
                    ui.text_edit_singleline(&mut self.api_key);
                } else {
                    let masked = "*".repeat(self.api_key.len().min(20));
                    ui.label(masked);
                }

                if ui.button(if self.show_api_key { "Hide" } else { "Show" }).clicked() {
                    self.show_api_key = !self.show_api_key;
                }
            });

            // Event Code
            ui.horizontal(|ui| {
                ui.label("Event Code:");
                ui.text_edit_singleline(&mut self.event_code);
            });

            // Watch Folder
            ui.horizontal(|ui| {
                ui.label("Watch Folder:");
                if let Some(ref folder) = self.watch_folder {
                    ui.label(folder.display().to_string());
                } else {
                    ui.label("No folder selected");
                }

                if ui.button("Select Folder").clicked() {
                    self.select_folder();
                }
            });

            // Test Connection button
            ui.horizontal(|ui| {
                if ui.button("Test Connection").clicked() {
                    self.test_connection();
                }

                match &self.connection_status {
                    ConnectionStatus::NotTested => { ui.label(""); }
                    ConnectionStatus::Testing => { ui.label("Testing..."); }
                    ConnectionStatus::Connected => {
                        ui.label("✅ Connected");
                    }
                    ConnectionStatus::Failed(msg) => {
                        ui.label(format!("❌ Failed: {}", msg));
                    }
                }
            });

            // Open Gallery button
            ui.horizontal(|ui| {
                if ui.button("Open Online Gallery").clicked() {
                    self.open_gallery();
                }
            });

            // Start/Stop Watching toggle button
            ui.horizontal(|ui| {
                let button_text = if self.is_watching { "Stop Watching" } else { "Start Watching" };
                if ui.button(button_text).clicked() {
                    if self.is_watching {
                        self.stop_watching();
                    } else {
                        self.start_watching();
                    }
                }
            });

            // Upload Queue section
            ui.separator();
            ui.heading("Upload Queue");

            // Display upload queue
            if let Ok(queue) = self.upload_queue.try_lock() {
                let stats = queue.get_stats();

                ui.horizontal(|ui| {
                    ui.label(format!("Total: {}", stats.total));
                    ui.label(format!("Queued: {}", stats.queued));
                    ui.label(format!("Active: {}", stats.active));
                    ui.label(format!("Completed: {}", stats.completed));
                    ui.label(format!("Failed: {}", stats.failed));
                });

                // Show items in queue (limited to latest 3 items, newest first)
                egui::ScrollArea::vertical()
                    .id_salt("upload_queue_scroll")
                    .max_height(150.0) // Reduced height to make room for logs
                    .show(ui, |ui| {
                        // Get all items, sort by added_at (newest first), and take only 3
                        let mut items = queue.get_items();
                        items.sort_by(|a, b| b.added_at.cmp(&a.added_at));
                        for item in items.iter().take(3) {
                            ui.horizontal(|ui| {
                                // Show thumbnail if available
                                if let Some(_thumbnail) = &item.thumbnail_data {
                                    ui.label("🖼");
                                } else {
                                    ui.label("📄");
                                }

                                ui.label(&item.file_name);

                                // Show status
                                match &item.status {
                                    crate::upload_queue::UploadStatus::Queued => {
                                        ui.label("Queued");
                                    }
                                    crate::upload_queue::UploadStatus::Uploading => {
                                        ui.label(format!("Uploading... {:.0}%", item.progress * 100.0));
                                        ui.add(egui::ProgressBar::new(item.progress).show_percentage());
                                    }
                                    crate::upload_queue::UploadStatus::Completed => {
                                        ui.label("✅ Completed");
                                    }
                                    crate::upload_queue::UploadStatus::Failed(msg) => {
                                        ui.label(format!("❌ Failed: {}", msg));
                                    }
                                }
                            });
                        }
                    });
            }

            // Logs section - use remaining vertical space
            ui.separator();
            ui.heading("Logs");

            // Calculate available height for logs
            let available_height = ui.available_height() - 20.0; // Leave some padding

            // Use available vertical space for logs
            egui::ScrollArea::vertical()
                .id_salt("logs_scroll")
                .stick_to_bottom(true)
                .auto_shrink([false; 2]) // Don't shrink in either direction
                .max_height(available_height)
                .show(ui, |ui| {
                    for log in &self.logs {
                        ui.label(log);
                    }

                    // Reset new logs count after displaying
                    if self.new_logs_count > 0 {
                        self.new_logs_count = 0;
                    }
                });
        });
    }
}