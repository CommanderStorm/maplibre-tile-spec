use std::collections::HashMap;
use std::mem;

use super::model::VertexBufferType;
use crate::MltResult;
use crate::codecs::morton::{encode_morton, morton_deltas, z_order_params};
use crate::codecs::zigzag::encode_componentwise_delta_vec2s;
use crate::decoder::GeometryType::{LineString, Point, Polygon};
use crate::decoder::{
    ColumnType, DictionaryType, GeometryType, GeometryValues, LengthType, LogicalEncoding,
    MortonMeta, OffsetType, StreamType,
};
use crate::encoder::Encoder;
use crate::encoder::model::StreamCtx;
use crate::encoder::stream::{write_precomputed_u32, write_u32_stream};
use crate::utils::AsUsize as _;

/// Compute `ZOrderCurve` parameters from the vertex value range.
///
/// Returns `(num_bits, coordinate_shift)` matching Java's `SpaceFillingCurve`.
/// Build a sorted unique Morton dictionary and per-vertex offset indices from a flat
/// `[x0, y0, x1, y1, …]` vertex slice.
///
/// Returns `(sorted_unique_codes, per_vertex_offsets)`.
#[hotpath::measure]
fn build_morton_dict(vertices: &[i32], meta: MortonMeta) -> MltResult<(Vec<u32>, Vec<u32>)> {
    let codes: Vec<u32> = vertices
        .chunks_exact(2)
        .map(|c| encode_morton(c[0], c[1], meta))
        .collect::<Result<_, _>>()?;

    let mut dict = codes.clone();
    dict.sort_unstable();
    dict.dedup();

    #[expect(
        clippy::cast_possible_truncation,
        reason = "dict.len() <= u32::MAX (deduped u32 codes)"
    )]
    let code_to_idx: HashMap<u32, u32> = dict
        .iter()
        .enumerate()
        .map(|(i, &c)| (c, i as u32))
        .collect();
    let offsets: Vec<u32> = codes.iter().map(|code| code_to_idx[code]).collect();

    Ok((dict, offsets))
}

/// Push consecutive offset-differences from `offsets` onto `lengths`.
///
/// Expects a slice of `n + 1` elements and produces `n` lengths,
/// one per consecutive pair: `offsets[i + 1] - offsets[i]`.
#[inline]
fn extend_offsets(lengths: &mut Vec<u32>, offsets: &[u32]) -> usize {
    lengths.extend(offsets.windows(2).map(|w| w[1] - w[0]));
    offsets.len() - 1
}

/// Convert dense geometry offsets to a length stream for encoding.
/// Extracts differences for geometry types greater than `buffer_id`.
fn encode_root_length_stream(
    geom_types: &[GeometryType],
    geom_offsets: &[u32],
    buffer_id: GeometryType,
) -> Vec<u32> {
    geom_types
        .iter()
        .zip(geom_offsets.windows(2))
        .filter(|&(&t, _)| t > buffer_id)
        .map(|(_, w)| w[1] - w[0])
        .collect()
}

/// Convert dense part offsets to a level-1 length stream.
/// Only emits lengths for geometry types that contribute real entries
/// (`Polygon`, and optionally `LineString` when `is_line_string_present`).
fn encode_level1_length_stream(
    geom_types: &[GeometryType],
    geom_offsets: &[u32],
    part_offsets: &[u32],
    is_line_string_present: bool,
) -> Vec<u32> {
    let mut lengths = Vec::new();
    for (i, &geom_type) in geom_types.iter().enumerate() {
        if geom_type.is_polygon() || (is_line_string_present && geom_type.is_linestring()) {
            let s = geom_offsets[i].as_usize();
            let e = geom_offsets[i + 1].as_usize();
            extend_offsets(&mut lengths, &part_offsets[s..=e]);
        }
    }
    lengths
}

