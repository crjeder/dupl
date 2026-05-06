# Spec: Inode Alias Grouping

## Overview

<!-- TBD: expand purpose -->
During the directory crawl, files that share the same `(device, inode)` pair (hard
links) are grouped into a single logical unit called an `AliasGroup`. This prevents
hard-linked paths from being reported as content duplicates of each other, ensures
only one file handle is opened per physical file, and makes all alias paths visible
in the output.

## Requirements

### Requirement: Files sharing an inode are grouped as aliases
During the directory crawl, the tool SHALL group all paths that share the same `(device, inode)` pair into a single logical unit called an `AliasGroup`. All paths in the group are preserved. The group is treated as one file for the purposes of size grouping, duplicate detection, and output.

#### Scenario: Hard-linked paths under one root
- **WHEN** `/photos/a.jpg` and `/photos/backup/a.jpg` share the same inode
- **THEN** they form one `AliasGroup`; only one file handle is opened during block comparison

#### Scenario: Independent files with the same size
- **WHEN** `/photos/a.jpg` (inode 42) and `/photos/b.jpg` (inode 99) have the same byte size but different inodes
- **THEN** they are placed in separate `AliasGroup` values and compared as independent candidates

### Requirement: Exactly one file handle is opened per AliasGroup
During block comparison, the tool SHALL open a file handle to exactly one representative path per `AliasGroup`. The representative path is the first path added to the group during the crawl.

#### Scenario: AliasGroup with three aliases
- **WHEN** three paths share one inode
- **THEN** one `File::open` call is made; the other two paths are carried as metadata only

### Requirement: All alias paths appear in output
When an `AliasGroup` is part of a reported duplicate group, ALL paths in the group SHALL appear in the output. Alias paths SHALL be identified with a `link:` prefix.

#### Scenario: Duplicate group where one physical file has two hard-linked paths
- **WHEN** inode 42 (paths: `/photos/a.jpg`, `/photos/backup/a.jpg`) is a content duplicate of inode 99 (path: `/archive/a.jpg`)
- **THEN** output contains `/photos/a.jpg`, `link: /photos/backup/a.jpg`, and `/archive/a.jpg`

### Requirement: Inode unavailability falls back conservatively
If the operating system or filesystem does not provide inode identity for a file (e.g. `file_index()` or `volume_serial_number()` returns `None` on Windows), the tool SHALL treat the file as having a unique inode and create a single-path `AliasGroup` for it.

#### Scenario: File on FAT volume on Windows
- **WHEN** `file_index()` returns `None` for a file
- **THEN** the file is placed in its own `AliasGroup` with no aliases; no error is emitted
