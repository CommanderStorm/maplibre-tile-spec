use geo::{Convert as _, TriangulateEarcut as _};
use geo_types::{LineString, MultiLineString, MultiPoint, MultiPolygon, Polygon};

use crate::decoder::{GeometryType, GeometryValues};
use crate::{Coord32, Geom32};

impl TryFrom<&Geom32> for GeometryType {
    type Error = ();

    fn try_from(geom: &Geom32) -> Result<Self, Self::Error> {
        Ok(match geom {
            Geom32::Point(_) => Self::Point,
            Geom32::MultiPoint(_) => Self::MultiPoint,
            Geom32::LineString(_) => Self::LineString,
            Geom32::MultiLineString(_) => Self::MultiLineString,
            Geom32::Polygon(_) => Self::Polygon,
            Geom32::MultiPolygon(_) => Self::MultiPolygon,
            Geom32::Line(_)
            | Geom32::GeometryCollection(_)
            | Geom32::Rect(_)
            | Geom32::Triangle(_) => {
                return Err(());
            }
        })
    }
}

/// Run the Earcut algorithm on `polygon`, append triangle indices (shifted by `vertex_offset`)
/// into `index_buf`, and return `(num_triangles, num_vertices)`.
fn earcut_into(polygon: &Polygon<i32>, vertex_offset: u32, index_buf: &mut Vec<u32>) -> (u32, u32) {
    let polygon_f64: Polygon<f64> = polygon.convert();
    let raw = polygon_f64.earcut_triangles_raw();
    let num_triangles = u32::try_from(raw.triangle_indices.len() / 3).expect("too many triangles");
    let num_vertices = u32::try_from(raw.vertices.len()).expect("too many vertices");

    for i in raw.triangle_indices {
        let base = u32::try_from(i).expect("mlt vertex index overflow");
        let idx = base
            .checked_add(vertex_offset)
            .expect("vertex index overflow");
        index_buf.push(idx);
    }

    (num_triangles, num_vertices)
}

impl GeometryValues {
    /// Compute tessellation for all polygon and multi-polygon features,
    /// populating `self.triangles` and `self.index_buffer`.
    ///
    /// No-op if tessellation data already exists (`self.triangles.is_some()`).
    /// This is the entry point for the FFI / columnar path where geometries are
    /// constructed via [`GeometryValues::from_columnar`] without tessellation data.
    #[cfg(feature = "tessellate")]
    pub fn compute_tessellation(&mut self) -> crate::MltResult<()> {
        if self.triangles.is_some() {
            return Ok(());
        }

        // Collect only polygon geometries to avoid reconstructing non-polygon
        // features. We must collect first to work around the borrow conflict
        // between `to_geojson(&self)` and the `&mut self` tessellation methods.
        let polys: Vec<Geom32> = self
            .vector_types
            .iter()
            .enumerate()
            .filter(|(_, gt)| gt.is_polygon())
            .map(|(i, _)| self.to_geojson(i))
            .collect::<Result<_, _>>()?;

        self.triangles = Some(Vec::new());

        for geom in &polys {
            match geom {
                Geom32::Polygon(p) => self.tessellate_polygon(p),
                Geom32::MultiPolygon(mp) => self.tessellate_multi_polygon(mp),
                _ => {}
            }
        }
        Ok(())
    }

    /// Returns a [`GeometryValues`] with an empty `triangles` buffer pre-initialized.
    ///
    /// When `triangles` is `Some`, polygon push methods automatically compute and store
    /// Earcut tessellation data as geometries are added.
    /// Use [`Self::default`] when tessellation is not required.
    #[must_use]
    pub fn new_tessellated() -> Self {
        Self {
            triangles: Some(vec![]),
            ..Default::default()
        }
    }

    /// Tessellate `polygon` using the Earcut algorithm and append the results directly into
    /// `self.index_buffer` and `self.triangles`.
    fn tessellate_polygon(&mut self, polygon: &Polygon<i32>) {
        if let Some(triangles) = self.triangles.as_mut() {
            let (num_triangles, _) =
                earcut_into(polygon, 0, self.index_buffer.get_or_insert_with(Vec::new));
            triangles.push(num_triangles);
        }
    }

