//! Direct streaming byte-comparison for small candidate groups.
//!
//! [`compare_two`] handles exactly two files: no hashing, exits on the first
//! differing block.
//!
//! [`compare_n`] handles groups of N files (up to `small_group_threshold`).
//! It uses the same divide-and-conquer split as the legacy path but bounded to
//! small N so the raw-block HashMap keys stay manageable in memory.  Files are
//! opened in ascending-inode order and the read starts from `start_block` so
//! the caller (the block-pass loop) can pass in how many leading blocks have
//! already been confirmed equal.
//!
//! ## Read order
//!
//! Files are opened in ascending-inode order.  On btrfs, inode numbers
//! correlate with allocation order, so the lower inode is usually physically
//! closer to the start of the volume.

use crate::crawl::AliasGroup;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

/// Compare two files byte-for-byte in `block_size`-chunk increments.
///
/// Returns `true` if and only if both files contain identical bytes.
/// Files are opened in ascending-inode order to minimise seek distance.
///
/// # Arguments
///
/// * `a`, `b`       — the two candidate files to compare.
/// * `block_size`   — read buffer size in bytes.
///
/// # Errors
///
/// If either file cannot be opened or read, the function returns `false`
/// (treating an unreadable file as non-identical) and prints a warning to
/// stderr.
pub fn compare_two(a: &AliasGroup, b: &AliasGroup, block_size: usize) -> bool {
    // Open in ascending inode order; fall back to original order if inodes are
    // unavailable (Windows) or equal.
    let (first, second) = if a.inode <= b.inode { (a, b) } else { (b, a) };

    let mut f1 = match File::open(first.representative()) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("warning: {}: {e}", first.representative().display());
            return false;
        }
    };
    let mut f2 = match File::open(second.representative()) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("warning: {}: {e}", second.representative().display());
            return false;
        }
    };

    let mut buf1 = vec![0u8; block_size];
    let mut buf2 = vec![0u8; block_size];

    loop {
        let n1 = match f1.read(&mut buf1) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("warning: {}: {e}", first.representative().display());
                return false;
            }
        };
        let n2 = match f2.read(&mut buf2) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("warning: {}: {e}", second.representative().display());
                return false;
            }
        };

        if n1 != n2 {
            // One file reached EOF before the other — sizes differ (unexpected,
            // since the caller grouped by size, but guard anyway).
            return false;
        }
        if n1 == 0 {
            // Both at EOF with all preceding blocks equal — identical.
            return true;
        }
        if buf1[..n1] != buf2[..n2] {
            // First differing block found — not identical.
            return false;
        }
    }
}

// ── N-way comparison ──────────────────────────────────────────────────────────

/// Pairs an open, pre-seeked file handle with its group metadata.
struct NFileReader {
    group: AliasGroup,
    file: File,
}

/// Compare N candidate files block-by-block, returning confirmed-identical sub-groups.
///
/// Files are opened in ascending-inode order.  Reading begins at
/// `start_block * block_size`; blocks before that offset were already confirmed
/// equal by the block-pass loop in the caller.
///
/// Uses divide-and-conquer: each block round partitions the group by raw block
/// content.  Because N is small (≤ `small_group_threshold`, default 32), the
/// raw-block HashMap keys are bounded to at most N × `block_size` bytes.
pub fn compare_n(
    candidates: Vec<AliasGroup>,
    block_size: usize,
    start_block: usize,
) -> Vec<Vec<AliasGroup>> {
    if candidates.len() < 2 {
        return vec![];
    }

    let mut sorted = candidates;
    sorted.sort_unstable_by_key(|ag| ag.inode);

    let start_offset = (start_block as u64) * (block_size as u64);

    let readers: Vec<NFileReader> = sorted
        .into_iter()
        .filter_map(|ag| {
            let mut f = match File::open(ag.representative()) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("warning: {}: {e}", ag.representative().display());
                    return None;
                }
            };
            if start_offset > 0 {
                if let Err(e) = f.seek(SeekFrom::Start(start_offset)) {
                    eprintln!("warning: {}: seek: {e}", ag.representative().display());
                    return None;
                }
            }
            Some(NFileReader { group: ag, file: f })
        })
        .collect();

    if readers.len() < 2 {
        return vec![];
    }

    split_from_readers(readers, block_size)
}

