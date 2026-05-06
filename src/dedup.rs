use crate::crawl::AliasGroup;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

struct FileReader {
    group: AliasGroup,
    file: File,
}

/// Find groups of identical files among `groups`. All groups must share the same size.
/// Returns only groups of size ≥2 (actual duplicates).
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

/// Recursively split `readers` into groups by reading one block at a time.
/// Files that diverge on a block are separated and stop being compared.
/// Files still grouped when a block returns empty are byte-for-byte identical.
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
            // All files returned EOF — they are byte-for-byte identical.
            result.push(group.into_iter().map(|r| r.group).collect());
        } else {
            result.extend(split_by_block(group, block_size));
        }
    }

    result
}
