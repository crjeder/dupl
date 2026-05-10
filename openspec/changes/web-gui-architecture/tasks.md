## 1. JSON Output Mode

- [x] 1.1 Add `serde` and `serde_json` dependencies to `Cargo.toml`
- [x] 1.2 Define `ScanResult`, `Group`, and `FileEntry` structs with `Serialize` derives
- [x] 1.3 Add `--json <path>` flag to the CLI (clap derive on main command or `scan` subcommand)
- [x] 1.4 Implement `write_json(path, result)` using atomic write (temp file + rename in same directory)
- [x] 1.5 Wire `--json` into the pipeline: collect output from `dedup.rs` into `ScanResult`, write via `write_json`
- [x] 1.6 Verify stdout is silent when `--json` is supplied; stderr output unchanged

## 2. CLI Subcommand Restructure

- [x] 2.1 Refactor `main.rs` clap definitions: introduce `Commands` enum with `Scan`, `Serve`, `Exec` variants
- [x] 2.2 Move existing scan flags (positional paths, `-e`, `--min-size`, `--json`) under the `Scan` subcommand
- [x] 2.3 Add `Serve` subcommand with `--data`, `--port` (default 8080), `--bind` (default `127.0.0.1`)
- [x] 2.4 Add `Exec` subcommand with `--actions`, `--watch`, `--interval` (default `10s`)
- [x] 2.5 Ensure backward compatibility: check if existing plain invocation still works or document the migration

## 3. Action Executor (`dupl exec`)

- [x] 3.1 Define `ActionsFile`, `PendingAction`, and `CompletedAction` structs with `Serialize`/`Deserialize`
- [x] 3.2 Implement `read_actions(path)` — reads and deserialises `actions.json`; tolerates missing file (empty queue)
- [x] 3.3 Implement staleness check: stat target file, compare inode and mtime against action record
- [x] 3.4 Implement `execute_action(action)`: delete or hardlink with OS error propagation
- [x] 3.5 Implement `write_actions(path, updated)` using atomic write
- [x] 3.6 Implement single-run mode: read → validate → execute → write results → exit
- [x] 3.7 Implement watch mode (`--watch`): loop with `thread::sleep(interval)` between passes
- [x] 3.8 Wire `Exec` subcommand into `main.rs`

## 4. Web Server (`dupl serve`)

- [x] 4.1 Add `axum`, `tokio` (async runtime), and `rust-embed` dependencies to `Cargo.toml`
- [x] 4.2 Create `src/web/` module; add embedded asset struct for HTML/CSS/JS
- [x] 4.3 Build minimal web UI: group listing page with scan timestamp, file paths, checkboxes, Apply button
- [x] 4.4 Implement `GET /api/groups` — reads and returns `dupl.json` as JSON response
- [x] 4.5 Implement `POST /api/actions` — validates request body, writes `actions.json` atomically
- [x] 4.6 Enforce "cannot mark all files in group" constraint server-side in `POST /api/actions`
- [x] 4.7 Implement static asset routes for embedded HTML/CSS/JS
- [x] 4.8 Wire `Serve` subcommand into `main.rs`; bind to configured address and log URL to stderr

## 5. Integration & Validation

- [ ] 5.1 End-to-end test: scan → write JSON → serve reads JSON → post actions → exec processes actions
- [ ] 5.2 Verify atomic write behaviour: no partial `dupl.json` visible under concurrent read
- [ ] 5.3 Verify staleness check: modify a file after scan, confirm exec skips it and logs `stale`
- [ ] 5.4 Verify exec runs independently without serve process running
- [ ] 5.5 Build Docker image: single binary, bind-mount `/data` for JSON files, expose port 8080

## 6. Documentation

- [x] 6.1 Update README with three-command Synology setup (`scan`, `serve`, `exec --watch`)
- [x] 6.2 Document Docker deployment and volume mount pattern
- [x] 6.3 Document `127.0.0.1` default bind and how to expose to LAN safely
