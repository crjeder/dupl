## Why

Multiple filesystem mechanisms — hard links, overlapping input directories, and bind mounts — allow different paths to refer to the same physical file. The tool currently treats each path as an independent file, causing identical-inode paths to be falsely reported as content duplicates and potentially prompting users to delete data they intended to keep.

## What Changes

- **New**: Pre-crawl input normalization removes redundant root directories by walking directory inodes — if one input directory's inode appears as a descendant of another, it is dropped before scanning begins.
- **New**: During the crawl, files sharing the same `(device, inode)` pair are grouped into an `AliasGroup` — a single logical file with multiple path aliases.
- **New**: The deduplication pipeline operates on `AliasGroup` values instead of bare `PathBuf` values; only one file handle is opened per group.
- **New**: Output includes a `link:` tag for alias paths within a group, so callers can distinguish hard-linked aliases from independent content duplicates.
- **New**: Conservative fallback on Windows when `file_index()` or `volume_serial_number()` is unavailable — inode deduplication is skipped for that file rather than failing.

## Capabilities

### New Capabilities

- `input-normalization`: Pre-crawl deduplication of input root directories based on directory inode comparison; removes roots whose inode is reachable as a descendant of another root.
- `inode-alias-grouping`: During the crawl, files with the same `(device, inode)` pair are collected into an `AliasGroup`; the group is treated as one logical file throughout the pipeline.

### Modified Capabilities

- `file-discovery`: Size-grouping now operates on `AliasGroup` values rather than bare paths; the singleton-pruning rule applies to the number of distinct inodes, not paths.

## Impact

- `crawl.rs`: New `AliasGroup` struct; inode tracking via `HashSet<(u64, u64)>`; new `normalize_inputs` function; return type changes from `HashMap<u64, Vec<PathBuf>>` to `HashMap<u64, Vec<AliasGroup>>`.
- `dedup.rs`: `find_duplicates` and `split_by_block` operate on `AliasGroup` instead of `PathBuf`; one `File::open` per group using a representative path.
- `main.rs`: `normalize_inputs` called before `crawl`; output loop emits `link:` lines for alias paths.
- No new crate dependencies; platform-conditional code via `#[cfg(unix)]` / `#[cfg(windows)]`.
