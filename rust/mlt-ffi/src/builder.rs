use geo_types::{Coord, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon};
use mlt_core::encoder::{StagedLayer01, StagedProperty};
use mlt_core::Geom32;
use mlt_core::{GeometryValues, IdValues};

use crate::ffi::MltColumnType;

/// Columnar layer encoder. Data is set column-at-a-time, then encoded.
pub struct MltLayerEncoder {
    name: String,
    extent: u32,
    ids: Option<IdValues>,
    geometries: Option<Vec<Geom32>>,
    properties: Vec<StagedProperty>,
}

/// Tile encoder holding completed staged layers.
#[derive(Default)]
pub struct MltTileEncoder {
    layers: Vec<StagedLayer01>,
}

impl MltLayerEncoder {
    pub fn new(name: String, extent: u32) -> Self {
        Self {
            name,
            extent,
            ids: None,
            geometries: None,
            properties: Vec::new(),
        }
    }

    pub fn set_ids(&mut self, ids: &[u64], present: Option<&[bool]>) -> Result<(), String> {
        let values: Vec<Option<u64>> = match present {
            Some(p) => {
                if p.len() != ids.len() {
                    return Err(format!(
                        "ids length ({}) != present length ({})",
                        ids.len(),
                        p.len()
                    ));
                }
                ids.iter()
                    .zip(p.iter())
                    .map(|(&id, &pr)| if pr { Some(id) } else { None })
                    .collect()
            }
            None => ids.iter().copied().map(Some).collect(),
        };
        self.ids = Some(IdValues(values));
        Ok(())
    }

    /// Set geometry from flat coordinate + meta arrays (row-oriented input).
    ///
    /// `coords` contains all coordinates flat: `[x0, y0, x1, y1, ...]`.
    /// `meta` contains per-feature type tags and structure metadata concatenated.
    /// See the module-level docs for the meta format.
    pub fn set_geometries(&mut self, coords: &[i32], meta: &[u32]) -> Result<(), String> {
        self.geometries = Some(parse_meta_geometries(coords, meta)?);
        Ok(())
    }

