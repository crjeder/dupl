# Spec: File Discovery

## Overview

File discovery walks one or more directory trees, applies filters, and groups candidate
files by exact byte size. Only groups with at least two members are forwarded to
duplicate detection.

## Requirements

### Directory Walking

- The crawler MUST recursively descend into each provided directory.
- The crawler MUST NOT follow symbolic links.
- Only regular files MUST be included; directories, symlinks, and special files MUST
  be ignored.

### Extension Filtering

- If one or more extensions are specified, a file MUST be included only when its
  extension matches one of the specified values (case-insensitive comparison).
- Files with no extension MUST be excluded when an extension filter is active.
- If no extensions are specified, all regular files MUST be included regardless of
  extension.

### Size Filtering

- A file MUST be included only when its size in bytes is greater than or equal to the
  configured minimum size.

### Size Grouping

Included files SHALL be grouped by their exact byte size. The unit of grouping is an `AliasGroup` (one logical file, which may have multiple path aliases). Any size group containing fewer than two `AliasGroup` values SHALL be discarded; it cannot contain duplicates. Only size groups with two or more `AliasGroup` values SHALL be returned to the caller.

#### Scenario: Two independent files with the same size
- **WHEN** two files with different inodes share the same byte size
- **THEN** they are placed in the same size group as separate `AliasGroup` values and forwarded to duplicate detection

#### Scenario: Two hard-linked paths with the same size
- **WHEN** two paths share the same inode (and therefore the same size)
- **THEN** they form one `AliasGroup`; the size group contains one entry and is discarded as a singleton

#### Scenario: Three paths — two aliased, one independent — sharing a size
- **WHEN** paths A and B share inode 42, and path C has inode 99, all with the same byte size
- **THEN** the size group contains two `AliasGroup` values (one with A+B, one with C) and is forwarded to duplicate detection

### Error Handling

- A failure to read a directory entry or its metadata MUST produce a warning on `stderr`.
- Such failures MUST NOT terminate the process; the affected entry MUST be skipped and
  all other entries MUST continue to be processed.