    /// Tessellate all polygons in `mp` and append the combined results into
    /// `self.index_buffer` and `self.triangles`.
    ///
    /// Indices for each constituent polygon are offset by the cumulative vertex count of all
    /// preceding polygons so they reference the correct positions in the shared vertex buffer.
    /// A single total triangle count (summed over all constituent polygons) is pushed into
    /// `self.triangles`.
    fn tessellate_multi_polygon(&mut self, mp: &MultiPolygon<i32>) {
        if let Some(triangles) = self.triangles.as_mut() {
            let mut total_triangles = 0u32;
            let mut vertex_offset = 0u32;
            let index_buffer = self.index_buffer.get_or_insert_with(Vec::new);
            for poly in &mp.0 {
                let (num_triangles, num_verts) = earcut_into(poly, vertex_offset, index_buffer);
                total_triangles += num_triangles;
                vertex_offset += num_verts;
            }
            triangles.push(total_triangles);
        }
    }

    /// Add a geometry to this decoded geometry collection.
    /// This is the reverse of `to_geojson` - it converts a `Geom32`
    /// into the internal MLT representation with offset arrays.
    #[must_use]
    pub fn with_geom(mut self, geom: &Geom32) -> Self {
        self.push_geom(geom);
        self
    }

    /// Build `GeometryValues` from a slice of row-level geometries.
    ///
    /// Pre-scans the types to determine which offset arrays are needed, then
    /// builds everything in a single pass. All offset arrays are **dense**:
    /// N+1 entries for N features, with implicit +1 entries for types that
    /// don't contribute real data at that level.
    #[must_use]
    pub fn from_geoms(geoms: &[Geom32], tessellate: bool) -> Self {
        // Pre-scan: determine which offset arrays are needed.
        let mut has_multi = false;
        let mut has_line_or_poly = false;
        let mut has_polygon = false;
        for g in geoms {
            match g {
                Geom32::MultiPoint(_) | Geom32::MultiLineString(_) | Geom32::MultiPolygon(_) => {
                    has_multi = true;
                    has_line_or_poly = true;
                    if matches!(g, Geom32::MultiPolygon(_)) {
                        has_polygon = true;
                    }
                }
                Geom32::LineString(_) | Geom32::Line(_) => has_line_or_poly = true,
                Geom32::Polygon(_) | Geom32::Triangle(_) | Geom32::Rect(_) => {
                    has_line_or_poly = true;
                    has_polygon = true;
                }
                _ => {}
            }
        }

        let mut gv = if tessellate {
            Self::new_tessellated()
        } else {
            Self::default()
        };

        // Pre-allocate and initialize offset arrays so push_geom always finds
        // them. This guarantees dense offsets regardless of feature order.
        if has_multi {
            init_offsets(gv.geometry_offsets.get_or_insert_with(Vec::new));
        }
        if has_line_or_poly {
            init_offsets(gv.part_offsets.get_or_insert_with(Vec::new));
        }
        if has_polygon {
            init_offsets(gv.ring_offsets.get_or_insert_with(Vec::new));
        }

        for g in geoms {
            gv.push_geom(g);
        }
        gv
    }

    /// Add a geometry to this decoded geometry collection (mutable version).
    ///
    /// Prefer [`from_geoms`](Self::from_geoms) when all geometries are known upfront —
    /// it pre-scans types and guarantees dense offsets. Direct `push_geom` calls
    /// also produce dense offsets as long as all three offset arrays are
    /// initialized first (which `from_geoms` does automatically).
    pub fn push_geom(&mut self, geom: &Geom32) {
        match geom {
            Geom32::Point(p) => self.push_point(p.0),
            Geom32::Line(l) => self.push_linestring(&LineString(vec![l.start, l.end])),
            Geom32::LineString(ls) => self.push_linestring(ls),
            Geom32::Polygon(p) => self.push_polygon(p),
            Geom32::MultiPoint(mp) => self.push_multi_point(mp),
            Geom32::MultiLineString(mls) => self.push_multi_linestring(mls),
            Geom32::MultiPolygon(mp) => self.push_multi_polygon(mp),
            Geom32::Triangle(t) => self.push_polygon(&t.to_polygon()),
            Geom32::Rect(r) => self.push_polygon(&r.to_polygon()),
            Geom32::GeometryCollection(gc) => {
                for g in gc {
                    self.push_geom(g);
                }
            }
        }
    }

