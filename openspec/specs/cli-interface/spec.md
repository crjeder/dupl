# Spec: CLI Interface

## Overview

`dedup` is invoked as a command-line tool. This spec defines the accepted arguments,
their semantics, and how invalid input is handled.

## Requirements

### Positional Arguments

- The tool MUST accept one or more directory paths as positional arguments.
- At least one directory path MUST be provided; the tool MUST refuse to run without one.

### Extension Filter (`-e` / `--extensions`)

- The tool MUST support an `-e` / `--extensions` flag accepting a comma-separated list
  of file extensions (e.g. `jpg,png,heic`).
- Extensions MUST be normalized to lowercase before matching.
- A leading dot on any extension value MUST be stripped (`.jpg` and `jpg` are equivalent).
- If the flag is omitted, all file extensions MUST be accepted.

### Minimum Size (`--min-size`)

- The tool MUST support a `--min-size` flag accepting an integer number of bytes.
- The default value MUST be `1`, meaning zero-byte files are excluded by default.
- Files strictly smaller than `min-size` bytes MUST be excluded from scanning.

### Error Handling

- An unreadable or non-existent directory path MUST produce a warning on `stderr`.
- Such errors MUST NOT terminate the process; remaining paths MUST still be processed.
