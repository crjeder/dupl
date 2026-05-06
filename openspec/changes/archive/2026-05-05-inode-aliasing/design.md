## Context

The dedup pipeline currently models every file as a bare `PathBuf`. Two directory entries that share an inode (hard links, overlapping input roots, bind-mounted subtrees) are indistinguishable from two independently created copies with identical content. They flow through size grouping and block comparison, match on every block, and are emitted as content duplicates — which they are not. A user acting on this output could delete a path they intended to keep, with no data loss protection.

The fix requires threading inode identity through the pipeline from the moment a file is discovered, so that all downstream stages treat a set of aliased paths as a single logical entity.

## Goals / Non-Goals

**Goals:**
- Eliminate false duplicates caused by hard links, overlapping input directories, and bind mounts.
- Preserve full path information for every alias so callers (e.g. a GUI) can present all locations and let the user choose which to keep.
- Open exactly one file handle per physical file during block comparison.
- Degrade gracefully on platforms or filesystems where inode identity is unavailable.

**Non-Goals:**
- Reflink / copy-on-write clone detection (different inodes, shared blocks). These are genuine content duplicates from the tool's perspective.
- Perceptual or content-aware image similarity.
- Any change to the block-comparison algorithm in `dedup.rs`.
- Output format redesign beyond adding the `link:` tag (separate change).

## Decisions

### D1 — `AliasGroup` as the core data type

**Decision:** Introduce `AliasGroup { paths: Vec<PathBuf> }` to replace bare `PathBuf` throughout the pipeline. The first path is the representative used for file I/O; all paths are preserved for output.

**Rationale:** The alternative — post-processing the existing `Vec<PathBuf>` output to group by inode — requires a second stat pass over all reported duplicates and loses the inode information that was already available during the crawl. Threading `AliasGroup` forward costs nothing extra and avoids the second pass.

**Alternative considered:** Store `(inode, Vec<PathBuf>)` tuples. Rejected — the inode is only needed during crawl to build the group; carrying it through dedup and output adds noise. `AliasGroup` keeps the structure minimal.

### D2 — Inode dedup happens in `crawl.rs`, not as a separate pass

**Decision:** Maintain a `HashSet<(u64, u64)>` (device, inode) during the directory walk. When a file's inode has already been seen, append its path to the existing `AliasGroup` rather than creating a new entry.

**Rationale:** The metadata is already available from `walkdir` at zero extra I/O cost. Doing it here keeps the pipeline stages clean: `crawl` handles physical-to-logical mapping; `dedup` handles content comparison; `main` handles output.

### D3 — Pre-crawl input normalization via directory inode walk

**Decision:** Before calling `crawl`, walk the directory tree of each input root (directories only, no files) and collect directory inodes. Any input root whose inode appears as a descendant of another input root is removed from the list.

**Rationale:** Without this step, a user passing `/photos` and `/photos/2024` causes every file under `2024/` to be visited twice by `walkdir`, producing pairs of same-inode entries that D2 would correctly collapse — but wastefully, after redundant I/O. The pre-crawl step eliminates the redundant walk entirely.

**Why walk down, not up:** Walking up the parent chain from each input dir is cheaper per-directory but fails for bind mounts: the bind-mount point has the same inode as the source directory, but the source directory's inode does not appear in the mount point's ancestor chain. Walking down from the candidate parent finds the inode regardless.

### D4 — Conservative fallback when inode is unavailable

**Decision:** On Windows, `file_index()` and `volume_serial_number()` return `Option`. If either is `None` (FAT, network drive, some virtual filesystems), skip inode dedup for that file and let it through as if it were a unique inode.

**Rationale:** Failing hard would break the tool on legitimate filesystems. Silently promoting a file to "unique" is the safe direction — the worst outcome is a false duplicate in the output (status quo), not silent data loss.

No warning is emitted per file (too noisy); a single warning at startup if inode info is unavailable for any input root could be added later.

### D5 — `dedup.rs` opens one file handle per `AliasGroup`

**Decision:** `find_duplicates` receives `Vec<AliasGroup>`. For each group, it opens `File::open(&group.paths[0])`. The `FileReader` struct gains a `paths: Vec<PathBuf>` field instead of a single `path`.

**Rationale:** All aliases point to the same inode; reading from any one of them reads the same bytes. Opening multiple handles to the same inode is wasteful and unnecessary.

## Risks / Trade-offs

**[Risk] Directory walk during input normalization touches the filesystem before crawl begins.**
→ Mitigation: Walk directories only (skip files). On a typical photo library with hundreds of directories but thousands of files, this is negligible. Errors are treated as warnings; the affected root is kept (conservative).

**[Risk] Inode numbers can be reused on some filesystems after deletion.**
→ Not a risk here: both the directory walk and the file crawl happen within a single process run. No caching across invocations.

**[Risk] `walkdir` visit order is not guaranteed; which path becomes the `AliasGroup` representative depends on traversal order.**
→ Acceptable: the representative is used only for file I/O, not for output ordering. All aliases are preserved. A future change could sort paths within a group.

**[Risk] On Windows, directory junctions and symlinks interact with `follow_links(false)` in non-obvious ways.**
→ Mitigation: existing behaviour (`follow_links(false)`) is preserved. Junction traversal behaviour is inherited from `walkdir`; no change is made to traversal strategy.

## Migration Plan

No persistent state, no database schema, no external API. The change is confined to a single binary. Deploy = ship a new binary. Rollback = revert to previous binary. No migration required.

## Open Questions

- Should a warning be emitted to `stderr` when inode information is unavailable for a given file (D4)? Currently: silent fallback.
- Should alias paths within an `AliasGroup` be sorted (e.g. shortest path first) for deterministic output? Currently: traversal order.
