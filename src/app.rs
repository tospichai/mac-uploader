use crate::api_client::ApiClient;
use crate::file_watcher::FileWatcher;
use crate::ui_theme::MacTheme;
use crate::upload_manager::UploadManager;
use crate::upload_queue::UploadQueue;
use eframe::egui::{self, Stroke};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

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

    // UI Theme
    theme: MacTheme,
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

        let theme = MacTheme::default();

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
            theme,
        }
    }

    fn load_config(path: &PathBuf) -> Option<AppConfig> {
        if path.exists() {
            match fs::read_to_string(path) {
                Ok(content) => match serde_json::from_str::<AppConfig>(&content) {
                    Ok(config) => {
                        println!("Loaded config from {:?}", path);
                        Some(config)
                    }
                    Err(e) => {
                        eprintln!("Failed to parse config: {}", e);
                        None
                    }
                },
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
            watch_folder: self
                .watch_folder
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
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
            self.logs
                .push("Please enter API endpoint and API key".to_string());
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

        self.logs.push(format!(
            "Created API client for endpoint: {}",
            self.api_endpoint
        ));

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
                                response.message, response.timestamp
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
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.watch_folder = Some(path.clone());
            self.logs
                .push(format!("Selected folder: {}", path.display()));

            // Save config
            self.save_config();

            // Start file watcher if all settings are configured
            if !self.api_endpoint.is_empty()
                && !self.api_key.is_empty()
                && !self.event_code.is_empty()
            {
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
            self.logs.push(format!(
                "Attempting to start file watcher for: {}",
                folder.display()
            ));

            // Create file watcher
            match FileWatcher::new(folder_clone.clone(), move |file_path| {
                let queue = upload_queue.clone();
                let file_path_clone = file_path.clone();
                let log_sender_clone = log_sender.clone();

                println!(
                    "🎯 File watcher callback triggered for: {}",
                    file_path_clone.display()
                );

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
                        let _ = sender.send(format!(
                            "📊 Queue size before adding: {}",
                            q.get_stats().total
                        ));
                    }

                    if let Some(item_id) = q.add_file(file_path).await {
                        // Log that file was added to queue
                        if let Some(sender) = &log_sender_clone {
                            let _ = sender.send(format!(
                                "➕ Added to upload queue: {} (ID: {})",
                                file_name, item_id
                            ));
                            let _ = sender.send(format!(
                                "📊 Queue size after adding: {}",
                                q.get_stats().total
                            ));
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
                    self.logs.push(format!(
                        "✅ Successfully started watching folder: {}",
                        folder.display()
                    ));
                    self.logs.push(
                        "📡 File watcher is now active and monitoring for new image files..."
                            .to_string(),
                    );
                }
                Err(e) => {
                    // Handle error with more detail
                    let error_msg = format!("❌ Failed to create file watcher: {}", e);
                    self.logs.push(error_msg.clone());
                    self.logs.push("💡 Possible solutions:".to_string());
                    self.logs.push("   • Check folder permissions".to_string());
                    self.logs.push("   • Try a different folder".to_string());
                    self.logs
                        .push("   • Ensure the folder exists and is accessible".to_string());

                    // Also log to stderr for terminal visibility
                    eprintln!("{}", error_msg);
                }
            }
        }
    }

    fn start_watching(&mut self) {
        if self.watch_folder.is_none() {
            self.logs
                .push("Please select a folder to watch first".to_string());
            return;
        }

        if self.api_endpoint.is_empty() || self.api_key.is_empty() || self.event_code.is_empty() {
            self.logs
                .push("Please configure API settings first".to_string());
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
        self.logs.push(format!(
            "API client created for endpoint: {}",
            self.api_endpoint
        ));

        // Create upload manager if not exists
        if self.upload_manager.is_none() {
            if let (Some(api_client), Some(folder)) =
                (self.api_client.as_ref(), self.watch_folder.as_ref())
            {
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
                self.logs.push(format!(
                    "🔑 API key configured: {}...",
                    &self.api_key[..self.api_key.len().min(10)]
                ));
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
                            let _ =
                                sender.send(format!("❌ Failed to start upload manager: {}", e));
                        }
                    } else {
                        if let Some(sender) = log_sender {
                            let _ =
                                sender.send("✅ Upload manager started successfully".to_string());
                        }
                    }
                });
                self.logs
                    .push("Upload manager start command sent".to_string());
            }
        }

        // Start file watcher
        self.start_file_watcher();
        self.logs
            .push("File watching initialization complete".to_string());

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
            let url = format!(
                "{}/{}/photos",
                self.api_endpoint.trim_end_matches('/'),
                self.event_code
            );
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
                let _ = sender
                    .send("⚠️ Please configure API endpoint and event code first".to_string());
            }
        }
    }
}

