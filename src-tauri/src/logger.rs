use chrono::Local;
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
const LOG_FILE_NAME: &str = "app.log";
const LOG_BACKUP_NAME: &str = "app.log.1";

struct LogFile {
    file: File,
    path: PathBuf,
    bytes_written: u64,
}

pub struct AppLogger {
    log_file: Mutex<Option<LogFile>>,
}

static LOGGER: OnceLock<AppLogger> = OnceLock::new();

fn open_log(path: &Path) -> Option<File> {
    OpenOptions::new().create(true).append(true).open(path).ok()
}

fn rotate_log(path: &Path) {
    let backup = path.with_file_name(LOG_BACKUP_NAME);
    let _ = fs::remove_file(&backup);
    if let Err(e) = fs::rename(path, &backup) {
        // Never recurse into the logger while handling a logger failure.
        eprintln!("Failed to rotate log file {:?}: {}", path, e);
    }
}

impl AppLogger {
    pub fn init() -> PathBuf {
        let log_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("Library/Logs/Velquo");
        let _ = fs::create_dir_all(&log_dir);
        let log_file_path = log_dir.join(LOG_FILE_NAME);

        if fs::metadata(&log_file_path)
            .map(|metadata| metadata.len() >= MAX_LOG_BYTES)
            .unwrap_or(false)
        {
            rotate_log(&log_file_path);
        }

        let bytes_written = fs::metadata(&log_file_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);

        let logger = LOGGER.get_or_init(|| AppLogger {
            log_file: Mutex::new(open_log(&log_file_path).map(|file| LogFile {
                file,
                path: log_file_path.clone(),
                bytes_written,
            })),
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
        let line = format!("[{}] {:5} [{}] {}", timestamp, level, target, message);

        // Print to terminal stdout for live CLI debugging
        println!("{}", line);

        let Ok(mut guard) = self.log_file.lock() else {
            return;
        };

        let needs_rotation = guard
            .as_ref()
            .map(|state| state.bytes_written + line.len() as u64 + 1 > MAX_LOG_BYTES)
            .unwrap_or(false);

        if needs_rotation {
            if let Some(mut state) = guard.take() {
                rotate_log(&state.path);
                state.file = match open_log(&state.path) {
                    Some(file) => file,
                    None => return,
                };
                state.bytes_written = 0;
                *guard = Some(state);
            }
        }

        if let Some(state) = guard.as_mut() {
            if writeln!(state.file, "{}", line).is_ok() {
                state.bytes_written += line.len() as u64 + 1;
            }
        }
    }

    fn flush(&self) {}
}