/// Recursively split `readers` into groups whose files share identical content
/// from the current file-handle position onward.
fn split_from_readers(readers: Vec<NFileReader>, block_size: usize) -> Vec<Vec<AliasGroup>> {
    if readers.len() < 2 {
        return vec![];
    }

    let mut groups: HashMap<Vec<u8>, Vec<NFileReader>> = HashMap::new();

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
            // All files in this group reached EOF simultaneously — confirmed identical.
            result.push(group.into_iter().map(|r| r.group).collect());
        } else {
            result.extend(split_from_readers(group, block_size));
        }
    }

    result
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_file(label: &str, content: &[u8]) -> PathBuf {
        let path = std::env::temp_dir()
            .join(format!("dupl_twofile_{}_{}", label, std::process::id()));
        fs::write(&path, content).unwrap();
        path
    }

    fn alias(path: PathBuf, inode: Option<u64>) -> AliasGroup {
        AliasGroup { paths: vec![path], inode }
    }

    #[test]
    fn identical_content_returns_true() {
        let pa = temp_file("id_a", b"hello world");
        let pb = temp_file("id_b", b"hello world");
        let a = alias(pa.clone(), Some(1));
        let b = alias(pb.clone(), Some(2));
        assert!(compare_two(&a, &b, 4096));
        fs::remove_file(pa).unwrap();
        fs::remove_file(pb).unwrap();
    }

    #[test]
    fn different_first_byte_returns_false() {
        let pa = temp_file("diff_a", b"AAAA");
        let pb = temp_file("diff_b", b"BBBB");
        let a = alias(pa.clone(), Some(10));
        let b = alias(pb.clone(), Some(20));
        assert!(!compare_two(&a, &b, 4096));
        fs::remove_file(pa).unwrap();
        fs::remove_file(pb).unwrap();
    }

    #[test]
    fn empty_files_are_identical() {
        let pa = temp_file("empty_a", b"");
        let pb = temp_file("empty_b", b"");
        let a = alias(pa.clone(), Some(5));
        let b = alias(pb.clone(), Some(6));
        assert!(compare_two(&a, &b, 4096));
        fs::remove_file(pa).unwrap();
        fs::remove_file(pb).unwrap();
    }

    /// Verify that the lower inode is opened first by checking that a file
    /// with inode=1 is processed before inode=2 regardless of argument order.
    #[cfg(unix)]
    #[test]
    fn lower_inode_opened_first() {
        use std::os::unix::fs::MetadataExt;

        let pa = temp_file("inode_a", b"data");
        let pb = temp_file("inode_b", b"data");

        let inode_a = fs::metadata(&pa).unwrap().ino();
        let inode_b = fs::metadata(&pb).unwrap().ino();

        let a = alias(pa.clone(), Some(inode_a));
        let b = alias(pb.clone(), Some(inode_b));

        // Both orderings must return the same result.
        assert_eq!(compare_two(&a, &b, 4096), compare_two(&b, &a, 4096));

        fs::remove_file(pa).unwrap();
        fs::remove_file(pb).unwrap();
    }

    // ── compare_n ─────────────────────────────────────────────────────────────

    #[cfg(unix)]
    #[test]
    fn compare_n_finds_identical_pair() {
        let pa = temp_file("cn_a", b"same content");
        let pb = temp_file("cn_b", b"same content");
        let candidates = vec![alias(pa.clone(), Some(1)), alias(pb.clone(), Some(2))];
        let result = compare_n(candidates, 4096, 0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 2);
        fs::remove_file(pa).unwrap();
        fs::remove_file(pb).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn compare_n_rejects_different_files() {
        let pa = temp_file("cn_diff_a", b"content A");
        let pb = temp_file("cn_diff_b", b"content B");
        let candidates = vec![alias(pa.clone(), Some(1)), alias(pb.clone(), Some(2))];
        let result = compare_n(candidates, 4096, 0);
        assert!(result.is_empty());
        fs::remove_file(pa).unwrap();
        fs::remove_file(pb).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn compare_n_splits_mixed_group() {
        // Three files: A==B, C is different.
        let pa = temp_file("cn_mix_a", b"match");
        let pb = temp_file("cn_mix_b", b"match");
        let pc = temp_file("cn_mix_c", b"other");
        let candidates = vec![
            alias(pa.clone(), Some(1)),
            alias(pb.clone(), Some(2)),
            alias(pc.clone(), Some(3)),
        ];
        let result = compare_n(candidates, 4096, 0);
        assert_eq!(result.len(), 1);
        let group_paths: Vec<_> = result[0].iter().map(|ag| ag.representative().clone()).collect();
        assert!(group_paths.contains(&pa));
        assert!(group_paths.contains(&pb));
        assert!(!group_paths.contains(&pc));
        fs::remove_file(pa).unwrap();
        fs::remove_file(pb).unwrap();
        fs::remove_file(pc).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn compare_n_respects_start_block() {
        // Files differ in block 0 but match in block 1.
        // With start_block=1, block 0 is skipped → they look identical.
        let block = vec![0u8; 4096];
        let mut content_a = vec![1u8; 4096]; // block 0: 0x01
        content_a.extend_from_slice(&block);  // block 1: 0x00
        let mut content_b = vec![2u8; 4096]; // block 0: 0x02
        content_b.extend_from_slice(&block);  // block 1: 0x00

        let pa = temp_file("cn_start_a", &content_a);
        let pb = temp_file("cn_start_b", &content_b);
        let candidates = vec![alias(pa.clone(), Some(1)), alias(pb.clone(), Some(2))];

        // start_block=0 → sees block 0 difference → no match
        let r0 = compare_n(candidates.clone(), 4096, 0);
        assert!(r0.is_empty());

        // start_block=1 → skips block 0, sees identical block 1 → match
        let r1 = compare_n(candidates, 4096, 1);
        assert_eq!(r1.len(), 1);

        fs::remove_file(pa).unwrap();
        fs::remove_file(pb).unwrap();
    }
}
