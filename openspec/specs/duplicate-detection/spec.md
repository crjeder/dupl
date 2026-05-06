# Spec: Duplicate Detection

## Overview

Duplicate detection takes a group of files known to share the same byte size and
determines which are byte-for-byte identical. Comparison is performed directly on file
content, block by block. No hash values are computed at any stage.

## Requirements

### No Hashing

- The implementation MUST NOT compute any hash (MD5, SHA-*, xxHash, or otherwise) of
  file content at any point.
- Files MUST be compared only by reading and comparing their raw bytes.

### Block-by-Block Comparison

- Files MUST be read and compared in sequential blocks of exactly 65,536 bytes (64 KiB).
- All files in a candidate group MUST be read one block at a time in lockstep.
- After each block is read, files MUST be re-grouped by the content of that block.
- Any file that diverges from the others on a given block MUST be separated into its
  own group and MUST NOT be compared further against the files it diverged from.

### Termination Condition

- A candidate group whose block read returns zero bytes (EOF) has been fully read.
- Because all files in the group share the same byte size and every preceding block
  matched, such a group MUST be reported as a set of identical (duplicate) files.

### Result Filtering

- Only groups of two or more identical files MUST be returned.
- A group that reduces to a single file at any point MUST be discarded.

### Error Handling

- A failure to open a file MUST produce a warning on `stderr`; the file MUST be
  excluded from its candidate group.
- A failure to read a block from a file MUST produce a warning on `stderr`; the file
  MUST be dropped from its candidate group at that point.
- These failures MUST NOT terminate the process.
