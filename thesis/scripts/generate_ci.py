# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "scipy>=1.11",
#     "numpy>=1.24",
# ]
# ///
"""Compute bootstrap 95% confidence intervals from benchmark JSONL data.

Reads all JSONL benchmark results, filters to the latest session per style,
and for each (style, variant) pair computes:
  - Bootstrap 95% CIs for loadMs and fps
  - IQR (Q1, Q3)
  - Wilcoxon signed-rank test p-value vs. baseline

Outputs:
  scripts/data/confidence_intervals.csv

Usage:
    python3 thesis/scripts/generate_ci.py
"""

from __future__ import annotations

import csv
import json
import re
import sys
from collections import defaultdict
from pathlib import Path
from statistics import median

import numpy as np
from scipy import stats

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------

SCRIPT_DIR = Path(__file__).resolve().parent
THESIS_DIR = SCRIPT_DIR.parent
ROOT = THESIS_DIR.parent
INPUT_DIR = ROOT.parent / "maplibre-optimizer" / "tests" / "bench" / "results"
OUTPUT_DIR = SCRIPT_DIR / "data"

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

EXCLUDED_STYLES = {"americana"}

_STEP_RE = re.compile(r"^step-(\d+)-(.+)$")

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

FULL_PIPELINE_STYLES = {"fiord", "liberty"}

# Key claims to check for statistical significance in diagnostics.
KEY_CLAIMS = [
    {
        "label": "Expression passes reduce load time (liberty, steps 1-5 vs baseline)",
        "style": "liberty",
        "variant": "step-05-simplify_expressions",
        "metric": "loadMs",
        "direction": "decrease",
    },
    {
        "label": "Full pipeline reduces load time (liberty)",
        "style": "liberty",
        "variant": "step-19-tile_rewrite",
        "metric": "loadMs",
        "direction": "decrease",
    },
    {
        "label": "Full pipeline reduces load time (fiord)",
        "style": "fiord",
        "variant": "step-19-tile_rewrite",
        "metric": "loadMs",
        "direction": "decrease",
    },
    {
        "label": "Layer merge reduces load time (liberty)",
        "style": "liberty",
        "variant": "step-15-layer_merge",
        "metric": "loadMs",
        "direction": "decrease",
    },
    {
        "label": "Tile shave reduces load time (liberty)",
        "style": "liberty",
        "variant": "step-17-tile_shave_only",
        "metric": "loadMs",
        "direction": "decrease",
    },
    {
        "label": "Full pipeline improves FPS (liberty)",
        "style": "liberty",
        "variant": "step-19-tile_rewrite",
        "metric": "fps",
        "direction": "increase",
    },
    {
        "label": "Full pipeline improves FPS (fiord)",
        "style": "fiord",
        "variant": "step-19-tile_rewrite",
        "metric": "fps",
        "direction": "increase",
    },
]


# ---------------------------------------------------------------------------
# Data loading (same logic as generate_tables.py)
# ---------------------------------------------------------------------------


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


def parse_variant(variant: str) -> tuple[int, str]:
    m = _STEP_RE.match(variant)
    if m:
        return int(m.group(1)), m.group(2)
    return 0, "baseline"


# ---------------------------------------------------------------------------
# Bootstrap CI computation
# ---------------------------------------------------------------------------


def bootstrap_ci(
    data: np.ndarray,
    n_resamples: int = 10_000,
    confidence_level: float = 0.95,
    rng_seed: int = 42,
) -> tuple[float, float, float]:
    """Compute median and bootstrap 95% CI.

    Returns (median, ci_lo, ci_hi).
    """
    if len(data) < 2:
        med = float(np.median(data))
        return med, med, med

    rng = np.random.default_rng(rng_seed)
    result = stats.bootstrap(
        (data,),
        statistic=np.median,
        n_resamples=n_resamples,
        confidence_level=confidence_level,
        random_state=rng,
        method="percentile",
    )
    med = float(np.median(data))
    return med, float(result.confidence_interval.low), float(result.confidence_interval.high)