    /// Add a scalar column from raw bytes + type tag.
    pub fn add_column(
        &mut self,
        name: String,
        col_type: MltColumnType,
        data: &[u8],
        count: usize,
        present: Option<&[bool]>,
    ) -> Result<(), String> {
        fn cast_slice<'a, T: bytemuck::Pod>(
            data: &'a [u8],
            count: usize,
            type_name: &str,
        ) -> Result<&'a [T], String> {
            let expected = count * size_of::<T>();
            if data.len() < expected {
                return Err(format!(
                    "{type_name} column: need {expected} bytes for {count} elements, got {}",
                    data.len()
                ));
            }
            Ok(bytemuck::cast_slice(&data[..expected]))
        }

        match col_type {
            MltColumnType::Bool => {
                if data.len() < count {
                    return Err(format!(
                        "bool column: need {count} bytes, got {}",
                        data.len()
                    ));
                }
                let values: Vec<bool> = data[..count].iter().map(|&b| b != 0).collect();
                self.properties.push(push_scalar_or_opt(
                    name,
                    &values,
                    present,
                    StagedProperty::bool,
                    StagedProperty::opt_bool,
                ));
            }
            MltColumnType::I8 => {
                let values: &[i8] = cast_slice(data, count, "i8")?;
                self.properties.push(push_scalar_or_opt(
                    name,
                    values,
                    present,
                    StagedProperty::i8,
                    StagedProperty::opt_i8,
                ));
            }
            MltColumnType::U8 => {
                let values: &[u8] = cast_slice(data, count, "u8")?;
                self.properties.push(push_scalar_or_opt(
                    name,
                    values,
                    present,
                    StagedProperty::u8,
                    StagedProperty::opt_u8,
                ));
            }
            MltColumnType::I32 => {
                let values: &[i32] = cast_slice(data, count, "i32")?;
                self.properties.push(push_scalar_or_opt(
                    name,
                    values,
                    present,
                    StagedProperty::i32,
                    StagedProperty::opt_i32,
                ));
            }
            MltColumnType::U32 => {
                let values: &[u32] = cast_slice(data, count, "u32")?;
                self.properties.push(push_scalar_or_opt(
                    name,
                    values,
                    present,
                    StagedProperty::u32,
                    StagedProperty::opt_u32,
                ));
            }
            MltColumnType::I64 => {
                let values: &[i64] = cast_slice(data, count, "i64")?;
                self.properties.push(push_scalar_or_opt(
                    name,
                    values,
                    present,
                    StagedProperty::i64,
                    StagedProperty::opt_i64,
                ));
            }
            MltColumnType::U64 => {
                let values: &[u64] = cast_slice(data, count, "u64")?;
                self.properties.push(push_scalar_or_opt(
                    name,
                    values,
                    present,
                    StagedProperty::u64,
                    StagedProperty::opt_u64,
                ));
            }
            MltColumnType::F32 => {
                let values: &[f32] = cast_slice(data, count, "f32")?;
                self.properties.push(push_scalar_or_opt(
                    name,
                    values,
                    present,
                    StagedProperty::f32,
                    StagedProperty::opt_f32,
                ));
            }
            MltColumnType::F64 => {
                let values: &[f64] = cast_slice(data, count, "f64")?;
                self.properties.push(push_scalar_or_opt(
                    name,
                    values,
                    present,
                    StagedProperty::f64,
                    StagedProperty::opt_f64,
                ));
            }
        }
        Ok(())
    }

    /// Add a string column from raw concatenated UTF-8 + offset array.
    pub fn add_string_column_raw(
        &mut self,
        name: String,
        data: &[u8],
        offsets: &[u32],
        present: Option<&[bool]>,
    ) {
        if offsets.len() < 2 {
            self.properties
                .push(StagedProperty::str(name, Vec::<String>::new()));
            return;
        }
        let count = offsets.len() - 1;
        if let Some(p) = present {
            let mut values = Vec::with_capacity(count);
            for i in 0..count {
                if i < p.len() && p[i] {
                    let start = offsets[i] as usize;
                    let end = offsets[i + 1] as usize;
                    let s = if start <= end && end <= data.len() {
                        crate::lossy_string(&data[start..end])
                    } else {
                        String::new()
                    };
                    values.push(Some(s));
                } else {
                    values.push(None);
                }
            }
            self.properties.push(StagedProperty::opt_str(name, values));
        } else {
            let mut values = Vec::with_capacity(count);
            for i in 0..count {
                let start = offsets[i] as usize;
                let end = offsets[i + 1] as usize;
                let s = if start <= end && end <= data.len() {
                    crate::lossy_string(&data[start..end])
                } else {
                    String::new()
                };
                values.push(s);
            }
            self.properties.push(StagedProperty::str(name, values));
        }
    }

    pub fn into_staged(self) -> Result<StagedLayer01, String> {
        let geoms = self
            .geometries
            .ok_or_else(|| "geometry was not set".to_string())?;
        let geometry = GeometryValues::from_geoms(&geoms, false);
        Ok(StagedLayer01 {
            name: self.name,
            extent: self.extent,
            id: self.ids,
            geometry,
            properties: self.properties,
        })
    }
}

impl MltTileEncoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_layer(&mut self, layer: StagedLayer01) {
        self.layers.push(layer);
    }

    pub fn into_layers(self) -> Vec<StagedLayer01> {
        self.layers
    }
}

/// Build the right `StagedProperty` variant depending on whether a presence
/// bitmap was supplied by the caller.
fn push_scalar_or_opt<T: Copy>(
    name: String,
    values: &[T],
    present: Option<&[bool]>,
    non_opt: fn(String, Vec<T>) -> StagedProperty,
    opt: fn(String, Vec<Option<T>>) -> StagedProperty,
) -> StagedProperty {
    match present {
        Some(p) => opt(
            name,
            values
                .iter()
                .zip(p.iter())
                .map(|(&v, &pr)| if pr { Some(v) } else { None })
                .collect(),
        ),
        None => non_opt(name, values.to_vec()),
    }
}

