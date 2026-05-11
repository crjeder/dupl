//! Block-by-block duplicate detection.
//!
//! This module provides two strategies:
//!
//! * [`find_duplicates`] — the original divide-and-conquer approach that
//!   partitions files by raw block bytes.  Retained for correctness comparison
//!   and as the `--legacy` fallback.
//!
//! * [`find_duplicates_blockwise`] — the optimised path used with `--fast`.
//!   It reads each block-pass in fiemap-sorted order (Phase 1), then partitions
//!   candidates depth-first using a two-byte bucket index and raw memcmp
//!   (Phase 2).  Groups that shrink to ≤ `small_group_threshold` are handed
//!   off to [`crate::two_file::compare_n`] for streaming N-way comparison.
//!   No hash function is used anywhere in this path.

use crate::crawl::AliasGroup;
use crate::readlist::ReadListEntry;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

// ── Fast path: block-compare engine ───────────────────────────────────────────

/// One bucket entry in the depth-first partition step.
struct Candidate {
    /// Block bytes from the first file that landed in this bucket slot.
    reference_block: Vec<u8>,
    /// Files whose block at the current pass matched `reference_block`.
    files: Vec<AliasGroup>,
}

/// Find duplicate files using block-by-block divide-and-conquer.
///
/// Large groups (> `small_group_threshold`) are processed pass-by-pass:
///
/// * **Phase 1** — reads block[pass] for every surviving file in fiemap order
///   (physical-block sort), minimising seek distance.
/// * **Phase 2** — partitions the surviving set depth-first using the first two
///   bytes of each block as a bucket index and raw `memcmp` within each bucket.
///
/// Groups that drop to ≤ `small_group_threshold` during any pass are handed
/// off to [`crate::two_file::compare_n`], which streams the remaining blocks
/// directly without the block-pass overhead.
///
/// Groups that survive all passes are confirmed identical.
pub fn find_duplicates_blockwise(
    groups: Vec<AliasGroup>,
    read_list: Vec<ReadListEntry>,
    block_size: usize,
    small_group_threshold: usize,
    file_size: u64,
) -> Vec<Vec<AliasGroup>> {
    if groups.len() < 2 {
        return vec![];
    }

    // Small initial groups bypass the block-pass loop entirely.
    if groups.len() <= small_group_threshold {
        return crate::two_file::compare_n(groups, block_size, 0);
    }

    let num_passes = (file_size as usize + block_size - 1) / block_size;
    let mut results: Vec<Vec<AliasGroup>> = vec![];
    let mut pending_groups: Vec<Vec<AliasGroup>> = vec![groups];

    for pass in 0..num_passes {
        if pending_groups.is_empty() {
            break;
        }

        let pass_offset = (pass as u64) * (block_size as u64);

        // Build a set of paths still alive across all pending groups.
        let survivor_paths: HashSet<PathBuf> = pending_groups
            .iter()
            .flat_map(|g| g.iter().map(|ag| ag.representative().clone()))
            .collect();

        // Phase 1 — I/O: collect this pass's read-list entries in fiemap order.
        let mut pass_rl: Vec<&ReadListEntry> = read_list
            .iter()
            .filter(|e| e.block_offset == pass_offset && survivor_paths.contains(&e.path))
            .collect();
        pass_rl.sort_unstable_by_key(|e| e.physical_block);

        // Read one block per file into the cache.
        let mut block_cache: HashMap<PathBuf, Vec<u8>> = HashMap::new();
        let mut buf = vec![0u8; block_size];
        for entry in &pass_rl {
            let mut file = match File::open(&entry.path) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("warning: {}: {e}", entry.path.display());
                    continue;
                }
            };
            if let Err(e) = file.seek(SeekFrom::Start(entry.block_offset)) {
                eprintln!("warning: {}: seek: {e}", entry.path.display());
                continue;
            }
            let n = match file.read(&mut buf) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("warning: {}: read: {e}", entry.path.display());
                    continue;
                }
            };
            block_cache.insert(entry.path.clone(), buf[..n].to_vec());
        }

        // Phase 2 — CPU: partition each pending group depth-first.
        let mut next_pending: Vec<Vec<AliasGroup>> = vec![];

        for group in pending_groups {
            // Bucket index: first two bytes of block as u16.
            // Within each bucket, candidates are discriminated by full memcmp.
            let mut buckets: HashMap<u16, Vec<Candidate>> = HashMap::new();

            for ag in group {
                let block = match block_cache.get(ag.representative()) {
                    Some(b) if !b.is_empty() => b,
                    _ => continue, // read error or unexpected EOF — eject
                };

                let idx = u16::from_le_bytes([block[0], block[1]]);
                let bucket = buckets.entry(idx).or_default();

                // Find an existing candidate whose reference block matches.
                let found_pos = bucket
                    .iter()
                    .position(|c| c.reference_block == *block);

                match found_pos {
                    Some(i) => bucket[i].files.push(ag),
                    None => bucket.push(Candidate {
                        reference_block: block.clone(),
                        files: vec![ag],
                    }),
                }
            }

            // Dispatch each candidate sub-group.
            for (_, candidates) in buckets {
                for candidate in candidates {
                    if candidate.files.len() < 2 {
                        continue;
                    }
                    if candidate.files.len() <= small_group_threshold {
                        // Hand off to streaming N-way compare, skipping confirmed blocks.
                        results.extend(crate::two_file::compare_n(
                            candidate.files,
                            block_size,
                            pass + 1,
                        ));
                    } else {
                        next_pending.push(candidate.files);
                    }
                }
            }
        }

        pending_groups = next_pending;
    }

    // Groups that survived all passes are confirmed identical.
    for group in pending_groups {
        if group.len() >= 2 {
            results.push(group);
        }
    }

    results
}

