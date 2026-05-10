<picture>
    <img alt="dupl wipe" src="dupl-logo.png" width="320" height="192">
</picture>

# dupl (wipe)

A fast, hash-free duplicate-file tool for photo collections and general file trees.

`dupl` finds identical files by comparing them block-by-block, stopping at the first
difference. No hashes are ever computed — this is faster than full-file hashing for
typical photo libraries where JPEG files diverge within the first 64 KiB due to differing
EXIF metadata.

## Getting Started

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (edition 2024, stable ≥ 1.85)

### Installing

Clone the repository and build a release binary:

```bash
git clone https://github.com/crjeder/dupl.git
cd dupl
cargo build --release
# binary is at target/release/dupl
```

Or install directly with Cargo:

```bash
cargo install --path .
```

## Usage

`dupl` has three subcommands that form a pipeline:

```
dupl scan  →  dupl.json  →  dupl serve  →  actions.json  →  dupl exec
```

### `dupl scan` — find duplicates

```bash
dupl scan ~/Photos
dupl scan ~/Photos -e jpg,png,heic          # filter by extension
dupl scan ~/Photos --min-size 1048576       # skip files < 1 MiB
dupl scan ~/Photos --json dupl.json         # write machine-readable output
```

Without `--json`, duplicate groups are printed to **stdout** as plain text (one group per
`# <bytes>` header). Progress and the summary go to **stderr**.

With `--json`, stdout is silent and all results are written atomically to the specified
file. This is the input for `dupl serve` and `dupl exec`.

### `dupl serve` — web UI for reviewing results

```bash
dupl serve --data dupl.json
dupl serve --data dupl.json --port 9090     # custom port
dupl serve --data dupl.json --bind 0.0.0.0 # expose to local network
```

Starts an embedded HTTP server (default: `http://127.0.0.1:8080`). Open the URL in a
browser to review duplicate groups, tick files you want to remove, and click
**Queue actions**. This writes `actions.json` next to `dupl.json`.

The server process never touches the file system directly — it only reads `dupl.json`
and writes `actions.json`. All file operations are performed by `dupl exec`.

### `dupl exec` — apply queued actions

```bash
dupl exec --actions actions.json            # single run
dupl exec --actions actions.json --watch    # poll every 10 s
dupl exec --actions actions.json --watch --interval 30
```

Reads `actions.json`, executes each pending action (delete or hardlink), records the
result, and rewrites the file atomically. Before touching any file it checks the stored
inode and mtime against the current file state; stale entries are skipped and logged.

`--watch` keeps the process running and re-checks the queue on each interval. This is
the mode to use as a scheduled or background task.

## Three-command Synology setup

This is the recommended workflow for a headless NAS like a Synology:

**Step 1 — scan** (run as a Task Scheduler job, e.g. nightly):
```bash
/opt/dupl/dupl scan /volume1/photo --json /opt/dupl/dupl.json -e jpg,jpeg,png,heic,mp4
```

**Step 2 — serve** (start once after the scan finishes, or keep running):
```bash
/opt/dupl/dupl serve --data /opt/dupl/dupl.json --bind 0.0.0.0 --port 8080
```
Open `http://<NAS-IP>:8080` from any browser on your local network, review the
duplicates, and click **Queue actions**. Then stop the server (Ctrl-C or kill).

**Step 3 — exec** (run as a Task Scheduler job after review, or with `--watch`):
```bash
/opt/dupl/dupl exec --actions /opt/dupl/actions.json
```

The three commands run under the same OS user that owns the photo files, so no
additional permissions are required.

