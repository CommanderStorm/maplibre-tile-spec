# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "plotly>=6.0",
#     "pandas>=2.0",
#     "kaleido>=0.4",
# ]
# ///
"""Generate a per-style tile shaving effectiveness figure for the thesis.

Reads the JSONL produced by verify_tile_shave.ts and produces a horizontal
bar chart showing tile-data reduction (%) per style, sorted by effectiveness.

Usage:
    uv run generate_tile_shave_per_style.py
    uv run generate_tile_shave_per_style.py --input data/tile_shave_per_style.jsonl
"""

import argparse
import json
from pathlib import Path

import pandas as pd
import plotly.graph_objects as go

SCRIPT_DIR = Path(__file__).resolve().parent
DATA_DIR = SCRIPT_DIR / "data"
OUTPUT_DIR = SCRIPT_DIR.parent / "figures"
FORMATS = ["png", "pdf"]

FIG_WIDTH = 700
FIG_HEIGHT = 400

# Match thesis palette
BAR_COLOR = "#5E94D4"       # light blue
BAR_COLOR_ALT = "#8F81EA"   # purple

LAYOUT_DEFAULTS = dict(
    template="plotly_white",
    font=dict(family="Helvetica, Arial, sans-serif", size=12),
    margin=dict(l=110, r=60, t=40, b=60),
)


def export_figure(fig: go.Figure, name: str) -> None:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    for fmt in FORMATS:
        path = OUTPUT_DIR / f"{name}.{fmt}"
        fig.write_image(str(path), scale=2, width=FIG_WIDTH, height=FIG_HEIGHT)
        print(f"  -> {path}")


def fmt_bytes(val: float) -> str:
    if val >= 1e9:
        return f"{val / 1e9:.2f} GB"
    if val >= 1e6:
        return f"{val / 1e6:.1f} MB"
    if val >= 1e3:
        return f"{val / 1e3:.0f} KB"
    return f"{val:.0f} B"


def load_jsonl(path: Path) -> pd.DataFrame:
    rows = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    if not rows:
        raise SystemExit(f"No data found in {path}")
    return pd.DataFrame(rows)


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate tile shave per-style figure.")
    parser.add_argument(
        "--input", type=Path,
        default=DATA_DIR / "tile_shave_per_style.jsonl",
        help="JSONL input file",
    )
    args = parser.parse_args()

    df = load_jsonl(args.input)
    df = df.sort_values("reduction_pct", ascending=True)

    print(f"Loaded {len(df)} styles")
    print(f"  Reduction range: {df['reduction_pct'].min():.1f}% - {df['reduction_pct'].max():.1f}%")
    print(f"  Mean reduction:  {df['reduction_pct'].mean():.1f}%")
    print(f"  Median reduction: {df['reduction_pct'].median():.1f}%")
    print()

    # ── horizontal bar chart: % tile data reduction per style ────────────────

    fig = go.Figure()

    fig.add_trace(go.Bar(
        y=df["style_id"],
        x=df["reduction_pct"],
        orientation="h",
        marker_color=BAR_COLOR,
        text=[f"{v:.1f}%" for v in df["reduction_pct"]],
        textposition="outside",
        textfont=dict(size=10),
    ))

    fig.update_layout(
        **LAYOUT_DEFAULTS,
        xaxis_title="Tile data reduction (%)",
        xaxis=dict(range=[0, max(df["reduction_pct"]) * 1.15]),
        yaxis_title=None,
        showlegend=False,
    )

    print("Generating tile_shave_per_style figure:")
    export_figure(fig, "tile_shave_per_style")
    print()

    # ── summary stats for thesis text ────────────────────────────────────────

    print("LaTeX-ready stats:")
    print(f"  Min reduction:  {df['reduction_pct'].min():.1f}%  ({df.iloc[0]['style_id']})")
    print(f"  Max reduction:  {df['reduction_pct'].max():.1f}%  ({df.iloc[-1]['style_id']})")
    print(f"  Mean reduction: {df['reduction_pct'].mean():.1f}%")
    print(f"  Median:         {df['reduction_pct'].median():.1f}%")
    print(f"  Total dropped:  {df['tiles_dropped'].sum()} tiles across all styles")
    print()

    for _, row in df.iterrows():
        print(f"  {row['style_id']:20s}  {fmt_bytes(row['original_bytes'])} -> {fmt_bytes(row['pruned_bytes'])}  "
              f"({row['reduction_pct']:.1f}%, {int(row['tiles_dropped'])} dropped)")


if __name__ == "__main__":
    main()
