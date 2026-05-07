//! Directory crawling and file-identity tracking.
//!
//! This module is responsible for the *discovery* phase:
//!
//! 1. Walk every input directory recursively with [`walkdir`].
//! 2. For each file, record its size and — on Unix — its inode number.
//! 3. Group files by size into a `HashMap<u64, Vec<AliasGroup>>`.
//! 4. Collapse hard links (multiple directory entries that point to the same
//!    inode) into a single [`AliasGroup`] so they are never mistakenly
//!    treated as independent duplicates.
//! 5. Discard size buckets that contain only one logical file, because a
//!    single file cannot be its own duplicate.
//!
//! The output of [`crawl`] is handed to `dupl::find_duplicates`, which does
//! the actual byte-level comparison.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// A platform-neutral identity for an open file on disk.
///
/// On Unix this is `(device_id, inode_number)`.  Both values together
/// uniquely identify a physical file regardless of how many directory entries
/// (hard links) point to it.
///
/// The type alias makes function signatures easier to read than writing
/// `(u64, u64)` everywhere.
pub type FileId = (u64, u64);

/// One *logical* file on disk, which may be reachable via several paths.
///
/// On Unix, two directory entries that share the same inode refer to the exact
/// same bytes on disk (hard links).  `AliasGroup` keeps track of all known
/// paths so the program can report them together instead of flagging them as
/// duplicates.
///
/// # Example layout
///
/// ```
/// // photo.jpg and backup/photo.jpg are hard links of the same inode:
/// AliasGroup {
///     paths: ["photo.jpg", "backup/photo.jpg"],
///     inode: Some(12345),
/// }
/// ```
#[derive(Debug, Clone)]
pub struct AliasGroup {
    /// All filesystem paths that lead to this inode.
    ///
    /// `paths[0]` is the first path encountered during the crawl and is used
    /// as the "representative" when opening the file.  Subsequent entries are
    /// additional hard-link names.
    pub paths: Vec<PathBuf>,

    /// The inode number of this file, if the platform supports it.
    ///
    /// `Option<u64>` means it is either `Some(number)` or `None`.
    /// On Windows the inode API is currently unstable in Rust, so this is
    /// always `None` there.  The inode is used to sort files into physical
    /// disk order before reading, which improves I/O performance on
    /// extent-based filesystems (ext4, XFS, btrfs).
    pub inode: Option<u64>,
}

impl AliasGroup {
    /// Create a new group containing exactly one path.
    fn new(path: PathBuf, inode: Option<u64>) -> Self {
        AliasGroup { paths: vec![path], inode }
    }

    /// Return the path used to open a file handle for comparison.
    ///
    /// All aliases of a group read the same bytes, so any one of them works.
    /// We always pick `paths[0]` for simplicity.
    pub fn representative(&self) -> &PathBuf {
        &self.paths[0]
    }
}

// ── Platform-conditional inode extraction ────────────────────────────────────
//
// Rust's `#[cfg(...)]` attribute compiles a block only on the specified
// platform.  This lets us use the Unix-specific `MetadataExt` trait without
// breaking the Windows build.

/// Extract a `(device, inode)` pair from file metadata on Unix.
///
/// Both values are needed because inode numbers are only unique *within* a
/// single device.  Two files on different mount points could share the same
/// inode number by coincidence.
#[cfg(unix)]
fn file_id(_path: &Path, meta: &std::fs::Metadata) -> Option<FileId> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

/// On Windows, `file_index` and `volume_serial_number` are gated behind the
/// unstable `windows_by_handle` feature (rust#63010). Until that stabilises,
/// inode dedup is skipped on Windows and the conservative fallback applies.
///
/// Returning `None` means the caller will treat every path as a distinct file,
/// which is safe (no false "already seen" merges) but slightly less efficient.
#[cfg(windows)]
fn file_id(_path: &Path, _meta: &std::fs::Metadata) -> Option<FileId> {
    None
}

