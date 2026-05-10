## Context

`dupl` is a fast block-comparison duplicate finder targeting photo collections on NAS devices (primarily Synology). The current pipeline is purely batch: crawl → dedup → print to stdout. There is no interactive review step and no way to act on results without external tooling.

The target deployment is headless Linux (Synology DSM, Docker). A browser-based UI is the right fit: no native GUI toolkit needed, works across all client platforms, and matches how Synology users already manage their NAS.

## Goals / Non-Goals

**Goals:**
- JSON output mode for scan results (machine-readable, file-persisted)
- Embedded web server for reviewing duplicates and marking actions
- Polling action executor that applies marked actions under OS user permissions
- Privilege separation: web server never touches the file system data
- Single binary deployment; no runtime dependencies beyond the binary itself

**Non-Goals:**
- Real-time scan progress in the browser (scan completes before the web UI is opened)
- Multi-user authentication (single-user NAS tool; network access control is the operator's responsibility)
- Streaming / incremental scan results to the web UI
- Actions beyond delete and hardlink in the initial version (symlink, move deferred)

## Decisions

### D1: File-based IPC between serve and exec (not sockets or HTTP)

**Decision:** `dupl serve` writes `actions.json`; `dupl exec` polls it. No Unix socket, no shared memory, no HTTP callback.

**Rationale:** Keeps privilege separation absolute. If `dupl serve` spawned `dupl exec` as a subprocess, the child would inherit the parent's uid — defeating the minimal-permissions model. With file-based IPC, each process is launched independently by the OS scheduler with its own uid. The web server needs write access only to `actions.json`, never to the data volumes.

**Alternative considered:** Unix socket between serve and exec. Rejected: requires both processes to be co-located and coordinated at startup; adds complexity with no benefit for this use case.

### D2: Staleness guard in exec — inode + mtime, not rehash

**Decision:** Before executing any action, `dupl exec` checks that the target file's current inode and mtime match what was recorded at scan time. If either differs, the action is skipped and logged as stale.

**Rationale:** Consistent with the core design invariant (no hashing). A changed mtime or inode reliably signals that the file has been modified or replaced since the scan. Rehashing would be correct but expensive and inconsistent with the engine's design philosophy.

### D3: Embedded web assets via `rust-embed`

**Decision:** HTML, CSS, and JS for the web UI are compiled into the binary using `rust-embed`. No static file serving from the filesystem at runtime.

**Rationale:** Single binary deployment. No risk of asset path misconfiguration on a NAS. Works inside Docker with a minimal image (no `COPY` of asset directories).

**Alternative considered:** Serve assets from a well-known path (e.g. `/usr/share/dupl/`). Rejected: complicates Docker and Synology package installation; breaks if the binary is moved.

### D4: `dupl exec --watch` as a long-running scheduled task

**Decision:** `dupl exec` in watch mode polls `actions.json` at a configurable interval (default 10 s). It is registered as a separate Synology Task Scheduler entry running as the file-owning user, not triggered by the web server.

**Rationale:** Decouples action latency from web server permissions. The web server has no ability to spawn privileged processes. Poll interval of 10 s gives acceptable responsiveness for a cleanup tool without meaningful CPU overhead on a NAS.

### D5: JSON schema includes scan timestamp and per-file staleness fields

**Decision:** `dupl.json` records `scanned_at` (ISO 8601), and each file entry includes `inode` and `mtime_secs`. `actions.json` records `requested_at` per action.

**Rationale:** Enables the staleness guard (D2) without re-reading original scan data. Also lets the web UI display "last scanned X hours ago" to help the user judge result freshness.

### D6: Frontend stack — HTMX over a JS framework

**Decision:** Web UI uses vanilla HTML + HTMX for interactivity, served from the embedded binary. No build step, no npm, no bundler.

**Rationale:** The interaction model is simple: load groups, check boxes, click Apply. HTMX handles this with minimal JS. A framework (React, Svelte) would require a build pipeline that conflicts with the single-binary goal and adds maintenance surface. HTMX embeds as a single minified JS file.

## Risks / Trade-offs

**[Stale results UX]** A scan from days ago may list files that have since moved or been deleted. → Mitigation: display `scanned_at` prominently in the web UI; exec logs stale skips back to `dupl.json` so the UI can show them.

**[Poll latency]** 10 s poll means up to 10 s between "Apply" click and action execution. → Acceptable for a cleanup tool; configurable via `--interval` flag if needed.

**[No auth on web server]** Anyone with network access to the port can view results and queue actions. → Mitigation: default bind to `127.0.0.1` only; document that exposing to LAN requires operator-level access control (firewall, reverse proxy with auth).

**[HTMX bundle size]** HTMX minified is ~14 KB; acceptable embedded in a Rust binary.

## Open Questions

- **Hardlink action on Synology Btrfs volumes**: reflink (CoW dedup) may be preferable to hardlink for Btrfs. Defer to a follow-up capability; track as a known gap.
- **`actions.json` write safety**: concurrent writes from multiple browser tabs could corrupt the file. For v1, document single-user assumption. Later: atomic write via temp file + rename.
- **Synology package (.spk) vs Docker**: Docker is the initial deployment target. A native .spk would be needed for users without Container Manager. Defer.
