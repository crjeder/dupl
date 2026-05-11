//! # dupl — duplicate-file finder
//!
//! This is the program entry point. It ties together the crawl, comparison,
//! and (optionally) the optimised fast-pipeline modules.
//!
//! ## How the program works (legacy path)
//!
//! 1. Parse command-line arguments with `clap`.
//! 2. Walk every requested directory and build a map of `file_size → [groups]`.
//!    Files that share the same filesystem inode (hard links) are merged into one
//!    `AliasGroup` so they are never reported as duplicates of each other.
//! 3. For every size bucket that has ≥2 candidate groups, compare the files
//!    block-by-block.  Only files that agree on *every* block are duplicates.
//! 4. Print the results and a summary to stdout/stderr.
//!
//! ## How the program works (default fast path)
//!
//! Steps 1–2 use `crawl::crawl_fast` which defers the first file at each size
//! until a second confirms the group, avoiding fiemap calls for singletons.
//!
//! Step 3 builds a read-list sorted by physical disk block via `fiemap` ioctl,
//! then dispatches:
//! * Two-file groups       → `two_file::compare_two` (direct streaming compare).
//! * Groups ≤ threshold    → `two_file::compare_n` (N-way streaming compare).
//! * Larger groups         → `dupl::find_duplicates_blockwise` (block-pass loop
//!                           with 2-byte bucket index and raw memcmp; no hash).
//!
//! The legacy divide-and-conquer path is still available via `--legacy`.

mod crawl;
mod dupl;
mod fiemap;
mod readlist;
mod two_file;

use clap::Parser;
use readlist::{SMALL_GROUP_LARGE_LIMIT, SMALL_GROUP_SMALL_LIMIT};
use std::path::PathBuf;

/// Upper file-size bound for "medium" files in the five-pass sort.
/// Files larger than this go to passes 4/5.  512 KiB is a reasonable default
/// for spinning-disk workloads; tune with --round1-max if needed.
const DEFAULT_ROUND1_MAX: u64 = 524_288; // 512 KiB

#[derive(Parser)]
#[command(name = "dupl", about = "Find duplicate files in a directory tree")]
struct Cli {
    /// Directories to scan.
    #[arg(required = true)]
    dirs: Vec<PathBuf>,

    /// File extensions to include, comma-separated (e.g. jpg,png,heic).
    /// If omitted, all files are scanned.
    #[arg(short, long, value_delimiter = ',')]
    extensions: Vec<String>,

    /// Minimum file size in bytes to consider.
    #[arg(long, default_value = "1")]
    min_size: u64,

    /// Block size in bytes for file comparison (default: 65536 = 64 KiB).
    #[arg(long, default_value = "65536")]
    block_size: usize,

    /// Use the legacy divide-and-conquer pipeline (raw block bytes as HashMap
    /// keys).  More memory-intensive than the default fast path but retained as
    /// a correctness reference.  Will be removed once the fast path is confirmed
    /// correct on the target hardware.
    #[arg(long)]
    legacy: bool,

    /// Upper file-size threshold (bytes) separating medium from large files
    /// in the five-pass read-list sort.  Only relevant without --legacy.
    #[arg(long, default_value_t = DEFAULT_ROUND1_MAX)]
    round1_max: u64,
}

fn main() {
    let cli = Cli::parse();

    let extensions: Vec<String> = cli
        .extensions
        .iter()
        .map(|e| e.to_lowercase().trim_start_matches('.').to_string())
        .collect();

    let dirs = crawl::normalize_inputs(cli.dirs);

    if cli.legacy {
        run_legacy(&dirs, &extensions, cli.min_size, cli.block_size);
    } else {
        run_fast(&dirs, &extensions, cli.min_size, cli.block_size, cli.round1_max);
    }
}

// ── Legacy pipeline ───────────────────────────────────────────────────────────

