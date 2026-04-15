# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Sweep MinHash Jaccard thresholds × similarity modes, measure output sizes.

For each (mode, threshold) pair this script patches the Rust source, rebuilds,
runs `mlt convert` on the Germany dataset, and records the output mbtiles size.
Results are written to thesis/scripts/data/minhash_sweep.csv.

Modes:
  hybrid       — max(exact, trigram) > threshold  (current default)
  only-trigram — tri > threshold
  only-plain   — exact > threshold

Thresholds: 0.025, 0.050, 0.075, 0.100, 0.125
"""

import csv
import re
import subprocess
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
ROOT = SCRIPT_DIR.parent.parent
OUTPUT_DIR = SCRIPT_DIR / "data"

SHARED_DICT_RS = ROOT / "rust" / "mlt-core" / "src" / "encoder" / "property" / "shared_dict.rs"
RUST_DIR = ROOT / "rust"
MLT_BIN = RUST_DIR / "target" / "release" / "mlt"
INPUT_MBTILES = ROOT / "data" / "germany.mbtiles"
TMP_OUTPUT = Path("/tmp/minhash_sweep_output.mbtiles")

THRESHOLDS = [0.025, 0.050, 0.075, 0.100, 0.125]

# The similarity comparison line in cluster_by_similarity()
COMPARISON_PATTERNS = {
    "hybrid": "if f64::max(exact, tri) > MINHASH_SIMILARITY_THRESHOLD {",
    "only-trigram": "if tri > MINHASH_SIMILARITY_THRESHOLD {",
    "only-plain": "if exact > MINHASH_SIMILARITY_THRESHOLD {",
}

THRESHOLD_RE = re.compile(
    r"(const MINHASH_SIMILARITY_THRESHOLD: f64 = )\d+\.\d+;"
)

CSV_HEADER = ["mode", "threshold", "total_bytes"]


def read_source() -> str:
    return SHARED_DICT_RS.read_text()


def patch_source(original: str, mode: str, threshold: float) -> str:
    """Patch the comparison line and threshold constant."""
    text = original

    # Patch the threshold constant
    text = THRESHOLD_RE.sub(rf"\g<1>{threshold};", text)

    # Patch the comparison line — replace any of the three known patterns
    for pattern in COMPARISON_PATTERNS.values():
        if pattern in text:
            text = text.replace(pattern, COMPARISON_PATTERNS[mode])
            break
    else:
        sys.exit(f"Could not find comparison pattern in {SHARED_DICT_RS}")

    return text


def build() -> None:
    subprocess.run(
        ["cargo", "build", "-p", "mlt", "--release"],
        cwd=RUST_DIR,
        check=True,
        capture_output=True,
    )


def convert_and_measure() -> int:
    TMP_OUTPUT.unlink(missing_ok=True)
    subprocess.run(
        [str(MLT_BIN), "convert", str(INPUT_MBTILES), str(TMP_OUTPUT)],
        check=True,
    )
    size = TMP_OUTPUT.stat().st_size
    TMP_OUTPUT.unlink(missing_ok=True)
    return size


def main() -> None:
    if not INPUT_MBTILES.exists():
        sys.exit(f"Input not found: {INPUT_MBTILES}")

    original_source = read_source()
    results: list[dict] = []

    try:
        for mode in COMPARISON_PATTERNS:
            for threshold in THRESHOLDS:
                label = f"{mode} @ {threshold}"
                print(f"\n{'='*60}")
                print(f"  {label}")
                print(f"{'='*60}")

                patched = patch_source(original_source, mode, threshold)
                SHARED_DICT_RS.write_text(patched)

                print("  Building…")
                build()

                print("  Converting…")
                size = convert_and_measure()
                print(f"  → {size:,} bytes ({size / 1e9:.3f} GB)")

                results.append({
                    "mode": mode,
                    "threshold": threshold,
                    "total_bytes": size,
                })
    finally:
        # Always restore original source
        SHARED_DICT_RS.write_text(original_source)
        print("\nRestored original source.")

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    out_path = OUTPUT_DIR / "minhash_sweep.csv"
    with open(out_path, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=CSV_HEADER)
        w.writeheader()
        w.writerows(results)
    print(f"\nWrote {out_path}  ({len(results)} rows)")


if __name__ == "__main__":
    main()
