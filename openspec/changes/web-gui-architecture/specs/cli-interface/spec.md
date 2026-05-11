## ADDED Requirements

### Requirement: serve subcommand
The CLI SHALL accept `serve` as a subcommand. It MUST accept `--data <path>` (required, path to `dupl.json`), `--port <n>` (optional, default 8080), and `--bind <addr>` (optional, default `127.0.0.1`).

#### Scenario: serve --data is required
- **WHEN** `dupl serve` is run without `--data`
- **THEN** the process exits with a non-zero code and prints a usage error to stderr

#### Scenario: serve --port overrides default
- **WHEN** `dupl serve --data /data/dupl.json --port 9000` is run
- **THEN** the server binds on port 9000

### Requirement: exec subcommand
The CLI SHALL accept `exec` as a subcommand. It MUST accept `--actions <path>` (required, path to `actions.json`), `--watch` (optional flag, enables polling mode), and `--interval <duration>` (optional, default `10s`, only meaningful with `--watch`).

#### Scenario: exec --actions is required
- **WHEN** `dupl exec` is run without `--actions`
- **THEN** the process exits with a non-zero code and prints a usage error to stderr

#### Scenario: exec without --watch exits after one pass
- **WHEN** `dupl exec --actions /data/actions.json` is run without `--watch`
- **THEN** the process processes the queue once and exits with code 0

### Requirement: --json flag on scan command
The existing scan behaviour (positional directory arguments) SHALL accept an additional `--json <path>` flag specifying the output file for structured JSON results.

#### Scenario: --json accepted alongside existing flags
- **WHEN** `dupl scan /volume1 -e jpg,png --json /data/dupl.json` is run
- **THEN** the command is accepted and results are written to `/data/dupl.json`
