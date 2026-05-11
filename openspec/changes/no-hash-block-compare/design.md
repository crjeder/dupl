# Design: No-Hash Block-Compare Engine

## Starting point: why remove BLAKE3?

The fast path (`find_duplicates_fast`) was introduced to reduce peak memory:
the original divide-and-conquer path stored raw 64 KB blocks as `HashMap`
keys, which scaled badly with large groups of large files.  BLAKE3 was the
fix — 32-byte digests cut key size by ~2000×.

That fix was correct.  But BLAKE3 brings a cryptographic primitive into a
workload that needs none of its security properties.  The real requirement is
much weaker:

> Identical files must produce the same key.  Different files must eventually
> be detected as different.

That's it.  No collision resistance beyond "rare enough not to matter."  No
keyed hashing.  No secret inputs.

## Why not just swap BLAKE3 for xxHash?

A non-cryptographic hash (xxHash3, wyhash, FxHash) would satisfy the
requirement and run 3–4× faster.  But a subtler question emerged: **is a hash
function needed at all?**

The 64 KB raw-key problem was a *memory* problem, not an algorithm problem.
With a hash you trade accuracy for size.  But the first two bytes of a block
are already a 16-bit fingerprint with zero CPU cost.  Index 65 536 buckets
directly by `u16::from_le_bytes([block[0], block[1]])` and use `memcmp` to
resolve collisions within each bucket.  Correctness is guaranteed by `memcmp`;
no hash function is involved.

## When does sequential comparison beat hashing?

Before settling on the bucket design, we worked through the crossover point:

| Files diverge at | memcmp cost (64 KB block) | Hash beats compare at N > |
|------------------|--------------------------|--------------------------|
| End (identical)  | ~2 000 cycles            | ~11                      |
| 10% through      | ~200 cycles              | ~115                     |
| 1% through       | ~20 cycles               | ~1 150                   |
| First 32 bytes   | ~1 cycle                 | ~23 000                  |

BLAKE3 on the same block costs ~23 000 cycles.  For typical non-duplicate
files (different headers, different magic bytes), `memcmp` exits after a
handful of SIMD comparisons.  The crossover is much higher than intuition
suggests: for most real workloads, sequential comparison dominates up to
hundreds of pairs.

This motivated the two-tier design: a block-pass loop for large groups, and
a direct streaming comparison for groups small enough that the pass-loop
overhead outweighs the savings.

## The two-byte bucket index: a direct-addressed filter

Rather than a hash map, the partition step uses a flat array of 65 536 buckets
indexed by the first two bytes of each block.  Each bucket contains a small
`Vec` of `Candidate` entries; within a bucket, candidates are distinguished by
full `memcmp` of the reference block.

```
block arrives from file F:
  idx = u16::from_le_bytes([block[0], block[1]])
  scan buckets[idx] for a candidate whose reference_block == block
    found  → push F into candidate.files
    not found → push new Candidate { reference_block: block.clone(), files: [F] }
```

This is depth-first: the reference block for each candidate stays in L2
cache for the entire inner scan.  Breadth-first ordering (as a hash table
would impose) would evict it between comparisons.

### Why not the XOR trick?

An early draft XOR'd bytes [0]^[4] and [1]^[5] to spread magic-byte files
(ZIP `PK`, JPEG `FF D8`, etc.) across more buckets.  This was rejected:

The XOR is applied to **the same file's own bytes** to compute the index.
Identical files always produce the same XOR, so correctness is preserved.
But the intent (break magic-byte clustering) fails: all ZIP files with the
same first block would still hash to the same bucket after XOR, because they
all have the same bytes.  The clustering comes from shared content, not from
the raw bytes overflowing a 16-bit space.

**Decision:** use raw `[block[0], block[1]]` as the index.  Files sharing
magic bytes will cluster in one bucket.  The inner `memcmp` loop handles this
correctly and exits early for non-duplicates.  The clustering cost at pass 0
is the accepted price of a zero-cost index.

## Two-phase discipline: I/O then CPU

Reading and comparing are separated into explicit phases per pass:

**Phase 1 — I/O** reads block[pass] for every surviving file in ascending
`physical_block` order (fiemap-sorted).  This minimises seek distance on
spinning HDDs.  The survivor set shrinks each pass — the sort and read window
shrinks with it.