/// Fallback for any platform that is neither Unix nor Windows.
#[cfg(not(any(unix, windows)))]
fn file_id(_path: &Path, _meta: &std::fs::Metadata) -> Option<FileId> {
    None
}

// ── Input normalization ───────────────────────────────────────────────────────

/// Remove redundant input roots before crawling.
///
/// A root is considered redundant when it is a descendant of another root in
/// the same input list.  This prevents double-counting files that live inside
/// an already-covered directory.  The function also drops exact duplicates
/// (the same directory listed twice).
///
/// When inode information is unavailable (Windows), roots are kept as-is to
/// avoid accidentally discarding valid paths.
///
/// # Arguments
///
/// * `dirs` — the raw list of directory paths from the command line.
///
/// # Returns
///
/// A deduplicated list of root paths, in the original order of first
/// occurrence.
pub fn normalize_inputs(dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    // Step 1: stat every directory and drop exact same-inode duplicates.
    // `HashSet` gives O(1) membership tests.  `seen.insert(id)` returns
    // `true` when the value was *newly* inserted and `false` if it was
    // already present — handy for deduplication in one line.
    let mut seen: HashSet<FileId> = HashSet::new();
    let mut candidates: Vec<(Option<FileId>, PathBuf)> = Vec::new();

    for dir in dirs {
        // `std::fs::metadata` returns a `Result`; `.map_err(...)` logs any
        // error to stderr and `.ok()` converts the `Result` into an `Option`
        // (`Some(metadata)` on success, `None` on error).
        let id = std::fs::metadata(&dir)
            .map_err(|e| eprintln!("warning: {}: {e}", dir.display()))
            .ok()
            .and_then(|m| file_id(&dir, &m));

        if let Some(id) = id {
            // We have inode info.  Skip this directory if we have already
            // seen the same inode (it was listed more than once).
            if !seen.insert(id) {
                continue; // exact duplicate — discard
            }
        }
        candidates.push((id, dir));
    }

    // Step 2: for each candidate, collect the inodes of every subdirectory
    // it contains.  A candidate is redundant if its own inode appears inside
    // another candidate's subtree.
    let descendant_sets: Vec<HashSet<FileId>> = candidates
        .iter()
        .map(|(_, dir)| collect_descendant_dir_ids(dir))
        .collect();

    // Step 3: keep only non-redundant candidates.
    let mut result = Vec::new();
    for (i, (id, dir)) in candidates.into_iter().enumerate() {
        let redundant = match id {
            // No inode info → assume it is not redundant (conservative).
            None => false,
            // Check whether this inode appears in any *other* candidate's
            // descendant set.  `enumerate` gives us the index `j` so we can
            // skip comparing a candidate against itself.
            Some(id) => descendant_sets
                .iter()
                .enumerate()
                .any(|(j, set)| j != i && set.contains(&id)),
        };
        if !redundant {
            result.push(dir);
        }
    }
    result
}

/// Collect the `FileId` of every *subdirectory* reachable from `root`.
///
/// Only directories are collected (not files), because we use this set to
/// detect when one input root is a subdirectory of another.
fn collect_descendant_dir_ids(root: &Path) -> HashSet<FileId> {
    let mut ids = HashSet::new();
    // `WalkDir` yields every entry under `root`.  `follow_links(false)` means
    // symbolic links are not followed — we only care about real directories.
    // `min_depth(1)` skips the root itself (depth 0).
    for entry in WalkDir::new(root).follow_links(false).min_depth(1) {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                eprintln!("warning: {e}");
                continue;
            }
        };
        if !entry.file_type().is_dir() {
            continue; // we only want directory inodes here
        }
        if let Ok(meta) = entry.metadata() {
            if let Some(id) = file_id(entry.path(), &meta) {
                ids.insert(id);
            }
        }
    }
    ids
}

