use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};

use crate::decoder::{LogicalEncoding, RawStream};
use crate::{DecodeState, Lazy};

/// Geometry column representation, parameterized by decode state.
///
/// - `Geometry<'a>` / `Geometry<'a, Lazy>` — either raw bytes or decoded, in an [`crate::LazyParsed`] enum.
/// - `Geometry<'a, Parsed>` — decoded [`GeometryValues`] directly (no enum wrapper).
pub type Geometry<'a, S = Lazy> = <S as DecodeState>::LazyOrParsed<RawGeometry<'a>, GeometryValues>;

/// Raw geometry data as read directly from the tile (borrows from input bytes)
#[derive(Debug, PartialEq, Clone)]
pub struct RawGeometry<'a> {
    pub(crate) meta: RawStream<'a>,
    pub(crate) items: Vec<RawStream<'a>>,
}

/// Parsed (decoded) geometry data
#[derive(Clone, Default, PartialEq, Eq)]
pub struct GeometryValues {
    pub(crate) vector_types: Vec<GeometryType>,
    pub(crate) geometry_offsets: Option<Vec<u32>>,
    pub(crate) part_offsets: Option<Vec<u32>>,
    pub(crate) ring_offsets: Option<Vec<u32>>,
    pub(crate) index_buffer: Option<Vec<u32>>,
    pub(crate) triangles: Option<Vec<u32>>,
    pub(crate) vertices: Option<Vec<i32>>,
}

impl GeometryValues {
    /// Construct from pre-built columnar arrays (no per-feature conversion).
    ///
    /// Offset arrays must be in the **dense** format produced by the decoder:
    /// - `geometry_offsets`: N+1 entries indexed by feature position (implicit 1 for
    ///   single types, sub-geometry count for Multi\* types).
    /// - `part_offsets`: indexed by cumulative sub-geometry count from `geometry_offsets`.
    /// - `ring_offsets`: indexed by cumulative part count from `part_offsets`.
    #[must_use]
    pub fn from_columnar(
        vector_types: Vec<GeometryType>,
        vertices: Option<Vec<i32>>,
        geometry_offsets: Option<Vec<u32>>,
        part_offsets: Option<Vec<u32>>,
        ring_offsets: Option<Vec<u32>>,
    ) -> Self {
        Self {
            vector_types,
            geometry_offsets,
            part_offsets,
            ring_offsets,
            index_buffer: None,
            triangles: None,
            vertices,
        }
    }
}

impl RawGeometry<'_> {
    /// Number of features, available from the metadata stream without decoding.
    ///
    /// When the geometry type stream uses RLE encoding, `num_values` reflects the
    /// physical (compressed) count. The logical feature count is stored in
    /// `RleMeta::num_rle_values`.
    #[must_use]
    pub fn feature_count(&self) -> u32 {
        match self.meta.meta.encoding.logical {
            LogicalEncoding::Rle(rle) | LogicalEncoding::DeltaRle(rle) => rle.num_rle_values,
            _ => self.meta.meta.num_values,
        }
    }
}

/// Types of geometries supported in MLT
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    PartialOrd,
    Eq,
    Hash,
    Ord,
    TryFromPrimitive,
    strum::Display,
    strum::IntoStaticStr,
    Serialize,
    Deserialize,
)]
#[repr(u8)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum GeometryType {
    /*
        ATTENTION: Do not modify the order of this enum - it is being used in geometry decoding
    */
    Point,
    LineString,
    Polygon,
    MultiPoint,
    MultiLineString,
    MultiPolygon,
}
