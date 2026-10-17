use chrono::Local;
use log::{Level, LevelFilter, Log, Metadata, Record};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

pub struct AppLogger {
    entries: Arc<Mutex<VecDeque<LogEntry>>>,
    app_handle: Arc<Mutex<Option<AppHandle>>>,
    log_file_path: PathBuf,
}

static LOGGER: OnceLock<AppLogger> = OnceLock::new();

impl AppLogger {
    pub fn init() -> PathBuf {
        let log_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("Library/Logs/Velquo");
        let _ = fs::create_dir_all(&log_dir);
        let log_file_path = log_dir.join("app.log");

        let logger = LOGGER.get_or_init(|| AppLogger {
            entries: Arc::new(Mutex::new(VecDeque::with_capacity(500))),
            app_handle: Arc::new(Mutex::new(None)),
            log_file_path: log_file_path.clone(),
        });

        let _ = log::set_logger(logger);
        log::set_max_level(LevelFilter::Debug);
        log_file_path
    }

    pub fn set_app_handle(handle: AppHandle) {
        if let Some(logger) = LOGGER.get() {
            if let Ok(mut guard) = logger.app_handle.lock() {
                *guard = Some(handle);
            }
        }
    }

    pub fn get_entries() -> Vec<LogEntry> {
        if let Some(logger) = LOGGER.get() {
            if let Ok(guard) = logger.entries.lock() {
                return guard.iter().cloned().collect();
            }
        }
        Vec::new()
    }

    pub fn clear_entries() {
        if let Some(logger) = LOGGER.get() {
            if let Ok(mut guard) = logger.entries.lock() {
                guard.clear();
            }
        }
    }
}

impl Log for AppLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        let target = metadata.target();
        if target.starts_with("velquo") || target.starts_with("kdeconnect") {
            metadata.level() <= Level::Debug
        } else if target.starts_with("mdns_sd") || target.starts_with("rustls") {
            metadata.level() <= Level::Info
        } else {
            metadata.level() <= Level::Warn
        }
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let timestamp = Local::now().format("%H:%M:%S%.3f").to_string();
        let level = record.level().to_string();
        let target = record.target().to_string();
        let message = format!("{}", record.args());

        // Print to terminal stdout for live CLI debugging
        println!("[{}] {:5} [{}] {}", timestamp, level, target, message);

        // Append to persistent log file
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file_path)
        {
            let _ = writeln!(file, "[{}] {:5} [{}] {}", timestamp, level, target, message);
        }

        let entry = LogEntry {
            timestamp,
            level,
            target,
            message,
        };

        if let Ok(mut guard) = self.entries.lock() {
            if guard.len() >= 400 {
                guard.pop_front();
            }
            guard.push_back(entry.clone());
        }

        if let Ok(guard) = self.app_handle.lock() {
            if let Some(app) = guard.as_ref() {
                let _ = app.emit("velquo://log", entry);
            }
        }
    }

    fn flush(&self) {}
}