// ---------------------------------------------------------------------------
// Flat coords + meta → Vec<Geom32>
// ---------------------------------------------------------------------------

/// Geometry type tags used in the flat coords+meta wire format.
/// Must stay in sync with `GeometryColumnizer.java`.
const META_POINT: u32 = 0;
const META_LINESTRING: u32 = 1;
const META_POLYGON: u32 = 2;
const META_MULTIPOINT: u32 = 3;
const META_MULTILINESTRING: u32 = 4;
const META_MULTIPOLYGON: u32 = 5;

fn parse_meta_geometries(coords: &[i32], meta: &[u32]) -> Result<Vec<Geom32>, String> {
    let mut geoms = Vec::new();
    let mut ci = 0usize;
    let mut mi = 0usize;

    while mi < meta.len() {
        let geom_type = meta[mi];
        mi += 1;
        let g = match geom_type {
            META_POINT => {
                let (x, y) = read_coord(coords, &mut ci)?;
                Geom32::Point(Point::new(x, y))
            }
            META_LINESTRING => {
                let n = take_meta(meta, &mut mi)? as usize;
                Geom32::LineString(read_linestring(coords, &mut ci, n)?)
            }
            META_POLYGON => {
                let num_rings = take_meta(meta, &mut mi)? as usize;
                let ring_lens = take_meta_slice(meta, &mut mi, num_rings)?;
                Geom32::Polygon(read_polygon(coords, &mut ci, ring_lens)?)
            }
            META_MULTIPOINT => {
                let n = take_meta(meta, &mut mi)? as usize;
                Geom32::MultiPoint(MultiPoint(read_points(coords, &mut ci, n)?))
            }
            META_MULTILINESTRING => {
                let n = take_meta(meta, &mut mi)? as usize;
                let lens = take_meta_slice(meta, &mut mi, n)?;
                Geom32::MultiLineString(MultiLineString(read_linestrings(coords, &mut ci, lens)?))
            }
            META_MULTIPOLYGON => {
                let num_polys = take_meta(meta, &mut mi)? as usize;
                let mut ring_slices = Vec::with_capacity(num_polys);
                for _ in 0..num_polys {
                    let num_rings = take_meta(meta, &mut mi)? as usize;
                    ring_slices.push(take_meta_slice(meta, &mut mi, num_rings)?);
                }
                let mut polygons = Vec::with_capacity(num_polys);
                for ring_lens in ring_slices {
                    polygons.push(read_polygon(coords, &mut ci, ring_lens)?);
                }
                Geom32::MultiPolygon(MultiPolygon(polygons))
            }
            _ => return Err(format!("invalid geometry type: {geom_type}")),
        };
        geoms.push(g);
    }

    if ci != coords.len() {
        return Err(format!(
            "unconsumed coords: used {ci}, total {}",
            coords.len()
        ));
    }
    Ok(geoms)
}

fn take_meta(meta: &[u32], mi: &mut usize) -> Result<u32, String> {
    if *mi >= meta.len() {
        return Err("unexpected end of meta array".into());
    }
    let v = meta[*mi];
    *mi += 1;
    Ok(v)
}

fn take_meta_slice<'a>(meta: &'a [u32], mi: &mut usize, n: usize) -> Result<&'a [u32], String> {
    let end = mi.checked_add(n).ok_or("meta overflow")?;
    if end > meta.len() {
        return Err(format!(
            "meta too short: need {end} entries, have {}",
            meta.len()
        ));
    }
    let slice = &meta[*mi..end];
    *mi = end;
    Ok(slice)
}

fn read_coord(coords: &[i32], ci: &mut usize) -> Result<(i32, i32), String> {
    if *ci + 1 >= coords.len() {
        return Err(format!(
            "coords too short: need index {}, have {}",
            *ci + 1,
            coords.len()
        ));
    }
    let x = coords[*ci];
    let y = coords[*ci + 1];
    *ci += 2;
    Ok((x, y))
}

