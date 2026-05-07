//! Block-by-block duplicate detection.
//!
//! This module takes the size-grouped file list produced by [`crate::crawl`]
//! and determines which files are truly identical by reading them in parallel,
//! one block at a time.
//!
//! ## Algorithm
//!
//! Reading entire files into memory would be slow and wasteful when most
//! candidate pairs diverge early.  Instead, [`split_by_block`] uses a
//! divide-and-conquer approach:
//!
//! 1. Read one block from every file in the current group.
//! 2. Partition the files by the bytes they returned — files that read the
//!    same block stay together; files that read different bytes are split into
//!    separate sub-groups.
//! 3. Recurse on each sub-group.
//! 4. When a group reads an **empty** block (EOF), all files in it have been
//!    fully compared and are byte-for-byte identical.
//!
//! This means two files that differ in their very first block are discarded
//! after a single read, without ever reading the rest of those files.

use crate::crawl::AliasGroup;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

/// Pairs an open file handle with its group metadata.
///
/// `File` is kept open across multiple [`split_by_block`] calls so we do not
/// pay the cost of re-opening the file for every block.  The OS kernel keeps
/// the file position, so the next `read` call automatically continues where
/// the previous one left off.
///
/// This struct is private to this module (`pub` is not used).
struct FileReader {
    /// Metadata about the file (paths, inode).
    group: AliasGroup,
    /// Open file handle positioned at the current read offset.
    file: File,
}

/// Find groups of byte-identical files among `groups`.
///
/// All entries in `groups` must have the same file size — this is guaranteed
/// by the caller, which passes one size bucket at a time.
///
/// # Returns
///
/// A `Vec` of duplicate sets.  Each inner `Vec<AliasGroup>` contains two or
/// more entries that are completely identical.  Groups with fewer than two
/// entries are silently discarded (no duplicate).
///
/// # Errors
///
/// Files that cannot be opened are skipped with a warning printed to stderr.
pub fn find_duplicates(groups: Vec<AliasGroup>, block_size: usize) -> Vec<Vec<AliasGroup>> {
    // Try to open the representative file for every AliasGroup.
    // `filter_map` is like `map` but also removes `None` values, so groups
    // whose file cannot be opened are automatically skipped.
    let readers: Vec<FileReader> = groups
        .into_iter()
        .filter_map(|g| {
            // `File::open` returns `Result<File, io::Error>`.
            // `.ok()` converts it to `Option<File>` (Some on success, None on error).
            // `.map(|f| ...)` wraps the open file in a `FileReader`.
            File::open(g.representative())
                .ok()
                .map(|f| FileReader { group: g, file: f })
        })
        .collect();

    split_by_block(readers, block_size)
}

/// Recursively split `readers` into groups whose files share the same content.
///
/// ## How the partitioning works
///
/// ```text
///  readers: [A, B, C, D]          (all same size, cursor at same offset)
///
///  Read one block from each:
///    A → [0x00, 0x01, ...]
///    B → [0x00, 0x01, ...]   ← same as A
///    C → [0xFF, 0x00, ...]   ← different
///    D → []                  ← EOF (file is a multiple of block_size)
///
///  Partition by block content:
///    [0x00, 0x01, ...] → [A, B]   recurse
///    [0xFF, 0x00, ...] → [C]      singleton — discard
///    []                → [D]      singleton — discard (or exact match if ≥2)
/// ```
///
/// ## Base cases
///
/// * Fewer than 2 readers — cannot form a duplicate pair, return empty.
/// * All readers return an empty block — they have all reached EOF at the same
///   point, meaning every preceding block was identical.  They are duplicates.
///
/// # Arguments
///
/// * `readers`    — files to compare, all positioned at the same byte offset.
/// * `block_size` — number of bytes to read per call.
fn split_by_block(readers: Vec<FileReader>, block_size: usize) -> Vec<Vec<AliasGroup>> {
    // Base case: cannot have a duplicate with fewer than 2 files.
    if readers.len() < 2 {
        return vec![];
    }

    // Partition readers by the content of the next block.
    // The key is `Vec<u8>` (the raw bytes read).  Files that read the same
    // bytes end up in the same bucket.
    //
    // `HashMap::entry(...).or_default()` is an ergonomic way to say:
    // "give me the Vec for this key, inserting an empty Vec if it doesn't
    //  exist yet, then push my value into it."
    let mut groups: HashMap<Vec<u8>, Vec<FileReader>> = HashMap::new();

    for mut reader in readers {
        // Allocate a buffer filled with zeros.  We read *up to* `block_size`
        // bytes; the actual number read is returned as `n`.
        let mut buf = vec![0u8; block_size];
        match reader.file.read(&mut buf) {
            Ok(n) => {
                // Shrink the buffer to the number of bytes actually read.
                // If n == 0 the buffer becomes empty, signalling EOF.
                buf.truncate(n);
                groups.entry(buf).or_default().push(reader);
            }
            Err(e) => eprintln!(
                "warning: {}: {e}",
                reader.group.representative().display()
            ),
        }
    }

    let mut result: Vec<Vec<AliasGroup>> = vec![];

    for (block, group) in groups {
        if group.len() < 2 {
            // Only one file read this block content — no match possible.
            continue;
        }
        if block.is_empty() {
            // Every file in this group returned EOF at the same offset.
            // They are byte-for-byte identical — collect them as one result.
            // `.into_iter().map(|r| r.group).collect()` extracts the
            // `AliasGroup` from each `FileReader`, discarding the file handle.
            result.push(group.into_iter().map(|r| r.group).collect());
        } else {
            // The files still agree so far but are not yet at EOF.
            // Read the next block and continue splitting.
            result.extend(split_by_block(group, block_size));
        }
    }

    result
}
