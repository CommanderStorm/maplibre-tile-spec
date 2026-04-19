# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "plotly>=6.0",
#     "pandas>=2.0",
#     "numpy>=1.24",
#     "kaleido>=0.4",
# ]
# ///
"""Read tile-size CSVs and benchmark JSONL to generate plotly figures for the thesis."""

import argparse
import json
import sys
from pathlib import Path

import numpy as np
import pandas as pd
import plotly.graph_objects as go
from plotly.subplots import make_subplots

SCRIPT_DIR = Path(__file__).resolve().parent
DATA_DIR = SCRIPT_DIR / "data"
OUTPUT_DIR = SCRIPT_DIR.parent / "figures"
FORMATS = ["png", "pdf"]

FIG_WIDTH = 700
FIG_HEIGHT = 450

# Colorblind-friendly palette (Tol muted)
SOURCE_COLORS = {
    "MVT": "#332288",       # indigo
    "MLT-Java": "#CC6677",  # rose
    "MLT-Rust": "#44AA99",  # teal
}

COMPRESSION_DASHES = {
    "plain": "solid",
    "gzip": "dash",
    "brotli": "dot",
    "zstd": "dashdot",
}

LAYOUT_DEFAULTS = dict(
    template="plotly_white",
    font=dict(size=12),
    margin=dict(l=70, r=30, t=40, b=60),
)


STEP_LABELS: dict[int, str] = {
    0: "Baseline",
    1: "Unary simpl.",
    2: "Kind norm.",
    3: "Const. fold",
    4: "Stats fold",
    5: "Expr. simpl.",
    6: "Strip defaults",
    7: "Minify colours",
    8: "Strip metadata",
    9: "Dead elim.",
    10: "DE stats",
    11: "Meta. refine.",
    12: "MR paint",
    13: "MR stats",
    14: "Cleanup",
    15: "Layer merge",
    16: "Selectivity",
    17: "Shave only",
    18: "Shave",
    19: "MLT rewrite",
}

# Full-pipeline steps (only available for fiord/liberty)
FULL_PIPELINE_STEPS = {16, 17, 18, 19}


def _bootstrap_ci(
    values: np.ndarray, n_boot: int = 10_000, seed: int = 42,
) -> tuple[float, float, float]:
    """Percentile-bootstrap 95% CI for the median.  Returns (median, ci_lo, ci_hi)."""
    arr = np.asarray(values, dtype=np.float64)
    arr = arr[~np.isnan(arr)]
    n = len(arr)
    if n < 2:
        m = float(np.median(arr)) if n == 1 else 0.0
        return m, m, m
    rng = np.random.default_rng(seed)
    meds = np.empty(n_boot)
    for i in range(n_boot):
        meds[i] = np.median(rng.choice(arr, n, replace=True))
    return (
        float(np.median(arr)),
        float(np.percentile(meds, 2.5)),
        float(np.percentile(meds, 97.5)),
    )


def export_figure(fig: go.Figure, name: str) -> None:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    for fmt in FORMATS:
        path = OUTPUT_DIR / f"{name}.{fmt}"
        fig.write_image(str(path), scale=2, width=FIG_WIDTH, height=FIG_HEIGHT)
        print(f"  → {path}")


def fmt_bytes(val: float) -> str:
    if val >= 1e9:
        return f"{val / 1e9:.1f} GB"
    if val >= 1e6:
        return f"{val / 1e6:.1f} MB"
    if val >= 1e3:
        return f"{val / 1e3:.0f} KB"
    return f"{val:.0f} B"


