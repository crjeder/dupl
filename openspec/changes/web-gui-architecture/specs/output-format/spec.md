## ADDED Requirements

### Requirement: JSON output mode via --json flag
When `--json <path>` is supplied, the tool SHALL write structured JSON to the specified file and MUST NOT write any duplicate group content to stdout. Progress and summary output on stderr MUST continue unchanged regardless of the output mode.

#### Scenario: stdout silent in JSON mode
- **WHEN** `dupl scan /volume1 --json /data/dupl.json` is run
- **THEN** stdout is empty and all group data appears only in the JSON file

#### Scenario: stderr unaffected by JSON mode
- **WHEN** `dupl scan /volume1 --json /data/dupl.json` is run
- **THEN** stderr still contains the progress and summary lines as specified in the existing output-format spec