    fn push_point(&mut self, coord: Coord32) {
        self.vector_types.push(GeometryType::Point);
        self.vertices
            .get_or_insert_with(Vec::new)
            .extend([coord.x, coord.y]);
        // Dense: every type gets an implicit +1 in all offset arrays.
        self.push_implicit(1);
    }

    fn push_linestring(&mut self, ls: &LineString<i32>) {
        self.vector_types.push(GeometryType::LineString);

        let verts = self.vertices.get_or_insert_with(Vec::new);
        let has_ring = self.ring_offsets.is_some();
        let offsets = if has_ring {
            self.ring_offsets.as_mut().unwrap()
        } else {
            self.part_offsets.get_or_insert_with(Vec::new)
        };
        push_linestrings(std::iter::once(ls), verts, offsets);

        if let Some(g) = self.geometry_offsets.as_mut() {
            g.push(g.last().unwrap() + 1);
        }
        if has_ring {
            // ring_offsets got the vertex count; part_offsets gets +1
            if let Some(p) = self.part_offsets.as_mut() {
                p.push(p.last().unwrap() + 1);
            }
        }
        // Without rings, part_offsets already got the vertex count via push_linestrings.
    }

    fn push_polygon(&mut self, poly: &Polygon<i32>) {
        self.vector_types.push(GeometryType::Polygon);
        self.init_polygon_offsets();

        let verts = self.vertices.get_or_insert_with(Vec::new);
        let rings = self.ring_offsets.as_mut().unwrap();
        let parts = self.part_offsets.as_mut().unwrap();

        push_polygon_rings(poly, verts, rings, parts);
        if let Some(g) = self.geometry_offsets.as_mut() {
            g.push(g.last().unwrap() + 1);
        }
        self.tessellate_polygon(poly);
    }

    /// Initialize offset arrays for polygon storage. On the first polygon,
    /// moves any `LineString` vertex offsets from `part_offsets` to `ring_offsets`.
    fn init_polygon_offsets(&mut self) {
        if self.ring_offsets.is_none()
            && let Some(ls_parts) = self.part_offsets.take()
        {
            self.ring_offsets = Some(ls_parts);
        }
        init_offsets(self.ring_offsets.get_or_insert_with(Vec::new));
        init_offsets(self.part_offsets.get_or_insert_with(Vec::new));
    }

    fn push_multi_point(&mut self, mp: &MultiPoint<i32>) {
        self.vector_types.push(GeometryType::MultiPoint);

        let verts = self.vertices.get_or_insert_with(Vec::new);
        for point in mp {
            verts.extend([point.0.x, point.0.y]);
        }

        let count = u32::try_from(mp.0.len()).expect("point count overflow");
        if let Some(g) = self.geometry_offsets.as_mut() {
            g.push(g.last().unwrap() + count);
        }
        for _ in 0..count {
            self.push_implicit_sub(1);
        }
    }

    fn push_multi_linestring(&mut self, mls: &MultiLineString<i32>) {
        self.vector_types.push(GeometryType::MultiLineString);

        let verts = self.vertices.get_or_insert_with(Vec::new);
        let has_ring = self.ring_offsets.is_some();
        let offsets = if has_ring {
            self.ring_offsets.as_mut().unwrap()
        } else {
            self.part_offsets.get_or_insert_with(Vec::new)
        };
        push_linestrings(mls.iter(), verts, offsets);

        let count = u32::try_from(mls.0.len()).expect("linestring count overflow");
        if let Some(g) = self.geometry_offsets.as_mut() {
            g.push(g.last().unwrap() + count);
        }
        if has_ring && let Some(p) = self.part_offsets.as_mut() {
            for _ in 0..count {
                p.push(p.last().unwrap() + 1);
            }
        }
        // Without rings, part_offsets already got vertex counts via push_linestrings.
    }

    fn push_multi_polygon(&mut self, mp: &MultiPolygon<i32>) {
        self.vector_types.push(GeometryType::MultiPolygon);
        self.init_polygon_offsets();

        let verts = self.vertices.get_or_insert_with(Vec::new);
        let rings = self.ring_offsets.as_mut().unwrap();
        let parts = self.part_offsets.as_mut().unwrap();

        for poly in mp {
            push_polygon_rings(poly, verts, rings, parts);
        }

        let count = u32::try_from(mp.0.len()).expect("polygon count overflow");
        if let Some(g) = self.geometry_offsets.as_mut() {
            g.push(g.last().unwrap() + count);
        }
        self.tessellate_multi_polygon(mp);
    }

