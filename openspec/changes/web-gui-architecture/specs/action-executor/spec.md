## ADDED Requirements

### Requirement: dupl exec subcommand
The tool SHALL provide an `exec` subcommand that reads `actions.json`, validates and executes each pending action, and then clears the pending list. It accepts `--actions <path>` (path to `actions.json`, required) and `--watch` (enable polling mode). In watch mode it also accepts `--interval <duration>` (default `10s`).

#### Scenario: Single-run mode processes and clears queue
- **WHEN** `dupl exec --actions /data/actions.json` is run and `actions.json` contains pending actions
- **THEN** all valid pending actions are executed and the `pending` array in `actions.json` is set to empty

#### Scenario: Watch mode polls at configured interval
- **WHEN** `dupl exec --actions /data/actions.json --watch --interval 10s` is run
- **THEN** the process remains alive and checks `actions.json` every 10 seconds, processing any new entries

#### Scenario: Watch mode with empty queue does nothing
- **WHEN** the executor polls and `actions.json` has an empty `pending` array
- **THEN** no file operations are performed and the process continues polling

### Requirement: Staleness check before action
Before executing any action, `dupl exec` SHALL stat the target file and compare its current inode and mtime against the values recorded in `actions.json`. If either differs, the action MUST be skipped and logged as stale.

#### Scenario: Stale file skipped
- **WHEN** an action targets a file whose mtime has changed since the action was queued
- **THEN** the action is skipped, a warning is written to stderr, and the completed log records `"status": "stale"`

#### Scenario: Valid file executed
- **WHEN** an action targets a file whose inode and mtime match the recorded values
- **THEN** the action is executed and the completed log records `"status": "ok"`

### Requirement: Action result log
After processing, `dupl exec` SHALL write (or append) results to a `completed` array in `actions.json`:

```json
{
  "pending": [],
  "completed": [
    {
      "action": "delete" | "hardlink",
      "path": "<absolute path>",
      "status": "ok" | "stale" | "error",
      "error": "<message or null>",
      "executed_at": "<ISO 8601 UTC timestamp>"
    },
    ...
  ]
}
```

The write MUST be atomic.

#### Scenario: Completed entries accumulated across runs
- **WHEN** `dupl exec` runs multiple times
- **THEN** each run appends its results to `completed` without overwriting previous entries

### Requirement: OS permission enforcement
`dupl exec` MUST NOT implement its own permission model. It SHALL attempt each file operation directly and propagate OS-level permission errors as `"status": "error"` entries in the log. The operator is responsible for running `dupl exec` as a user with appropriate file permissions.

#### Scenario: Permission denied logged as error
- **WHEN** `dupl exec` attempts to delete a file it does not have permission to remove
- **THEN** the OS error is caught, the action is logged with `"status": "error"` and the error message, and the process continues with remaining actions

### Requirement: No dependency on dupl serve
`dupl exec` MUST be runnable independently of whether `dupl serve` is running. It reads `actions.json` from disk; it does not communicate with the web server process in any way.

#### Scenario: Exec runs without serve
- **WHEN** `dupl exec --actions /data/actions.json` is run while `dupl serve` is not running
- **THEN** pending actions are processed normally
