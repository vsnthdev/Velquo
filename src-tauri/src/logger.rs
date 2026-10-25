use chrono::Local;
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

pub struct AppLogger {
    log_file: Mutex<Option<File>>,
}

static LOGGER: OnceLock<AppLogger> = OnceLock::new();

impl AppLogger {
    pub fn init() -> PathBuf {
        let log_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("Library/Logs/Velquo");
        let _ = fs::create_dir_all(&log_dir);
        let log_file_path = log_dir.join("app.log");

        let opened_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file_path)
            .ok();

        let logger = LOGGER.get_or_init(|| AppLogger {
            log_file: Mutex::new(opened_file),
        });

        let _ = log::set_logger(logger);
        log::set_max_level(LevelFilter::Debug);
        log_file_path
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

        // Append to persistent open log file handle
        if let Ok(mut file_guard) = self.log_file.lock() {
            if let Some(file) = file_guard.as_mut() {
                let _ = writeln!(file, "[{}] {:5} [{}] {}", timestamp, level, target, message);
            }
        }
    }

    fn flush(&self) {}
}