    /// Push cumulative +delta to all **existing** offset arrays.
    /// Does NOT create arrays — only `from_geoms` or `init_polygon_offsets` create them.
    fn push_implicit(&mut self, delta: u32) {
        if let Some(g) = self.geometry_offsets.as_mut() {
            g.push(g.last().unwrap() + delta);
        }
        self.push_implicit_sub(delta);
    }

    /// Push cumulative +delta to existing part and ring offsets only.
    fn push_implicit_sub(&mut self, delta: u32) {
        if let Some(p) = self.part_offsets.as_mut() {
            p.push(p.last().unwrap() + delta);
        }
        if let Some(r) = self.ring_offsets.as_mut() {
            r.push(r.last().unwrap() + delta);
        }
    }
}

/// Ensure offset array starts with 0.
fn init_offsets(v: &mut Vec<u32>) {
    if v.is_empty() {
        v.push(0);
    }
}

/// Push a single polygon's rings (exterior + interiors) to the offset arrays.
/// MLT omits closing vertices, so we strip them if present.
fn push_polygon_rings(
    poly: &Polygon<i32>,
    verts: &mut Vec<i32>,
    rings: &mut Vec<u32>,
    parts: &mut Vec<u32>,
) {
    let mut ring_count = *parts.last().unwrap();
    for ring in std::iter::once(poly.exterior()).chain(poly.interiors()) {
        push_ring(ring, verts, rings);
        ring_count += 1;
    }
    parts.push(ring_count);
}

/// Push a ring's coordinates (stripping closing vertex) to verts and update rings offset.
fn push_ring(ring: &LineString<i32>, verts: &mut Vec<i32>, rings: &mut Vec<u32>) {
    let coords = &ring.0;
    let len = if coords.len() > 1 && coords.last() == coords.first() {
        coords.len() - 1
    } else {
        coords.len()
    };
    for c in &coords[..len] {
        verts.extend([c.x, c.y]);
    }
    let prev = *rings.last().unwrap();
    rings.push(prev + u32::try_from(len).expect("vertex count overflow"));
}

/// Push linestrings to vertex buffer and offset array.
fn push_linestrings<'a>(
    iter: impl Iterator<Item = &'a LineString<i32>>,
    verts: &mut Vec<i32>,
    offsets: &mut Vec<u32>,
) {
    init_offsets(offsets);
    for ls in iter {
        for c in ls.coords() {
            verts.extend([c.x, c.y]);
        }
        let prev = *offsets.last().unwrap();
        offsets.push(prev + u32::try_from(ls.0.len()).expect("vertex count overflow"));
    }
}

#[cfg(test)]
mod tests {
    use fastpfor::FastPFor256;
    use geo_types::{LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon, wkt};
    use insta::assert_snapshot;
    use proptest::prelude::*;

    use super::*;
    use crate::LazyParsed;
    use crate::decoder::{
        DictionaryType, IntEncoding, LengthType, LogicalEncoding, MortonMeta, OffsetType,
        RawGeometry, StreamMeta, StreamType,
    };
    use crate::encoder::{EncodedStream, EncodedStreamData, Encoder, IntEncoder, do_write_u32};
    use crate::test_helpers::{assert_empty, dec, parser};
    use crate::utils::BinarySerializer as _;

    /// Encode, serialize, parse, and decode a `GeometryValues`.
    /// The input must already be in the dense canonical form that `from_encoded`
    /// produces (i.e. built via a previous `roundtrip` call, not via `push_*`).
    fn roundtrip(decoded: &GeometryValues) -> GeometryValues {
        let mut enc = Encoder::default();
        decoded
            .clone()
            .write_to(&mut enc)
            .expect("Failed to encode");

        let parsed = assert_empty(RawGeometry::from_bytes(&enc.data, &mut parser()));

        LazyParsed::Raw(parsed)
            .into_parsed(&mut dec())
            .expect("Failed to decode")
    }

