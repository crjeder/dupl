# Data Structures for File de-duplication
## Algorithm
```
for each file of size s:
    for each block
        read block
        if block already exists in data-structure 
            mark as duplicate
        else
            insert block in data-structure
```

90 % or more of the performance is due to the lazy read of files. the rest is a efficient data structure which fits in memory and is cache optimized.
Performance is important for the find and insert operation(s) since the standard case would be that the two files are different. The algorithm has to find the spot to instert the file and check if it's already taken by an other file (candidate for a duplicate).

For a 100 % write (probe_and_insert) workload the key factors are: no per-item allocation, minimal pointer chasing, bounded probe count, and amortised O(1) insert without restructuring. Sorted arrays are immediately disqualified — O(n) element shift per insert.

| Data-structure | Cache (0–100) | Mem @ 100 k (0–100) | Write (0–100) |
|---|---|---|---|
| Robin-Hood / SwissTable (hashbrown) | 95 | 95 | **92** |
| Cuckoo hashing (2-way / 4-way) | 90 | 96 | **85** |
| Hopscotch hashing | 90 | 95 | **83** |
| Adaptive Radix Tree (ART) | 86 | 88 | **70** |
| HAT-Trie | 82 | 88 | **70** |
| Bucketed Cuckoo Trie | 82 | 90 | **68** |
| Burst Trie | 80 | 90 | **68** |
| C3BT – Compact Clustered Crit-Bit Tree | 82 | 90 | **65** |
| Hash-Array-Mapped Trie (HAMT) | 78 | 90 | **63** |
| Standard unordered_map | 63 | 92 | **60** |
| Patricia (compressed binary) Trie | 72 | 88 | **60** |
| Crit-Bit Tree (plain) | 62 | 88 | **58** |
| Y-Fast Trie | 72 | 90 | **58** |
| Skip List | 62 | 88 | **55** |
| Succinct LOUDS Trie *(read-heavy only)* | 58 | 100 | **10** |
| Interpolation search on sorted flat array | 80 | 100 | **10** |
| Naïve sorted array + binary search | 75 | 100 | **5** |
| MPHF / RecSplit *(static / immutable sets only)* | 90 | 100 | **0** |

**Notes on the top three:**

- **SwissTable (92)**: no per-item allocation, SIMD metadata scan locates the first empty slot in the group in one instruction, resize is amortised. Best fit for mostly-unique write-heavy load.
- **Cuckoo (85)**: ≤ 2 probes per insert on average, but a cascade of "kicks" at high load factor adds latency variance. Keep load factor ≤ 0.9.
- **Hopscotch (83)**: neighbourhood-bounded insert, very predictable latency, but more bookkeeping than Cuckoo.

The gap between the hash tables (55–92) and everything below (≤ 70) is driven by **per-node heap allocation** — every tree or trie insert that calls the allocator adds ~50–100 ns of allocator overhead on top of the structural traversal cost.

The pre-filters in the write path they are redundant overhead. 

**MPHF / RecSplit / BBHash** is a static, zero-false-positive index, not a pre-filter. It belongs in the table but only for immutable key sets (sealed archive tier, not a live write path).

**Sharded hash table** is also not a distinct structure — it is an implementation pattern on top of any hash table, not a separate data structure.

**Flat linear-probing hash table** The absolute worst-case scenario for compact-dict is Unsuccessful Lookups. Linear probing is forced to search sequentially until it hits an empty slot (which takes longer at higher load factors), making it roughly ~2x slower than hashbrown's SIMD metadata scanning which immediately rejects misses.