impl eframe::App for MacUploaderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply the theme
        self.theme.apply_to_ctx(ctx);

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

        // Main container with padding
        egui::CentralPanel::default().show(ctx, |ui| {
            // ui.add_space(self.theme.spacing_large);

            // App title
            // ui.horizontal(|ui| {
            //     ui.add_space(self.theme.spacing_large);
            //     ui.heading(egui::RichText::new("Mac Photo Uploader").size(24.0).color(self.theme.text_primary));
            //     ui.add_space(self.theme.spacing_large);
            // });
            ui.add_space(self.theme.padding_medium);

            // Configuration Panel with attached buttons
            self.show_configuration(ui);
            self.show_action_buttons(ui);

            // Calculate remaining height for dynamic layout
            let remaining_height = ui.available_height();

            // Upload Queue Panel - content-based height with maximum
            ui.allocate_ui_with_layout(
                egui::Vec2::new(ui.available_width(), 160.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    self.show_upload_queue_panel(ui);
                },
            );

            // Logs Panel - fill remaining space to bottom
            ui.allocate_ui_with_layout(
                egui::Vec2::new(ui.available_width(), ui.available_height()),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    self.show_logs_panel(ui);
                },
            );

            ui.add_space(self.theme.spacing_large);
        });
    }
}

impl MacUploaderApp {
    fn show_configuration(&mut self, ui: &mut egui::Ui) {
        let frame = self.theme.card_frame_borderless();
        frame.show(ui, |ui| {
            ui.vertical(|ui| {
                // Section title
                ui.label(
                    egui::RichText::new("Configuration")
                        .size(18.0)
                        .strong()
                        .color(self.theme.text_primary),
                );
                ui.add_space(self.theme.spacing_medium);

                // Calculate label width based on longest label
                let labels = ["API Endpoint", "API Key", "Event Code", "Watch Folder"];
                let label_width = labels
                    .iter()
                    .map(|label| label.len() as f32 * 8.0) // Approximate width based on character count
                    .fold(0.0, f32::max)
                    + 20.0; // Add some padding

                // Two-column layout for form fields
                ui.vertical(|ui| {
                    // API Endpoint
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [label_width, 24.0],
                            egui::Label::new(
                                egui::RichText::new("API Endpoint")
                                    .size(14.0)
                                    .color(self.theme.text_secondary),
                            ),
                        );
                        ui.add_sized(
                            [ui.available_width(), 24.0],
                            egui::TextEdit::singleline(&mut self.api_endpoint)
                                .font(egui::TextStyle::Body)
                                .margin(egui::Vec2::new(8.0, 4.0)),
                        );
                    });
                    ui.add_space(self.theme.spacing_medium);

                    // API Key
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [label_width, 24.0],
                            egui::Label::new(
                                egui::RichText::new("API Key")
                                    .size(14.0)
                                    .color(self.theme.text_secondary),
                            ),
                        );
                        ui.horizontal(|ui| {
                            if self.show_api_key {
                                ui.add_sized(
                                    [ui.available_width() - 80.0, 24.0],
                                    egui::TextEdit::singleline(&mut self.api_key)
                                        .font(egui::TextStyle::Body)
                                        .margin(egui::Vec2::new(8.0, 4.0)),
                                );
                            } else {
                                let masked = "*".repeat(self.api_key.len().min(20));
                                ui.add_sized(
                                    [ui.available_width() - 80.0, 24.0],
                                    egui::Label::new(
                                        egui::RichText::new(masked).color(self.theme.text_muted),
                                    ),
                                );
                            }

                            if ui
                                .add_sized(
                                    [70.0, 24.0],
                                    egui::Button::new(
                                        egui::RichText::new(if self.show_api_key {
                                            "Hide"
                                        } else {
                                            "Show"
                                        })
                                        .size(12.0)
                                        .color(self.theme.text_primary),
                                    ),
                                )
                                .clicked()
                            {
                                self.show_api_key = !self.show_api_key;
                            }
                        });
                    });
                    ui.add_space(self.theme.spacing_medium);

                    // Event Code
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [label_width, 24.0],
                            egui::Label::new(
                                egui::RichText::new("Event Code")
                                    .size(14.0)
                                    .color(self.theme.text_secondary),
                            ),
                        );
                        ui.add_sized(
                            [ui.available_width(), 24.0],
                            egui::TextEdit::singleline(&mut self.event_code)
                                .font(egui::TextStyle::Body)
                                .margin(egui::Vec2::new(8.0, 4.0)),
                        );
                    });
                    ui.add_space(self.theme.spacing_medium);

                    // Watch Folder
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [label_width, 24.0],
                            egui::Label::new(
                                egui::RichText::new("Watch Folder")
                                    .size(14.0)
                                    .color(self.theme.text_secondary),
                            ),
                        );
                        ui.horizontal(|ui| {
                            let folder_text = if let Some(ref folder) = self.watch_folder {
                                folder.display().to_string()
                            } else {
                                "No folder selected".to_string()
                            };

                            ui.add_sized(
                                [ui.available_width() - 120.0, 24.0],
                                egui::Label::new(
                                    egui::RichText::new(folder_text).color(self.theme.text_primary),
                                ),
                            );

                            if ui
                                .add_sized(
                                    [110.0, 24.0],
                                    egui::Button::new(
                                        egui::RichText::new("Select Folder")
                                            .size(12.0)
                                            .color(self.theme.text_primary),
                                    ),
                                )
                                .clicked()
                            {
                                self.select_folder();
                            }
                        });
                    });
                    ui.add_space(self.theme.spacing_medium);

                    // Connection status and test button
                    ui.horizontal(|ui| {
                        if ui
                            .add_sized(
                                [120.0, 25.0],
                                egui::Button::new(
                                    egui::RichText::new("Test Connection")
                                        .size(14.0)
                                        .color(self.theme.text_primary),
                                ),
                            )
                            .clicked()
                        {
                            self.test_connection();
                        }

                        ui.add_space(self.theme.spacing_small);

                        match &self.connection_status {
                            ConnectionStatus::NotTested => {
                                ui.label(
                                    egui::RichText::new("Not tested").color(self.theme.text_muted),
                                );
                            }
                            ConnectionStatus::Testing => {
                                ui.spinner();
                                ui.label(
                                    egui::RichText::new("Testing...").color(self.theme.warning),
                                );
                            }
                            ConnectionStatus::Connected => {
                                ui.label(
                                    egui::RichText::new("✅ Connected").color(self.theme.success),
                                );
                            }
                            ConnectionStatus::Failed(msg) => {
                                ui.label(
                                    egui::RichText::new(format!("❌ {}", msg))
                                        .color(self.theme.error),
                                );
                            }
                        }
                    });
                });
            });
        });
        ui.add_space(self.theme.spacing_medium);
    }

    fn show_action_buttons(&mut self, ui: &mut egui::Ui) {
        let frame = self.theme.card_frame_borderless();
        frame.show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    // -----------------------------------------
                    // Start / Stop Watching Button
                    // -----------------------------------------
                    let (button_text, normal_color, hover_color) = if self.is_watching {
                        (
                            "Stop Watching",
                            self.theme.error,
                            egui::Color32::from_rgb(220, 38, 38),
                        )
                    } else {
                        (
                            "Start Watching",
                            self.theme.success,
                            egui::Color32::from_rgb(34, 197, 94),
                        )
                    };

                    let main_size = egui::vec2(140.0, 36.0);
                    let (main_rect, main_response) =
                        ui.allocate_exact_size(main_size, egui::Sense::click());

                    let main_bg = if main_response.hovered() {
                        hover_color
                    } else {
                        normal_color
                    };

                    let main_button = egui::Button::new(
                        egui::RichText::new(button_text)
                            .size(14.0)
                            .color(egui::Color32::WHITE)
                            .strong(),
                    )
                    .rounding(self.theme.radius_medium)
                    .fill(main_bg);

                    let main_click = ui.put(main_rect, main_button);

                    if main_click.clicked() {
                        if self.is_watching {
                            self.stop_watching();
                        } else {
                            self.start_watching();
                        }
                    }

                    ui.add_space(self.theme.spacing_small);

                    // -----------------------------------------
                    // Open Online Gallery Button
                    // -----------------------------------------
                    let gallery_size = egui::vec2(160.0, 36.0);
                    let (gallery_rect, gallery_response) =
                        ui.allocate_exact_size(gallery_size, egui::Sense::click());

                    let gallery_bg = if gallery_response.hovered() {
                        self.theme.surface_hover
                    } else {
                        self.theme.surface
                    };

                    let gallery_button = egui::Button::new(
                        egui::RichText::new("Open Online Gallery")
                            .size(14.0)
                            .color(self.theme.text_primary),
                    )
                    .rounding(self.theme.radius_medium)
                    .fill(gallery_bg);

                    let gallery_click = ui.put(gallery_rect, gallery_button);

                    if gallery_click.clicked() {
                        self.open_gallery();
                    }
                });
            });
        });
        ui.add_space(self.theme.spacing_medium);
    }

    fn show_upload_queue_panel(&mut self, ui: &mut egui::Ui) {
        let frame = self.theme.card_frame();
        frame.show(ui, |ui| {
            ui.vertical(|ui| {
                // Section title
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Upload Queue")
                            .size(18.0)
                            .color(self.theme.text_primary),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Ok(queue) = self.upload_queue.try_lock() {
                            let stats = queue.get_stats();
                            ui.label(
                                egui::RichText::new(format!("{} total", stats.total))
                                    .size(14.0)
                                    .color(self.theme.text_muted),
                            );
                        }
                    });
                });
                ui.add_space(self.theme.spacing_medium);

                // Display upload queue stats
                if let Ok(queue) = self.upload_queue.try_lock() {
                    let stats = queue.get_stats();

                    // Stats row with better visual design - distribute evenly across full width
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(ui.available_width() / 5.0, ui.available_height()),
                            egui::Layout::centered_and_justified(egui::Direction::TopDown),
                            |ui| {
                                self.show_stat_item(
                                    ui,
                                    "Total",
                                    stats.total,
                                    self.theme.text_primary,
                                )
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(ui.available_width() / 4.0, ui.available_height()),
                            egui::Layout::centered_and_justified(egui::Direction::TopDown),
                            |ui| {
                                self.show_stat_item(ui, "Queued", stats.queued, self.theme.warning)
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(ui.available_width() / 3.0, ui.available_height()),
                            egui::Layout::centered_and_justified(egui::Direction::TopDown),
                            |ui| self.show_stat_item(ui, "Active", stats.active, self.theme.info),
                        );
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(ui.available_width() / 2.0, ui.available_height()),
                            egui::Layout::centered_and_justified(egui::Direction::TopDown),
                            |ui| {
                                self.show_stat_item(
                                    ui,
                                    "Completed",
                                    stats.completed,
                                    self.theme.success,
                                )
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(ui.available_width(), ui.available_height()),
                            egui::Layout::centered_and_justified(egui::Direction::TopDown),
                            |ui| self.show_stat_item(ui, "Failed", stats.failed, self.theme.error),
                        );
                    });
                    ui.add_space(self.theme.spacing_medium);

                    // Show items in queue - content-based height with scroll
                    if stats.total > 0 {
                        // Fixed maximum height to prevent unnecessary expansion
                        let max_height = 150.0;
                        egui::ScrollArea::vertical()
                            .id_salt("upload_queue_scroll")
                            .max_height(max_height)
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                let mut items = queue.get_items();
                                items.sort_by(|a, b| b.added_at.cmp(&a.added_at));

                                // Show items with content-based height
                                for item in items.iter().take(10) {
                                    self.show_queue_item(ui, item);
                                }
                            });
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                egui::RichText::new("No files in queue")
                                    .size(14.0)
                                    .color(self.theme.text_muted),
                            );
                        });
                    }
                }
            });
        });
        ui.add_space(self.theme.spacing_medium);
    }

    fn show_stat_item(&self, ui: &mut egui::Ui, label: &str, count: usize, color: egui::Color32) {
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new(format!("{}", count))
                    .size(20.0)
                    .color(color)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(label)
                    .size(12.0)
                    .color(self.theme.text_muted),
            );
        });
    }

    fn show_queue_item(&self, ui: &mut egui::Ui, item: &crate::upload_queue::UploadItem) {
        let frame = egui::Frame {
            inner_margin: egui::Margin::symmetric(
                self.theme.spacing_small,
                self.theme.spacing_small,
            ),
            outer_margin: egui::Margin::symmetric(0.0, 0.0),
            rounding: self.theme.radius_small,
            // fill: self.theme.surface,
            // stroke: Stroke::new(1.0, self.theme.border),
            ..Default::default()
        };

        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                // File icon
                ui.label(
                    egui::RichText::new(if item.thumbnail_data.is_some() {
                        "🖼"
                    } else {
                        "📄"
                    })
                    .size(16.0),
                );

                ui.add_space(self.theme.spacing_small);

                // File name and status
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&item.file_name)
                            .size(14.0)
                            .color(self.theme.text_primary),
                    );

                    // Status with appropriate color
                    let (status_text, status_color) = match &item.status {
                        crate::upload_queue::UploadStatus::Queued => {
                            ("Queued", self.theme.text_muted)
                        }
                        crate::upload_queue::UploadStatus::Uploading => {
                            ("Uploading...", self.theme.warning)
                        }
                        crate::upload_queue::UploadStatus::Completed => {
                            ("✅ Completed", self.theme.success)
                        }
                        crate::upload_queue::UploadStatus::Failed(msg) => {
                            (&format!("❌ {}", msg) as &str, self.theme.error)
                        }
                    };

                    ui.label(
                        egui::RichText::new(status_text)
                            .size(12.0)
                            .color(status_color),
                    );

                    // Progress bar for uploading items
                    if matches!(item.status, crate::upload_queue::UploadStatus::Uploading) {
                        ui.add_space(2.0);
                        ui.add(
                            egui::ProgressBar::new(item.progress)
                                .desired_width(ui.available_width())
                                .fill(self.theme.surface_hover)
                                .show_percentage(),
                        );
                    }
                });
            });
        });
    }

    fn show_logs_panel(&mut self, ui: &mut egui::Ui) {
        let frame = self.theme.card_frame();
        frame.show(ui, |ui| {
            // Use all available height
            ui.allocate_ui_with_layout(
                egui::Vec2::new(ui.available_width(), ui.available_height()),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    // Section title
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Logs")
                                .size(18.0)
                                .color(self.theme.text_primary),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if self.new_logs_count > 0 {
                                ui.label(
                                    egui::RichText::new(format!("{} new", self.new_logs_count))
                                        .size(12.0)
                                        .color(self.theme.accent),
                                );
                            }
                        });
                    });
                    ui.add_space(self.theme.spacing_medium);

                    // Logs scroll area - use all remaining height
                    let available_height = ui.available_height();
                    egui::ScrollArea::vertical()
                        .id_salt("logs_scroll")
                        .stick_to_bottom(true)
                        .auto_shrink([false; 2])
                        .max_height(available_height)
                        .show(ui, |ui| {
                            if self.logs.is_empty() {
                                ui.centered_and_justified(|ui| {
                                    ui.label(
                                        egui::RichText::new("No logs yet")
                                            .size(14.0)
                                            .color(self.theme.text_muted),
                                    );
                                });
                            } else {
                                // Show more log entries with better formatting
                                for (i, log) in self.logs.iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        // Add timestamp or index for better readability
                                        ui.label(
                                            egui::RichText::new(format!("{:>3}", i + 1))
                                                .size(10.0)
                                                .color(self.theme.text_muted),
                                        );
                                        ui.add_space(self.theme.spacing_small);
                                        ui.label(
                                            egui::RichText::new(log)
                                                .size(12.0)
                                                .color(self.theme.text_secondary),
                                        );
                                    });
                                }
                            }
                        });

                    // Reset new logs count after displaying
                    if self.new_logs_count > 0 {
                        self.new_logs_count = 0;
                    }
                },
            );
        });
    }
}