fn read_coords(coords: &[i32], ci: &mut usize, n: usize) -> Result<Vec<Coord<i32>>, String> {
    let needed = n * 2;
    if *ci + needed > coords.len() {
        return Err(format!(
            "coords too short: need {needed} values from index {}, have {}",
            *ci,
            coords.len()
        ));
    }
    let mut result = Vec::with_capacity(n);
    for _ in 0..n {
        let x = coords[*ci];
        let y = coords[*ci + 1];
        *ci += 2;
        result.push(Coord { x, y });
    }
    Ok(result)
}

fn read_linestring(coords: &[i32], ci: &mut usize, n: usize) -> Result<LineString<i32>, String> {
    Ok(LineString(read_coords(coords, ci, n)?))
}

fn read_linestrings(
    coords: &[i32],
    ci: &mut usize,
    lens: &[u32],
) -> Result<Vec<LineString<i32>>, String> {
    let mut lines = Vec::with_capacity(lens.len());
    for &len in lens {
        lines.push(read_linestring(coords, ci, len as usize)?);
    }
    Ok(lines)
}

fn read_polygon(coords: &[i32], ci: &mut usize, ring_lens: &[u32]) -> Result<Polygon<i32>, String> {
    if ring_lens.is_empty() {
        return Err("polygon with zero rings".into());
    }
    let exterior = read_linestring(coords, ci, ring_lens[0] as usize)?;
    let mut interiors = Vec::with_capacity(ring_lens.len() - 1);
    for &len in &ring_lens[1..] {
        interiors.push(read_linestring(coords, ci, len as usize)?);
    }
    Ok(Polygon::new(exterior, interiors))
}

