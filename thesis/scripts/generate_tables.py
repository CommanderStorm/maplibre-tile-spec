#!/usr/bin/env python3

from __future__ import annotations

import csv
import json
import re
import sys
from collections import defaultdict
from pathlib import Path
from statistics import median

# Paths

SCRIPT_DIR = Path(__file__).resolve().parent
THESIS_DIR = SCRIPT_DIR.parent
ROOT = THESIS_DIR.parent
INPUT_DIR = ROOT.parent / "maplibre-optimizer" / "tests" / "bench" / "results"
OUTPUT_DIR = SCRIPT_DIR / "data"

# Constants

EXCLUDED_STYLES = {"americana"}
FULL_PIPELINE_STYLES = {"fiord", "liberty"}

_STEP_RE = re.compile(r"^step-(\d+)-(.+)$")

# Short pass labels for per-style ablation tables.
PASS_LABELS: dict[str, str] = {
    "baseline": "Baseline",
    "simplify_unary": "Unary simpl.",
    "expression_kind": "Expr. kind",
    "constant_fold": "Const. fold",
    "constant_fold_stats": "CF stats",
    "simplify_expressions": "Simplify expr.",
    "strip_defaults": "Strip defaults",
    "minify_colors": "Minify colours",
    "strip_metadata": "Strip metadata",
    "dead_elimination": "Dead elim.",
    "dead_elimination_stats": "DE stats",
    "metadata_refinement": "Meta. refine.",
    "metadata_refinement_paint": "MR paint",
    "metadata_refinement_stats": "MR stats",
    "cleanup": "Cleanup",
    "layer_merge": "Layer merge",
    "selectivity_reorder": "Selectivity reorder",
    "tile_shave_only": "Tile shave only",
    "tile_shave": "Tile shave",
    "tile_rewrite": "Tile rewrite",
}

# Longer names for the marginal-contribution table.
MARGINAL_LABELS: dict[int, str] = {
    1: "Unary simplification",
    2: "Kind normalisation",
    3: "Constant folding",
    4: "Stats-driven folding",
    5: "Expression simplification",
    6: "Default stripping",
    7: "Colour minification",
    8: "Metadata stripping",
    9: "Dead elimination",
    10: "Dead elimination (stats)",
    11: "Metadata refinement",
    12: "Paint-based refinement",
    13: "Stats-driven refinement",
    14: "Cleanup",
    15: "Layer merging",
    16: "Selectivity reordering",
    17: "Tile shave only",
    18: "Tile shave",
    19: "Tile rewriting",
}

# Style ID → display name (for table captions).
STYLE_NAMES: dict[str, str] = {
    "liberty": "Liberty",
    "bright": "Bright",
    "positron": "Positron",
    "fiord": "Fiord",
    "dark-matter": "Dark Matter",
    "osm-bright": "OSM Bright",
    "klokan-basic": "Klokan Basic",
    "toner": "Toner",
    "osm-liberty": "OSM Liberty",
    "basemap-col": "BasemapDE Colour",
    "basemap-top": "BasemapDE Topo",
    "icgc-fosc": "ICGC Fosc",
    "icgc-gris": "ICGC Gris",
    "stadia-outdoors": "Stadia Outdoors",
}

# Style ID → label in the summary table (matching existing text).
SUMMARY_LABELS: dict[str, str] = {
    "liberty": "liberty",
    "bright": "bright",
    "positron": "positron",
    "fiord": "fiord",
    "dark-matter": "dark-matter",
    "osm-bright": "osm-bright",
    "klokan-basic": "klokan-basic",
    "toner": "toner",
    "osm-liberty": "osm-liberty",
    "stadia-outdoors": "stadia-outdoors",
    "icgc-fosc": "ICGC fosc",
    "icgc-gris": "ICGC gris",
    "basemap-top": "BasemapDE topo",
    "basemap-col": "BasemapDE colour",
}

# Appendix order: existing 9 tables first, then 5 new styles.
APPENDIX_ORDER: list[str] = [
    "liberty", "bright", "positron", "fiord", "dark-matter",
    "osm-bright", "klokan-basic", "toner", "osm-liberty",
    "basemap-col", "basemap-top", "icgc-fosc", "icgc-gris",
    "stadia-outdoors",
]