// ── Directory crawl ───────────────────────────────────────────────────────────

/// Walk `dirs` and return files grouped by exact byte size.
///
/// The returned map has the structure `HashMap<size, Vec<AliasGroup>>`.  Each
/// key is a file size in bytes; each value is the list of distinct logical
/// files at that size.  Only size buckets with **≥2 entries** are returned —
/// a single file at a given size cannot be a duplicate.
///
/// # Hard-link handling
///
/// When two directory entries share the same `(device, inode)` pair they are
/// collapsed into a single `AliasGroup` with multiple paths.  This prevents
/// them from being reported as duplicates of each other.
///
/// # I/O order optimisation
///
/// Within each size bucket, `AliasGroup` entries are sorted by inode number.
/// On extent-based filesystems the inode number roughly correlates with the
/// physical location on disk, so reading in inode order reduces seek distance.
///
/// # Arguments
///
/// * `dirs`       — root directories to crawl (duplicates already removed).
/// * `extensions` — lowercase extension whitelist; empty means "accept all".
/// * `min_size`   — files smaller than this many bytes are ignored.
pub fn crawl(
    dirs: &[PathBuf],
    extensions: &[String],
    min_size: u64,
) -> HashMap<u64, Vec<AliasGroup>> {
    // Primary map: file size → list of AliasGroups seen at that size.
    let mut by_size: HashMap<u64, Vec<AliasGroup>> = HashMap::new();

    // Secondary index: (size, FileId) → position in `by_size[size]`.
    // This lets us find an existing AliasGroup for a given inode in O(1) so
    // we can append an alias path without scanning the whole list.
    let mut inode_index: HashMap<(u64, FileId), usize> = HashMap::new();

    for dir in dirs {
        // `WalkDir` recursively yields directory entries.
        // `follow_links(false)` prevents infinite loops from symlink cycles.
        for entry in WalkDir::new(dir).follow_links(false) {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("warning: {e}");
                    continue; // skip unreadable entries
                }
            };

            // We only want regular files, not directories or symlinks.
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();

            // If the user asked for specific extensions, skip non-matching files.
            if !extensions.is_empty() && !has_matching_extension(path, extensions) {
                continue;
            }

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("warning: {}: {e}", path.display());
                    continue;
                }
            };

            let size = metadata.len(); // file size in bytes
            if size < min_size {
                continue; // too small — skip
            }

            // Attempt to get a unique identity for this file.
            let id = file_id(path, &metadata);

            // `path.to_path_buf()` converts the borrowed `&Path` into an
            // owned `PathBuf` that we can store in the map.
            let path_buf = path.to_path_buf();

            match id {
                Some(id) => {
                    // We have an inode.  Check whether this exact (size, inode)
                    // combination was already encountered — if so, this is
                    // another hard-link alias of a file we already have.
                    let key = (size, id);
                    if let Some(&idx) = inode_index.get(&key) {
                        // Append the new path to the existing AliasGroup.
                        by_size.get_mut(&size).unwrap()[idx].paths.push(path_buf);
                    } else {
                        // New inode: create a fresh AliasGroup.
                        // `entry(...).or_default()` returns the existing Vec or
                        // inserts an empty one if the key is not yet present.
                        let groups = by_size.entry(size).or_default();
                        inode_index.insert(key, groups.len()); // record position
                        groups.push(AliasGroup::new(path_buf, Some(id.1)));
                    }
                }
                None => {
                    // No inode info (Windows) — treat every path as a distinct
                    // file.  This is conservative: we may compare files that
                    // are actually hard links, but we will never miss real
                    // duplicates.
                    by_size.entry(size).or_default().push(AliasGroup::new(path_buf, None));
                }
            }
        }
    }

    // Remove size buckets where only one logical file exists — nothing to
    // compare.  `retain` keeps only the entries for which the closure returns
    // `true`.
    by_size.retain(|_, groups| groups.len() > 1);

    // Sort each size bucket by inode so block reads approximate physical disk
    // order.  `sort_unstable_by_key` is slightly faster than `sort_by_key`
    // because it does not need to preserve the original order of equal elements.
    // Groups without inode info (Windows) sort by `None`, which all compare
    // equal, so they stay in insertion order.
    for groups in by_size.values_mut() {
        groups.sort_unstable_by_key(|g| g.inode);
    }

    by_size
}

