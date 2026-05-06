mod crawl;
mod dupl;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "dupl", about = "Find duplicate files in a photo collection")]
struct Cli {
    /// Directories to scan
    #[arg(required = true)]
    dirs: Vec<PathBuf>,

    /// File extensions to include, comma-separated (e.g. jpg,png,heic).
    /// If omitted, all files are scanned.
    #[arg(short, long, value_delimiter = ',')]
    extensions: Vec<String>,

    /// Minimum file size in bytes to consider
    #[arg(long, default_value = "1")]
    min_size: u64,

    /// Block size in bytes for file comparison (default: 65536)
    #[arg(long, default_value = "65536")]
    block_size: usize,
}

fn main() {
    let cli = Cli::parse();

    let extensions: Vec<String> = cli
        .extensions
        .iter()
        .map(|e| e.to_lowercase().trim_start_matches('.').to_string())
        .collect();

    let dirs = crawl::normalize_inputs(cli.dirs);
    let size_groups = crawl::crawl(&dirs, &extensions, cli.min_size);

    eprintln!(
        "Scanning {} size group(s) with ≥2 candidates...",
        size_groups.len()
    );

    let mut total_dup_groups: usize = 0;
    let mut wasted_bytes: u64 = 0;

    for (size, alias_groups) in &size_groups {
        for content_group in dupl::find_duplicates(alias_groups.clone(), cli.block_size) {
            println!("# {} bytes", size);
            for alias_group in &content_group {
                println!("  {}", alias_group.representative().display());
                for alias in &alias_group.paths[1..] {
                    println!("  link: {}", alias.display());
                }
            }
            total_dup_groups += 1;
            wasted_bytes += (content_group.len() as u64 - 1) * size;
        }
    }

    eprintln!(
        "\n{} duplicate group(s) found, {:.1} MB of duplicated content",
        total_dup_groups,
        wasted_bytes as f64 / 1_048_576.0
    );
}
