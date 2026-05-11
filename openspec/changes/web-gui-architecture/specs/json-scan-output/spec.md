## ADDED Requirements

### Requirement: JSON scan results file
When invoked with `--json <path>`, the tool SHALL write scan results to the specified file as UTF-8 encoded JSON instead of printing plain text to stdout. The file MUST be written atomically (write to a temp file in the same directory, then rename) to prevent partial reads by `dupl serve`.

#### Scenario: JSON file written on successful scan
- **WHEN** `dupl scan /volume1 --json /data/dupl.json` is run and duplicates are found
- **THEN** `/data/dupl.json` exists and contains valid JSON with at least one group

#### Scenario: Atomic write prevents partial reads
- **WHEN** the scan completes and the JSON file is written
- **THEN** the file appears at the target path only after all content is flushed (no partial file visible to concurrent readers)

### Requirement: JSON schema — top-level structure
The JSON output file SHALL conform to the following top-level structure:

```json
{
  "scanned_at": "<ISO 8601 UTC timestamp>",
  "paths_scanned": ["<absolute path>", ...],
  "total_files_examined": <integer>,
  "groups": [ <group object>, ... ]
}
```

#### Scenario: Metadata fields present
- **WHEN** the JSON file is written
- **THEN** `scanned_at`, `paths_scanned`, `total_files_examined`, and `groups` are all present at the top level

### Requirement: JSON schema — group object
Each entry in `groups` SHALL be an object of the form:

```json
{
  "size_bytes": <integer>,
  "files": [ <file object>, ... ]
}
```

where `files` contains two or more entries (singleton groups are never emitted).

#### Scenario: Group has minimum two files
- **WHEN** a group is written to JSON
- **THEN** its `files` array contains at least two entries

### Requirement: JSON schema — file object
Each file entry within a group SHALL be an object of the form:

```json
{
  "path": "<absolute path>",
  "inode": <integer>,
  "mtime_secs": <integer>
}
```

The `inode` and `mtime_secs` fields MUST reflect the values at scan time and are used by `dupl exec` for staleness detection.

#### Scenario: File object staleness fields present
- **WHEN** a file entry is written to JSON
- **THEN** `path`, `inode`, and `mtime_secs` are all present and non-null

### Requirement: Plain-text output unchanged when --json not specified
If `--json` is not provided, the tool MUST behave exactly as before: plain-text groups to stdout, progress to stderr. The JSON flag MUST NOT affect the existing output contract.

#### Scenario: Default mode unaffected
- **WHEN** `dupl scan /volume1` is run without `--json`
- **THEN** stdout contains plain-text duplicate groups and no JSON is written