**Phase 2 — CPU** partitions the in-RAM blocks depth-first using the bucket
structure above.  Once a block is in RAM, fetching it costs ~4 ns (L3) vs
~100 µs (SSD): 25 000× cheaper.  The ordering of the comparison loop is
irrelevant to I/O cost, so it can be chosen for cache efficiency.

Never mix the phases: don't read the next file while comparing the current
one.  Batch reads into Phase 1, do all comparisons in Phase 2.

## Early-exit compounding

Files are grouped by size before reaching this engine (done upstream).
Within a size group:

```
Pass 0: N files → most non-duplicates eject (different headers/magic bytes)
Pass 1: k₁ ≤ N → further splits on second block
Pass 2: k₂ ≤ k₁ → ...
```

For a typical directory of mixed files, 90%+ of non-duplicates are rejected
at pass 0.  Total I/O approaches the theoretical minimum:

- One first-block read per non-duplicate file
- One full read per confirmed duplicate

## Small-group threshold and compare_n

The block-pass loop has fixed overhead per pass: building the survivor set,
filtering the read list, opening and seeking files, allocating buckets.  For
small groups, this overhead exceeds the cost of streaming the files directly.

**Decision:** when a candidate group drops to ≤ `small_group_threshold`
(default 32) during any pass, hand it off to `compare_n` immediately.

`compare_n` is an extension of the existing `compare_two`:
- Sorts files by inode before opening (approximates physical order)
- Seeks to `start_block * block_size` (skipping blocks already confirmed equal)
- Reads one block per file per round into a `HashMap<Vec<u8>, Vec<FileReader>>`
- Recursively splits groups by block equality
- Groups that reach EOF simultaneously are confirmed identical

Using raw block bytes as `HashMap` keys is fine here: N ≤ 32 means at most
32 × 64 KB = 2 MB of key data total, and most groups shrink faster than that.

### Why `start_block` matters

When `find_duplicates_blockwise` hands a group to `compare_n` at pass P, the
files have already been confirmed equal on blocks 0..(P).  `compare_n` skips
those blocks by seeking to `P+1 * block_size`.  This avoids re-reading
(and re-confirming) data that was already verified by the block-pass loop.

For groups that are handed off at pass 0, `start_block = 1`.  For groups that
survive many passes before dropping below threshold, the seek offset is larger
but still correct.

### Sub-group handling

When `compare_n` compares a block and some files match the reference while
others do not, the diverging files are **not discarded** — they may be
duplicates of each other.  They form a new candidate group and are recursed
on from the same file position (no rewind).

Because N ≤ 32, recursion depth is at most log₂(32) = 5 levels, and the
raw-key `HashMap` stays small throughout.

## What happens to groups that survive all passes

A group remaining in `pending_groups` after pass `num_passes - 1` has been
confirmed identical on every block.  It is added to results directly — no
further comparison needed.

Groups that started small (≤ threshold) bypass the block-pass loop entirely:
`find_duplicates_blockwise` calls `compare_n(groups, block_size, 0)` and
returns immediately.

## Memory profile

| Object | Size |
|--------|------|
| `reference_block` per distinct content | 1 × `block_size` (e.g. 64 KB) |
| `files` list per candidate | N × `PathBuf` (small) |
| `block_cache` per pass | survivors × `block_size` (shrinks) |
| `compare_n` keys | ≤ 32 × `block_size` |

All allocations are scoped to a single size group and dropped before the
next group is processed.  Memory is not proportional to total file count,
only to the survivors within the current pass.

## Interface summary

```rust
// dupl.rs — new engine
pub fn find_duplicates_blockwise(
    groups: Vec<AliasGroup>,
    read_list: Vec<ReadListEntry>,
    block_size: usize,
    small_group_threshold: usize,  // default: 32
    file_size: u64,
) -> Vec<Vec<AliasGroup>>

// two_file.rs — generalised from compare_two
pub fn compare_n(
    candidates: Vec<AliasGroup>,
    block_size: usize,
    start_block: usize,
) -> Vec<Vec<AliasGroup>>
```

`compare_two` (two-file fast path, unchanged) remains the entry point for
exactly two-file size groups.  `find_duplicates_blockwise` is called for
3+ file groups.  Both are invoked from `main.rs`; neither uses any hash
function.

## What was removed

- `hash_block` — no longer needed
- `find_duplicates_fast` — replaced by `find_duplicates_blockwise`
- `blake3` crate — removed from `Cargo.toml`