# Summary table order: style-only first, then full-pipeline (fiord, liberty).
SUMMARY_ORDER: list[str] = [
    "bright", "positron", "dark-matter", "osm-bright",
    "klokan-basic", "toner", "osm-liberty", "stadia-outdoors",
    "icgc-fosc", "icgc-gris", "basemap-top", "basemap-col",
    "fiord", "liberty",
]


# Data loading


def load_jsonl(input_dir: Path) -> list[dict]:
    """Load all ``*.jsonl`` files under *input_dir*."""
    rows: list[dict] = []
    for path in sorted(input_dir.glob("*.jsonl")):
        with path.open() as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                try:
                    rows.append(json.loads(line))
                except json.JSONDecodeError:
                    pass
    if not rows:
        sys.exit(f"No JSONL files found under {input_dir}")
    return rows


def filter_latest_session(records: list[dict]) -> list[dict]:
    """Keep only the session with the most variants per style (latest ts tiebreak)."""
    by_style: dict[str, dict[str, list[dict]]] = defaultdict(lambda: defaultdict(list))
    for r in records:
        style = r.get("style", "")
        ts = r.get("timestamp", "")
        if style and ts:
            by_style[style][ts].append(r)

    filtered: list[dict] = []
    for style in sorted(by_style):
        by_ts = by_style[style]
        variant_counts = {
            ts: len({r["variant"] for r in recs})
            for ts, recs in by_ts.items()
        }
        best_ts = max(variant_counts, key=lambda ts: (variant_counts[ts], ts))
        filtered.extend(by_ts[best_ts])
        print(f"  {style}: {best_ts} ({variant_counts[best_ts]} variants)")

    return filtered


# Aggregation


def parse_variant(variant: str) -> tuple[int, str]:
    m = _STEP_RE.match(variant)
    if m:
        return int(m.group(1)), m.group(2)
    return 0, "baseline"


def aggregate_steps(records: list[dict], style: str) -> list[dict]:
    """Produce one row per ablation step, with deterministic + median runtime values."""
    recs = [r for r in records if r.get("style") == style]
    by_step: dict[int, list[dict]] = defaultdict(list)
    for r in recs:
        step, _ = parse_variant(r["variant"])
        by_step[step].append(r)

    steps: list[dict] = []
    for step_num in sorted(by_step):
        group = by_step[step_num]
        _, pass_name = parse_variant(group[0]["variant"])

        # Deterministic values (identical across runs/scenarios).
        style_bytes = int(group[0].get("style_bytes", 0))
        gzip_bytes = int(group[0].get("gzip_bytes", 0))
        brotli_bytes = int(group[0].get("brotli_bytes", 0))
        layer_count = int(group[0].get("layer_count", 0))

        # Runtime values: median across all runs × scenarios.
        loads = [r["loadMs"] for r in group if r.get("loadMs") is not None]
        fpss = [r["fps"] for r in group if r.get("fps") is not None]

        steps.append({
            "step": step_num,
            "pass_name": pass_name,
            "style_bytes": style_bytes,
            "gzip_bytes": gzip_bytes,
            "brotli_bytes": brotli_bytes,
            "load_ms": median(loads) if loads else None,
            "fps": median(fpss) if fpss else None,
            "layers": layer_count,
        })

    return steps


# CSV writers

PER_STYLE_HEADER = ["step", "pass", "rawB", "gzipB", "brotliB", "loadMs", "fps", "layers"]
SUMMARY_HEADER = ["style", "grp", "baseGzip", "optGzip", "reduction", "deltaLoad", "deltaFps", "isBold", "midruleBefore"]
MARGINAL_HEADER = ["step", "pass", "deltaRaw", "deltaGzip", "deltaBrotli", "deltaLoad", "deltaLayers", "dagger"]


def write_per_style_csv(path: Path, steps: list[dict]) -> None:
    """Write a per-style ablation CSV."""
    with path.open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(PER_STYLE_HEADER)
        for s in steps:
            label = PASS_LABELS.get(s["pass_name"], s["pass_name"])
            load_val = f"{s['load_ms']:.1f}" if s["load_ms"] is not None else ""
            fps_val = f"{s['fps']:.1f}" if s["fps"] is not None else ""
            w.writerow([
                s["step"], label, s["style_bytes"], s["gzip_bytes"],
                s["brotli_bytes"], load_val, fps_val, s["layers"],
            ])