def plot_size_total_per_zoom(df: pd.DataFrame) -> None:
    """Grouped bar chart: total tile size per zoom, one group per source+compression."""
    print("Generating size_total_per_zoom…")

    fig = make_subplots(
        rows=2, cols=2,
        subplot_titles=["Plain (uncompressed)", "Gzip", "Brotli", "Zstd"],
        shared_xaxes=True,
        shared_yaxes=True,
        vertical_spacing=0.12,
        horizontal_spacing=0.08,
    )

    positions = {"plain": (1, 1), "gzip": (1, 2), "brotli": (2, 1), "zstd": (2, 2)}
    show_legend_for = set()

    for comp, (row, col) in positions.items():
        sub = df[df["compression"] == comp]
        for source in ["MVT", "MLT-Java", "MLT-Rust"]:
            s = sub[sub["source"] == source].sort_values("zoom")
            if s.empty:
                continue
            show = source not in show_legend_for
            show_legend_for.add(source)
            fig.add_trace(go.Bar(
                x=s["zoom"],
                y=s["total_bytes"],
                name=source,
                marker_color=SOURCE_COLORS[source],
                showlegend=show,
                legendgroup=source,
            ), row=row, col=col)

    fig.update_layout(
        **LAYOUT_DEFAULTS,
        barmode="group",
        legend=dict(x=0.02, y=1.12, orientation="h", bgcolor="rgba(255,255,255,0.8)"),
        height=600,
    )
    fig.update_xaxes(title_text="Zoom level", dtick=2, row=2)
    fig.update_yaxes(type="log", row=1, col=1, title_text="Total size (bytes)")
    fig.update_yaxes(type="log", row=2, col=1, title_text="Total size (bytes)")
    fig.update_yaxes(type="log", row=1, col=2)
    fig.update_yaxes(type="log", row=2, col=2)

    export_figure(fig, "size_total_per_zoom")


def plot_compression_ratio_per_zoom(df: pd.DataFrame) -> None:
    """Line chart: size ratio (MLT-Java/MVT and MLT-Rust/MVT) per compression method."""
    print("Generating compression_ratio_per_zoom…")

    fig = go.Figure()

    for comp in ["plain", "gzip", "brotli", "zstd"]:
        sub = df[df["compression"] == comp]
        mvt = sub[sub["source"] == "MVT"].set_index("zoom")["total_bytes"]

        for source in ["MLT-Java", "MLT-Rust"]:
            src = sub[sub["source"] == source].set_index("zoom")["total_bytes"]
            if src.empty:
                continue
            common = mvt.index.intersection(src.index)
            ratio = src.loc[common] / mvt.loc[common]

            fig.add_trace(go.Scatter(
                x=list(common),
                y=ratio.values,
                mode="lines+markers",
                name=f"{source} / {comp}",
                marker=dict(color=SOURCE_COLORS[source], size=6),
                line=dict(color=SOURCE_COLORS[source], width=2, dash=COMPRESSION_DASHES[comp]),
                hovertemplate=f"{source} ({comp})<br>Zoom %{{x}}<br>Ratio: %{{y:.3f}}<extra></extra>",
            ))

    fig.add_hline(y=1.0, line_dash="dash", line_color="grey", annotation_text="1.0 (parity)")

    fig.update_layout(
        **LAYOUT_DEFAULTS,
        xaxis=dict(title="Zoom level", dtick=1),
        yaxis=dict(title="Size ratio vs MVT"),
        legend=dict(x=0.02, y=0.98, bgcolor="rgba(255,255,255,0.8)"),
    )

    export_figure(fig, "compression_ratio_per_zoom")


MODE_COLORS = {
    "hybrid": "#44AA99",      # teal
    "only-trigram": "#CC6677", # rose
    "only-plain": "#332288",   # indigo
}


def plot_minhash_sweep(df: pd.DataFrame) -> None:
    """Grouped bar chart: total mbtiles size per (mode, threshold)."""
    print("Generating minhash_sweep…")

    fig = go.Figure()

    for mode in ["hybrid", "only-trigram", "only-plain"]:
        sub = df[df["mode"] == mode].sort_values("threshold")
        fig.add_trace(go.Bar(
            x=sub["threshold"],
            y=sub["total_bytes"],
            name=mode,
            marker_color=MODE_COLORS[mode],
        ))

    y_min = df["total_bytes"].min()
    y_max = df["total_bytes"].max()
    y_pad = (y_max - y_min) * 0.3
    fig.update_layout(
        **LAYOUT_DEFAULTS,
        barmode="group",
        xaxis=dict(title="Jaccard similarity threshold", dtick=0.025),
        yaxis=dict(
            title="Total mbtiles size (bytes)",
            range=[y_min - y_pad, y_max + y_pad],
        ),
        legend=dict(x=0.02, y=0.98, bgcolor="rgba(255,255,255,0.8)"),
    )

    export_figure(fig, "minhash_sweep")


def _load_ci_levels() -> pd.DataFrame | None:
    """Load confidence_intervals.csv and return a (style, step) → loadMs pivot.

    Missing (deduped) steps are forward-filled so that every step 0..max has a value.
    """
    ci_csv = DATA_DIR / "confidence_intervals.csv"
    if not ci_csv.exists():
        return None
    ci = pd.read_csv(ci_csv)
    load = ci[ci["metric"] == "loadMs"][["style", "step", "median"]].copy()
    pivot = load.pivot_table(index="step", columns="style", values="median")
    all_steps = range(int(pivot.index.min()), int(pivot.index.max()) + 1)
    pivot = pivot.reindex(all_steps).ffill()
    return pivot