> **Security note:** `--bind 0.0.0.0` exposes the UI to your entire local network with
> no authentication. Only use this on a trusted LAN (not exposed to the internet).
> See [Network access](#network-access) for details.

## Docker deployment

A single-binary Docker image is the cleanest way to run `dupl` on a NAS or any
Linux host without a Rust toolchain.

### Build the image

```bash
docker build -t dupl .
```

### Run

Mount a volume at `/data` for the JSON files and bind-mount your photo directory
read-only for scanning:

```bash
# Scan (writes /data/dupl.json)
docker run --rm \
  -v /path/to/photos:/photos:ro \
  -v /path/to/data:/data \
  dupl scan /photos --json /data/dupl.json -e jpg,png,heic

# Serve (exposes port 8080)
docker run --rm \
  -v /path/to/data:/data \
  -p 8080:8080 \
  dupl serve --data /data/dupl.json --bind 0.0.0.0

# Exec (writes back to /data/actions.json after review)
docker run --rm \
  -v /path/to/photos:/photos \
  -v /path/to/data:/data \
  dupl exec --actions /data/actions.json
```

> **Note:** `dupl exec` needs write access to the photo directory to delete files.
> Mount it without `:ro` for that step.

### Docker Compose example

```yaml
services:
  dupl-serve:
    image: dupl
    command: serve --data /data/dupl.json --bind 0.0.0.0
    volumes:
      - ./data:/data
    ports:
      - "8080:8080"
    restart: unless-stopped
```

Run scan and exec as one-shot containers (e.g. from a cron job or Synology Task
Scheduler) and keep only the `dupl-serve` service running persistently.

## Network access

By default `dupl serve` binds to `127.0.0.1` (loopback only). This is safe for
local development — the UI is only reachable from the same machine.

To reach the UI from another device on your LAN:

```bash
dupl serve --data dupl.json --bind 0.0.0.0
```

`0.0.0.0` binds on all interfaces, including your LAN IP. **Do not expose this port
to the internet** — there is no authentication. Options for safe LAN exposure:

- Put the NAS behind a firewall that blocks external access to port 8080.
- Use a VPN (WireGuard, Tailscale) to reach the NAS remotely instead of opening a port.
- Bind to a specific interface IP (e.g. `--bind 192.168.1.10`) instead of `0.0.0.0`.

## Running the Tests

```bash
cargo test
```

Lint:

```bash
cargo clippy
```

## How It Works

The pipeline has three stages:

```
crawl.rs  →  dupl.rs  →  main.rs (output)
```

1. **`crawl.rs`** — walks directories with `walkdir`, groups files by exact byte size.
   Size-singletons are dropped immediately. Symlinks are never followed. Input roots are
   normalised before the walk: if one root is reachable as a descendant of another (or via
   a bind mount), the redundant root is removed to prevent double-counting.

2. **`dupl.rs`** — processes one size group at a time using recursive block-splitting.
   All file handles in a group are opened once and kept open across recursion. Each
   recursion reads the next 64 KiB block, re-groups files by content, eliminates
   singletons, and recurses. A group that reaches EOF without diverging is a set of true
   duplicates. Files sharing the same `(device, inode)` pair are treated as a single
   logical file (`AliasGroup`) — hard links are never reported as duplicates of each other.

3. **`main.rs`** — CLI via `clap` derive. Routes to `scan`, `serve`, or `exec`.
   `src/json.rs` handles serialisation. `src/exec.rs` implements the action executor.
   `src/web/` implements the embedded HTTP server and UI.

### Design decisions

- **No stored hashes.** Pre-computed hashes are invalidated by copy/move/rename, which is
  exactly how photo duplicates are created. Attaching a cache to the file (e.g. extended
  attributes) or avoiding caching entirely are the only correct strategies.
- **Exact byte equality only.** This tool does not perform perceptual or content-aware
  image comparison.
- **O(n) worst-case disk reads**, sub-linear in practice for photo collections.
- **Privilege separation.** `dupl serve` never accesses photo files. `dupl exec` runs as
  the file-owning OS user and is the only component that modifies the filesystem.
- **File-based IPC.** `dupl.json` and `actions.json` are the only communication channels
  between the three commands. They can run on different machines, at different times, and
  under different users (as long as exec has write permission on the target files).

## Built With

- [Rust](https://www.rust-lang.org/) — systems language
- [walkdir](https://crates.io/crates/walkdir) — recursive directory traversal
- [clap](https://crates.io/crates/clap) — command-line argument parsing
- [axum](https://crates.io/crates/axum) — async HTTP server
- [tokio](https://crates.io/crates/tokio) — async runtime
- [rust-embed](https://crates.io/crates/rust-embed) — compile-time asset embedding
- [serde](https://crates.io/crates/serde) / [serde_json](https://crates.io/crates/serde_json) — JSON serialisation

## Versioning

This project uses [Semantic Versioning](http://semver.org/). For available versions, see
the [CHANGELOG](CHANGELOG.md).

## License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Inspired by the analysis in
  [Gedanken zur Datei-dupllizierung](https://gist.github.com/crjeder/6b9b198562379370887887edcdc746d1) —
  a detailed critique of hash-based dupllication and the case for direct block comparison
- [fclones](https://github.com/pkolaczk/fclones),
  [rdfind](https://rdfind.pauldreik.se/), and
  [dupe-krill](https://github.com/kornelski/dupe-krill) — prior art examined during design
- [PurpleBooth/a-good-readme-template](https://github.com/PurpleBooth/a-good-readme-template)