fn read_points(coords: &[i32], ci: &mut usize, n: usize) -> Result<Vec<Point<i32>>, String> {
    let mut points = Vec::with_capacity(n);
    for _ in 0..n {
        let (x, y) = read_coord(coords, ci)?;
        points.push(Point::new(x, y));
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use mlt_core::GeometryType;

    use super::*;

    #[test]
    fn test_roundtrip() {
        let mut layer = MltLayerEncoder::new("test".to_string(), 4096);
        layer.set_ids(&[1, 2, 3], None).unwrap();
        // 3 points via set_geometries: meta = [Point, Point, Point]
        layer
            .set_geometries(&[10, 20, 30, 40, 50, 60], &[0, 0, 0])
            .unwrap();
        let pop_values: [i32; 3] = [100, 200, 300];
        layer
            .add_column(
                "pop".to_string(),
                MltColumnType::I32,
                bytemuck::cast_slice(&pop_values),
                3,
                None,
            )
            .unwrap();
        // "a" + "c" concatenated, offsets delimit each string
        layer.add_string_column_raw(
            "name".to_string(),
            b"ac",
            &[0, 1, 1, 2], // 3 entries: "a", "" (null), "c"
            Some(&[true, false, true]),
        );

        let staged = layer.into_staged().unwrap();
        assert_eq!(staged.name, "test");
        assert_eq!(staged.extent, 4096);
        assert_eq!(staged.id.as_ref().unwrap().0.len(), 3);
        assert_eq!(staged.geometry.vector_types().len(), 3);
        assert_eq!(staged.properties.len(), 2);
    }

    #[test]
    fn test_missing_geometry_rejected() {
        let layer = MltLayerEncoder::new("t".to_string(), 4096);
        let err = layer.into_staged().unwrap_err();
        assert!(err.contains("geometry was not set"), "{err}");
    }

    #[test]
    fn test_ids_presence_length_mismatch() {
        let mut layer = MltLayerEncoder::new("t".to_string(), 4096);
        let err = layer.set_ids(&[1, 2], Some(&[true])).unwrap_err();
        assert!(err.contains("present length"), "{err}");
    }

    #[test]
    fn test_invalid_geometry_type() {
        let mut layer = MltLayerEncoder::new("t".to_string(), 4096);
        let err = layer.set_geometries(&[], &[99]).unwrap_err();
        assert!(err.contains("invalid geometry type"), "{err}");
    }

    #[test]
    fn test_presence_bitmap() {
        let prop = push_scalar_or_opt(
            "x".to_string(),
            &[10i32, 20, 30],
            Some(&[true, false, true]),
            StagedProperty::i32,
            StagedProperty::opt_i32,
        );
        // Should produce the optional variant since presence was provided
        assert_eq!(prop.name(), "x");
    }

    /// Encode a polygon layer with `tessellate=true` via `encode_try_sort`,
    /// then decode and verify that triangle data is present in the output.
    #[test]
    fn test_tessellate_polygon_via_encode_try_sort() {
        use mlt_core::Layer;

        // Build a layer with a single square polygon via set_geometries.
        // Polygon: (0,0) (10,0) (10,10) (0,10) — 4 vertices (closing vertex included in meta).
        let mut layer = MltLayerEncoder::new("poly".to_string(), 4096);
        layer
            .set_geometries(
                &[0, 0, 10, 0, 10, 10, 0, 10],
                &[2, 1, 4], // type=Polygon, 1 ring, 4 coord pairs
            )
            .unwrap();

        let staged = layer.into_staged().unwrap();

        let cfg = mlt_core::encoder::EncoderConfig {
            tessellate: true,
            try_spatial_morton_sort: false,
            try_spatial_hilbert_sort: false,
            try_id_sort: false,
            allow_fsst: false,
            allow_fpf: false,
            allow_shared_dict: false,
        };
        let bytes = staged.encode_try_sort(cfg).expect("encode_try_sort");
        assert!(!bytes.is_empty());

        // Decode the encoded bytes and verify tessellation streams are present.
        let mut parser = mlt_core::Parser::default();
        let (_, parsed_layer) = Layer::from_bytes(&bytes, &mut parser).expect("parse");
        let Layer::Tag01(layer01) = parsed_layer else {
            panic!("expected Tag01 layer");
        };
        let mut decoder = mlt_core::Decoder::default();
        let parsed = layer01.decode_all(&mut decoder).expect("decode_all");

        // The decoded geometry should contain tessellation data.
        let geom = &parsed.geometry;
        assert_eq!(geom.vector_types(), &[GeometryType::Polygon]);

        let tris = geom
            .triangles()
            .expect("triangles stream should be present");
        assert_eq!(tris.len(), 1, "one polygon → one triangle count");
        assert!(tris[0] > 0, "a quad should produce at least one triangle");

        let ib = geom
            .index_buffer()
            .expect("index buffer stream should be present");
        assert_eq!(
            ib.len(),
            usize::try_from(tris[0]).unwrap() * 3,
            "3 indices per triangle"
        );
        // 4 vertices → indices in 0..4
        assert!(ib.iter().all(|&i| i < 4));
    }

    // -------------------------------------------------------------------
    // set_geometries (flat coords + meta) tests
    // -------------------------------------------------------------------

    #[test]
    fn test_set_geometries_points() {
        let mut layer = MltLayerEncoder::new("pts".to_string(), 4096);
        // 3 points: (10,20), (30,40), (50,60)
        let coords = [10, 20, 30, 40, 50, 60];
        let meta = [0, 0, 0]; // 3x Point
        layer.set_geometries(&coords, &meta).unwrap();
        let staged = layer.into_staged().unwrap();
        assert_eq!(staged.geometry.vector_types().len(), 3);
        assert_eq!(
            staged.geometry.vector_types(),
            &[
                GeometryType::Point,
                GeometryType::Point,
                GeometryType::Point
            ]
        );
    }

    #[test]
    fn test_set_geometries_linestring() {
        let mut layer = MltLayerEncoder::new("ls".to_string(), 4096);
        // LineString with 3 points
        let coords = [0, 0, 10, 10, 20, 0];
        let meta = [1, 3]; // type=LineString, 3 coord pairs
        layer.set_geometries(&coords, &meta).unwrap();
        let staged = layer.into_staged().unwrap();
        assert_eq!(staged.geometry.vector_types(), &[GeometryType::LineString]);
    }

    #[test]
    fn test_set_geometries_polygon() {
        let mut layer = MltLayerEncoder::new("poly".to_string(), 4096);
        // Polygon with exterior ring of 5 points (including closing vertex)
        let coords = [0, 0, 10, 0, 10, 10, 0, 10, 0, 0];
        let meta = [2, 1, 5]; // type=Polygon, 1 ring, ring has 5 coord pairs
        layer.set_geometries(&coords, &meta).unwrap();
        let staged = layer.into_staged().unwrap();
        assert_eq!(staged.geometry.vector_types(), &[GeometryType::Polygon]);
    }

    #[test]
    fn test_set_geometries_multipoint() {
        let mut layer = MltLayerEncoder::new("mp".to_string(), 4096);
        let coords = [1, 2, 3, 4];
        let meta = [3, 2]; // type=MultiPoint, 2 points
        layer.set_geometries(&coords, &meta).unwrap();
        let staged = layer.into_staged().unwrap();
        assert_eq!(staged.geometry.vector_types(), &[GeometryType::MultiPoint]);
    }

    #[test]
    fn test_set_geometries_multilinestring() {
        let mut layer = MltLayerEncoder::new("mls".to_string(), 4096);
        // 2 linestrings: first has 2 pts, second has 3 pts
        let coords = [0, 0, 1, 1, 2, 2, 3, 3, 4, 4];
        let meta = [4, 2, 2, 3]; // type=MLS, 2 lines, line0=2pts, line1=3pts
        layer.set_geometries(&coords, &meta).unwrap();
        let staged = layer.into_staged().unwrap();
        assert_eq!(
            staged.geometry.vector_types(),
            &[GeometryType::MultiLineString]
        );
    }

    #[test]
    fn test_set_geometries_multipolygon() {
        let mut layer = MltLayerEncoder::new("mpoly".to_string(), 4096);
        // 2 triangles (no closing vertex) as a MultiPolygon
        let coords = [0, 0, 1, 0, 0, 1, 10, 10, 11, 10, 10, 11];
        let meta = [
            5, 2, // type=MultiPolygon, 2 polygons
            1, 3, // poly0: 1 ring, 3 coord pairs
            1, 3, // poly1: 1 ring, 3 coord pairs
        ];
        layer.set_geometries(&coords, &meta).unwrap();
        let staged = layer.into_staged().unwrap();
        assert_eq!(
            staged.geometry.vector_types(),
            &[GeometryType::MultiPolygon]
        );
    }

    #[test]
    fn test_set_geometries_mixed() {
        let mut layer = MltLayerEncoder::new("mix".to_string(), 4096);
        // Point + LineString
        let coords = [5, 5, 0, 0, 10, 10];
        let meta = [0, 1, 2]; // Point, then LineString with 2 points
        layer.set_geometries(&coords, &meta).unwrap();
        let staged = layer.into_staged().unwrap();
        assert_eq!(
            staged.geometry.vector_types(),
            &[GeometryType::Point, GeometryType::LineString]
        );
    }

    #[test]
    fn test_set_geometries_invalid_type() {
        let mut layer = MltLayerEncoder::new("t".to_string(), 4096);
        let err = layer.set_geometries(&[], &[99]).unwrap_err();
        assert!(err.contains("invalid geometry type"), "{err}");
    }

    #[test]
    fn test_set_geometries_unconsumed_coords() {
        let mut layer = MltLayerEncoder::new("t".to_string(), 4096);
        let err = layer.set_geometries(&[1, 2, 3, 4], &[0]).unwrap_err(); // Point uses 2, 2 left over
        assert!(err.contains("unconsumed coords"), "{err}");
    }

    #[test]
    fn test_tile_builder() {
        let mut l1 = MltLayerEncoder::new("a".to_string(), 4096);
        l1.set_geometries(&[1, 2], &[0]).unwrap(); // 1 Point
        let mut l2 = MltLayerEncoder::new("b".to_string(), 4096);
        l2.set_geometries(&[3, 4], &[0]).unwrap(); // 1 Point

        let mut tile = MltTileEncoder::new();
        tile.add_layer(l1.into_staged().unwrap());
        tile.add_layer(l2.into_staged().unwrap());

        let layers = tile.into_layers();
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].name, "a");
        assert_eq!(layers[1].name, "b");
    }
}