    /// Build a `GeometryValues` from a sequence of `Geom32` values via
    /// `push_geom` and perform a two-cycle encode/decode:
    ///
    /// 1. push -> encode -> decode  (`canonical`): exercises `push_geom` and
    ///    `normalize_geometry_offsets`; normalizes the sparse push_* layout to
    ///    the dense form that `from_encoded` always returns.
    /// 2. canonical -> encode -> decode  (`output`): verifies idempotency of
    ///    encode/decode on the canonical form
    ///
    /// Comparing `canonical == output` catches both panics in the push path
    /// and silent data corruption in encode/decode
    fn roundtrip_via_push(geoms: &[Geom32]) -> (GeometryValues, GeometryValues) {
        let pushed = GeometryValues::from_geoms(geoms, false);
        let canonical = roundtrip(&pushed);
        let output = roundtrip(&canonical);
        (canonical, output)
    }

    fn arb_coord() -> impl Strategy<Value = Coord32> {
        (any::<i32>(), any::<i32>()).prop_map(|(x, y)| Coord32 { x, y })
    }

    fn arb_geom() -> impl Strategy<Value = Geom32> {
        prop_oneof![
            // Point
            arb_coord().prop_map(Point).prop_map(Geom32::Point),
            // LineString
            prop::collection::vec(arb_coord(), 2..10)
                .prop_map(|coords| Geom32::LineString(LineString(coords))),
            // Polygon (single exterior ring, no holes)
            prop::collection::vec(arb_coord(), 3..8).prop_map(|mut coords| {
                coords.push(coords[0]);
                Geom32::Polygon(Polygon::new(LineString(coords), vec![]))
            }),
            // MultiPoint
            prop::collection::vec(arb_coord(), 2..8).prop_map(|coords| {
                Geom32::MultiPoint(MultiPoint(coords.into_iter().map(Point).collect()))
            }),
            // MultiLineString
            prop::collection::vec(prop::collection::vec(arb_coord(), 2..6), 2..5,).prop_map(
                |lines| Geom32::MultiLineString(MultiLineString(
                    lines.into_iter().map(LineString).collect(),
                ))
            ),
            // MultiPolygon
            prop::collection::vec(arb_coord(), 3..6).prop_map(|mut coords| {
                coords.push(coords[0]);
                Geom32::MultiPolygon(MultiPolygon(vec![Polygon::new(LineString(coords), vec![])]))
            }),
        ]
    }

    /// Mixing `LineString` with `MultiLineString`
    fn arb_mixed_linestring_geoms() -> impl Strategy<Value = Vec<Geom32>> {
        prop::collection::vec(arb_geom(), 2..12)
            .prop_map(|geoms| {
                geoms
                    .into_iter()
                    .filter(|g| matches!(g, Geom32::LineString(_) | Geom32::MultiLineString(_)))
                    .collect::<Vec<_>>()
            })
            .prop_filter("needs both LS and MLS", |geoms| {
                geoms.iter().any(|g| matches!(g, Geom32::LineString(_)))
                    && geoms
                        .iter()
                        .any(|g| matches!(g, Geom32::MultiLineString(_)))
            })
    }

    /// Mixing `Point` with `MultiPoint`
    fn arb_mixed_point_geoms() -> impl Strategy<Value = Vec<Geom32>> {
        prop::collection::vec(arb_geom(), 2..12)
            .prop_map(|geoms| {
                geoms
                    .into_iter()
                    .filter(|g| matches!(g, Geom32::Point(_) | Geom32::MultiPoint(_)))
                    .collect::<Vec<_>>()
            })
            .prop_filter("needs both P and MP", |geoms| {
                geoms.iter().any(|g| matches!(g, Geom32::Point(_)))
                    && geoms.iter().any(|g| matches!(g, Geom32::MultiPoint(_)))
            })
    }

    /// Mixing `Polygon` with `MultiPolygon`
    fn arb_mixed_polygon_geoms() -> impl Strategy<Value = Vec<Geom32>> {
        prop::collection::vec(arb_geom(), 2..8)
            .prop_map(|geoms| {
                geoms
                    .into_iter()
                    .filter(|g| matches!(g, Geom32::Polygon(_) | Geom32::MultiPolygon(_)))
                    .collect::<Vec<_>>()
            })
            .prop_filter("needs both Poly and MPoly", |geoms| {
                geoms.iter().any(|g| matches!(g, Geom32::Polygon(_)))
                    && geoms.iter().any(|g| matches!(g, Geom32::MultiPolygon(_)))
            })
    }

