use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Platform-neutral file identity: (device_id, inode).
pub type FileId = (u64, u64);

/// One logical file on disk, which may have multiple directory-entry aliases (hard links).
#[derive(Debug, Clone)]
pub struct AliasGroup {
    pub paths: Vec<PathBuf>,
}

impl AliasGroup {
    fn new(path: PathBuf) -> Self {
        AliasGroup { paths: vec![path] }
    }

    /// The path used to open a file handle; all aliases read the same bytes.
    pub fn representative(&self) -> &PathBuf {
        &self.paths[0]
    }
}

// ── Platform-conditional inode extraction ────────────────────────────────────

#[cfg(unix)]
fn file_id(_path: &Path, meta: &std::fs::Metadata) -> Option<FileId> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

/// On Windows, `file_index` and `volume_serial_number` are gated behind the
/// unstable `windows_by_handle` feature (rust#63010). Until that stabilises,
/// inode dedup is skipped on Windows and the conservative fallback applies.
#[cfg(windows)]
fn file_id(_path: &Path, _meta: &std::fs::Metadata) -> Option<FileId> {
    None
}

#[cfg(not(any(unix, windows)))]
fn file_id(_path: &Path, _meta: &std::fs::Metadata) -> Option<FileId> {
    None
}

// ── Input normalization ───────────────────────────────────────────────────────

/// Remove redundant input roots before crawling.
///
/// A root is redundant when its inode is reachable as a descendant of another
/// root (covers hard links, bind mounts, overlapping paths, same dir twice).
/// When inode information is unavailable the root is kept (conservative).
pub fn normalize_inputs(dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    // Stat each dir; drop exact-same-inode duplicates (keep first occurrence).
    let mut seen: HashSet<FileId> = HashSet::new();
    let mut candidates: Vec<(Option<FileId>, PathBuf)> = Vec::new();

    for dir in dirs {
        let id = std::fs::metadata(&dir)
            .map_err(|e| eprintln!("warning: {}: {e}", dir.display()))
            .ok()
            .and_then(|m| file_id(&dir, &m));

        if let Some(id) = id {
            if !seen.insert(id) {
                continue; // exact duplicate
            }
        }
        candidates.push((id, dir));
    }

    // Walk subdirectories of each candidate (dirs only, no files).
    let descendant_sets: Vec<HashSet<FileId>> = candidates
        .iter()
        .map(|(_, dir)| collect_descendant_dir_ids(dir))
        .collect();

    // Keep a candidate only if its inode does not appear in any other
    // candidate's descendant set.
    let mut result = Vec::new();
    for (i, (id, dir)) in candidates.into_iter().enumerate() {
        let redundant = match id {
            None => false,
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

fn collect_descendant_dir_ids(root: &Path) -> HashSet<FileId> {
    let mut ids = HashSet::new();
    for entry in WalkDir::new(root).follow_links(false).min_depth(1) {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                eprintln!("warning: {e}");
                continue;
            }
        };
        if !entry.file_type().is_dir() {
            continue;
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

/// Crawl `dirs` and return files grouped by size, excluding singletons.
/// Files sharing a filesystem inode are collected into one `AliasGroup`.
/// Only groups with ≥2 `AliasGroup` entries are returned.
pub fn crawl(
    dirs: &[PathBuf],
    extensions: &[String],
    min_size: u64,
) -> HashMap<u64, Vec<AliasGroup>> {
    let mut by_size: HashMap<u64, Vec<AliasGroup>> = HashMap::new();
    // (size, FileId) → index into by_size[size], for collapsing aliases.
    let mut inode_index: HashMap<(u64, FileId), usize> = HashMap::new();

    for dir in dirs {
        for entry in WalkDir::new(dir).follow_links(false) {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("warning: {e}");
                    continue;
                }
            };

            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();

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

            let size = metadata.len();
            if size < min_size {
                continue;
            }

            let id = file_id(path, &metadata);
            let path_buf = path.to_path_buf();

            match id {
                Some(id) => {
                    let key = (size, id);
                    if let Some(&idx) = inode_index.get(&key) {
                        // Already seen this inode — append as alias.
                        by_size.get_mut(&size).unwrap()[idx].paths.push(path_buf);
                    } else {
                        let groups = by_size.entry(size).or_default();
                        inode_index.insert(key, groups.len());
                        groups.push(AliasGroup::new(path_buf));
                    }
                }
                None => {
                    // No inode info — treat as unique (conservative).
                    by_size.entry(size).or_default().push(AliasGroup::new(path_buf));
                }
            }
        }
    }

    // Discard size groups with fewer than two distinct inodes.
    by_size.retain(|_, groups| groups.len() > 1);
    by_size
}

fn has_matching_extension(path: &Path, extensions: &[String]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| extensions.contains(&e.to_lowercase()))
        .unwrap_or(false)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::*;
    #[cfg(unix)]
    use std::fs;

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
