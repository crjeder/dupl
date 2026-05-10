## ADDED Requirements

### Requirement: dupl serve subcommand
The tool SHALL provide a `serve` subcommand that starts an embedded HTTP server for reviewing scan results in a browser. It accepts `--data <path>` (path to `dupl.json`) and `--port <n>` (default 8080). It SHALL bind to `127.0.0.1` by default and accept `--bind <addr>` to override.

#### Scenario: Server starts and listens
- **WHEN** `dupl serve --data /data/dupl.json --port 8080` is run
- **THEN** the process binds to `127.0.0.1:8080` and logs the URL to stderr

#### Scenario: Default bind address is loopback
- **WHEN** `dupl serve --data /data/dupl.json` is run without `--bind`
- **THEN** the server binds to `127.0.0.1` only, not `0.0.0.0`

### Requirement: Web assets embedded in binary
All HTML, CSS, and JavaScript assets for the web UI SHALL be compiled into the binary at build time. The server MUST NOT require any files on the host filesystem beyond the `dupl.json` and `actions.json` data files.

#### Scenario: Web UI accessible without external assets
- **WHEN** the binary is copied to a new machine with no asset files present
- **THEN** the web UI loads correctly in the browser

### Requirement: Duplicate group listing
The web UI SHALL display all duplicate groups from `dupl.json`, showing for each group the file size and the list of file paths. The UI MUST display the `scanned_at` timestamp so the user can judge result freshness.

#### Scenario: Groups displayed with scan age
- **WHEN** the user opens the web UI
- **THEN** all groups from `dupl.json` are listed and the scan timestamp is visible

### Requirement: Action marking
The user SHALL be able to mark one or more files within a group for deletion or hardlinking. At least one file in each group MUST remain unmarked (the tool MUST enforce that you cannot mark all copies for deletion). Marked actions are held in browser state until the user clicks Apply.

#### Scenario: Cannot mark all files in a group
- **WHEN** the user attempts to mark every file in a group for deletion
- **THEN** the UI prevents the last file from being marked and shows an explanatory message

#### Scenario: Apply queues actions
- **WHEN** the user clicks Apply with one or more files marked
- **THEN** the web server writes those actions to `actions.json` and the UI shows a "pending" indicator

### Requirement: actions.json schema
When the user applies actions, `dupl serve` SHALL write (or append to) `actions.json` with the following structure:

```json
{
  "pending": [
    {
      "action": "delete" | "hardlink",
      "path": "<absolute path>",
      "inode": <integer>,
      "mtime_secs": <integer>,
      "requested_at": "<ISO 8601 UTC timestamp>"
    },
    ...
  ]
}
```

The write MUST be atomic (temp file + rename).

#### Scenario: actions.json written atomically on Apply
- **WHEN** the user clicks Apply
- **THEN** `actions.json` appears at the target path only after the full write completes

### Requirement: No direct file system access
`dupl serve` MUST NOT read, write, delete, or modify any file other than `dupl.json` (read-only) and `actions.json` (write). It MUST NOT traverse or stat the paths listed in `dupl.json`.

#### Scenario: Server has no access to scanned paths
- **WHEN** `dupl serve` is running
- **THEN** it issues no filesystem calls to paths outside its two data files
