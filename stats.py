#!/usr/bin/env python3
"""
Collection statistics for tuning dupl.

Walks one or more directories and reports:
  - File size distribution
  - Size-group shape (how many files share the same size)
  - Intra-group inode span (proxy for disk locality)
  - Filesystem type and block size per mount
  - Estimated duplicate bytes

Usage:
    python stats.py /path/to/photos [/path/to/more ...]
"""

from __future__ import annotations

import os
import sys
import math
import statistics
from collections import defaultdict
from dataclasses import dataclass


# ── Filesystem detection ──────────────────────────────────────────────────────


def load_mounts() -> dict[int, str]:
    """Return {device_id: filesystem_type} from /proc/mounts."""
    dev_to_fs: dict[int, str] = {}
    try:
        with open("/proc/mounts") as f:
            for line in f:
                parts = line.split()
                if len(parts) < 3:
                    continue
                mountpoint, fstype = parts[1], parts[2]
                try:
                    dev = os.stat(mountpoint).st_dev
                    dev_to_fs[dev] = fstype
                except OSError:
                    pass
    except OSError:
        pass
    return dev_to_fs


def statvfs_block_size(path: str) -> int:
    if not hasattr(os, "statvfs"):
        return 4096  # Windows — statvfs unavailable; NTFS default cluster size
    try:
        sv = os.statvfs(path)
        return sv.f_bsize
    except OSError:
        return 4096


# ── Data collection ───────────────────────────────────────────────────────────


@dataclass
class FileEntry:
    size: int
    inode: int
    dev: int


IMAGE_EXTENSIONS = frozenset(
    {".jpg", ".jpeg", ".png", ".gif", ".bmp", ".tiff", ".tif", ".webp", ".heic", ".heif", ".avif"}
)


@dataclass
class WalkResult:
    entries: list[FileEntry]
    unreadable_files: int
    unreadable_dirs: int


def _onerror(exc: OSError, unreadable_dirs: list[int]) -> None:
    unreadable_dirs[0] += 1


def walk(roots: list[str], min_size: int = 1) -> WalkResult:
    seen: set[tuple[int, int]] = set()  # (dev, inode) — collapse hard links
    entries: list[FileEntry] = []
    unreadable_files = 0
    unreadable_dirs_count = [0]  # mutable container for closure

    for root in roots:
        for dirpath, _, filenames in os.walk(
            root,
            followlinks=False,
            onerror=lambda exc: _onerror(exc, unreadable_dirs_count),
        ):
            for name in filenames:
                if os.path.splitext(name)[1].lower() not in IMAGE_EXTENSIONS:
                    continue
                path = os.path.join(dirpath, name)
                try:
                    st = os.stat(path, follow_symlinks=False)
                except OSError:
                    unreadable_files += 1
                    continue
                if not os.path.isfile(path):
                    continue
                if st.st_size < min_size:
                    continue
                key = (st.st_dev, st.st_ino)
                if key in seen:
                    continue
                seen.add(key)
                entries.append(FileEntry(st.st_size, st.st_ino, st.st_dev))

    return WalkResult(entries, unreadable_files, unreadable_dirs_count[0])


# ── Histogram helpers ─────────────────────────────────────────────────────────


def log2_bucket(n: int) -> str:
    if n == 0:
        return "0"
    exp = int(math.log2(n))
    lo: int = 1 << exp
    hi: int = 1 << (exp + 1)
    units = [(1 << 30, "GB"), (1 << 20, "MB"), (1 << 10, "KB")]

    def fmt(v: int) -> str:
        for div, label in units:
            if v >= div:
                return f"{v // div}{label}"
        return f"{v}B"

    return f"{fmt(lo)}–{fmt(hi)}"


def percentile(sorted_data: list[float], p: float) -> float:
    if not sorted_data:
        return 0.0
    idx = (len(sorted_data) - 1) * p / 100
    lo, hi = int(idx), min(int(idx) + 1, len(sorted_data) - 1)
    return sorted_data[lo] + (sorted_data[hi] - sorted_data[lo]) * (idx - lo)


def bar(value: float, max_value: float, width: int = 40) -> str:
    filled = int(round(value / max_value * width)) if max_value > 0 else 0
    return "█" * filled + "░" * (width - filled)


# ── Report sections ───────────────────────────────────────────────────────────


