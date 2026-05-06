# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-05-05

### Added

- Block-by-block duplicate detection with no hashing. Files are compared
  directly in 64 KiB blocks; comparison stops at the first differing block.
  This is O(n) disk reads worst-case and sub-linear in practice for photo
  collections, where EXIF metadata typically diverges within the first block.

- Directory crawling via `walkdir`. Symbolic links are never followed.
  Files are grouped by exact byte size; size-singletons are discarded before
  any I/O comparison begins.

- Extension filter (`-e / --extensions`): restrict scanning to a
  comma-separated list of file extensions, matched case-insensitively.

- Minimum size filter (`--min-size`): skip files smaller than N bytes
  (default: 1 byte, i.e. empty files are excluded).

- Inode-aware alias grouping. Files that share the same `(device, inode)`
  pair — hard links, bind-mount duplicates — are treated as one logical file
  (`AliasGroup`) with multiple path aliases. Only one file handle is opened
  per physical file during comparison. Hard-linked paths are never reported
  as content duplicates of each other.

- Pre-crawl input normalization. Before scanning begins, redundant input
  roots are eliminated: if one root's inode is reachable as a descendant of
  another root, it is dropped. This prevents double-counting when the user
  passes `/photos` and `/photos/2024` as separate arguments, or when bind
  mounts cause the same tree to appear under two paths.

- Machine-readable output on `stdout`, human-readable progress and warnings
  on `stderr`. Duplicate groups are printed one per `# <bytes>` header.
  Hard-linked aliases within a group are prefixed with `link:`.

- Summary line on `stderr` after scanning: number of duplicate groups found
  and total wasted space in MiB (calculated as one removable physical file
  per extra `AliasGroup` in a content-duplicate group).

[Unreleased]: https://github.com/example/dedup/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/example/dedup/releases/tag/v0.1.0
