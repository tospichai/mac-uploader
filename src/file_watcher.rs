use std::path::{Path, PathBuf};
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Event, EventKind};
use std::sync::mpsc;
use std::thread;
use std::fs;

pub type FileCallback = Box<dyn Fn(PathBuf) + Send>;

pub struct FileWatcher {
    _watcher: RecommendedWatcher,
    _thread_handle: thread::JoinHandle<()>,
}

impl FileWatcher {
    pub fn new<P: AsRef<Path>>(path: P, tx: mpsc::Sender<PathBuf>) -> Result<Self, Box<dyn std::error::Error>> {
        let path = path.as_ref().to_path_buf();

        // Check if the path exists and is accessible
        if !path.exists() {
            return Err(format!("Watch path does not exist: {}", path.display()).into());
        }

        if !path.is_dir() {
            return Err(format!("Watch path is not a directory: {}", path.display()).into());
        }

        // Test write permissions
        let test_file = path.join(".watcher_test");
        match fs::write(&test_file, "test") {
            Ok(_) => {
                let _ = fs::remove_file(&test_file);
                println!("✓ Watch directory is writable: {}", path.display());
            }
            Err(e) => {
                eprintln!("⚠ Warning: Watch directory may not be writable: {} - {}", path.display(), e);
            }
        }

        let tx_clone = tx.clone();
        
        // Create the file system watcher with default config (uses FSEvents on macOS)
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        // Debug log all events
                        // println!("📁 File system event: {:?} for paths: {:?}", event.kind, event.paths);

                        // Handle different event types
                        match event.kind {
                            EventKind::Create(_) => {
                                for path in event.paths {
                                    // println!("🔍 Create event for: {}", path.display());
                                    // Check if it's a file and an image
                                    if path.is_file() && is_image_file(&path) {
                                        println!("✓ Image file detected: {}", path.display());
                                        let _ = tx_clone.send(path);
                                    } 
                                }
                            }
                            EventKind::Modify(_) => {
                                // Handle modify events for all image files
                                for path in event.paths {
                                    if path.is_file() && is_image_file(&path) {
                                        // println!("🔍 Modify event for image: {}", path.display());
                                        // Process all image files regardless of modification time
                                        // println!("✓ Image file detected for processing: {}", path.display());
                                        let _ = tx_clone.send(path);
                                    }
                                }
                            }
                            _ => {
                                // Ignore other events
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ Watch error: {:?}", e);
                        // Try to provide more helpful error messages
                        let error_str = e.to_string().to_lowercase();
                        if error_str.contains("permission") || error_str.contains("denied") {
                            eprintln!("💡 This might be a permissions issue. Try running with 'sudo' or check folder permissions.");
                        } else if error_str.contains("not found") {
                            eprintln!("💡 The watched folder might have been moved or deleted.");
                        }
                    }
                }
            },
            notify::Config::default(), // Use default config which will use FSEvents on macOS
        )?;

        // Start watching the directory
        println!("🔎 Starting to watch directory: {}", path.display());
        watcher.watch(&path, RecursiveMode::NonRecursive)?;
        println!("✓ Successfully started watching: {}", path.display());

        // We don't need a separate thread since the watcher callback now sends directly to the channel
        // But to keep the struct definition satisfied for now (or we can remove the thread handle from struct)
        // Let's spawn a dummy thread or better yet, remove the thread handle field from struct.
        // For minimal changes, let's keep the struct signatures similar but simplify logic.
        
        let thread_handle = thread::spawn(|| {});

        Ok(Self {
            _watcher: watcher,
            _thread_handle: thread_handle,
        })
    }
}

fn is_image_file(path: &Path) -> bool {
    if let Some(extension) = path.extension() {
        if let Some(ext_str) = extension.to_str() {
            let ext_lower = ext_str.to_lowercase();
            matches!(ext_lower.as_str(), "jpg" | "jpeg" | "png" | "nef")
        } else {
            false
        }
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_image_file() {
        assert!(is_image_file(Path::new("test.jpg")));
        assert!(is_image_file(Path::new("test.jpeg")));
        assert!(is_image_file(Path::new("test.png")));
        assert!(is_image_file(Path::new("test.heic")));
        assert!(is_image_file(Path::new("test.nef")));
        assert!(is_image_file(Path::new("TEST.JPG"))); // Test case insensitive
        assert!(!is_image_file(Path::new("test.txt")));
        assert!(!is_image_file(Path::new("test")));
        assert!(!is_image_file(Path::new("test.mp4")));
    }
}