# Velquo — Performance & Bug Fix Backlog

## Performance Optimizations

- [ ] **Eliminate per-snapshot `logs` cloning and unused `velquo://log` IPC events (`src-tauri/src/lib.rs:28`, `src-tauri/src/logger.rs:113`)** — `get_app_snapshot` clones up to 400 `LogEntry` structs on every single UI state update even though `src/types.ts` (`AppSnapshot`) never reads `logs`, and `AppLogger::log` emits `"velquo://log"` over Tauri IPC on every log line with no frontend listener.
