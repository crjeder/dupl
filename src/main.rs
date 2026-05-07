//! # dupl — duplicate-file finder
//!
//! This is the program entry point. It ties together two modules:
//!
//! * [`crawl`] — walks directories, groups files by size, and collapses hard
//!   links into a single logical entry.
//! * [`dupl`] — compares same-size files block-by-block to confirm they are
//!   byte-for-byte identical, without reading entire files into memory at once.
//!
//! ## How the program works
//!
//! 1. Parse command-line arguments with `clap`.
//! 2. Walk every requested directory and build a map of `file_size → [groups]`.
//!    Files that share the same filesystem inode (hard links) are merged into one
//!    `AliasGroup` so they are never reported as duplicates of each other.
//! 3. For every size bucket that has ≥2 candidate groups, compare the files
//!    block-by-block.  Only files that agree on *every* block are duplicates.
//! 4. Print the results and a summary to stdout/stderr.

mod crawl;
mod dupl;

use clap::Parser;
use std::path::PathBuf;

/// Command-line interface definition.
///
/// `clap` reads the `#[arg(...)]` attributes and automatically generates
/// `--help` text and argument parsing from them.  The `#[derive(Parser)]`
/// attribute tells `clap` to implement the `Parser` trait for this struct.
#[derive(Parser)]
#[command(name = "dupl", about = "Find duplicate files in a photo collection")]
struct Cli {
    /// Directories to scan.
    ///
    /// At least one directory is required. Provide multiple paths separated by
    /// spaces to scan several locations in one pass.
    #[arg(required = true)]
    dirs: Vec<PathBuf>,

    /// File extensions to include, comma-separated (e.g. jpg,png,heic).
    /// If omitted, all files are scanned.
    ///
    /// Matching is case-insensitive; leading dots are stripped automatically.
    #[arg(short, long, value_delimiter = ',')]
    extensions: Vec<String>,

    /// Minimum file size in bytes to consider.
    ///
    /// Files smaller than this threshold are ignored entirely.  The default of
    /// 1 byte excludes empty files (which are trivially "equal" and usually
    /// uninteresting).
    #[arg(long, default_value = "1")]
    min_size: u64,

    /// Block size in bytes for file comparison (default: 65536).
    ///
    /// During comparison, files are read in chunks of this size.  A larger
    /// block size means fewer read calls but more memory used at once.
    /// 64 KiB is a good default for most systems.
    #[arg(long, default_value = "65536")]
    block_size: usize,
}

fn main() {
    // `Cli::parse()` reads `std::env::args()`, validates the input, and
    // returns a populated `Cli` struct — or prints an error and exits.
    let cli = Cli::parse();

    // Normalise extensions: lowercase and strip any leading dot so that
    // "JPG", ".jpg", and "jpg" all match the same files.
    // `.iter()` borrows each element; `.map(|e| ...)` transforms it;
    // `.collect()` gathers the results into a new `Vec<String>`.
    let extensions: Vec<String> = cli
        .extensions
        .iter()
        .map(|e| e.to_lowercase().trim_start_matches('.').to_string())
        .collect();

    // Remove redundant input roots (e.g. the same directory listed twice, or a
    // child of another listed root) before crawling.
    let dirs = crawl::normalize_inputs(cli.dirs);

    // Walk all directories and group files by their exact byte-size.
    // The returned map only contains size buckets with ≥2 distinct inodes,
    // so every bucket is a potential set of duplicates.
    let size_groups = crawl::crawl(&dirs, &extensions, cli.min_size);

    eprintln!(
        "Scanning {} size group(s) with ≥2 candidates...",
        size_groups.len()
    );

    let mut total_dup_groups: usize = 0;
    let mut wasted_bytes: u64 = 0;

    // Iterate over every size bucket.  `size` is the shared file size;
    // `alias_groups` is the list of candidate files at that size.
    for (size, alias_groups) in &size_groups {
        // `find_duplicates` returns only groups that are truly identical.
        // Each inner `Vec<AliasGroup>` is one set of duplicate files.
        for content_group in dupl::find_duplicates(alias_groups.clone(), cli.block_size) {
            // Print one header line per duplicate group, then each file path.
            println!("# {} bytes", size);
            for alias_group in &content_group {
                // The first path is the "representative" (canonical name).
                println!("  {}", alias_group.representative().display());
                // Any additional paths are hard-link aliases of the same inode.
                for alias in &alias_group.paths[1..] {
                    println!("  link: {}", alias.display());
                }
            }
            total_dup_groups += 1;
            // Only count bytes beyond the first copy as "wasted".
            wasted_bytes += (content_group.len() as u64 - 1) * size;
        }
    }

    eprintln!(
        "\n{} duplicate group(s) found, {:.1} MB of duplicated content",
        total_dup_groups,
        wasted_bytes as f64 / 1_048_576.0
    );
}