/// Compute ring vertex-count lengths from dense part/ring offset arrays.
fn encode_ring_lengths_for_mixed(
    geom_types: &[GeometryType],
    part_offsets: &[u32],
    ring_offsets: &[u32],
    has_line_string: bool,
) -> Vec<u32> {
    let mut lengths = Vec::new();
    for (i, &geom_type) in geom_types.iter().enumerate() {
        if geom_type.is_polygon() || (has_line_string && geom_type.is_linestring()) {
            let s = part_offsets[i].as_usize();
            let e = part_offsets[i + 1].as_usize();
            extend_offsets(&mut lengths, &ring_offsets[s..=e]);
        }
    }
    lengths
}

/// Convert dense ring offsets to a level-2 length stream.
/// Uses `geom_offsets` to index into `part_offsets`, then `part_offsets` to index into `ring_offsets`.
fn encode_level2_length_stream(
    geom_types: &[GeometryType],
    geom_offsets: &[u32],
    part_offsets: &[u32],
    ring_offsets: &[u32],
) -> Vec<u32> {
    let mut lengths = Vec::new();
    for (i, &geom_type) in geom_types.iter().enumerate() {
        let gs = geom_offsets[i].as_usize();
        let ge = geom_offsets[i + 1].as_usize();

        if geom_type.is_polygon() || geom_type.is_linestring() {
            for j in gs..ge {
                let ps = part_offsets[j].as_usize();
                let pe = part_offsets[j + 1].as_usize();
                extend_offsets(&mut lengths, &ring_offsets[ps..=pe]);
            }
        }
    }
    lengths
}

/// Convert dense part offsets without ring buffer to a length stream.
/// Only LineString/MultiLineString contribute vertex-count lengths.
fn encode_level1_without_ring_buffer_length_stream(
    geom_types: &[GeometryType],
    geom_offsets: &[u32],
    part_offsets: &[u32],
) -> Vec<u32> {
    let mut lengths = Vec::new();
    for (i, &geom_type) in geom_types.iter().enumerate() {
        if geom_type.is_linestring() {
            let s = geom_offsets[i].as_usize();
            let e = geom_offsets[i + 1].as_usize();
            extend_offsets(&mut lengths, &part_offsets[s..=e]);
        }
    }
    lengths
}

/// Encode vertices using the given strategy. Returns the number of streams written (1 or 2).
fn encode_vertices_as(
    strategy: VertexBufferType,
    vertices: &[i32],
    enc: &mut Encoder,
) -> MltResult<u8> {
    match strategy {
        VertexBufferType::Vec2 => {
            encode_componentwise_delta_vec2s(vertices, &mut enc.tmp_u32);
            let delta = mem::take(&mut enc.tmp_u32);
            let ctx = StreamCtx::geom(StreamType::Data(DictionaryType::Vertex), "vertex");
            let logical = LogicalEncoding::ComponentwiseDelta;
            let n = write_geo_precomputed_stream(&delta, ctx, logical, enc)?;
            enc.tmp_u32 = delta;
            Ok(n)
        }
        VertexBufferType::Morton => {
            let morton_meta = get_z_order_params(vertices, enc)?;
            let (dict, offsets) = build_morton_dict(vertices, morton_meta)?;
            let ctx = StreamCtx::geom(StreamType::Offset(OffsetType::Vertex), "vertex_offsets");
            let mut n = write_geo_u32_stream(&offsets, ctx, enc)?;

            morton_deltas(&dict, &mut enc.tmp_u32);
            let delta = mem::take(&mut enc.tmp_u32);
            let ctx = StreamCtx::geom(StreamType::Data(DictionaryType::Morton), "vertex");
            let logical = LogicalEncoding::MortonDelta(morton_meta);
            n += write_geo_precomputed_stream(&delta, ctx, logical, enc)?;
            enc.tmp_u32 = delta;
            Ok(n)
        }
    }
}

