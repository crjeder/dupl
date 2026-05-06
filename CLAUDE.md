# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build                          # debug build
cargo build --release                # optimized build
cargo run -- <dir> [dir...]          # run against directories
cargo run -- <dir> -e jpg,png,heic   # filter by extension
cargo test                           # run all tests
cargo clippy                         # lint
```

## Architecture

The pipeline has three stages, each in its own module:

**`crawl.rs` → `dedup.rs` → `main.rs` (output)**

### `crawl.rs`
Walks directories with `walkdir` and returns `HashMap<u64, Vec<PathBuf>>` — files grouped by byte size. Size-singletons are dropped immediately; only groups with ≥2 candidates are returned. Symlinks are not followed.

### `dedup.rs`
Takes one size group at a time and finds identical files using **recursive block-splitting** (`split_by_block`):
1. Open all files in the group (file handles stay open across recursion).
2. Read the next 64 KiB block from each file.
3. Re-group files by their block content — files that diverge become singletons and are eliminated.
4. Recurse on each sub-group of size ≥2.
5. A sub-group whose block read returns 0 bytes (EOF) has matched on every block and is reported as a duplicate set.

This is the key design invariant: **no hashes are computed anywhere**. Files are compared directly, block by block, stopping at the first difference. This satisfies the goals of O(n) worst-case disk reads, sublinear expected reads (JPEG/photo files typically diverge within the first block due to differing EXIF metadata), and O(n log n) comparisons.

### `main.rs`
CLI via `clap` derive. Progress/stats go to `stderr`; the machine-readable duplicate list goes to `stdout` (one group per `# <bytes>` header, `keep`/`dupe` lines).

## Design constraints

- **No stored hashes.** Pre-computed hashes are invalidated by copy/move/rename, which is how most photo duplicates are created. Any caching scheme must attach to the file itself (e.g. extended attributes) or be avoided entirely.
- **Exact byte equality only.** This tool does not do perceptual/content-aware image comparison.
- **Hardlinks/inodes** are not yet deduplicated — two paths to the same inode will currently be reported as duplicates.

# Code Quality

## Before writing new code
- Use fossil_inspect to check blast radius before refactoring
- Search the codebase for existing similar functions before writing new ones
- Prefer extending existing abstractions over creating parallel ones

## After writing code
- Run `ruff check --fix` and `ruff format` on changed Python files
- Use fossil detect_clones if you added a function similar to existing code
- Keep functions focused: single responsibility, ideally <30 lines
- All public functions need type hints and a one-line docstring

## Architecture (sentrux)
- Call sentrux scan() at session start to establish baseline
- Call sentrux session_end() when finishing — if quality degrades, fix the bottleneck before stopping
- If sentrux reports modularity as bottleneck: split files, reduce cross-module imports
- Never leave a session with a lower quality signal than you started with

## Python
- Use type hints on all functions and public methods
- Keep functions focused: one clear responsibility, ideally <30 lines
- No duplicate logic — check for existing utilities before writing new ones
- Use descriptive names; avoid abbreviations except well-known ones (e.g. `idx`, `cfg`)
- Add docstrings to all public functions (one-line minimum)
- Prefer composition over copy-paste; extract shared logic into helpers
- Run `ruff check` and `ruff format` after editing Python files

## General
- Prefer explicit imports over wildcard imports
- Keep modules small and cohesive

## Workflow
- After completing a task, use fossil-mcp to scan for dead code and clones
- Fix any high-confidence findings before finishing

<!-- CODEGRAPH_START -->
## CodeGraph

CodeGraph builds a semantic knowledge graph of codebases for faster, smarter code exploration.

### If `.codegraph/` exists in the project

**NEVER call `codegraph_explore` or `codegraph_context` directly in the main session.** These tools return large amounts of source code that fills up main session context. Instead, ALWAYS spawn an Explore agent for any exploration question (e.g., "how does X work?", "explain the Y system", "where is Z implemented?").

**When spawning Explore agents**, include this instruction in the prompt:

> This project has CodeGraph initialized (.codegraph/ exists). Use `codegraph_explore` as your PRIMARY tool — it returns full source code sections from all relevant files in one call.
>
> **Rules:**
> 1. Follow the explore call budget in the `codegraph_explore` tool description — it scales automatically based on project size.
> 2. Do NOT re-read files that codegraph_explore already returned source code for. The source sections are complete and authoritative.
> 3. Only fall back to grep/glob/read for files listed under "Additional relevant files" if you need more detail, or if codegraph returned no results.

**The main session may only use these lightweight tools directly** (for targeted lookups before making edits, not for exploration):

| Tool | Use For |
|------|---------|
| `codegraph_search` | Find symbols by name |
| `codegraph_callers` / `codegraph_callees` | Trace call flow |
| `codegraph_impact` | Check what's affected before editing |
| `codegraph_node` | Get a single symbol's details |

### If `.codegraph/` does NOT exist

At the start of a session, ask the user if they'd like to initialize CodeGraph:

"I notice this project doesn't have CodeGraph initialized. Would you like me to run `codegraph init -i` to build a code knowledge graph?"
<!-- CODEGRAPH_END -->