def wilcoxon_test(baseline: np.ndarray, treatment: np.ndarray) -> float | None:
    """Compute Wilcoxon signed-rank test p-value.

    Both arrays must have the same length (paired samples: same scenarios x runs).
    Returns p-value or None if test cannot be performed.
    """
    if len(baseline) != len(treatment):
        # If lengths differ we cannot do a paired test.
        # Fall back to Mann-Whitney U (unpaired).
        if len(baseline) < 2 or len(treatment) < 2:
            return None
        try:
            _, p = stats.mannwhitneyu(baseline, treatment, alternative="two-sided")
            return float(p)
        except ValueError:
            return None

    diff = treatment - baseline
    # Remove zero differences (Wilcoxon cannot handle them).
    nonzero = diff[diff != 0]
    if len(nonzero) < 10:
        return None

    try:
        result = stats.wilcoxon(nonzero, alternative="two-sided")
        return float(result.pvalue)
    except ValueError:
        return None


# ---------------------------------------------------------------------------
# Main analysis
# ---------------------------------------------------------------------------


def collect_values(
    records: list[dict],
    style: str,
    variant: str,
    metric: str,
) -> np.ndarray:
    """Collect all individual run values for a (style, variant, metric) triple.

    Returns values sorted by (scenario, run) for consistent pairing.
    """
    recs = [
        r for r in records
        if r.get("style") == style
        and r.get("variant") == variant
        and r.get(metric) is not None
        and not r.get("deduped", False)
    ]
    # Sort by (scenario, run) so paired tests line up.
    recs.sort(key=lambda r: (r.get("scenario", ""), r.get("run", 0)))
    return np.array([r[metric] for r in recs], dtype=np.float64)


