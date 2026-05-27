# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "pandas>=2.0",
#     "scipy>=1.10",
# ]
# ///
"""Generate cross-style summary + per-style CSVs from cross_style.jsonl.

Emits two CSVs and prints LaTeX-ready stats used by §5.4 of the thesis:
  - cross_style_per_style.csv  one row per style, with all metrics
  - cross_style_summary.csv    one row per metric, with min/p25/median/p75/max
"""

import argparse
import csv
import json
from pathlib import Path
from typing import Any

import numpy as np
import pandas as pd
from scipy import stats

SCRIPT_DIR = Path(__file__).resolve().parent
DATA_DIR = SCRIPT_DIR / "data"

SUMMARY_METRICS: list[tuple[str, str, str]] = [
    ("raw", "Raw size reduction (%)", "reduction_pct"),
    ("gzip", "Gzip size reduction (%)", "gzip_reduction_pct"),
    ("brotli", "Brotli size reduction (%)", "brotli_reduction_pct"),
    ("layers", "Layer count reduction (%)", "layer_reduction_pct"),
    ("ast", "AST node reduction (%)", "ast_reduction_pct"),
]


def load_jsonl(path: Path) -> pd.DataFrame:
    rows: list[dict[str, Any]] = []
    with path.open() as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    if not rows:
        raise SystemExit(f"No data found in {path}")
    return pd.DataFrame(rows)


def safe_reduction(orig: pd.Series, opt: pd.Series) -> pd.Series:
    o = orig.astype(float).to_numpy()
    p = opt.astype(float).to_numpy()
    pct = np.where(o > 0, (1.0 - p / np.where(o > 0, o, 1.0)) * 100.0, 0.0)
    return pd.Series(pct, index=orig.index)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, default=DATA_DIR / "cross_style.jsonl")
    parser.add_argument("--per-style-out", type=Path,
                        default=DATA_DIR / "cross_style_per_style.csv")
    parser.add_argument("--summary-out", type=Path,
                        default=DATA_DIR / "cross_style_summary.csv")
    args = parser.parse_args()

    df = load_jsonl(args.input)
    df["layer_reduction_pct"] = safe_reduction(
        pd.Series(df["original_layer_count"]),
        pd.Series(df["optimized_layer_count"]),
    )
    df["ast_reduction_pct"] = safe_reduction(
        pd.Series(df["original_ast_nodes"]),
        pd.Series(df["optimized_ast_nodes"]),
    )

    # ── per-style CSV ───────────────────────────────────────────────────────
    per_style_cols = [
        "style_id", "style_title",
        "original_bytes", "optimized_bytes", "reduction_pct",
        "original_gzip_bytes", "optimized_gzip_bytes", "gzip_reduction_pct",
        "original_brotli_bytes", "optimized_brotli_bytes", "brotli_reduction_pct",
        "original_layer_count", "optimized_layer_count", "layer_reduction_pct",
        "original_ast_nodes", "optimized_ast_nodes", "ast_reduction_pct",
        "original_max_depth", "optimized_max_depth",
        "original_filter_count", "optimized_filter_count",
    ]
    cols = [c for c in per_style_cols if c in df.columns]
    per_style: pd.DataFrame = pd.DataFrame(df[cols]).copy()
    per_style = per_style.sort_values(by=["reduction_pct"], ascending=False)
    args.per_style_out.parent.mkdir(parents=True, exist_ok=True)
    per_style.to_csv(args.per_style_out, index=False, float_format="%.4f")

    # ── summary CSV ─────────────────────────────────────────────────────────
    summary_rows: list[dict[str, Any]] = []
    for key, label, col in SUMMARY_METRICS:
        s = df[col].astype(float)
        summary_rows.append({
            "metric": key,
            "label": label,
            "min": s.min(),
            "p25": s.quantile(0.25),
            "median": s.median(),
            "p75": s.quantile(0.75),
            "max": s.max(),
        })
    with args.summary_out.open("w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=["metric", "label", "min", "p25",
                                          "median", "p75", "max"])
        w.writeheader()
        for r in summary_rows:
            w.writerow({
                "metric": r["metric"],
                "label": r["label"],
                **{k: f"{r[k]:.4f}" for k in ("min", "p25", "median", "p75", "max")},
            })

    print(f"Wrote {args.per_style_out}")
    print(f"Wrote {args.summary_out}")
    print()

    # ── LaTeX-ready stats ───────────────────────────────────────────────────
    print(f"n = {len(df)} styles")
    print()
    print("Summary (min / P25 / median / P75 / max):")
    for r in summary_rows:
        print(f"  {r['label']:30s} "
              f"{r['min']:6.1f}  {r['p25']:6.1f}  {r['median']:6.1f}  "
              f"{r['p75']:6.1f}  {r['max']:6.1f}")
    print()

    sorted_raw = df.sort_values("reduction_pct")
    print("Bottom 3 (raw reduction):")
    for _, row in sorted_raw.head(3).iterrows():
        print(f"  {row['style_title']:24s}  {row['reduction_pct']:5.1f}%  "
              f"(AST nodes: {int(row['original_ast_nodes'])})")
    print("Top 3 (raw reduction):")
    for _, row in sorted_raw.tail(3).iloc[::-1].iterrows():
        print(f"  {row['style_title']:24s}  {row['reduction_pct']:5.1f}%  "
              f"(AST nodes: {int(row['original_ast_nodes'])})")
    print()

    # Pearson correlation: AST nodes vs raw reduction
    r_ast, p_ast = stats.pearsonr(df["original_ast_nodes"], df["reduction_pct"])
    r_layers, p_layers = stats.pearsonr(df["original_layer_count"], df["reduction_pct"])
    print(f"Pearson r (AST nodes vs raw reduction):   r = {r_ast:+.3f}, p = {p_ast:.3f}")
    print(f"Pearson r (layer count vs raw reduction): r = {r_layers:+.3f}, p = {p_layers:.3f}")
    print()

    # Spot-check values cited in chapter prose
    am = df[df["style_id"] == "americana"].iloc[0]
    print(f"Americana original AST nodes: {int(am['original_ast_nodes'])}")
    print(f"Americana raw reduction:      {am['reduction_pct']:.1f}%")


if __name__ == "__main__":
    main()