/// Return `true` when `path`'s extension is present in `extensions`.
///
/// Comparison is case-insensitive.  Files without an extension always return
/// `false`.
///
/// `path.extension()` returns `Option<&OsStr>`.  We chain `.and_then` and
/// `.map` to transform it step-by-step without unwrapping manually:
///
/// ```
/// OsStr → str → lowercase → contains?
/// ```
fn has_matching_extension(path: &Path, extensions: &[String]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())         // OsStr → &str (None if not valid UTF-8)
        .map(|e| extensions.contains(&e.to_lowercase()))
        .unwrap_or(false)                 // no extension → not a match
}

// ── Tests ─────────────────────────────────────────────────────────────────────
//
// These tests only compile and run on Unix because they rely on hard links and
// inode numbers.  The `#[cfg(unix)]` attributes ensure the Windows build is
// not broken by Unix-only APIs.

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::*;
    #[cfg(unix)]
    use std::fs;

    /// Create a fresh temporary directory with a unique name for each test.
    /// Using the process id in the name avoids collisions when tests run in
    /// parallel.
    #[cfg(unix)]
    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("dedup_{}_{}", label, std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    #[cfg(unix)]
    fn normalize_drops_subdirectory() {
        let parent = temp_dir("norm_sub");
        let child = parent.join("child");
        fs::create_dir(&child).unwrap();

        let result = normalize_inputs(vec![parent.clone(), child]);
        assert_eq!(result, vec![parent.clone()]);

        fs::remove_dir_all(&parent).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn normalize_deduplicates_same_dir() {
        let dir = temp_dir("norm_dup");

        let result = normalize_inputs(vec![dir.clone(), dir.clone()]);
        assert_eq!(result.len(), 1);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn crawl_collapses_hard_links_into_alias_group() {
        let dir = temp_dir("crawl_hl");
        let file_a = dir.join("a.txt");
        let file_b = dir.join("b.txt");

        fs::write(&file_a, b"same content").unwrap();
        if fs::hard_link(&file_a, &file_b).is_err() {
            fs::remove_dir_all(&dir).unwrap();
            return; // filesystem doesn't support hard links — skip
        }

        let result = crawl(&[dir.clone()], &[], 1);
        // Both paths share one inode → one AliasGroup → singleton → pruned.
        assert!(
            result.is_empty(),
            "hard-linked pair must not appear as a duplicate group"
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn crawl_reports_hard_link_plus_independent_copy_as_duplicates() {
        let dir = temp_dir("crawl_hl_dup");
        let file_a = dir.join("a.txt");
        let file_b = dir.join("b.txt"); // hard link of a
        let file_c = dir.join("c.txt"); // independent copy, same content

        fs::write(&file_a, b"same content").unwrap();
        if fs::hard_link(&file_a, &file_b).is_err() {
            fs::remove_dir_all(&dir).unwrap();
            return;
        }
        fs::write(&file_c, b"same content").unwrap();

        let result = crawl(&[dir.clone()], &[], 1);
        // Two distinct inodes → two AliasGroups → not a singleton → forwarded.
        let size = b"same content".len() as u64;
        let groups = result.get(&size).expect("should have a size group");
        assert_eq!(groups.len(), 2, "expected two AliasGroups");

        let alias_group = groups.iter().find(|g| g.paths.len() == 2);
        assert!(alias_group.is_some(), "one AliasGroup should have two aliases");

        fs::remove_dir_all(&dir).unwrap();
    }
}