    /// Mixing `Point` with `MultiLineString`
    fn arb_cross_point_mls_geoms() -> impl Strategy<Value = Vec<Geom32>> {
        prop::collection::vec(
            prop_oneof![
                arb_coord().prop_map(Point).prop_map(Geom32::Point),
                prop::collection::vec(prop::collection::vec(arb_coord(), 2..6), 2..5).prop_map(
                    |lines| {
                        Geom32::MultiLineString(MultiLineString(
                            lines.into_iter().map(LineString).collect(),
                        ))
                    }
                ),
            ],
            2..12,
        )
        .prop_filter("needs both Point and MultiLineString", |geoms| {
            geoms.iter().any(|g| matches!(g, Geom32::Point(_)))
                && geoms
                    .iter()
                    .any(|g| matches!(g, Geom32::MultiLineString(_)))
        })
    }

    /// Mixing `Point` with `MultiPolygon`.
    fn arb_cross_point_mpoly_geoms() -> impl Strategy<Value = Vec<Geom32>> {
        prop::collection::vec(
            prop_oneof![
                arb_coord().prop_map(Point).prop_map(Geom32::Point),
                prop::collection::vec(arb_coord(), 3..6).prop_map(|mut coords| {
                    coords.push(coords[0]);
                    Geom32::MultiPolygon(MultiPolygon(vec![Polygon::new(
                        LineString(coords),
                        vec![],
                    )]))
                }),
            ],
            2..10,
        )
        .prop_filter("needs both Point and MultiPolygon", |geoms| {
            geoms.iter().any(|g| matches!(g, Geom32::Point(_)))
                && geoms.iter().any(|g| matches!(g, Geom32::MultiPolygon(_)))
        })
    }

    /// Mixing `LineString` with `MultiPolygon`
    fn arb_cross_ls_mpoly_geoms() -> impl Strategy<Value = Vec<Geom32>> {
        prop::collection::vec(
            prop_oneof![
                prop::collection::vec(arb_coord(), 2..8)
                    .prop_map(|coords| Geom32::LineString(LineString(coords))),
                prop::collection::vec(arb_coord(), 3..6).prop_map(|mut coords| {
                    coords.push(coords[0]);
                    Geom32::MultiPolygon(MultiPolygon(vec![Polygon::new(
                        LineString(coords),
                        vec![],
                    )]))
                }),
            ],
            2..10,
        )
        .prop_filter("needs both LineString and MultiPolygon", |geoms| {
            geoms.iter().any(|g| matches!(g, Geom32::LineString(_)))
                && geoms.iter().any(|g| matches!(g, Geom32::MultiPolygon(_)))
        })
    }

    proptest! {
        #[test]
        fn test_geometry_roundtrip(geom in arb_geom()) {
            let (canonical, output) = roundtrip_via_push(&[geom]);
            prop_assert_eq!(output, canonical);
        }

        #[test]
        fn test_mixed_linestring_roundtrip(geoms in arb_mixed_linestring_geoms()) {
            let (canonical, output) = roundtrip_via_push(&geoms);
            prop_assert_eq!(output, canonical);
        }

        #[test]
        fn test_mixed_point_roundtrip(geoms in arb_mixed_point_geoms()) {
            let (canonical, output) = roundtrip_via_push(&geoms);
            prop_assert_eq!(output, canonical);
        }

        #[test]
        fn test_mixed_polygon_roundtrip(geoms in arb_mixed_polygon_geoms()) {
            let (canonical, output) = roundtrip_via_push(&geoms);
            prop_assert_eq!(output, canonical);
        }

        #[ignore = "encoder does not implement this correctly"]
        #[test]
        fn test_cross_point_mls_roundtrip(geoms in arb_cross_point_mls_geoms()) {
            let (canonical, output) = roundtrip_via_push(&geoms);
            prop_assert_eq!(output, canonical);
        }

        #[ignore = "encoder does not implement this correctly"]
        #[test]
        fn test_cross_point_mpoly_roundtrip(geoms in arb_cross_point_mpoly_geoms()) {
            let (canonical, output) = roundtrip_via_push(&geoms);
            prop_assert_eq!(output, canonical);
        }

        #[test]
        fn test_cross_ls_mpoly_roundtrip(geoms in arb_cross_ls_mpoly_geoms()) {
            let (canonical, output) = roundtrip_via_push(&geoms);
            prop_assert_eq!(output, canonical);
        }
    }

