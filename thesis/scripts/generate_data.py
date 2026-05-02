# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "brotli>=1.1",
#     "zstandard>=0.23",
# ]
# ///
"""Extract per-zoom tile size data from mbtiles files, save as CSV.

For each source (MVT, MLT-Java, MLT-Rust), tiles are decompressed to raw bytes
and then measured under four compression schemes: plain, gzip, brotli, zstd.
"""

import csv
import gzip
import sqlite3
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

import brotli
import zstandard

SCRIPT_DIR = Path(__file__).resolve().parent
ROOT = SCRIPT_DIR.parent.parent
OUTPUT_DIR = SCRIPT_DIR / "data"

SOURCES = {
    "MVT": ROOT / "data" / "germany.mbtiles",
    "MVT-shaved": ROOT / "data" / "mvt-shaved" / "germany.mbtiles",
    "MLT-Java": ROOT / "java" / "germany.mlt.mbtiles",
    "MLT-Rust": ROOT / "data" / "germany-mlt.mbtiles",
    "MLT-Rust-shaved": ROOT / "data" / "mlt-shaved" / "germany.mbtiles",
}

COMPRESSORS = {
    "plain": lambda data: data,
    "gzip": lambda data: gzip.compress(data, compresslevel=9),
    "brotli": lambda data: brotli.compress(data, quality=11),
    "zstd": lambda data: zstandard.ZstdCompressor(level=22).compress(data),
}

CSV_HEADER = [
    "source", "compression", "zoom", "tile_count",
    "total_bytes", "avg_bytes", "min_bytes", "max_bytes",
]

TILE_QUERY = "SELECT zoom_level, tile_data FROM tiles ORDER BY zoom_level"


def is_gzip(data: bytes) -> bool:
    return len(data) >= 2 and data[0] == 0x1F and data[1] == 0x8B


def raw_bytes(data: bytes) -> bytes:
    """Decompress tile data if gzip-compressed, otherwise return as-is."""
    if is_gzip(data):
        return gzip.decompress(data)
    return data


def build_mlt(project_root: Path) -> Path:
    rust_dir = project_root / "rust"
    print("Building mlt (release)…")
    subprocess.run(
        ["cargo", "build", "-p", "mlt", "--release"],
        cwd=rust_dir,
        check=True,
    )
    binary = rust_dir / "target" / "release" / "mlt"
    if not binary.exists():
        sys.exit(f"mlt binary not found at {binary}")
    return binary


def convert_mvt_to_mlt(mlt_bin: Path, src: Path, dst: Path) -> None:
    print(f"Converting {src.name} → {dst.name} (this may take a while)…")
    subprocess.run(
        [str(mlt_bin), "convert", str(src), str(dst)],
        check=True,
    )
    print(f"Conversion complete: {dst}  ({dst.stat().st_size / 1e9:.2f} GB)")


def extract_compressed_stats(mbtiles_path: Path, source_label: str) -> list[dict]:
    """Read all tiles, decompress, recompress with each algorithm, aggregate per zoom."""
    print(f"  Processing {source_label} ({mbtiles_path.name})…", flush=True)
    con = sqlite3.connect(f"file:{mbtiles_path}?mode=ro", uri=True)

    # zoom -> compressor -> list of sizes
    accum: dict[int, dict[str, list[int]]] = defaultdict(lambda: defaultdict(list))

    count = 0
    for zoom, tile_data in con.execute(TILE_QUERY):
        raw = raw_bytes(tile_data)
        for comp_name, compress_fn in COMPRESSORS.items():
            accum[zoom][comp_name].append(len(compress_fn(raw)))
        count += 1
        if count % 50000 == 0:
            print(f"    {count} tiles…", flush=True)

    con.close()
    print(f"    {count} tiles total.", flush=True)

    rows = []
    for zoom in sorted(accum):
        for comp_name in COMPRESSORS:
            sizes = accum[zoom][comp_name]
            rows.append({
                "source": source_label,
                "compression": comp_name,
                "zoom": zoom,
                "tile_count": len(sizes),
                "total_bytes": sum(sizes),
                "avg_bytes": round(sum(sizes) / len(sizes), 2),
                "min_bytes": min(sizes),
                "max_bytes": max(sizes),
            })
    return rows


def write_csv(rows: list[dict], out_path: Path) -> None:
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=CSV_HEADER)
        w.writeheader()
        w.writerows(rows)
    print(f"Wrote {out_path}  ({len(rows)} rows)")


def main() -> None:
    mvt_path = SOURCES["MVT"]
    mlt_rust_path = SOURCES["MLT-Rust"]

    if not mvt_path.exists():
        sys.exit(f"MVT source not found: {mvt_path}")

    # Build and convert if needed
    mlt_bin = build_mlt(ROOT)
    if not mlt_rust_path.exists():
        convert_mvt_to_mlt(mlt_bin, mvt_path, mlt_rust_path)
    else:
        print(f"Skipping conversion — {mlt_rust_path.name} already exists.")

    # Extract stats for each source
    all_rows: list[dict] = []
    for label, path in SOURCES.items():
        if not path.exists():
            print(f"  WARNING: {path} not found, skipping {label}")
            continue
        all_rows.extend(extract_compressed_stats(path, label))

    write_csv(all_rows, OUTPUT_DIR / "tile_sizes.csv")
    print("Done.")


if __name__ == "__main__":
    main()
