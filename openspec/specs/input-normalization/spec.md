# Spec: Input Normalization

## Overview

<!-- TBD: expand purpose -->
Before the directory crawl begins, input roots are normalized to eliminate redundant
entries (e.g. a subdirectory supplied alongside its parent, or the same path given
twice). Normalization uses inode identity so that bind mounts and symlink-resolved
paths are also detected.

## Requirements

### Requirement: Redundant input roots are removed before crawling
Before the directory crawl begins, the tool SHALL eliminate any input root directory whose filesystem inode is reachable as a descendant of another input root directory. Only the containing root is kept; the redundant root is silently dropped.

#### Scenario: Subdirectory passed alongside its parent
- **WHEN** the user supplies `/photos` and `/photos/2024` as input roots
- **THEN** `/photos/2024` is dropped and only `/photos` is walked

#### Scenario: Same directory passed twice
- **WHEN** the user supplies the same directory path twice, or two paths that resolve to the same inode
- **THEN** only one instance is retained

#### Scenario: Bind-mounted subtree covered by another root
- **WHEN** an input root's inode matches a directory inode encountered while walking another input root
- **THEN** the covered root is dropped before crawling begins

### Requirement: Input normalization walks directories only
During input normalization the tool SHALL stat directory entries only. Regular files SHALL NOT be opened or read during this phase.

#### Scenario: Large photo library with many files
- **WHEN** normalization runs against a directory containing thousands of files
- **THEN** only directory inodes are stat'd; no file reads occur during normalization

### Requirement: Normalization errors are non-fatal
If a directory entry cannot be stat'd during input normalization, the tool SHALL emit a warning to `stderr` and treat the affected input root as non-redundant (keep it).

#### Scenario: Permission error on a subdirectory
- **WHEN** a subdirectory under an input root cannot be read during normalization
- **THEN** a warning is emitted to `stderr` and the walk continues; no root is incorrectly dropped

### Requirement: Inode unavailability is handled conservatively
If the operating system or filesystem does not provide inode identity for a directory (e.g. FAT on Windows where `volume_serial_number` or `file_index` returns `None`), the tool SHALL skip redundancy checking for that entry and retain it.

#### Scenario: FAT or network filesystem on Windows
- **WHEN** `volume_serial_number()` or `file_index()` returns `None` for a directory
- **THEN** the directory is treated as non-redundant and kept in the input list