    /// Verifies that a Morton-encoded vertex dictionary is fully expanded inside `from_encoded`.
    /// This ensures `GeometryValues` always holds flat `(x, y)` pairs.
    #[test]
    fn test_morton_vertex_dictionary_expansion() {
        use integer_encoding::VarIntWriter as _;

        // Morton vertex dictionary: 3 unique entries.
        // Raw codes [0, 16, 32] -> delta-encoded as [0, 16, 16].
        // The MortonDelta logical encoding means the decoder will undo the delta,
        // then decode each Morton code to an (x, y) pair.
        let morton_deltas = vec![0u32, 16, 16];
        let mut raw_bytes = Vec::new();
        let mut scratch = Vec::new();
        let mut codec = FastPFor256::default();
        let physical_encoding = IntEncoder::varint()
            .physical
            .encode_u32s(&morton_deltas, &mut raw_bytes, &mut scratch, &mut codec)
            .unwrap();
        let morton_dict = EncodedStream {
            meta: StreamMeta::new(
                StreamType::Data(DictionaryType::Morton),
                IntEncoding::new(
                    LogicalEncoding::MortonDelta(MortonMeta {
                        num_bits: 3,
                        coordinate_shift: 0,
                    }),
                    physical_encoding,
                ),
                3, // 3 dictionary entries -> 3 physical u32 values
            ),
            data: EncodedStreamData::VarInt(raw_bytes),
        };

        // Assemble, serialize, parse, decode — same wire layout as geometry encoder:
        // stream count, then meta (geom type), parts, vertex offsets, Morton dict.
        let mut enc = Encoder::default();
        enc.write_varint(4u32).unwrap();
        do_write_u32(
            &[GeometryType::LineString as u32],
            StreamType::Length(LengthType::VarBinary),
            IntEncoder::varint(),
            &mut enc,
        )
        .unwrap();
        do_write_u32(
            &[4u32],
            StreamType::Length(LengthType::Parts),
            IntEncoder::varint(),
            &mut enc,
        )
        .unwrap();
        do_write_u32(
            &[0u32, 1, 2, 1],
            StreamType::Offset(OffsetType::Vertex),
            IntEncoder::varint(),
            &mut enc,
        )
        .unwrap();
        enc.write_stream(&morton_dict).unwrap();
        let buffer = enc.data;

        let mut p = parser();
        let parsed = assert_empty(RawGeometry::from_bytes(&buffer, &mut p));
        assert_snapshot!(p.reserved(), @"72");

        let mut d = dec();
        let decoded = LazyParsed::Raw(parsed).into_parsed(&mut d).unwrap();
        assert_snapshot!(d.consumed(), @"100");
        assert_eq!(decoded.vertices, Some(vec![0i32, 0, 4, 0, 0, 4, 4, 0]));

        let geom = decoded.to_geojson(0).unwrap();
        assert_eq!(geom, wkt!(LINESTRING(0 0,4 0,0 4,4 0)).into());
    }

    mod tessellation_tests {
        use geo_types::{LineString, MultiPolygon, Polygon};

        use crate::Geom32;
        use crate::decoder::GeometryValues;

        #[test]
        fn earcut_polygon_indices_in_range() {
            let exterior = LineString::from(vec![(0_i32, 0), (10, 0), (10, 10), (0, 10), (0, 0)]);
            let polygon = Polygon::new(exterior, vec![]);
            let mut g = GeometryValues::new_tessellated();
            g.push_geom(&Geom32::Polygon(polygon));
            let tris = g.triangles().expect("triangles");
            let n = tris[0];
            assert!(n > 0, "expected at least one triangle");
            let ib = g.index_buffer().expect("index buffer");
            assert_eq!(ib.len(), usize::try_from(n).unwrap() * 3);
            // 4 unique (non-closing) vertices → indices in 0..4
            assert!(ib.iter().all(|&i| i < 4));
        }

