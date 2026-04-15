#!/usr/bin/env python3
"""Generate per-zoom tile size CSV from tile_sizes.csv."""

import csv
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
CSV_PATH = SCRIPT_DIR / "data" / "tile_sizes.csv"
OUT_PATH = SCRIPT_DIR / "data" / "tile_sizes_per_zoom.csv"

SOURCES = ["MVT", "MLT-Java", "MLT-Rust", "MLT-Rust-shaved"]


def main() -> None:
    rows = list(csv.DictReader(CSV_PATH.open()))

    # Index: (source, zoom) → gzip total_bytes
    data: dict[tuple[str, int], int] = {}
    tile_counts: dict[int, int] = {}
    for r in rows:
        if r["compression"] == "gzip":
            data[(r["source"], int(r["zoom"]))] = int(r["total_bytes"])
            if r["source"] == "MVT":
                tile_counts[int(r["zoom"])] = int(r["tile_count"])

    zooms = sorted(tile_counts.keys())

    totals = {s: 0 for s in SOURCES}
    total_tiles = 0

    out_rows: list[list] = []
    header = ["zoom", "tiles", "mvtBytes", "mltJavaBytes", "mltRustBytes", "mltShavedBytes", "deltaPct", "isTotal"]

    for z in zooms:
        vals = {s: data.get((s, z), 0) for s in SOURCES}
        for s in SOURCES:
            totals[s] += vals[s]
        tc = tile_counts[z]
        total_tiles += tc

        mvt = vals["MVT"]
        delta = (vals["MLT-Rust-shaved"] - mvt) / mvt * 100 if mvt > 0 else 0
        out_rows.append([
            z, tc, mvt, vals["MLT-Java"], vals["MLT-Rust"],
            vals["MLT-Rust-shaved"], f"{delta:.1f}", 0,
        ])

    delta_total = (totals["MLT-Rust-shaved"] - totals["MVT"]) / totals["MVT"] * 100
    out_rows.append([
        "Total", total_tiles, totals["MVT"], totals["MLT-Java"],
        totals["MLT-Rust"], totals["MLT-Rust-shaved"],
        f"{delta_total:.1f}", 1,
    ])

    OUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    with OUT_PATH.open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(header)
        w.writerows(out_rows)

    print(f"Wrote {OUT_PATH}")
    print(
        f"Totals: MVT={totals['MVT']/1e9:.2f} GB, "
        f"Shaved={totals['MLT-Rust-shaved']/1e9:.2f} GB, "
        f"delta={delta_total:.1f}%"
    )


if __name__ == "__main__":
    main()