/// Try both CWD and Morton vertex encoding and keep the smaller result.
///
/// Called only when `geom_offsets` is absent (no Multi\* types) and `ring_offsets` is
/// present.  In this context `part_offsets` is a compact polygon-only array; this function
/// expands it to a dense per-geometry array so that `encode_ring_lengths_for_mixed` can index
/// directly by geometry position.
///
/// Each slot in the output holds the first index into `ring_offsets` for that geometry:
/// - `Point`: no contribution — slot range is empty (`ring_idx` unchanged).
/// - `LineString`: contributes 1 slot (vertex count) — slot range is 1.
/// - `Polygon`: contributes `ring_count` slots — slot range equals its ring count.
fn normalize_part_offsets_for_rings(
    vector_types: &[GeometryType],
    part_offsets: &[u32],
    ring_offsets: &[u32],
) -> Vec<u32> {
    let mut normalized = Vec::with_capacity(vector_types.len() + 1);
    let mut ring_idx = 0_u32;
    let mut part_idx = 0_usize;

    for &geom_type in vector_types {
        normalized.push(ring_idx);

        if geom_type == Point {
            // Point has no vertex-count slot in ring_offsets.
        } else if geom_type.is_linestring() {
            // Each LineString occupies exactly one slot in ring_offsets.
            ring_idx += 1;
        } else if geom_type.is_polygon() && part_idx + 1 < part_offsets.len() {
            // Polygon occupies ring_count slots (one vertex-count per ring).
            let ring_count = part_offsets[part_idx + 1] - part_offsets[part_idx];
            ring_idx += ring_count;
            part_idx += 1;
        }
        // No Multi* types can appear here (they always produce geom_offsets).
    }

    // ring_idx must equal ring_offsets.len() - 1 for well-formed data.
    debug_assert_eq!(
        ring_idx as usize,
        ring_offsets.len().saturating_sub(1),
        "ring index mismatch after normalization"
    );
    normalized.push(ring_idx);
    normalized
}

/// Choose between Vec2 componentwise-delta and Morton dictionary encoding.
///
/// Morton is only selected when:
/// - The coordinate range fits within 16 bits per axis (required by the spec), and
/// - The uniqueness ratio is below the threshold, meaning enough vertices are
///   repeated that the dictionary overhead is worthwhile.
///
/// Calls `get_z_order_params` so the [`MortonMeta`] is cached on the encoder
/// and can be retrieved again in the Morton encoding branch without a second vertex scan.
#[hotpath::measure]
fn select_best_vertex_encoding(vertices: &[i32], enc: &mut Encoder) -> MltResult<u8> {
    let morton_ok = !vertices.is_empty() && get_z_order_params(vertices, enc).is_ok();

    if !morton_ok {
        // Morton not applicable (empty or coordinate range too large) — use CWD directly.
        return encode_vertices_as(VertexBufferType::Vec2, vertices, enc);
    }

    // Encode CWD first
    let data_cp = enc.data.len();
    let meta_cp = enc.meta.len();
    encode_vertices_as(VertexBufferType::Vec2, vertices, enc)?;
    let cwd_data_len = enc.data.len() - data_cp;
    let cwd_meta_len = enc.meta.len() - meta_cp;
    let cwd_total = cwd_data_len + cwd_meta_len;

    // Save CWD output and reset
    let cwd_data: Vec<u8> = enc.data[data_cp..].to_vec();
    let cwd_meta: Vec<u8> = enc.meta[meta_cp..].to_vec();
    enc.data.truncate(data_cp);
    enc.meta.truncate(meta_cp);

    // Encode Morton
    encode_vertices_as(VertexBufferType::Morton, vertices, enc)?;
    let morton_total = (enc.data.len() - data_cp) + (enc.meta.len() - meta_cp);

    if morton_total < cwd_total {
        Ok(2) // Morton wins: 2 streams (offsets + dictionary)
    } else {
        // CWD wins: restore its output
        enc.data.truncate(data_cp);
        enc.meta.truncate(meta_cp);
        enc.data.extend_from_slice(&cwd_data);
        enc.meta.extend_from_slice(&cwd_meta);
        Ok(1) // CWD: 1 stream
    }
}

/// Compute or return the cached [`MortonMeta`] for `vertices`.
fn get_z_order_params(vertices: &[i32], enc: &mut Encoder) -> MltResult<MortonMeta> {
    Ok(if let Some(meta) = enc.morton_meta_cache {
        meta
    } else {
        let meta = z_order_params(vertices)?;
        enc.morton_meta_cache = Some(meta);
        meta
    })
}

