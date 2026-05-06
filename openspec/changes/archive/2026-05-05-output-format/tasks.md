## 1. Output Loop

- [x] 1.1 Remove the `enumerate` index and `if i == 0` branch from the output loop in `main.rs`
- [x] 1.2 Print each AliasGroup representative as `  {path}` with two leading spaces and no label
- [x] 1.3 Confirm `link:` alias lines are still printed immediately after their representative (no change needed if already correct)

## 2. Summary Line

- [x] 2.1 Remove `total_dupes` counter and its per-dupe increment
- [x] 2.2 Update `eprintln!` summary to `"{N} duplicate group(s) found, {X:.1} MB of duplicated content"`
- [x] 2.3 Verify `wasted_bytes` calculation is still `(content_group.len() as u64 - 1) * size` (unchanged from inode-aliasing)

## 3. Tests and Verification

- [x] 3.1 Run `cargo build` with no errors
- [x] 3.2 Run `cargo test` with no failures
- [x] 3.3 Run `cargo clippy -- -D warnings` with no warnings
- [x] 3.4 Manually verify stdout contains no `keep` or `dupe` tokens by running against a small test directory
