# Velquo — Performance & Bug Fix Backlog

## Performance Optimizations

- [ ] **Eliminate per-snapshot `logs` cloning and unused `velquo://log` IPC events (`src-tauri/src/lib.rs:28`, `src-tauri/src/logger.rs:113`)** — `get_app_snapshot` clones up to 400 `LogEntry` structs on every single UI state update even though `src/types.ts` (`AppSnapshot`) never reads `logs`, and `AppLogger::log` emits `"velquo://log"` over Tauri IPC on every log line with no frontend listener.
- [ ] **Keep a persistent buffered log file handle instead of re-opening `app.log` on every log line (`src-tauri/src/logger.rs:98`)** — `AppLogger::log` calls `OpenOptions::new().create(true).append(true).open(&self.log_file_path)` on every log statement (`open` + `write` + `close` syscalls per line). Holding a `Mutex<BufWriter<File>>` avoids repeated filesystem open/close overhead.
- [ ] **Batch `upsert_transfer` UI notifications when starting multi-file uploads (`src-tauri/src/kdeconnect/daemon.rs:4653`)** — `send_files_to_device` calls `self.upsert_transfer(...)` inside a loop for each file in a batch, which fires `self.notify_ui()` $N$ times back-to-back instead of inserting all $N$ items under one lock and calling `notify_ui()` once.
