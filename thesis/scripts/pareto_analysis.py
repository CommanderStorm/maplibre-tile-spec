# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "plotly>=6.0",
#     "pandas>=2.0",
#     "numpy>=1.26",
#     "kaleido>=0.4",
# ]
# ///

from __future__ import annotations

import argparse
import json
import math
import re
import sys
from collections.abc import Iterable
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import pandas as pd
import plotly.graph_objects as go

# Shared thesis-figure palette and layout defaults. Imported so a future
# palette swap in ``generate_plots.py`` propagates automatically to the
# Pareto figures as well.
from generate_plots import LAYOUT_DEFAULTS, SOURCE_COLORS
from plotly.subplots import make_subplots

# Axes

# Candidate axes considered for the frontier. Only ``gzip_bytes`` passes
# the noise-floor bar; the other three are measured for the noise report
# and then excluded (see thesis §4.X).
CANDIDATE_AXES: list[str] = [
    "gzip_bytes",
    "loadMs",
    "meanFrameMs",
    "peakHeapMB",
]

# Human labels for figure axes only. The noise-report table now uses
# \fmtaxis{} in LaTeX, so only the frontier-plot axes need labels here.
AXIS_LABELS: dict[str, str] = {
    "pass_count": "enabled passes",
    "preprocessing_ms": "preprocessing (ms)",
}

# Verdict column of the noise-floor table, keyed in the same order as
# CANDIDATE_AXES and emitted by ``write_noise_csv``.
AXIS_VERDICTS: dict[str, str] = {
    "gzip_bytes": "reliable (deterministic)",
    "loadMs": "noise comparable to signal",
    "meanFrameMs": "noise comparable to signal",
    "peakHeapMB": "borderline (excluded for consistency)",
}

# Frontier-plot colours. The accent comes from the shared thesis palette
# (``generate_plots.SOURCE_COLORS``); the dominated marker colour is a
# neutral grey that doesn't need to live in the palette.
FRONTIER_COLOUR = SOURCE_COLORS["MLT-Java"]  # orange
DOMINATED_COLOUR = "#BBBBBB"
DOMINATED_OUTLINE = "#666666"

# Variant ID conventions used by ``tests/bench/run.ts``. A variant ID is
# one of ``step-NN-<pass>`` (cumulative), ``step-00-baseline``, or
# ``isolated-NN-<pass>``. Parsing is centralised in ``parse_variant``.
BASELINE_VARIANT = "step-00-baseline"
_STEP_RE = re.compile(r"^step-(\d+)-(.+)$")
_ISOLATED_RE = re.compile(r"^isolated-(\d+)-(.+)$")


@dataclass(frozen=True)
class VariantId:
    """Parsed representation of a benchmark variant identifier.

    ``mode`` is one of ``"baseline"``, ``"cumulative"``, or ``"isolated"``.
    ``step_num`` is the numeric step index embedded in the variant name
    (0 for the baseline), and ``pass_name`` is the pass name without the
    ``step-NN-`` / ``isolated-NN-`` prefix (empty for the baseline).
    """

    mode: str
    step_num: int
    pass_name: str


def parse_variant(variant: str) -> VariantId:
    """Parse a benchmark variant string into its (mode, step_num, pass)."""
    if variant == BASELINE_VARIANT:
        return VariantId(mode="baseline", step_num=0, pass_name="")
    m = _ISOLATED_RE.match(variant)
    if m is not None:
        return VariantId(
            mode="isolated", step_num=int(m.group(1)), pass_name=m.group(2)
        )
    m = _STEP_RE.match(variant)
    if m is not None:
        return VariantId(
            mode="cumulative", step_num=int(m.group(1)), pass_name=m.group(2)
        )
    # Malformed — treat as a cumulative step at position -1 so downstream
    # sorting still produces a deterministic order, but the caller can spot
    # the anomaly in the JSON summary.
    return VariantId(mode="cumulative", step_num=-1, pass_name=variant)


# Data loading


