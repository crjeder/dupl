## Why

`dupl` currently produces plain-text output consumed by a human at a terminal, with no way to review and act on results interactively — especially from a NAS or headless server where a native GUI is impractical. Adding a web-based review and action workflow makes the tool usable on Synology and similar devices via a browser, without requiring any installed GUI toolkit.

## What Changes

- Add a `--json` output flag to `dupl scan` (or the root command) that emits duplicate groups as structured JSON to a file instead of plain text to stdout.
- Add a `dupl serve` subcommand: a minimal embedded HTTP server that reads the JSON results file and presents a browser UI for reviewing groups and marking actions.
- Add a `dupl exec` subcommand: a privileged action executor that polls an `actions.json` queue file and applies marked actions (delete, hardlink) with OS-enforced permission checks.
- Define the JSON schema for `dupl.json` (scan results) and `actions.json` (pending action queue).

## Capabilities

### New Capabilities

- `json-scan-output`: Structured JSON format for scan results written to a file; the contract between the scan engine and the web server.
- `web-server`: The `dupl serve` subcommand — embedded HTTP server with browser UI for reviewing duplicates, marking actions, and triggering the executor.
- `action-executor`: The `dupl exec` subcommand — polls `actions.json`, validates file staleness (inode + mtime), executes actions as the file-owning OS user, logs results.

### Modified Capabilities

- `output-format`: Adds a JSON output mode alongside the existing plain-text stdout format. The existing plain-text behaviour is unchanged; JSON is opt-in via flag.
- `cli-interface`: Adds `serve` and `exec` as top-level subcommands alongside the existing scanning behaviour.

## Impact

- **New dependencies**: An embedded HTTP server crate (e.g. `axum`); static assets embedded via `rust-embed` or `include_bytes!`. No runtime file-system dependency for the web UI.
- **Privilege model**: `dupl serve` is designed to run as a low-privilege user with access only to the two JSON files. `dupl exec` runs as the file-owning user; it is never spawned by `dupl serve`. Communication is file-based only (`actions.json`).
- **Deployment target**: Single binary. On Synology: three Task Scheduler entries — nightly scan, on-boot serve, on-boot exec (watch mode). Docker is the primary packaging path.
- **No breaking changes** to existing CLI or stdout output format.
