<picture>
    <img alt="dupl wipe" src="dupl-logo.png" width="320" height="192">
</picture>

# dupl (wipe)

A fast, hash-free file dupllication tool for photo collections and general file trees.

`dupl` finds identical files by comparing them block-by-block, stopping at the first
difference. No hashes are ever computed — this is faster than full-file hashing for
typical photo libraries where JPEG files diverge within the first 64 KiB due to differing
EXIF metadata.

## Getting Started

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (edition 2024, stable ≥ 1.85)

### Installing

Clone the repository and build a release binary:

```bash
git clone https://github.com/crjeder/dupl.git
cd dupl
cargo build --release
# binary is at target/release/dupl
```

Or install directly with Cargo:

```bash
cargo install --path .
```

## Usage

```
dupl [OPTIONS] <DIR> [DIR...]
```

Scan one or more directories for duplicate files:

```bash
dupl ~/Photos
```

Restrict the scan to specific file extensions (case-insensitive):

```bash
dupl ~/Photos -e jpg,png,heic
```

Skip files below a minimum size:

```bash
dupl ~/Photos --min-size 1048576   # ignore files smaller than 1 MiB
```

### Output format

Duplicate groups are written to **stdout** in a machine-readable format — one group per
`# <bytes>` header, with `keep` and `dupe` lines for each path. Hard-linked aliases within
a group are prefixed with `link:`. Progress, warnings, and a summary are written to
**stderr**.

Example output:

```
# 4823142
keep  /photos/2023/IMG_0042.jpg
dupe  /photos/backup/IMG_0042.jpg
link: /photos/raw/IMG_0042_alias.jpg
```

The summary line reports the number of duplicate groups found and the total reclaimable
space in MiB.

## Running the Tests

```bash
cargo test
```

Lint:

```bash
cargo clippy
```

## How It Works

The pipeline has three stages:

```
crawl.rs  →  dupl.rs  →  main.rs (output)
```

1. **`crawl.rs`** — walks directories with `walkdir`, groups files by exact byte size.
   Size-singletons are dropped immediately. Symlinks are never followed. Input roots are
   normalised before the walk: if one root is reachable as a descendant of another (or via
   a bind mount), the redundant root is removed to prevent double-counting.

2. **`dupl.rs`** — processes one size group at a time using recursive block-splitting.
   All file handles in a group are opened once and kept open across recursion. Each
   recursion reads the next 64 KiB block, re-groups files by content, eliminates
   singletons, and recurses. A group that reaches EOF without diverging is a set of true
   duplicates. Files sharing the same `(device, inode)` pair are treated as a single
   logical file (`AliasGroup`) — hard links are never reported as duplicates of each other.

3. **`main.rs`** — CLI via `clap` derive. Formats and emits the results.

### Design decisions

- **No stored hashes.** Pre-computed hashes are invalidated by copy/move/rename, which is
  exactly how photo duplicates are created. Attaching a cache to the file (e.g. extended
  attributes) or avoiding caching entirely are the only correct strategies.
- **Exact byte equality only.** This tool does not perform perceptual or content-aware
  image comparison.
- **O(n) worst-case disk reads**, sub-linear in practice for photo collections.

## Built With

- [Rust](https://www.rust-lang.org/) — systems language
- [walkdir](https://crates.io/crates/walkdir) — recursive directory traversal
- [clap](https://crates.io/crates/clap) — command-line argument parsing

## Versioning

This project uses [Semantic Versioning](http://semver.org/). For available versions, see
the [CHANGELOG](CHANGELOG.md).

## License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Inspired by the analysis in
  [Gedanken zur Datei-dupllizierung](https://gist.github.com/crjeder/6b9b198562379370887887edcdc746d1) —
  a detailed critique of hash-based dupllication and the case for direct block comparison
- [fclones](https://github.com/pkolaczk/fclones),
  [rdfind](https://rdfind.pauldreik.se/), and
  [dupe-krill](https://github.com/kornelski/dupe-krill) — prior art examined during design
- [PurpleBooth/a-good-readme-template](https://github.com/PurpleBooth/a-good-readme-template)