def report_overview(
    entries: list[FileEntry],
    dev_to_fs: dict[int, str],
    unreadable_files: int,
    unreadable_dirs: int,
) -> None:
    total_bytes = sum(e.size for e in entries)
    devs: defaultdict[int, int] = defaultdict(int)
    for e in entries:
        devs[e.dev] += 1

    print("=" * 60)
    print("OVERVIEW")
    print("=" * 60)
    print(f"  Total files (unique inodes) : {len(entries):>12,}")
    print(f"  Total data                  : {total_bytes / (1 << 30):>11.1f} GB")
    print(f"  Distinct devices            : {len(devs):>12,}")
    print(f"  Unreadable files            : {unreadable_files:>12,}")
    print(f"  Unreadable directories      : {unreadable_dirs:>12,}")
    print()
    for dev, count in sorted(devs.items(), key=lambda x: -x[1]):
        fs = dev_to_fs.get(dev, "unknown")
        print(f"    dev {dev:#010x}  {fs:<12}  {count:,} files")
    print()


def report_size_distribution(entries: list[FileEntry]) -> None:
    # Key: exponent e such that file size is in [2^e, 2^(e+1))
    buckets: dict[int, int] = defaultdict(int)
    for e in entries:
        exp = int(math.log2(max(1, e.size)))
        buckets[exp] += 1

    print("=" * 60)
    print("FILE SIZE DISTRIBUTION  (log₂ buckets)")
    print("=" * 60)
    max_count = max(buckets.values(), default=1)
    for exp in sorted(buckets):
        count = buckets[exp]
        label = log2_bucket(1 << exp)
        print(f"  {label:>12}  {bar(count, max_count, 35)}  {count:,}")
    print()


def report_size_groups(entries: list[FileEntry]) -> list[list[FileEntry]]:
    by_size: dict[tuple[int, int], list[FileEntry]] = defaultdict(list)
    for e in entries:
        by_size[(e.dev, e.size)].append(e)

    groups = [g for g in by_size.values() if len(g) >= 2]
    singletons = sum(1 for g in by_size.values() if len(g) == 1)
    total_dup_bytes = sum((len(g) - 1) * g[0].size for g in groups)

    group_sizes = sorted(len(g) for g in groups)

    print("=" * 60)
    print("SIZE GROUPS  (files sharing the same byte count)")
    print("=" * 60)
    print(f"  Singleton sizes (no dup candidate)  : {singletons:,}")
    print(f"  Sizes with ≥2 files                 : {len(groups):,}")
    if groups:
        print(f"  Max files in one size group         : {max(group_sizes):,}")
        print(
            f"  Median group size                   : {statistics.median(group_sizes):.1f}"
        )
        print(
            f"  Upper-estimate duplicate data       : {total_dup_bytes / (1 << 30):.1f} GB"
        )
        print()
        print("  Group size distribution:")
        size_buckets: dict[str, int] = defaultdict(int)
        for s in group_sizes:
            if s <= 5:
                size_buckets[str(s)] += 1
            elif s <= 10:
                size_buckets["6–10"] += 1
            elif s <= 50:
                size_buckets["11–50"] += 1
            else:
                size_buckets["51+"] += 1
        max_b = max(size_buckets.values(), default=1)
        for label in ["2", "3", "4", "5", "6–10", "11–50", "51+"]:
            count = size_buckets.get(label, 0)
            if count:
                print(f"    {label:>6} files/group  {bar(count, max_b, 30)}  {count:,}")
    print()
    return groups


