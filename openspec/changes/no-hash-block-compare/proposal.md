# Proposal: No-Hash Block-Compare Engine

## Summary

Replace the BLAKE3-based fast path (`find_duplicates_fast`) with a hash-free
divide-and-conquer engine that uses raw block bytes with a two-byte direct
array index, fiemap-sorted batched I/O, and depth-first in-memory comparison.
Large groups are partitioned block-by-block; small groups (≤ threshold) are
handed off to direct N-way byte comparison.

Remove the `blake3` dependency entirely.

## Motivation

BLAKE3 was introduced to cut HashMap key size from 64 KB (raw blocks) down to
32 bytes, reducing peak memory by ~2000×.  That was the right fix, but it
introduced an unnecessary cryptographic primitive into a workload that needs
none of its security properties.

The real requirements for the key are:

1. Small enough to fit many keys in cache.
2. Correct: identical blocks must map to the same bucket; different blocks must
   eventually be detected as different.

Both are satisfied by **the first two bytes of a block used as a direct array
index** into 65 536 buckets — zero CPU cost, correctness guaranteed by the
memcmp step that follows.

## Design

### Core data structure

```
buckets: [Vec<Candidate>; 65_536]   // indexed by u16::from_le_bytes(block[0..2])

struct Candidate {
    reference_block: Box<[u8]>,   // one block_size buffer — the reference content
    files: Vec<FileRef>,          // files confirmed identical up to this block
}
```

### Algorithm

```
SMALL_GROUP_THRESHOLD: usize = 32   // configurable, default 32

for each size group (already grouped upstream):

  pass = 0
  survivors = all files in group

  while survivors.len() > SMALL_GROUP_THRESHOLD
    and pass < ceil(file_size / block_size):

    // Phase 1 — I/O: read block[pass] for each survivor in fiemap order
    read_blocks_fiemap_sorted(survivors, pass) → block_cache

    // Phase 2 — CPU: partition depth-first using block_cache
    buckets = fresh [65_536 empty vecs]
    for each (file, block) in block_cache:
      idx = u16::from_le_bytes([block[0], block[1]])
      for candidate in buckets[idx]:
        if memcmp(block, candidate.reference_block) == 0:
          candidate.files.push(file)
          goto next_file
      buckets[idx].push(Candidate { reference_block: block, files: [file] })

    // Collect surviving groups (>= 2 files per candidate)
    // Each candidate whose files.len() >= 2 is a sub-group to continue with
    candidate_groups = buckets.flatten().filter(|c| c.files.len() >= 2)

    // Groups that dropped to <= SMALL_GROUP_THRESHOLD hand off immediately
    for group in candidate_groups:
      if group.files.len() <= SMALL_GROUP_THRESHOLD:
        results.extend(direct_compare(group.files))   // N-way byte comparison
      else:
        survivors = group.files   // continue block-pass loop

    pass++

  // survivors still above threshold after all blocks → confirmed duplicates
  report survivors as confirmed duplicate groups
```

### Two-phase discipline

**Phase 1 (I/O):** reads blocks in fiemap order (physical location on disk),
minimising seek distance.  The survivor set shrinks each pass, so each
subsequent sort and read is cheaper.

**Phase 2 (CPU):** compares blocks depth-first from the in-RAM cache.
The 64 KB `reference_block` stays hot in L2 for the entire inner loop over
a bucket's candidates.  Breadth-first would evict it between passes.

### Early-exit compounding

```
Pass 0:  N files enter  →  most non-duplicates eject here (different headers)
Pass 1:  k₁ ≤ N remain →  further splits
Pass 2:  k₂ ≤ k₁ remain
...
groups of ≤ 32 files hand off to direct_compare at any pass
```

For typical workloads, the majority of non-duplicates are ejected at pass 0
(different file headers).  Total I/O approaches the minimum: one full read of
each confirmed duplicate, plus one first-block read of each non-duplicate.

### Hot-bucket behaviour on magic bytes

Files sharing a magic-byte prefix (ZIP = `PK`, JPEG = `FF D8`, PDF = `%P`)
will cluster in the same bucket.  Within that bucket the inner loop runs
memcmp against each prior candidate.  This is accepted: memcmp exits on the
first differing byte (usually early for non-duplicates), and the algorithm
remains correct.  No mitigation is applied; the clustering cost at pass 0 is
the price paid for a zero-cost index.