def load_jsonl(input_dir: Path) -> pd.DataFrame:
    """Load every ``*.jsonl`` file under ``input_dir`` into a single frame."""
    rows: list[dict] = []
    jsonl_paths = sorted(input_dir.glob("*.jsonl"))
    if not jsonl_paths:
        raise SystemExit(f"no JSONL files found under {input_dir}")
    for jsonl_path in jsonl_paths:
        with jsonl_path.open("r", encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                try:
                    rows.append(json.loads(line))
                except json.JSONDecodeError as exc:
                    print(
                        f"warning: skipping malformed line in {jsonl_path.name}: {exc}",
                        file=sys.stderr,
                    )
    if not rows:
        raise SystemExit(f"no JSONL records found under {input_dir}")
    return pd.DataFrame(rows)


def filter_latest_session_per_style(
    df: pd.DataFrame,
) -> tuple[pd.DataFrame, dict[str, str]]:
    """Keep only the most recent complete ablation session per style.

    The benchmark harness writes one JSONL file per run, but the results
    directory accumulates many of them over time, and the ``ABLATION_STEPS``
    table in ``run.ts`` has been reordered between sessions. Mixing them
    creates duplicate-looking variants like ``step-04-simplify_expressions``
    and ``step-05-simplify_expressions`` that are semantically the same pass
    at different ablation positions. For each style, the session with the
    most distinct variants is selected; ties are broken by the most recent
    timestamp (ISO-8601 strings compare lexicographically).

    Returns the filtered DataFrame and a ``{style: session_description}``
    map so callers can log which session was picked.
    """
    if "style" not in df.columns or "timestamp" not in df.columns:
        return df, {}
    df = df[df["style"].apply(lambda s: isinstance(s, str))].copy()
    if df.empty:
        return df, {}

    keep_rows: list[pd.DataFrame] = []
    session_log: dict[str, str] = {}
    for style, style_df in df.groupby("style"):
        session_counts = (
            style_df.groupby("timestamp")["variant"]
            .nunique()
            .sort_values(ascending=False)
        )
        if session_counts.empty:
            continue
        top_count = session_counts.iloc[0]
        candidates = session_counts[session_counts == top_count].index.tolist()
        chosen = max(candidates)
        session_log[str(style)] = f"{chosen} ({top_count} variants)"
        keep_rows.append(style_df[style_df["timestamp"] == chosen])

    filtered = (
        pd.concat(keep_rows, ignore_index=True) if keep_rows else df.iloc[0:0].copy()
    )
    return filtered, session_log


# Noise decomposition


_EMPTY_NOISE_ROW = {
    "within_cv_median": float("nan"),
    "within_cv_p95": float("nan"),
    "cross_range_median": float("nan"),
    "cross_range_p95": float("nan"),
}


def compute_noise_report(df: pd.DataFrame) -> pd.DataFrame:
    """For each candidate axis, compute within-cell CV and cross-variant range.

    Within-cell CV is the distribution of ``std / mean`` computed per
    (style, scenario, variant) across the 15 runs that populate each cell.
    Cross-variant range is the distribution of ``(max - min) / mean`` of the
    per-variant medians within each (style, scenario) pair.

    Returns a DataFrame indexed by axis with columns ``within_cv_median``,
    ``within_cv_p95``, ``cross_range_median``, ``cross_range_p95``. Axes that
    are not present in ``df`` are emitted as NaN rows so the output table
    row count always matches :data:`CANDIDATE_AXES`.
    """
    report_rows: list[dict] = []
    for axis in CANDIDATE_AXES:
        if axis not in df.columns:
            report_rows.append({"axis": axis, **_EMPTY_NOISE_ROW})
            continue

        # One groupby + agg gets us mean, std, and median per cell. Folding
        # the three stats into a single pass halves the work vs. running
        # separate aggregations.
        per_cell = (
            df.groupby(["style", "scenario", "variant"])[axis]
            .agg(["mean", "std", "median"])
            .reset_index()
        )
        # Drop cells whose mean is zero or NaN: the within-cell CV is
        # undefined there, and they would poison the cross-variant range too.
        usable = per_cell[per_cell["mean"] > 0]

        within_cv = (usable["std"] / usable["mean"]).dropna()

        # Cross-variant range: one row per (style, scenario) pair with the
        # relative spread of that pair's variant medians. Computed fully
        # vectorised via a second groupby on the per-cell frame.
        per_pair = (
            usable.groupby(["style", "scenario"])["median"]
            .agg(["min", "max", "mean", "size"])
            .reset_index()
        )
        per_pair = per_pair[(per_pair["size"] > 1) & (per_pair["mean"] > 0)]
        cross_range = (per_pair["max"] - per_pair["min"]) / per_pair["mean"]

        report_rows.append(
            {
                "axis": axis,
                "within_cv_median": _pct_or_nan(within_cv, np.median),
                "within_cv_p95": _pct_or_nan(within_cv, lambda s: np.percentile(s, 95)),
                "cross_range_median": _pct_or_nan(cross_range, np.median),
                "cross_range_p95": _pct_or_nan(
                    cross_range, lambda s: np.percentile(s, 95)
                ),
            }
        )
    return pd.DataFrame(report_rows).set_index("axis")


def _pct_or_nan(series: pd.Series, reducer) -> float:
    """Reduce ``series`` to a percentage, or ``NaN`` if the series is empty."""
    if len(series) == 0:
        return float("nan")
    return float(reducer(series) * 100)


def _fmt_noise_val(value: float) -> str:
    """Format a percentage for CSV (empty string for NaN)."""
    if math.isnan(value):
        return ""
    return f"{value:.1f}"


def write_noise_csv(path: Path, report: pd.DataFrame) -> None:
    """Write the noise-floor report as a CSV file."""
    import csv as _csv

    header = [
        "axis",
        "withinCvMedian",
        "withinCvHigh",
        "crossRangeMedian",
        "crossRangeHigh",
        "verdict",
    ]
    rows: list[list[str]] = []
    for axis in CANDIDATE_AXES:
        if axis not in report.index:
            continue
        row = report.loc[axis]
        rows.append(
            [
                axis,
                _fmt_noise_val(row["within_cv_median"]),
                _fmt_noise_val(row["within_cv_p95"]),
                _fmt_noise_val(row["cross_range_median"]),
                _fmt_noise_val(row["cross_range_p95"]),
                AXIS_VERDICTS.get(axis, ""),
            ]
        )
    with path.open("w", newline="", encoding="utf-8") as f:
        w = _csv.writer(f)
        w.writerow(header)
        w.writerows(rows)


# Aggregation and frontier


def build_variant_table(df: pd.DataFrame) -> pd.DataFrame:
    """Produce one row per (style, variant), deterministic axes only.

    The ``gzip_bytes`` value is deterministic per (style, variant): within
    a session the optimiser output is byte-identical for a given pass set,
    so the median across runs and scenarios is identical to any single
    observation. ``pass_count``, ``mode``, and ``step_num`` are derived
    from the variant ID via :func:`parse_variant` so every downstream
    consumer reads the same structured value rather than re-parsing strings.
    ``preprocessing_ms`` is emitted when the harness corpus contains it.
    """
    if "style" not in df.columns or "variant" not in df.columns:
        raise SystemExit("JSONL is missing required columns ``style``/``variant``")
    if "gzip_bytes" not in df.columns:
        raise SystemExit("JSONL is missing required column ``gzip_bytes``")

    agg_spec: dict[str, str] = {"gzip_bytes": "median"}
    if "preprocessing_ms" in df.columns:
        agg_spec["preprocessing_ms"] = "median"
    variants = df.groupby(["style", "variant"], as_index=False).agg(agg_spec)

    parsed = variants["variant"].astype(str).apply(parse_variant)
    variants["mode"] = [p.mode for p in parsed]
    variants["step_num"] = [p.step_num for p in parsed]
    # ``pass_count`` approximates preprocessing cost: for baseline it's 0,
    # for isolated it's 1, and for cumulative step ``k`` it's ``k``.
    variants["pass_count"] = [
        0 if p.mode == "baseline" else 1 if p.mode == "isolated" else p.step_num
        for p in parsed
    ]
    return variants


def non_dominated_mask(points: np.ndarray) -> np.ndarray:
    """Boolean mask of Pareto non-dominated rows over a minimise-all 2-D array.

    O(n^2), which is fine at n ≈ 35 per style. Reserves an all-true mask
    and flips rows to ``False`` as they are dominated by a surviving row.
    """
    if points.size == 0:
        return np.zeros(0, dtype=bool)
    n = points.shape[0]
    keep = np.ones(n, dtype=bool)
    for i in range(n):
        if not keep[i]:
            continue
        diff = points - points[i]
        if (np.all(diff <= 0, axis=1) & np.any(diff < 0, axis=1)).any():
            keep[i] = False
    return keep


def compute_frontier(variants: pd.DataFrame, cost_axis: str) -> pd.DataFrame:
    """Return ``variants`` with an ``is_pareto`` column over (gzip, cost_axis).

    The frontier is computed per-style. Rows missing the cost axis are
    marked non-Pareto. Computation writes into a single preallocated mask
    and then assigns it back to ``variants`` in one shot.
    """
    is_pareto = np.zeros(len(variants), dtype=bool)
    if cost_axis not in variants.columns or variants[cost_axis].isna().all():
        result = variants.copy()
        result["is_pareto"] = is_pareto
        return result

    idx_array = variants.index.to_numpy()
    for _style, group in variants.groupby("style", sort=False):
        pts = group[["gzip_bytes", cost_axis]].to_numpy(dtype=float)
        mask = non_dominated_mask(pts)
        positions = np.searchsorted(idx_array, group.index.to_numpy())
        is_pareto[positions] = mask

    result = variants.copy()
    result["is_pareto"] = is_pareto
    return result


# Plotting


def write_frontier_plot(
    out_path: Path, frontier_df: pd.DataFrame, cost_axis: str, styles: list[str]
) -> None:
    """Write a multi-panel PDF scatter plot, one panel per style."""
    if frontier_df.empty or not styles:
        return
    n = len(styles)
    cols = min(3, n)
    rows = math.ceil(n / cols)

    cost_label = AXIS_LABELS.get(cost_axis, cost_axis)

    fig = make_subplots(
        rows=rows,
        cols=cols,
        subplot_titles=styles,
        horizontal_spacing=0.08,
        vertical_spacing=0.14,
    )

    by_style = {
        str(style): group for style, group in frontier_df.groupby("style", sort=False)
    }
    legend_drawn = False
    for idx, style in enumerate(styles):
        sdf = by_style.get(style)
        if sdf is None or sdf.empty:
            continue
        r = idx // cols + 1
        c = idx % cols + 1
        dominated = sdf[~sdf["is_pareto"]]
        pareto = sdf[sdf["is_pareto"]].sort_values("gzip_bytes")

        fig.add_trace(
            go.Scatter(
                x=dominated["gzip_bytes"],
                y=dominated[cost_axis],
                mode="markers",
                marker=dict(
                    size=7,
                    color=DOMINATED_COLOUR,
                    line=dict(width=0.5, color=DOMINATED_OUTLINE),
                ),
                name="dominated",
                showlegend=not legend_drawn,
                hovertext=dominated["variant"],
            ),
            row=r,
            col=c,
        )
        fig.add_trace(
            go.Scatter(
                x=pareto["gzip_bytes"],
                y=pareto[cost_axis],
                mode="lines+markers",
                line=dict(color=FRONTIER_COLOUR, width=1.2),
                marker=dict(
                    size=9, color=FRONTIER_COLOUR, line=dict(width=1, color="black")
                ),
                name="Pareto frontier",
                showlegend=not legend_drawn,
                hovertext=pareto["variant"],
            ),
            row=r,
            col=c,
        )
        legend_drawn = True

        fig.update_xaxes(title_text="gzip_bytes", row=r, col=c)
        fig.update_yaxes(title_text=cost_label, row=r, col=c)

    # Start from the shared thesis layout defaults (template, font) and
    # override only what this multi-panel figure actually needs.
    layout: dict = {**LAYOUT_DEFAULTS}
    layout.update(
        height=340 * rows,
        width=420 * cols,
        margin=dict(l=70, r=40, t=70, b=60),
        title_text=f"Per-style Pareto frontier (gzip_bytes, {cost_label})",
        legend=dict(orientation="h", y=-0.06),
    )
    fig.update_layout(**layout)

    out_path.parent.mkdir(parents=True, exist_ok=True)
    pdf_path = out_path.with_suffix(".pdf")
    try:
        fig.write_image(str(pdf_path))
    except Exception as exc:  # noqa: BLE001 - kaleido errors are environment-specific
        print(f"warning: could not write frontier image: {exc}", file=sys.stderr)


# Main


def parse_args(argv: Iterable[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--input", type=Path, required=True, help="directory of bench-*.jsonl files"
    )
    parser.add_argument(
        "--out", type=Path, required=True, help="figure output directory"
    )
    parser.add_argument(
        "--tables", type=Path, required=True, help="LaTeX-table output directory"
    )
    parser.add_argument(
        "--json-summary",
        type=Path,
        default=None,
        help="optional path for the JSON summary; defaults to <tables>/pareto_summary.json",
    )
    return parser.parse_args(list(argv) if argv is not None else None)


def main(argv: Iterable[str] | None = None) -> int:
    args = parse_args(argv)

    raw = load_jsonl(args.input)
    filtered, session_log = filter_latest_session_per_style(raw)

    # Noise report is computed on the filtered (but not yet per-variant
    # aggregated) frame, because the cross-variant range needs per-cell
    # medians that only exist before aggregation collapses the scenario axis.
    noise_report = compute_noise_report(filtered)

    variants = build_variant_table(filtered)

    # Cost axis: prefer wall-clock preprocessing_ms if present, else fall
    # back to pass_count (always present).
    has_preprocessing = (
        "preprocessing_ms" in variants.columns
        and variants["preprocessing_ms"].notna().any()
    )
    cost_axis = "preprocessing_ms" if has_preprocessing else "pass_count"

    frontier_df = compute_frontier(variants, cost_axis)

    args.out.mkdir(parents=True, exist_ok=True)
    args.tables.mkdir(parents=True, exist_ok=True)

    styles = sorted(frontier_df["style"].unique().tolist())

    # Figure (PDF only — the thesis includes the PDF directly and does not
    # reference a PNG copy, so rendering PNG would double kaleido wall-time
    # for no benefit).
    write_frontier_plot(args.out / "pareto_per_style", frontier_df, cost_axis, styles)

    # Noise table (CSV)
    noise_csv_path = args.tables / "pareto_noise.csv"
    write_noise_csv(noise_csv_path, noise_report)

    # JSON summary
    noise_summary = noise_report.replace({np.nan: None}).to_dict(orient="index")
    summary: dict[str, object] = {
        "cost_axis": cost_axis,
        "styles": styles,
        "session_used": session_log,
        "noise_report": noise_summary,
        "frontier_sizes": frontier_df[frontier_df["is_pareto"]]
        .groupby("style")
        .size()
        .to_dict(),
        "baseline_to_final_gzip_reduction": _compute_gzip_reductions(frontier_df),
    }
    summary_path = args.json_summary or (args.tables / "pareto_summary.json")
    summary_path.write_text(
        json.dumps(summary, indent=2, default=str), encoding="utf-8"
    )

    print(
        f"wrote frontier figure, noise table, and JSON summary to "
        f"{args.out} / {args.tables} (cost axis: {cost_axis})"
    )
    return 0


def _compute_gzip_reductions(frontier_df: pd.DataFrame) -> dict[str, float]:
    """Per-style baseline → final cumulative gzip reduction, as a percentage.

    Relies on the ``step_num`` column populated by :func:`build_variant_table`
    so no variant-string parsing happens here.
    """
    reductions: dict[str, float] = {}
    for style, group in frontier_df.groupby("style"):
        cumulative = group[group["mode"].isin(["baseline", "cumulative"])]
        if cumulative.empty:
            continue
        ordered = cumulative.sort_values("step_num")
        base = ordered.iloc[0]["gzip_bytes"]
        last = ordered.iloc[-1]["gzip_bytes"]
        if base > 0:
            reductions[str(style)] = round(100 * (base - last) / base, 2)
    return reductions


if __name__ == "__main__":
    raise SystemExit(main())