def report_inode_locality(groups: list[list[FileEntry]]) -> None:
    spans: list[float] = []
    for g in groups:
        if len(g) < 2:
            continue
        inodes = [e.inode for e in g]
        spans.append(max(inodes) - min(inodes))

    if not spans:
        print("(no size groups to analyse)")
        return

    spans.sort()

    print("=" * 60)
    print("INODE LOCALITY WITHIN SIZE GROUPS")
    print("(small span → files created together → good disk locality)")
    print("=" * 60)
    print(f"  Groups analysed   : {len(spans):,}")
    print(f"  Median inode span : {percentile(spans, 50):>14,.0f}")
    print(f"  p75 inode span    : {percentile(spans, 75):>14,.0f}")
    print(f"  p90 inode span    : {percentile(spans, 90):>14,.0f}")
    print(f"  p99 inode span    : {percentile(spans, 99):>14,.0f}")
    print(f"  Max inode span    : {max(spans):>14,.0f}")
    print()

    # Classify groups by locality quality
    excellent = sum(1 for s in spans if s < 1_000)
    good = sum(1 for s in spans if 1_000 <= s < 100_000)
    fair = sum(1 for s in spans if 100_000 <= s < 10_000_000)
    poor = sum(1 for s in spans if s >= 10_000_000)
    total = len(spans)

    print("  Locality quality (inode span thresholds):")
    for label, count in [
        ("excellent (<1K)", excellent),
        ("good (1K–100K)", good),
        ("fair (100K–10M)", fair),
        ("poor (≥10M)", poor),
    ]:
        pct = 100 * count / total if total else 0
        print(f"    {label:<22}  {bar(pct, 100, 30)}  {pct:5.1f}%  ({count:,})")
    print()

    print("  Inode span histogram (log₂ buckets):")
    hist: dict[int, int] = defaultdict(int)
    for s in spans:
        bucket = int(math.log2(max(1, s)))
        hist[bucket] += 1
    max_h = max(hist.values(), default=1)
    for exp in sorted(hist):
        lo: int = 1 << exp
        hi: int = 1 << (exp + 1)
        print(f"    {lo:>12,}–{hi:<12,}  {bar(hist[exp], max_h, 28)}  {hist[exp]:,}")
    print()


def report_block_size_hint(groups: list[list[FileEntry]], roots: list[str]) -> None:
    fs_block = statvfs_block_size(roots[0])

    print("=" * 60)
    print("BLOCK SIZE HINTS")
    print("=" * 60)
    print(f"  Filesystem block size (first root) : {fs_block:,} bytes")
    print("  Default dupl block_size            : 65,536 bytes")
    print()

    # Count files per size range to show where tiny files cluster
    BLOCK_SIZE = 65536
    tiny: list[list[FileEntry]] = [g for g in groups if g[0].size < BLOCK_SIZE]
    large: list[list[FileEntry]] = [g for g in groups if g[0].size >= BLOCK_SIZE]
    tiny_files = sum(len(g) for g in tiny)
    large_files = sum(len(g) for g in large)

    print(
        f"  Size groups with files < block_size : {len(tiny):,}  ({tiny_files:,} files)"
    )
    print(
        f"  Size groups with files ≥ block_size : {len(large):,}  ({large_files:,} files)"
    )
    print()

    # Warn about spikes — size buckets with suspiciously many files
    by_exp: dict[int, int] = defaultdict(int)
    for g in groups:
        exp = int(math.log2(max(1, g[0].size)))
        by_exp[exp] += len(g)
    total_grouped = sum(by_exp.values())
    print("  Dominant size ranges in duplicate candidates:")
    for exp in sorted(by_exp, key=lambda e: -by_exp[e])[:5]:
        count = by_exp[exp]
        label = log2_bucket(1 << exp)
        pct = 100 * count / total_grouped if total_grouped else 0
        flag = "  ← possible metadata/thumbnail noise" if 2**exp < 4096 else ""
        print(f"    {label:>12}  {count:>8,} files  ({pct:4.1f}%){flag}")
    print()


# ── Main ──────────────────────────────────────────────────────────────────────


def main() -> None:
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <dir> [<dir> ...]", file=sys.stderr)
        sys.exit(1)

    roots = sys.argv[1:]
    for r in roots:
        if not os.path.isdir(r):
            print(f"error: not a directory: {r}", file=sys.stderr)
            sys.exit(1)

    print(f"Scanning {len(roots)} root(s)…", file=sys.stderr)
    dev_to_fs = load_mounts()
    result = walk(roots)
    entries = result.entries
    print(f"Found {len(entries):,} unique files.", file=sys.stderr)
    if result.unreadable_files or result.unreadable_dirs:
        print(
            f"Skipped {result.unreadable_files:,} unreadable file(s), "
            f"{result.unreadable_dirs:,} unreadable director(ies).",
            file=sys.stderr,
        )
    print()

    report_overview(entries, dev_to_fs, result.unreadable_files, result.unreadable_dirs)
    report_size_distribution(entries)
    groups = report_size_groups(entries)
    report_inode_locality(groups)
    report_block_size_hint(groups, roots)


if __name__ == "__main__":
    main()