        #[test]
        fn earcut_vertex_offset_for_multi_polygon_parts() {
            let exterior1 = LineString::from(vec![(0_i32, 0), (10, 0), (10, 10), (0, 10), (0, 0)]);
            let poly1 = Polygon::new(exterior1, vec![]);
            let exterior2 = LineString::from(vec![(20, 0), (30, 0), (30, 10), (20, 10), (20, 0)]);
            let poly2 = Polygon::new(exterior2, vec![]);
            let mut g = GeometryValues::new_tessellated();
            g.push_geom(&Geom32::MultiPolygon(MultiPolygon(vec![poly1, poly2])));
            let ib = g.index_buffer().expect("index buffer");
            let tris = g.triangles().expect("triangles");
            assert_eq!(tris.len(), 1);
            let total = usize::try_from(tris[0]).unwrap();
            assert_eq!(ib.len(), total * 3);
            // First quad: 4 verts → 2 triangles, 6 indices
            let split = 6;
            let (first, second) = ib.split_at(split);
            assert!(
                first.iter().all(|&i| i < 4),
                "first polygon indices should reference verts 0..4: {first:?}"
            );
            assert!(
                second.iter().all(|&i| (4..8).contains(&i)),
                "second polygon indices should reference verts 4..8: {second:?}"
            );
        }
    }

    /// Verify that `compute_tessellation` produces the same result as
    /// building with `new_tessellated()` + `push_geom`.
    #[test]
    fn compute_tessellation_matches_push_geom() {
        let exterior = LineString::from(vec![(0_i32, 0), (10, 0), (10, 10), (0, 10), (0, 0)]);
        let polygon = Geom32::Polygon(Polygon::new(exterior, vec![]));

        // Path 1: push_geom with new_tessellated (existing path)
        let mut via_push = GeometryValues::new_tessellated();
        via_push.push_geom(&polygon);

        // Path 2: push_geom without tessellation, then compute_tessellation
        let mut via_compute = GeometryValues::default();
        via_compute.push_geom(&polygon);
        assert!(via_compute.triangles().is_none());
        via_compute.compute_tessellation().unwrap();

        assert_eq!(via_push.triangles(), via_compute.triangles());
        assert_eq!(via_push.index_buffer(), via_compute.index_buffer());
    }

    /// `compute_tessellation` on a multi-polygon from columnar data.
    #[test]
    fn compute_tessellation_multi_polygon_columnar() {
        let ext1 = LineString::from(vec![(0_i32, 0), (10, 0), (10, 10), (0, 10), (0, 0)]);
        let ext2 = LineString::from(vec![(20, 0), (30, 0), (30, 10), (20, 10), (20, 0)]);
        let mp = Geom32::MultiPolygon(MultiPolygon(vec![
            Polygon::new(ext1, vec![]),
            Polygon::new(ext2, vec![]),
        ]));

        // Path 1: from_geoms with tessellation
        let via_push = GeometryValues::from_geoms(std::slice::from_ref(&mp), true);

        // Path 2: from_geoms without tessellation, then compute
        let mut via_compute = GeometryValues::from_geoms(&[mp], false);
        via_compute.compute_tessellation().unwrap();

        assert_eq!(via_push.triangles(), via_compute.triangles());
        assert_eq!(via_push.index_buffer(), via_compute.index_buffer());
    }

    /// `compute_tessellation` is a no-op when tessellation data already exists.
    #[test]
    fn compute_tessellation_idempotent() {
        let exterior = LineString::from(vec![(0_i32, 0), (10, 0), (10, 10), (0, 10), (0, 0)]);
        let polygon = Geom32::Polygon(Polygon::new(exterior, vec![]));

        let mut g = GeometryValues::new_tessellated();
        g.push_geom(&polygon);
        let tris_before = g.triangles().unwrap().to_vec();
        let ib_before = g.index_buffer().unwrap().to_vec();

        g.compute_tessellation().unwrap(); // should be no-op

        assert_eq!(g.triangles().unwrap(), &tris_before);
        assert_eq!(g.index_buffer().unwrap(), &ib_before);
    }

    /// Non-polygon types are ignored by `compute_tessellation`.
    #[test]
    fn compute_tessellation_skips_non_polygons() {
        use geo_types::Point;

        let mut g = GeometryValues::default();
        g.push_geom(&Geom32::Point(Point::new(0_i32, 0)));
        g.push_geom(&Geom32::LineString(LineString::from(vec![
            (0_i32, 0),
            (1, 1),
        ])));
        g.compute_tessellation().unwrap();

        assert_eq!(g.triangles(), Some(&[][..]));
        // index_buffer is only initialized when a polygon is actually tessellated
        assert!(g.index_buffer().is_none_or(<[u32]>::is_empty));
    }
}