// ── Legacy path: raw-byte divide-and-conquer ──────────────────────────────────

/// Pairs an open file handle with its group metadata.
struct FileReader {
    group: AliasGroup,
    file: File,
}

/// Find groups of byte-identical files among `groups` (original algorithm).
///
/// Uses raw block bytes as `HashMap` keys — memory-intensive for large block
/// sizes and many files, but retained as the correctness reference and
/// `--legacy` fallback.
pub fn find_duplicates(groups: Vec<AliasGroup>, block_size: usize) -> Vec<Vec<AliasGroup>> {
    let readers: Vec<FileReader> = groups
        .into_iter()
        .filter_map(|g| {
            File::open(g.representative())
                .ok()
                .map(|f| FileReader { group: g, file: f })
        })
        .collect();

    split_by_block(readers, block_size)
}

/// Recursively split `readers` into groups whose files share the same content.
fn split_by_block(readers: Vec<FileReader>, block_size: usize) -> Vec<Vec<AliasGroup>> {
    if readers.len() < 2 {
        return vec![];
    }

    let mut groups: HashMap<Vec<u8>, Vec<FileReader>> = HashMap::new();

    for mut reader in readers {
        let mut buf = vec![0u8; block_size];
        match reader.file.read(&mut buf) {
            Ok(n) => {
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
            continue;
        }
        if block.is_empty() {
            result.push(group.into_iter().map(|r| r.group).collect());
        } else {
            result.extend(split_by_block(group, block_size));
        }
    }

    result
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::find_duplicates_blockwise;
    #[cfg(unix)]
    use super::AliasGroup;
    #[cfg(unix)]
    use std::fs;
    #[cfg(unix)]
    use std::path::PathBuf;

    #[cfg(unix)]
    fn temp_file(label: &str, content: &[u8]) -> PathBuf {
        let path = std::env::temp_dir()
            .join(format!("dupl_engine_{}_{}", label, std::process::id()));
        fs::write(&path, content).unwrap();
        path
    }

    #[cfg(unix)]
    fn alias(path: PathBuf) -> AliasGroup {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let inode = fs::metadata(&path).ok().map(|m| m.ino());
            AliasGroup { paths: vec![path], inode }
        }
        #[cfg(not(unix))]
        {
            AliasGroup { paths: vec![path], inode: None }
        }
    }

    #[cfg(unix)]
    #[test]
    fn find_duplicates_blockwise_detects_identical_files() {
        let pa = temp_file("bw_a", b"duplicate content");
        let pb = temp_file("bw_b", b"duplicate content");
        let pc = temp_file("bw_c", b"unique content---");

        let groups = vec![alias(pa.clone()), alias(pb.clone()), alias(pc.clone())];
        let size = b"duplicate content".len() as u64;

        use crate::readlist::{build_read_list, sort_read_list, SMALL_GROUP_LARGE_LIMIT, SMALL_GROUP_SMALL_LIMIT};
        let pairs = vec![(size, groups.clone())];
        let mut rl = build_read_list(&pairs, 65_536);
        sort_read_list(&mut rl, 65_536, 524_288, SMALL_GROUP_SMALL_LIMIT, SMALL_GROUP_LARGE_LIMIT);

        let result = find_duplicates_blockwise(groups, rl, 65_536, 32, size);
        assert!(!result.is_empty());
        let paths: Vec<_> = result[0].iter().map(|ag| ag.representative().clone()).collect();
        assert!(paths.contains(&pa));
        assert!(paths.contains(&pb));
        assert!(!paths.contains(&pc));

        fs::remove_file(pa).unwrap();
        fs::remove_file(pb).unwrap();
        fs::remove_file(pc).unwrap();
    }
}
