## Context

The current output loop in `main.rs` assigns a `keep` label to the first path in each content-duplicate group and a `dupe` label to all others. This ordering is arbitrary (walkdir traversal order) and has no semantic meaning to the tool. A downstream GUI or script must currently parse and then discard these labels before presenting a genuine choice to the user. The inode-aliasing change has already introduced `AliasGroup` as the unit of output; labels now apply to AliasGroups, not raw paths, making the inconsistency more visible.

## Goals / Non-Goals

**Goals:**
- Remove `keep` and `dupe` prefixes from stdout entirely.
- Retain the `# <bytes>` group header and the `link:` alias prefix unchanged.
- Update the `stderr` summary to be accurate without the keep/dupe framing.
- Keep the change scoped entirely to `main.rs` output logic.

**Non-Goals:**
- Changing the order in which groups or paths are printed.
- Adding new output modes (JSON, CSV, etc.).
- Changing any behaviour in `crawl.rs` or `dedup.rs`.

## Decisions

### D1 — Print paths with consistent two-space indent, no label

**Decision:** Each group member (first path of an AliasGroup) is printed as `  <path>` with two leading spaces. Alias paths remain `  link: <path>`. The only line without indentation is the `# <bytes>` header.

**Rationale:** Two-space indent preserves the visual grouping that `keep`/`dupe` provided without encoding any policy. Parsers can identify members as any indented line that does not start with `link:`.

**Alternative considered:** No indent, bare paths. Rejected — makes group boundaries ambiguous when consecutive groups have different sizes but the header could be confused with a path on some filesystems.

### D2 — Summary counts groups, not removed files

**Decision:** The `stderr` summary line changes from:
```
N duplicate file(s) found, wasting X.X MB
```
to:
```
N duplicate group(s) found, X.X MB of duplicated content
```

**Rationale:** "Duplicate files found" implied a count of files to delete, which requires choosing a keep candidate — exactly the policy we are removing. "Duplicate groups" counts the problem units (sets of identical content) and "duplicated content" correctly describes the space occupied by redundant copies without prescribing which to remove.

The wasted-bytes calculation is unchanged: `(AliasGroup count − 1) × size` per content group.

## Risks / Trade-offs

**[Risk] Breaking change for existing consumers.**
→ Documented as BREAKING in the proposal. No mitigation within this change — callers must adapt. The new format is simpler to parse (every indented non-`link:` line is a group member).

**[Risk] Loss of a "safe default" for scripted deletion.**
→ Accepted by design. Scripts that delete dupes must now implement their own keep-selection policy (e.g. shortest path, oldest mtime). This is strictly more correct than relying on traversal order.

## Migration Plan

Single-binary tool with no persistent state. Deploy = ship new binary. Existing callers parsing `keep`/`dupe` tokens must be updated before upgrading; no in-place migration is possible.

## Open Questions

_(none — the change is fully specified)_