/// Write a geometry `u32` stream: [`Encoder::override_int_enc`] when explicit mode is active,
/// otherwise try all pruned candidates and keep the shortest.
///
/// Returns `1` if the stream was written, `0` if it was skipped.  Empty streams are skipped
/// unless [`Encoder::force_stream`] returns `true` for this stream's [`StreamCtx`].
fn write_geo_u32_stream(data: &[u32], ctx: StreamCtx, enc: &mut Encoder) -> MltResult<u8> {
    Ok(if data.is_empty() && !enc.force_stream(&ctx) {
        0
    } else {
        write_u32_stream(data, &ctx, enc)?;
        1
    })
}

/// Like [`write_geo_u32_stream`] but for pre-logically-encoded data: delegates to
/// [`write_precomputed_u32`] instead of [`write_u32_stream`].
///
/// Returns `1` if the stream was written, `0` if skipped (empty + no force).
fn write_geo_precomputed_stream(
    data: &[u32],
    ctx: StreamCtx,
    logical: LogicalEncoding,
    enc: &mut Encoder,
) -> MltResult<u8> {
    Ok(if data.is_empty() && !enc.force_stream(&ctx) {
        0
    } else {
        write_precomputed_u32(data, logical, &ctx, enc)?;
        1
    })
}

impl GeometryValues {
    /// Write the geometry column to `enc`.
    #[hotpath::measure]
    pub fn write_to(self, enc: &mut Encoder) -> MltResult<()> {
        let Self {
            vector_types,
            geometry_offsets,
            part_offsets,
            ring_offsets,
            index_buffer,
            triangles,
            vertices,
        } = self;

        // Flatten every Option<Vec> → Vec  (empty == not present).
        // triangles: None means no tessellation; Some([]) can't occur in practice (each
        // push_geom appends a count), so empty == absent is safe here too.
        // vertices: None means no coordinate data (e.g. empty layer).
        let geom_offsets = geometry_offsets.unwrap_or_default();
        let part_offsets = part_offsets.unwrap_or_default();
        let ring_offsets = ring_offsets.unwrap_or_default();
        let index_buffer = index_buffer.unwrap_or_default();
        let triangles = triangles.unwrap_or_default();
        let vertices = vertices.unwrap_or_default();

        let meta: Vec<u32> = vector_types.iter().map(|t| *t as u32).collect();

        // Write column type to meta; reserve exactly 1 byte for stream count
        // (geometry never exceeds ~8 streams, always fits in a single varint byte).
        ColumnType::Geometry.write_to(&mut enc.meta)?;
        let stream_count_pos = enc.data.len();
        enc.data.push(0); // placeholder — patched below
        let mut n: u8 = 0;

        // Meta stream — always written, even for a zero-feature layer.
        let ctx = StreamCtx::geom(StreamType::Length(LengthType::VarBinary), "meta");
        write_u32_stream(&meta, &ctx, enc)?;
        n += 1;

        // Topology: compute each length stream and write it immediately.
        if !geom_offsets.is_empty() {
            let data = encode_root_length_stream(&vector_types, &geom_offsets, Polygon);
            let ctx = StreamCtx::geom(StreamType::Length(LengthType::Geometries), "geometries");
            n += write_geo_u32_stream(&data, ctx, enc)?;

            if !part_offsets.is_empty() {
                if ring_offsets.is_empty() {
                    // geom → parts only (no rings).
                    let data = encode_level1_without_ring_buffer_length_stream(
                        &vector_types,
                        &geom_offsets,
                        &part_offsets,
                    );
                    let ctx = StreamCtx::geom(StreamType::Length(LengthType::Parts), "no_rings");
                    n += write_geo_u32_stream(&data, ctx, enc)?;
                } else {
                    // Full topology: geom → parts → rings.
                    // LineStrings contribute to rings here, not to parts.
                    let data = encode_level1_length_stream(
                        &vector_types,
                        &geom_offsets,
                        &part_offsets,
                        false,
                    );
                    let ctx = StreamCtx::geom(StreamType::Length(LengthType::Parts), "rings");
                    n += write_geo_u32_stream(&data, ctx, enc)?;

                    let data = encode_level2_length_stream(
                        &vector_types,
                        &geom_offsets,
                        &part_offsets,
                        &ring_offsets,
                    );
                    let ctx = StreamCtx::geom(StreamType::Length(LengthType::Rings), "rings2");
                    n += write_geo_u32_stream(&data, ctx, enc)?;
                }
            }
        } else if !part_offsets.is_empty() {
            if ring_offsets.is_empty() {
                let data = encode_root_length_stream(&vector_types, &part_offsets, Point);
                let ctx = StreamCtx::geom(StreamType::Length(LengthType::Parts), "no_rings");
                n += write_geo_u32_stream(&data, ctx, enc)?;
            } else {
                // No Multi* types; parts → rings (Polygon / mixed Point+Polygon).
                // Java writes an empty GEOMETRIES stream here for tessellated polygons; only do
                // so when explicitly forced (e.g. to preserve byte-for-byte Java compatibility).
                let ctx = StreamCtx::geom(StreamType::Length(LengthType::Geometries), "geometries");
                n += write_geo_u32_stream(&[], ctx, enc)?;

                let data = encode_root_length_stream(&vector_types, &part_offsets, LineString);
                let ctx = StreamCtx::geom(StreamType::Length(LengthType::Parts), "parts");
                n += write_geo_u32_stream(&data, ctx, enc)?;

                // part_offs is a dense N+1 array (one slot per geometry incl. Points);
                // ring_offs stores vertex offsets per slot.  The dense-aware helper skips
                // Point slots by index rather than a running counter.
                let has_line_string = vector_types
                    .iter()
                    .copied()
                    .any(GeometryType::is_linestring);
                let data = encode_ring_lengths_for_mixed(
                    &vector_types,
                    &part_offsets,
                    &ring_offsets,
                    has_line_string,
                );
                let ctx = StreamCtx::geom(StreamType::Length(LengthType::Rings), "parts_ring");
                n += write_geo_u32_stream(&data, ctx, enc)?;
            }
        }

        let ctx = StreamCtx::geom(StreamType::Length(LengthType::Triangles), "triangles");
        n += write_geo_u32_stream(&triangles, ctx, enc)?;
        let ctx = StreamCtx::geom(StreamType::Offset(OffsetType::Index), "triangles_indexes");
        n += write_geo_u32_stream(&index_buffer, ctx, enc)?;

        // When an explicit vertex buffer type is pinned (synthetics / __private),
        // use it directly. Otherwise try both CWD and Morton and keep the smaller.
        if let Some(pinned) = enc.override_vertex_buffer_type() {
            n += encode_vertices_as(pinned, &vertices, enc)?;
        } else {
            n += select_best_vertex_encoding(&vertices, enc)?;
        }

        // Patch the reserved stream-count byte.
        debug_assert!(n <= 127, "geometry stream count must fit in one byte");
        enc.data[stream_count_pos] = n;
        enc.increment_column_count();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_morton_dict() {
        let meta = MortonMeta {
            num_bits: 4,
            coordinate_shift: 0,
        };
        // vertices: [x0,y0, x1,y1, x2,y2, x3,y3] — repeat (1,2) to test dedup
        let vertices = [1, 2, 3, 4, 1, 2, 0, 0];
        let (dict, offsets) = build_morton_dict(&vertices, meta).unwrap();

        assert!(
            dict.windows(2).all(|w| w[0] < w[1]),
            "dict not sorted/unique"
        );
        assert_eq!(offsets.len(), 4, "offsets length == number of vertex pairs");
        assert_eq!(offsets[0], offsets[2], "duplicate (1,2) should share index");
        assert!(offsets.iter().all(|&o| (o as usize) < dict.len()));
    }

    #[test]
    fn test_encode_root_length_stream() {
        // Single Polygon geometry (no Multi)
        let types = vec![Polygon];
        let offsets = vec![0, 1]; // One polygon

        let lengths = encode_root_length_stream(&types, &offsets, Polygon);
        // Polygon == buffer_id, so no length encoded
        assert!(lengths.is_empty());

        // MultiPolygon needs length encoded
        let types = vec![GeometryType::MultiPolygon];
        let offsets = vec![0, 2]; // MultiPolygon with 2 polygons

        let lengths = encode_root_length_stream(&types, &offsets, Polygon);
        assert_eq!(lengths, vec![2]);
    }
}