fn run_legacy(dirs: &[PathBuf], extensions: &[String], min_size: u64, block_size: usize) {
    let size_groups = crawl::crawl(dirs, extensions, min_size);

    eprintln!(
        "Scanning {} size group(s) with ≥2 candidates...",
        size_groups.len()
    );

    let (total, wasted) = process_groups_legacy(size_groups, block_size);

    eprintln!(
        "\n{} duplicate group(s) found, {:.1} MB of duplicated content",
        total,
        wasted as f64 / 1_048_576.0
    );
}

fn process_groups_legacy(
    size_groups: std::collections::HashMap<u64, Vec<crawl::AliasGroup>>,
    block_size: usize,
) -> (usize, u64) {
    let mut total_dup_groups: usize = 0;
    let mut wasted_bytes: u64 = 0;

    for (size, alias_groups) in &size_groups {
        for content_group in dupl::find_duplicates(alias_groups.clone(), block_size) {
            print_group(*size, &content_group);
            total_dup_groups += 1;
            wasted_bytes += (content_group.len() as u64 - 1) * size;
        }
    }

    (total_dup_groups, wasted_bytes)
}

// ── Fast pipeline ─────────────────────────────────────────────────────────────

fn run_fast(
    dirs: &[PathBuf],
    extensions: &[String],
    min_size: u64,
    block_size: usize,
    round1_max: u64,
) {
    // Deferred first-file: singletons never enter by_size → no fiemap for them.
    let size_groups = crawl::crawl_fast(dirs, extensions, min_size);

    eprintln!(
        "[fast] {} size group(s) with ≥2 candidates",
        size_groups.len()
    );

    // Collect groups into a Vec for the read-list builder.
    let mut group_pairs: Vec<(u64, Vec<crawl::AliasGroup>)> = size_groups.into_iter().collect();
    // Process in ascending size order (matches pass-1 first, small files done
    // quickly and their memory freed before large-file passes).
    group_pairs.sort_unstable_by_key(|(size, _)| *size);

    // Build and sort the read list across ALL groups, then process sequentially.
    let mut read_list = readlist::build_read_list(&group_pairs, block_size as u64);
    readlist::sort_read_list(
        &mut read_list,
        block_size as u64,
        round1_max,
        SMALL_GROUP_SMALL_LIMIT,
        SMALL_GROUP_LARGE_LIMIT,
    );

    let mut total_dup_groups: usize = 0;
    let mut wasted_bytes: u64 = 0;

    for (size, alias_groups) in group_pairs {
        // Extract read-list entries that belong to this size group.
        let group_paths: std::collections::HashSet<_> = alias_groups
            .iter()
            .map(|ag| ag.representative().clone())
            .collect();
        let group_rl: Vec<readlist::ReadListEntry> = read_list
            .iter()
            .filter(|e| group_paths.contains(&e.path))
            .cloned()
            .collect();

        let dup_sets = if alias_groups.len() == 2 {
            // Two-file fast-path: direct streaming compare.
            let identical = two_file::compare_two(&alias_groups[0], &alias_groups[1], block_size);
            if identical {
                vec![alias_groups]
            } else {
                vec![]
            }
        } else {
            // Block-compare engine for 3+ file groups (no hash).
            dupl::find_duplicates_blockwise(alias_groups, group_rl, block_size, 32, size)
        };

        for content_group in dup_sets {
            print_group(size, &content_group);
            total_dup_groups += 1;
            wasted_bytes += (content_group.len() as u64 - 1) * size;
        }
    }

    eprintln!(
        "\n[fast] {} duplicate group(s) found, {:.1} MB of duplicated content",
        total_dup_groups,
        wasted_bytes as f64 / 1_048_576.0
    );
}

// ── Output ────────────────────────────────────────────────────────────────────

fn print_group(size: u64, group: &[crawl::AliasGroup]) {
    println!("# {} bytes", size);
    for ag in group {
        println!("  {}", ag.representative().display());
        for alias in &ag.paths[1..] {
            println!("  link: {}", alias.display());
        }
    }
}