def plot_waterfall_loadMs() -> None:
    """Waterfall chart: cumulative load-time change per ablation step with bootstrap 95% CI."""
    print("Generating waterfall_loadMs…")
    pivot = _load_ci_levels()
    if pivot is None:
        print("  Skipped (run generate_ci.py first)")
        return

    baseline = pivot.loc[0]
    # Percentage change from baseline per style
    pct = (pivot.subtract(baseline)) / baseline * 100

    steps = sorted(pct.index)
    labels = [STEP_LABELS.get(int(s), f"Step {s}") for s in steps]

    # Per-step cross-style bootstrap CI of cumulative percentage change
    cum_med = []
    for s in steps:
        vals = pct.loc[s].dropna().values
        m, _, _ = _bootstrap_ci(vals)
        cum_med.append(m)

    # Compute per-step deltas and their CIs
    deltas, delta_err_lo, delta_err_hi = [], [], []
    for i, s in enumerate(steps):
        if i == 0:
            # Step 0 is baseline → 0% change
            deltas.append(0.0)
            delta_err_lo.append(0.0)
            delta_err_hi.append(0.0)
            continue
        prev = steps[i - 1]
        # Per-style delta between consecutive steps
        per_style_delta = (pct.loc[s] - pct.loc[prev]).dropna().values
        m, lo, hi = _bootstrap_ci(per_style_delta)
        deltas.append(m)
        delta_err_lo.append(m - lo)
        delta_err_hi.append(hi - m)

    # Build waterfall using stacked bars (base + delta)
    # Compute running total for bar bases
    running = 0.0
    bases, heights = [], []
    colors = []
    for i, d in enumerate(deltas):
        if i == 0:
            bases.append(0.0)
            heights.append(0.0)
            colors.append("#AAAAAA")  # neutral baseline
        elif d <= 0:
            bases.append(running + d)
            heights.append(abs(d))
            colors.append("#44AA99")  # teal = improvement
        else:
            bases.append(running)
            heights.append(d)
            colors.append("#CC6677")  # rose = regression
        running += d

    fig = go.Figure()
    # Invisible base bars
    fig.add_trace(go.Bar(
        x=labels, y=bases,
        marker_color="rgba(0,0,0,0)", showlegend=False,
        hoverinfo="skip",
    ))
    # Visible delta bars
    fig.add_trace(go.Bar(
        x=labels, y=heights,
        marker_color=colors, showlegend=False,
        error_y=dict(
            type="data", symmetric=False,
            array=delta_err_hi, arrayminus=delta_err_lo,
            visible=True,
        ),
        hovertemplate="%{x}<br>Δ: %{y:.1f}%<extra></extra>",
    ))

    fig.update_layout(
        **LAYOUT_DEFAULTS,
        barmode="stack",
        xaxis=dict(tickangle=45),
        yaxis=dict(title="Cumulative load-time change (%)"),
    )
    export_figure(fig, "waterfall_loadMs")


def plot_marginal_loadMs() -> None:
    """Bar chart: marginal load-time contribution per ablation step with bootstrap 95% CI."""
    print("Generating marginal_loadMs…")
    pivot = _load_ci_levels()
    if pivot is None:
        print("  Skipped (run generate_ci.py first)")
        return

    baseline = pivot.loc[0]
    pct = (pivot.subtract(baseline)) / baseline * 100

    steps = sorted(pct.index)

    # Compute per-step deltas (skip baseline)
    plot_steps, plot_labels = [], []
    deltas, err_lo, err_hi = [], [], []
    colors = []

    for i, s in enumerate(steps):
        if i == 0:
            continue
        prev = steps[i - 1]
        per_style_delta = (pct.loc[s] - pct.loc[prev]).dropna().values
        m, lo, hi = _bootstrap_ci(per_style_delta)

        # Skip near-zero deltas (deduped steps)
        if abs(m) < 0.05 and abs(lo) < 0.1 and abs(hi) < 0.1:
            continue

        plot_steps.append(s)
        plot_labels.append(STEP_LABELS.get(int(s), f"Step {s}"))
        deltas.append(m)
        err_lo.append(m - lo)
        err_hi.append(hi - m)
        colors.append("#44AA99" if m <= 0 else "#CC6677")

    fig = go.Figure()
    fig.add_trace(go.Bar(
        x=plot_labels, y=deltas,
        marker_color=colors, showlegend=False,
        error_y=dict(
            type="data", symmetric=False,
            array=err_hi, arrayminus=err_lo,
            visible=True,
        ),
        hovertemplate="%{x}<br>Δ: %{y:.1f}%<extra></extra>",
    ))

    fig.update_layout(
        **LAYOUT_DEFAULTS,
        xaxis=dict(tickangle=45),
        yaxis=dict(title="Marginal load-time change (%)"),
    )
    export_figure(fig, "marginal_loadMs")