def main() -> int:
    print("Loading JSONL data...")
    records = load_jsonl(INPUT_DIR)
    print(f"  {len(records)} records from {INPUT_DIR}")

    print("\nFiltering to latest session per style...")
    filtered = filter_latest_session(records)
    print(f"  {len(filtered)} records after filtering")

    # Discover styles and variants.
    styles = sorted({r["style"] for r in filtered if r.get("style") and r["style"] not in EXCLUDED_STYLES})
    variants_by_style: dict[str, list[str]] = {}
    for style in styles:
        vs = sorted(
            {r["variant"] for r in filtered if r.get("style") == style},
            key=lambda v: parse_variant(v)[0],
        )
        variants_by_style[style] = vs

    print(f"\nStyles: {styles}")
    for s in styles:
        print(f"  {s}: {len(variants_by_style[s])} variants")

    # Compute CIs for each (style, variant, metric).
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    csv_path = OUTPUT_DIR / "confidence_intervals.csv"
    header = [
        "style", "variant", "step", "metric",
        "median", "ci_lo", "ci_hi",
        "iqr_lo", "iqr_hi",
        "n_runs", "wilcoxon_p",
    ]

    rows: list[list] = []
    # Also collect data for key claim diagnostics.
    claim_data: dict[tuple[str, str, str], dict] = {}

    print("\nComputing bootstrap CIs (10,000 resamples each)...")
    total_combos = sum(len(vs) * 2 for vs in variants_by_style.values())
    done = 0

    for style in styles:
        baseline_variant = variants_by_style[style][0]

        for variant in variants_by_style[style]:
            step_num, pass_name = parse_variant(variant)

            for metric in ("loadMs", "fps"):
                vals = collect_values(filtered, style, variant, metric)

                if len(vals) == 0:
                    # Deduped step -- skip, no independent measurements.
                    done += 1
                    continue

                med, ci_lo, ci_hi = bootstrap_ci(vals)
                q1 = float(np.percentile(vals, 25))
                q3 = float(np.percentile(vals, 75))

                # Wilcoxon test vs baseline.
                wilcoxon_p = None
                if variant != baseline_variant:
                    baseline_vals = collect_values(filtered, style, baseline_variant, metric)
                    if len(baseline_vals) > 0:
                        wilcoxon_p = wilcoxon_test(baseline_vals, vals)

                rows.append([
                    style, variant, step_num, metric,
                    f"{med:.4f}",
                    f"{ci_lo:.4f}",
                    f"{ci_hi:.4f}",
                    f"{q1:.4f}",
                    f"{q3:.4f}",
                    len(vals),
                    f"{wilcoxon_p:.6f}" if wilcoxon_p is not None else "",
                ])

                # Store for claim diagnostics.
                claim_data[(style, variant, metric)] = {
                    "median": med,
                    "ci_lo": ci_lo,
                    "ci_hi": ci_hi,
                    "n": len(vals),
                    "wilcoxon_p": wilcoxon_p,
                }

                done += 1

            # Progress.
            if done % 20 == 0 or done == total_combos:
                print(f"  {done}/{total_combos} computed...", end="\r")

    print(f"\n  Done: {len(rows)} rows computed.")

    # Write CSV.
    with csv_path.open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(header)
        w.writerows(rows)
    print(f"\nCSV written to {csv_path}")

    # ---------------------------------------------------------------------------
    # Summary diagnostics: statistical significance of key claims
    # ---------------------------------------------------------------------------

    print("\n" + "=" * 72)
    print("STATISTICAL SIGNIFICANCE DIAGNOSTICS")
    print("=" * 72)

    for claim in KEY_CLAIMS:
        style = claim["style"]
        variant = claim["variant"]
        metric = claim["metric"]
        direction = claim["direction"]
        label = claim["label"]

        baseline_key = (style, variants_by_style[style][0], metric)
        variant_key = (style, variant, metric)

        if baseline_key not in claim_data or variant_key not in claim_data:
            print(f"\n  {label}")
            print(f"    SKIPPED: no data (deduped step or missing)")
            continue

        b = claim_data[baseline_key]
        v = claim_data[variant_key]

        if direction == "decrease":
            change_pct = (v["median"] - b["median"]) / b["median"] * 100
            better = change_pct < 0
            sign = "decrease" if change_pct < 0 else "increase"
        else:
            change_pct = (v["median"] - b["median"]) / b["median"] * 100
            better = change_pct > 0
            sign = "increase" if change_pct > 0 else "decrease"

        p = v.get("wilcoxon_p")
        sig = p is not None and p < 0.05

        print(f"\n  {label}")
        print(f"    Baseline median: {b['median']:.1f} (n={b['n']})")
        print(f"    Variant  median: {v['median']:.1f} (n={v['n']})")
        print(f"    Change: {change_pct:+.1f}% ({sign})")
        print(f"    95% CI: [{v['ci_lo']:.1f}, {v['ci_hi']:.1f}]")
        if p is not None:
            print(f"    Wilcoxon p-value: {p:.2e}")
            if sig and better:
                print(f"    --> SIGNIFICANT and in expected direction (p < 0.05)")
            elif sig and not better:
                print(f"    --> SIGNIFICANT but OPPOSITE to expected direction (p < 0.05)")
            else:
                print(f"    --> NOT significant at alpha=0.05")
        else:
            print(f"    Wilcoxon p-value: N/A (insufficient non-zero differences)")

    # Summary of CIs for full-pipeline styles.
    print("\n" + "-" * 72)
    print("FULL-PIPELINE LOAD TIME SUMMARY")
    print("-" * 72)

    for style in sorted(FULL_PIPELINE_STYLES):
        if style not in variants_by_style:
            continue
        baseline_key = (style, variants_by_style[style][0], "loadMs")
        final_variant = variants_by_style[style][-1]
        final_key = (style, final_variant, "loadMs")

        if baseline_key not in claim_data or final_key not in claim_data:
            continue

        b = claim_data[baseline_key]
        v = claim_data[final_key]

        reduction_pct = (b["median"] - v["median"]) / b["median"] * 100
        # CI on reduction: worst case is baseline_ci_lo vs variant_ci_hi.
        red_lo = (b["median"] - v["ci_hi"]) / b["median"] * 100
        red_hi = (b["median"] - v["ci_lo"]) / b["median"] * 100

        print(f"\n  {style}:")
        print(f"    Baseline: {b['median']:.1f} ms (95% CI: [{b['ci_lo']:.1f}, {b['ci_hi']:.1f}])")
        print(f"    Final:    {v['median']:.1f} ms (95% CI: [{v['ci_lo']:.1f}, {v['ci_hi']:.1f}])")
        print(f"    Reduction: {reduction_pct:.1f}% (approx CI: [{red_lo:.1f}%, {red_hi:.1f}%])")

    print()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
