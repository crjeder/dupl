## 1. AliasGroup Data Type

- [x] 1.1 Define `AliasGroup` struct in `crawl.rs` with `paths: Vec<PathBuf>`
- [x] 1.2 Add platform-conditional `FileId` type: `(u64, u64)` for `(device, inode)` on Unix and Windows
- [x] 1.3 Implement helper to extract `FileId` from `std::fs::Metadata` on Unix (`dev` + `ino`) with `#[cfg(unix)]`
- [x] 1.4 Implement helper to extract `FileId` from `std::fs::Metadata` on Windows (`volume_serial_number` + `file_index`) returning `Option<FileId>` with `#[cfg(windows)]` — returns `None` (conservative) because `windows_by_handle` is unstable (rust#63010)

## 2. Input Normalization

- [x] 2.1 Implement `normalize_inputs(dirs: &[PathBuf]) -> Vec<PathBuf>` in `main.rs` (or a new `normalize.rs`)
- [x] 2.2 Stat each input dir; collect `(FileId, PathBuf)` pairs; drop exact-same-inode duplicates (keep first)
- [x] 2.3 For each remaining input dir, walk its subdirectories (directories only, no files) and collect all descendant `FileId` values
- [x] 2.4 Remove any input dir whose `FileId` appears in another input dir's descendant set
- [x] 2.5 Emit a `stderr` warning and keep the entry when a directory cannot be stat'd during normalization
- [x] 2.6 Skip inode comparison and keep the entry when `FileId` extraction returns `None` (conservative fallback)

## 3. Crawl with Inode Grouping

- [x] 3.1 Change `crawl` return type from `HashMap<u64, Vec<PathBuf>>` to `HashMap<u64, Vec<AliasGroup>>`
- [x] 3.2 Maintain a `HashMap<FileId, usize>` during the walk mapping `FileId` to the index of its `AliasGroup` in the current size bucket
- [x] 3.3 When a file's `FileId` is already in the map, append its path to the existing `AliasGroup` instead of creating a new one
- [x] 3.4 When a file's `FileId` is new (or unavailable), create a new `AliasGroup` with that path as the sole entry
- [x] 3.5 Update singleton pruning to count `AliasGroup` values (not paths): discard size buckets with fewer than two groups

## 4. Deduplication Pipeline Update

- [x] 4.1 Change `find_duplicates` signature to accept `Vec<AliasGroup>` and return `Vec<Vec<AliasGroup>>`
- [x] 4.2 Update `FileReader` struct: replace `path: PathBuf` with `group: AliasGroup`
- [x] 4.3 Open `File::open(&group.paths[0])` as the representative handle; store the full group for output
- [x] 4.4 Update `split_by_block` to propagate `AliasGroup` through grouping and recursion
- [x] 4.5 Ensure EOF-matched groups return `Vec<AliasGroup>` (not `Vec<PathBuf>`)

## 5. Output and Main Integration

- [x] 5.1 Call `normalize_inputs` in `main.rs` before calling `crawl`
- [x] 5.2 Update the output loop to iterate over `Vec<Vec<AliasGroup>>`
- [x] 5.3 For each group, print the first path of each `AliasGroup` as a plain path line
- [x] 5.4 For each additional path in an `AliasGroup` (aliases), print a `link: <path>` line immediately after the representative
- [x] 5.5 Update the `stderr` summary stat if needed (count of duplicate groups, not individual dupe paths)

## 6. Tests

- [x] 6.1 Unit test: `normalize_inputs` drops a subdirectory when its parent is also an input
- [x] 6.2 Unit test: `normalize_inputs` deduplicates identical input paths
- [x] 6.3 Unit test: `crawl` collapses hard-linked paths into one `AliasGroup` (use `std::fs::hard_link` in a temp dir)
- [x] 6.4 Integration test: two hard-linked files are not reported as content duplicates of each other
- [x] 6.5 Integration test: a hard-linked file and an independent copy with identical content are reported as duplicates, with the `link:` alias visible in output
- [x] 6.6 Run `cargo clippy` and `cargo test` with no warnings or failures