def main() -> None:
    tile_sizes_csv = DATA_DIR / "tile_sizes.csv"
    if not tile_sizes_csv.exists():
        sys.exit(f"tile_sizes.csv not found at {tile_sizes_csv}\nRun generate_data.py first.")

    df = pd.read_csv(tile_sizes_csv)

    plot_size_total_per_zoom(df)
    plot_compression_ratio_per_zoom(df)

    # MinHash sweep plot (optional — generated by generate_minhash_sweep.py)
    sweep_csv = DATA_DIR / "minhash_sweep.csv"
    if sweep_csv.exists():
        sweep_df = pd.read_csv(sweep_csv)
        plot_minhash_sweep(sweep_df)
    else:
        print("\nSkipped: minhash_sweep (run generate_minhash_sweep.py first)")

    # Benchmark-derived plots (interaction and rendering metrics across configs 1-5)
    parser = argparse.ArgumentParser(description="Generate thesis figures.")
    parser.add_argument("--bench", type=Path, nargs="*", help="Benchmark JSONL file(s) with tile_shave variants")
    args, _ = parser.parse_known_args()

    if args.bench:
        bench_df = load_bench_jsonl(args.bench)
        if bench_df is not None:
            plot_interaction(bench_df)
            plot_rendering_metrics_mlt(bench_df)
    else:
        print("\nSkipped: interaction_plot (pass --bench <jsonl> with tile_shave data)")
        print("Skipped: rendering_metrics_mlt (pass --bench <jsonl> with tile_shave data)")

    # CI-derived plots (waterfall and marginal) — use precomputed confidence_intervals.csv
    plot_waterfall_loadMs()
    plot_marginal_loadMs()

    print(f"\nAll figures written to {OUTPUT_DIR}")


# ── Benchmark JSONL helpers ──────────────────────────────────────────────────

# Map benchmark variant names to thesis configuration labels.
# The "style-only" config is the last cumulative style pass before tile steps;
# we match both step-15 (without selectivity) and step-16 (with selectivity).
CONFIG_MAP = {
    "step-00-baseline": "1: MVT baseline",
    "step-15-layer_merge": "2: Style-only",
    "step-16-selectivity_reorder": "2: Style-only",
    "step-17-tile_shave_only": "3: Shaving-only",
    "step-18-tile_shave": "4: Style+shaving",
    "step-19-tile_rewrite": "5: Style+shaving+MLT",
}

CONFIG_COLORS = {
    "1: MVT baseline": "#332288",
    "2: Style-only": "#CC6677",
    "3: Shaving-only": "#88CCEE",
    "4: Style+shaving": "#44AA99",
    "5: Style+shaving+MLT": "#117733",
}


def load_bench_jsonl(paths: list[Path]) -> pd.DataFrame | None:
    """Load benchmark JSONL and filter to the 5 thesis configurations."""
    rows = []
    for p in paths:
        with open(p) as f:
            for line in f:
                line = line.strip()
                if line:
                    rows.append(json.loads(line))
    if not rows:
        print("No benchmark data found.", file=sys.stderr)
        return None
    df = pd.DataFrame(rows)
    # Keep only the 5 thesis configurations
    df = df[df["variant"].isin(CONFIG_MAP)]
    if df.empty:
        print("Benchmark JSONL has no tile_shave variants — skipping interaction/rendering plots.")
        return None
    df["config"] = df["variant"].map(CONFIG_MAP)
    print(f"\nLoaded {len(df)} benchmark records for configs: {sorted(df['config'].unique())}")
    return df


