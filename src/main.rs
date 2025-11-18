mod app;
mod file_watcher;
mod upload_queue;
mod api_client;
mod upload_manager;
mod ui_theme;

use eframe::egui;
use std::env;

fn main() -> Result<(), eframe::Error> {
    // Force OpenGL backend on macOS to avoid Metal compatibility issues
    env::set_var("wgpu_backend", "gl");

    env_logger::init(); // Initialize logger

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([550.0, 650.0])
            .with_resizable(false)
            .with_decorations(true)   // Restore title bar and borders
            .with_transparent(true),  // Allow transparent background for seamless look
        ..Default::default()
    };

    eframe::run_native(
        "Mac Photo Uploader",
        options,
        Box::new(|_cc| {
            // This is where you initialize your app
            Ok(Box::new(app::MacUploaderApp::new()))
        }),
    )
}
