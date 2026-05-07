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

| #   | Data‑structure                                              | I/O (0‑100)                                 | Cache (0‑100)                                            | Mem (0‑100)                                 | Overall |
|-----|-------------------------------------------------------------|--------------------------------------------|----------------------------------------------------------|--------------------------------------------|---------|
| **3**  | **Hybrid "filter → hash‑table"** (Bloom/​Cuckoo filter + Robin‑Hood/SwissTable) | **100** (all look‑ups end in RAM)          | **85** (filter + cache‑line‑aligned hash table)          | **80** (≈ 18 GB total)                     |         |
| **14** | **Cuckoo hashing (2‑way / 4‑way)**                         | **100**                                    | **88** (≤ 2 probes, good line‑alignment)                | **80** (≈ 19 GB for 2 B keys at 1.2 load factor) |         |
| **12** | **Robin‑Hood / SwissTable (absl::flat_hash_map)**        | **100**                                    | **90** (cache‑line buckets, low‑variance probing)       | **70** (≈ 22‑24 GB; a bit heavy but still ≤ 32 GB) |         |
| **13** | **Hopscotch hashing**                                     | **100**                                    | **88** (neighbourhood‑preserving, good prefetch)        | **70** (≈ 22 GB)                            |         |
| **4**  | **Bucketed Cuckoo Trie**                                   | **100**                                    | **80** (tiny buckets per level, good line use)          | **70** (≈ 22 GB)                            |         |
| **5**  | **Adaptive Radix Tree (ART)**                              | **100**                                    | **85** (nodes sized to cache lines, good fan‑out)       | **70** (≈ 22 GB)                            |         |
| **6**  | **C3BT – Compact Clustered Crit‑Bit Tree**                 | **100**                                    | **80** (clusters packed tightly)                         | **75** (≈ 20 GB)                            |         |
| **9**  | **HAT‑Trie (Hybrid Adaptive Trie)**                        | **100**                                    | **80** (array‑mapped top + hash leaves)                  | **70** (≈ 22 GB)                            |         |
| **10** | **Burst Trie**                                             | **100**                                    | **78** (hash buckets only in leaves)                     | **68** (≈ 21 GB)                            |         |
| **8**  | **Hash‑Array‑Mapped Trie (HAMT)**                           | **100**                                    | **75** (bitmap nodes ⇒ extra indirection)               | **70** (≈ 22 GB)                            |         |
| **15** | **Sparse / Dense Google hash maps**                         | **100**                                    | **85** (cache‑friendly, but larger control structures) | **75** (≈ 24 GB)                            |         |
| **7**  | **Patricia‑Merkle Trie**                                    | **100**                                    | **70** (extra hash stored per node)                     | **65** (≈ 26 GB)                            |         |
| **16** | **Patricia (compressed binary) Trie**                      | **100**                                    | **70** (pointer chasing)                                 | **70** (≈ 22 GB)                            |         |
| **18** | **Y‑Fast Trie**                                            | **100**                                    | **70** (summary level + BST buckets)                     | **65** (≈ 24 GB)                            |         |
| **17** | **X‑Fast Trie**                                            | **100**                                    | **65** (one hash table per level)                        | **60** (≈ 27 GB)                            |         |
| **19** | **Skip List / Finger‑Search Tree**                         | **100**                                    | **60** (sequential nodes, bad locality)                 | **70** (≈ 22 GB)                            |         |
| **30** | **Crit‑Bit Tree (plain)**                                  | **100**                                    | **60**                                                   | **70**                                     |         |
| **29** | **Radix Tree (plain)**                                      | **100**                                    | **55**                                                   | **70**                                     |         |
| **20** | **Blocked Bloom filter (pre‑filter only)**                | **70** (many false‑positives ⇒ extra disk reads) | **90**                                                   | **85**                                     |         |
| **21** | **Cuckoo filter (pre‑filter)**                             | **60** (≈ 0.2 % FP → few extra reads)       | **90**                                                   | **80**                                     |         |
| **22** | **Quotient filter (pre‑filter)**                           | **60**                                      | **85**                                                   | **80**                                     |         |
| **24** | **Merkle Tree / Hash‑Tree (disk‑resident)**                | **40** (log N random reads)                | **50** (balanced tree, poor locality)                   | **90** (tiny in‑memory index)               |         |
| **25** | **B‑Tree / B+‑Tree (disk‑resident)**                       | **40**                                      | **55**                                                   | **85**                                     |         |
| **1**  | **LSM‑Tree (RocksDB / LevelDB)**                            | **30** (compactions cause extra reads)     | **50**                                                   | **90**                                     |         |
| **2**  | **Bε‑Tree (Fractal Tree)**                                 | **30**                                      | **55**                                                   | **85**                                     |         |
| **11** | **Cache‑Oblivious Trie (disk‑resident)**                   | **60** (sequential layout reduces reads)   | **70**                                                   | **80**                                     |         |
| **26** | **Judy array**                                             | **100**                                    | **55** (pointer heavy)                                   | **30** (≈ 45 GB for 2 B keys → exceeds budget) |         |
| **27** | **Succinct LOUDS Trie**                                    | **100**                                    | **55** (rank/select overhead)                            | **90** (compact bits/entry)                |         |
| **28** | **qp‑tries / qp‑tree**                                      | **100**                                    | **50** (experimental layout, poor locality)             | **70**                                     |         |
| **32** | **Skip‑list variants (finger‑search)**                     | **100**                                    | **60**                                                   | **70**                                     |         |
| **33** | **Standard unordered_map**                                 | **100**                                    | **60**                                                   | **65**                                     |         |
| **23** | **Bloom filter (stand‑alone)**                             | **20** (high FP ⇒ many disk reads)         | **90**                                                   | **90**                                     |         |
| **34** | **Naïve sorted array of 64‑bit hashes**                    | **100**                                    | **70**                                                   | **0** (needs > 200 GB)                     |         |


https://tirsus.com/lsm-trees/
https://crates.io/crates/lsm-tree