def plot_interaction(df: pd.DataFrame) -> None:
    """Bar chart showing style-only, shaving-only, and style+shaving reductions.

    If the combined bar exceeds the sum of the two individual bars, the
    optimisations are synergistic."""
    print("Generating interaction_plot…")

    # Compute median gzip_bytes per config across all (style, scenario) pairs
    medians = df.groupby(["config", "style", "scenario"])["gzip_bytes"].median().reset_index()

    # Pivot to get one column per config
    baseline = medians[medians.config == "1: MVT baseline"].groupby("style")["gzip_bytes"].median()
    style_only = medians[medians.config == "2: Style-only"].groupby("style")["gzip_bytes"].median()
    shave_only = medians[medians.config == "3: Shaving-only"].groupby("style")["gzip_bytes"].median()
    combined = medians[medians.config == "4: Style+shaving"].groupby("style")["gzip_bytes"].median()

    styles = sorted(baseline.index)
    if not styles:
        print("  (skipped — not enough config data)")
        return

    style_pct = [(1 - style_only.get(s, baseline[s]) / baseline[s]) * 100 for s in styles]
    shave_pct = [(1 - shave_only.get(s, baseline[s]) / baseline[s]) * 100 for s in styles]
    combined_pct = [(1 - combined.get(s, baseline[s]) / baseline[s]) * 100 for s in styles]
    sum_pct = [s + h for s, h in zip(style_pct, shave_pct)]

    fig = go.Figure()
    fig.add_trace(go.Bar(name="Style-only", x=styles, y=style_pct,
                         marker_color=CONFIG_COLORS["2: Style-only"]))
    fig.add_trace(go.Bar(name="Shaving-only", x=styles, y=shave_pct,
                         marker_color=CONFIG_COLORS["3: Shaving-only"]))
    fig.add_trace(go.Bar(name="Style+shaving (actual)", x=styles, y=combined_pct,
                         marker_color=CONFIG_COLORS["4: Style+shaving"]))
    fig.add_trace(go.Scatter(name="Sum of individual", x=styles, y=sum_pct,
                             mode="markers", marker=dict(symbol="diamond", size=10, color="black")))

    fig.update_layout(
        **LAYOUT_DEFAULTS,
        barmode="group",
        xaxis=dict(title="Style"),
        yaxis=dict(title="% Gzip Size Reduction vs Baseline"),
        legend=dict(x=0.02, y=0.98, bgcolor="rgba(255,255,255,0.8)"),
    )
    export_figure(fig, "interaction_plot")


def plot_rendering_metrics_mlt(df: pd.DataFrame) -> None:
    """Grouped bar chart with bootstrap 95% CI: load time and FPS across the 5 thesis configurations."""
    print("Generating rendering_metrics_mlt…")

    configs = [c for c in CONFIG_MAP.values() if c in df["config"].unique()]

    # Compute per-(style, scenario) medians, then bootstrap the cross-scenario median per config
    scenario_med = df.groupby(["config", "style", "scenario"]).agg(
        loadMs=("loadMs", "median"),
        fps=("fps", "median"),
    ).reset_index()

    load_meds, load_err_lo, load_err_hi = [], [], []
    fps_meds, fps_err_lo, fps_err_hi = [], [], []

    for config in configs:
        sub = scenario_med[scenario_med["config"] == config]
        m, lo, hi = _bootstrap_ci(sub["loadMs"].values)
        load_meds.append(m)
        load_err_lo.append(m - lo)
        load_err_hi.append(hi - m)

        m, lo, hi = _bootstrap_ci(sub["fps"].values)
        fps_meds.append(m)
        fps_err_lo.append(m - lo)
        fps_err_hi.append(hi - m)

    fig = make_subplots(rows=1, cols=2, subplot_titles=["Load Time", "Frames per Second"],
                        horizontal_spacing=0.15)

    colors = [CONFIG_COLORS.get(c, "#999") for c in configs]

    fig.add_trace(go.Bar(
        x=configs, y=load_meds,
        error_y=dict(type="data", symmetric=False, array=load_err_hi, arrayminus=load_err_lo),
        marker_color=colors, showlegend=False,
    ), row=1, col=1)

    fig.add_trace(go.Bar(
        x=configs, y=fps_meds,
        error_y=dict(type="data", symmetric=False, array=fps_err_hi, arrayminus=fps_err_lo),
        marker_color=colors, showlegend=False,
    ), row=1, col=2)

    fig.update_yaxes(title_text="Load Time (ms)", row=1, col=1)
    fig.update_yaxes(title_text="FPS", row=1, col=2)
    fig.update_xaxes(tickangle=30)
    fig.update_layout(**LAYOUT_DEFAULTS, height=500)

    export_figure(fig, "rendering_metrics_mlt")


if __name__ == "__main__":
    main()