def write_summary_csv(path: Path, all_steps: dict[str, list[dict]]) -> None:
    """Write the per-style summary CSV."""
    style_only = [s for s in SUMMARY_ORDER if s not in FULL_PIPELINE_STYLES]
    full_pipe = [s for s in SUMMARY_ORDER if s in FULL_PIPELINE_STYLES]

    all_base: list[int] = []
    all_opt: list[int] = []
    all_red: list[float] = []
    all_dload: list[float] = []
    all_dfps: list[float] = []

    rows: list[list] = []

    def _add_row(style_id: str, group: str) -> None:
        steps = all_steps[style_id]
        baseline = steps[0]
        best = steps[-1]
        label = SUMMARY_LABELS[style_id]

        base_gzip = baseline["gzip_bytes"]
        opt_gzip = best["gzip_bytes"]
        reduction = (base_gzip - opt_gzip) / base_gzip * 100 if base_gzip else 0.0

        d_load = None
        if baseline["load_ms"] and best["load_ms"] and baseline["load_ms"] > 0:
            d_load = (best["load_ms"] - baseline["load_ms"]) / baseline["load_ms"] * 100

        d_fps = None
        if baseline["fps"] and best["fps"] and baseline["fps"] > 0:
            d_fps = (best["fps"] - baseline["fps"]) / baseline["fps"] * 100

        all_base.append(base_gzip)
        all_opt.append(opt_gzip)
        all_red.append(reduction)
        if d_load is not None:
            all_dload.append(d_load)
        if d_fps is not None:
            all_dfps.append(d_fps)

        rows.append([
            label, group, base_gzip, opt_gzip,
            f"{reduction:.1f}",
            f"{d_load:.1f}" if d_load is not None else "",
            f"{d_fps:.1f}" if d_fps is not None else "",
            0, 0,  # isBold, midruleBefore — set below
        ])

    for s in style_only:
        if s in all_steps:
            _add_row(s, "style_only")

    # Mark midrule before first full_pipeline row
    first_fp = True
    for s in full_pipe:
        if s in all_steps:
            _add_row(s, "full_pipeline")
            if first_fp:
                rows[-1][-1] = 1  # midruleBefore
                first_fp = False

    # Mean row
    if all_base:
        mean_base = int(sum(all_base) / len(all_base))
        mean_opt = int(sum(all_opt) / len(all_opt))
        mean_red = sum(all_red) / len(all_red)
        mean_dload = sum(all_dload) / len(all_dload) if all_dload else None
        mean_dfps = sum(all_dfps) / len(all_dfps) if all_dfps else None

        rows.append([
            "Mean", "mean", mean_base, mean_opt,
            f"{mean_red:.1f}",
            f"{mean_dload:.1f}" if mean_dload is not None else "",
            f"{mean_dfps:.1f}" if mean_dfps is not None else "",
            1, 1,  # isBold=1, midruleBefore=1
        ])

    with path.open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(SUMMARY_HEADER)
        w.writerows(rows)


def _fmt_median(vals: list[float], decimals: int = 1) -> str:
    v = median(vals)
    if abs(v) < 10 ** (-decimals) / 2:
        return "0.0"
    return f"{v:.{decimals}f}"


def _fmt_median_int(vals: list[float]) -> str:
    return str(int(round(median(vals))))