### Threshold rationale

Direct N-way comparison (`direct_compare`) with early-exit `memcmp` beats the
block-pass overhead when the group is small.  The crossover depends on how
early files diverge:

| Files diverge at | memcmp cost/compare | block-pass beats compare at N > |
|------------------|--------------------|---------------------------------|
| End (identical)  | ~2 000 cycles      | ~11                             |
| 10% through      | ~200 cycles        | ~115                            |
| First block      | ~1 cycle           | ~23 000                         |

Default threshold of 32 is conservative (safe on the "diverge early" side).
Users scanning many same-format files (e.g., video files with identical
headers) may benefit from a lower threshold.

## Interface changes

### `dupl.rs` — new engine

`find_duplicates_fast` is replaced by `find_duplicates_blockwise`.  Signature
unchanged from the caller's perspective, with one additional parameter:

```rust
pub fn find_duplicates_blockwise(
    groups: Vec<AliasGroup>,
    read_list: Vec<ReadListEntry>,
    block_size: usize,
    small_group_threshold: usize,   // default: 32
) -> Vec<Vec<AliasGroup>>
```

`hash_block` is removed.  The `blake3` entry in `Cargo.toml` is removed.

### `two_file.rs` → generalised to N-way comparison

`compare_two` handles exactly two files and is already optimal for that case
(two open file handles, one buffer per file, early exit on first differing
block).  It is retained unchanged.

A new function `compare_n` handles groups of 2–`small_group_threshold` files:

```rust
/// Compare N files block-by-block, returning confirmed-identical sub-groups.
///
/// Files within each group are opened in ascending-inode order.
/// Streaming: one block_size buffer per open file, no full file loaded.
///
/// Algorithm per round:
///   1. Read next block from every open file.
///   2. Use the first file's block as reference.
///   3. Partition: files whose block matches reference stay; others split off.
///   4. Split-offs become new candidate groups (recurse on next round).
///   5. Stop when a group reaches EOF — those files are confirmed duplicates.
pub fn compare_n(
    candidates: Vec<AliasGroup>,
    block_size: usize,
) -> Vec<Vec<AliasGroup>>
```

**Why not O(N²) pairwise?**  For N = 32, reference-based partitioning does at
most 31 memcmp calls per block round, each with early exit.  A group where all
files are true duplicates exits after one memcmp chain.  Pairwise would do
496 comparisons for the same N.

**Read order within `compare_n`:** files are sorted by inode before opening
(same heuristic as `compare_two`).  For small groups on spinning HDDs,
ascending-inode order approximates physical allocation order.

**Sub-group handling:** files that diverge from the reference are not simply
discarded — they may be duplicates of each other.  They are collected into a
new candidate group and processed in the same recursive round structure.
Because N ≤ 32, the recursion depth is at most log₂(32) = 5 levels.

**Caller integration:** `find_duplicates_blockwise` calls `compare_n` when a
candidate group drops to ≤ `small_group_threshold` during the block-pass loop.
`main.rs` calls `compare_two` directly for two-file size groups (unchanged).

## Trade-offs

| Property             | BLAKE3 path                  | Block-compare path               |
|----------------------|------------------------------|----------------------------------|
| CPU/block            | ~23 000 cycles (hash)        | ~2 000 cycles (memcmp, identical)|
| Memory/candidate     | 32 B (digest)                | `block_size` (reference block)   |
| False positives      | Astronomically unlikely      | Impossible (memcmp is exact)     |
| Dependencies         | `blake3`                     | none added                       |
| Hot-bucket risk      | None                         | Magic bytes (accepted)           |
| Small-group handling | Same path as large groups    | `compare_n` (streaming, N ≤ 32) |

Memory per candidate is bounded: one `Box<[u8; block_size]>` per **distinct**
block content in the current pass, not one per file.  Confirmed-duplicate
files share a single reference block.

## Out of scope

- The legacy divide-and-conquer path (`find_duplicates`) is unchanged.
- The fiemap / read-list infrastructure is unchanged.
- No new CLI flags; `--fast` continues to select the optimised path.
