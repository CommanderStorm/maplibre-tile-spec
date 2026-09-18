#!/usr/bin/env bash
# THROWAWAY: fills public/fixtures/ with the tiles the prototype serves and their dumps.
# The real app symlinks test/synthetic/ and calls the wasm; this stands in for both.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
out="$here/public/fixtures"

mkdir -p "$out/0x01" "$out/0x02" "$out/real"

dump() {
  local src="$1" dst="$2"
  cp "$src" "$out/$dst.mlt"
  (cd "$repo/rust" && cargo run -q --release -p mlt-core --example dump_json \
    --features unstable-v2 -- "$src") > "$out/$dst.json"
  echo "  $dst  $(stat -c%s "$out/$dst.mlt") B -> $(stat -c%s "$out/$dst.json") B"
}

echo "synthetic fixtures:"
dump "$repo/test/synthetic/0x01/point.mlt" 0x01/point
dump "$repo/test/synthetic/0x01/props_u32_fpf_511.mlt" 0x01/props_u32_fpf_511
dump "$repo/test/synthetic/0x01/props_shared_dict_fsst.mlt" 0x01/props_shared_dict_fsst
dump "$repo/test/synthetic/0x02/mvalues_f64_alp.mlt" 0x02/mvalues_f64_alp
dump "$repo/test/synthetic/0x02/nested_map_str.mlt" 0x02/nested_map_str
dump "$repo/test/synthetic/0x02/mvalues_15props_7cols.mlt" 0x02/mvalues_15props_7cols

# A real-world tile. The committed *-mlt.mbtiles archives are NOT readable by today's
# walker (`mlt hexdump` fails on them too), so this re-encodes an MVT tile from the
# reference archive instead.
real_mvt="$(mktemp -d)/omt_14_8298_10748.pbf"
echo "real-world tile (re-encoded from test/omt-ref.mbtiles):"
sqlite3 "$repo/test/omt-ref.mbtiles" \
  "select writefile('$real_mvt', tile_data) from tiles
   where zoom_level=14 and tile_column=8298 and tile_row=10748;" > /dev/null
(cd "$repo/rust" && cargo run -q --release -p mlt --features unstable-v2 -- \
  convert "$real_mvt" "$(dirname "$real_mvt")/mlt") > /dev/null
dump "$(dirname "$real_mvt")/mlt/omt_14_8298_10748.mlt" real/omt_z14_8298_10748

cat > "$out/index.json" <<'JSON'
{
  "0x01": [
    { "name": "point", "path": "0x01/point" },
    { "name": "props_shared_dict_fsst", "path": "0x01/props_shared_dict_fsst" },
    { "name": "props_u32_fpf_511", "path": "0x01/props_u32_fpf_511" }
  ],
  "0x02": [
    { "name": "mvalues_f64_alp", "path": "0x02/mvalues_f64_alp" },
    { "name": "nested_map_str", "path": "0x02/nested_map_str" },
    { "name": "mvalues_15props_7cols", "path": "0x02/mvalues_15props_7cols" }
  ],
  "real": [
    { "name": "OMT z14/8298/10748 (re-encoded)", "path": "real/omt_z14_8298_10748" }
  ]
}
JSON
echo "wrote $out/index.json"