def write_marginal_csv(path: Path, all_steps: dict[str, list[dict]]) -> None:
    """Write the marginal contribution CSV."""
    max_step = max(s["step"] for steps in all_steps.values() for s in steps)

    rows: list[list] = []
    for step_num in range(1, max_step + 1):
        d_raw: list[float] = []
        d_gzip: list[float] = []
        d_brotli: list[float] = []
        d_load: list[float] = []
        d_layers: list[float] = []

        for style_id, steps in all_steps.items():
            step_map = {s["step"]: s for s in steps}
            if step_num not in step_map or (step_num - 1) not in step_map:
                continue

            curr = step_map[step_num]
            prev = step_map[step_num - 1]
            base = step_map[0]

            if base["style_bytes"] > 0:
                d_raw.append(
                    (curr["style_bytes"] - prev["style_bytes"]) / base["style_bytes"] * 100
                )
            if base["gzip_bytes"] > 0:
                d_gzip.append(
                    (curr["gzip_bytes"] - prev["gzip_bytes"]) / base["gzip_bytes"] * 100
                )
            if base["brotli_bytes"] > 0:
                d_brotli.append(
                    (curr["brotli_bytes"] - prev["brotli_bytes"]) / base["brotli_bytes"] * 100
                )
            if (
                base["load_ms"] and curr["load_ms"] and prev["load_ms"]
                and base["load_ms"] > 0
            ):
                d_load.append(
                    (curr["load_ms"] - prev["load_ms"]) / base["load_ms"] * 100
                )
            d_layers.append(curr["layers"] - prev["layers"])

        if not d_raw:
            continue

        label = MARGINAL_LABELS.get(step_num, f"Step {step_num}")
        dagger = 1 if step_num >= 16 else 0

        rows.append([
            step_num, label,
            _fmt_median(d_raw), _fmt_median(d_gzip), _fmt_median(d_brotli),
            _fmt_median(d_load) if d_load else "0.0",
            _fmt_median_int(d_layers), dagger,
        ])

    with path.open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(MARGINAL_HEADER)
        w.writerows(rows)


# Main


def main() -> int:
    print("Loading JSONL data...")
    records = load_jsonl(INPUT_DIR)
    print(f"  {len(records)} records from {INPUT_DIR}")

    print("\nFiltering to latest session per style...")
    filtered = filter_latest_session(records)
    print(f"  {len(filtered)} records after filtering")

    # Build per-style step data.
    styles = sorted({r["style"] for r in filtered if r.get("style")})
    all_steps: dict[str, list[dict]] = {}
    for style_id in styles:
        if style_id in EXCLUDED_STYLES:
            print(f"  {style_id}: EXCLUDED (incomplete data)")
            continue
        all_steps[style_id] = aggregate_steps(filtered, style_id)

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

    # 1a. Per-style ablation CSVs
    print("\nPer-style ablation CSVs:")
    for style_id in APPENDIX_ORDER:
        if style_id not in all_steps:
            print(f"  WARNING: no data for {style_id}")
            continue
        steps = all_steps[style_id]
        fname = f"per_style_{style_id.replace('-', '_')}.csv"
        write_per_style_csv(OUTPUT_DIR / fname, steps)
        print(
            f"  {style_id}: {len(steps)} steps, "
            f"baseline gzip={steps[0]['gzip_bytes']}, "
            f"final gzip={steps[-1]['gzip_bytes']}"
        )

    # 1b. Summary CSV
    print("\nSummary CSV:")
    write_summary_csv(OUTPUT_DIR / "per_style_summary.csv", all_steps)

    # Print gzip-reduction diagnostics.
    reductions: dict[str, float] = {}
    for sid, steps in all_steps.items():
        base = steps[0]["gzip_bytes"]
        final = steps[-1]["gzip_bytes"]
        if base > 0:
            reductions[sid] = (base - final) / base * 100
    reds = sorted(reductions.values())
    print(f"  Gzip reduction range: {min(reds):.1f}%--{max(reds):.1f}%")
    print(f"  Gzip reduction median: {median(reds):.1f}%")

    # Print load-time diagnostics for full-pipeline styles.
    for sid in sorted(FULL_PIPELINE_STYLES):
        if sid in all_steps:
            steps = all_steps[sid]
            base_load = steps[0]["load_ms"]
            final_load = steps[-1]["load_ms"]
            if base_load and final_load:
                pct = (base_load - final_load) / base_load * 100
                print(f"  {sid} load reduction: {pct:.1f}% ({base_load:.0f} → {final_load:.0f} ms)")

    # 1c. Marginal contribution CSV
    print("\nMarginal contribution CSV:")
    write_marginal_csv(OUTPUT_DIR / "marginal_contribution.csv", all_steps)

    print(f"\nAll files written to {OUTPUT_DIR}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
