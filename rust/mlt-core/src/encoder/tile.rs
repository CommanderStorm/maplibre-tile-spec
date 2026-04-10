//! Row-to-columnar conversion for the encoding pipeline.
//!
//! [`TileLayer01`] holds one [`TileFeature`] per map feature (row-oriented).
//! The methods here convert it into [`StagedLayer01`] (column-oriented) for
//! encoding.

use std::collections::HashMap;

use crate::decoder::{GeometryValues, IdValues, PropValue, TileFeature, TileLayer01};
use crate::encoder::model::StagedLayer01;
use crate::encoder::{SortStrategy, StagedProperty, StagedSharedDict, StringGroup};

impl TileLayer01 {
    /// Sort features by `sort`, then stage the tile for encoding.
    ///
    /// Convenience wrapper around [`stage_permuted`](Self::stage_permuted)
    /// for callers that don't need permutation-level control.
    #[must_use]
    #[hotpath::measure]
    pub fn stage(mut self, sort: SortStrategy, groups: &[StringGroup]) -> StagedLayer01 {
        assert!(!self.features.is_empty(), "empty tile");
        self.sort(sort);
        let perm: Vec<usize> = (0..self.features.len()).collect();
        self.stage_permuted(&perm, groups, &HashMap::new())
    }

    /// Convert this tile into a [`StagedLayer01`] by reading features in the
    /// order given by `perm`.
    ///
    /// `perm` is a permutation array (as returned by [`compute_permutation`](crate::encoder::compute_permutation)):
    /// `perm[0]` is the index of the first feature in the output, etc.
    ///
    /// `invariant_cols` maps column indices to pre-built [`StagedProperty`]
    /// values for columns that are sort-invariant (all values identical).
    /// Those columns are cloned from the cache rather than rebuilt.
    #[must_use]
    #[hotpath::measure]
    pub fn stage_permuted(
        &self,
        perm: &[usize],
        groups: &[StringGroup],
        invariant_cols: &HashMap<usize, StagedProperty>,
    ) -> StagedLayer01 {
        assert!(!self.features.is_empty(), "empty tile");

        let mut geometry = GeometryValues::default();
        for &i in perm {
            geometry.push_geom(&self.features[i].geometry);
        }

        let id = if self.features.iter().any(|f| f.id.is_some()) {
            Some(IdValues(
                perm.iter().map(|&i| self.features[i].id).collect(),
            ))
        } else {
            None
        };

        let col_to_group: HashMap<_, _> = groups
            .iter()
            .flat_map(|g| g.columns.iter().map(move |(_, i)| (*i, g)))
            .collect();

        let mut group_start: HashMap<_, _> = groups.iter().map(|g| (g.columns[0].1, g)).collect();

        let mut properties = Vec::with_capacity(self.property_names.len());
        for (col_idx, name) in self.property_names.iter().enumerate() {
            if let Some(cached) = invariant_cols.get(&col_idx) {
                properties.push(cached.clone());
            } else if let Some(g) = group_start.remove(&col_idx) {
                properties.push(build_shared_dict_permuted(g, &self.features, perm));
            } else if !col_to_group.contains_key(&col_idx) {
                properties.push(build_column_permuted(
                    name.clone(),
                    col_idx,
                    &self.features,
                    perm,
                ));
            }
        }

        StagedLayer01 {
            name: self.name.clone(),
            extent: self.extent,
            id,
            geometry,
            properties,
        }
    }
}

pub(crate) fn build_column_permuted(
    name: String,
    col: usize,
    features: &[TileFeature],
    perm: &[usize],
) -> StagedProperty {
    let first_val = perm.iter().find_map(|&i| features[i].properties.get(col));

    macro_rules! scalar_col {
        ($opt_ctor:ident, $non_opt_ctor:ident, $ty:ty, $sv:ident) => {{
            let opt_values: Vec<Option<$ty>> = perm
                .iter()
                .map(|&i| {
                    if let Some(PropValue::$sv(v)) = features[i].properties.get(col) {
                        *v
                    } else {
                        None
                    }
                })
                .collect();
            if opt_values.iter().any(Option::is_none) {
                StagedProperty::$opt_ctor(name, opt_values)
            } else {
                StagedProperty::$non_opt_ctor(name, opt_values.into_iter().flatten().collect())
            }
        }};
    }

    match first_val {
        Some(PropValue::Bool(_)) => scalar_col!(opt_bool, bool, bool, Bool),
        Some(PropValue::I8(_)) => scalar_col!(opt_i8, i8, i8, I8),
        Some(PropValue::U8(_)) => scalar_col!(opt_u8, u8, u8, U8),
        Some(PropValue::I32(_)) => scalar_col!(opt_i32, i32, i32, I32),
        Some(PropValue::U32(_)) => scalar_col!(opt_u32, u32, u32, U32),
        Some(PropValue::I64(_)) => scalar_col!(opt_i64, i64, i64, I64),
        Some(PropValue::U64(_)) => scalar_col!(opt_u64, u64, u64, U64),
        Some(PropValue::F32(_)) => scalar_col!(opt_f32, f32, f32, F32),
        Some(PropValue::F64(_)) => scalar_col!(opt_f64, f64, f64, F64),
        Some(PropValue::Str(_)) | None => {
            let opt_values: Vec<Option<String>> = perm
                .iter()
                .map(|&i| match features[i].properties.get(col) {
                    Some(PropValue::Str(v)) => v.clone(),
                    _ => None,
                })
                .collect();
            if opt_values.iter().any(Option::is_none) {
                StagedProperty::opt_str(name, opt_values)
            } else {
                StagedProperty::str(name, opt_values.into_iter().flatten())
            }
        }
    }
}

fn build_shared_dict_permuted(
    group: &StringGroup,
    features: &[TileFeature],
    perm: &[usize],
) -> StagedProperty {
    let mut order: Vec<usize> = (0..group.columns.len()).collect();
    order.sort_by_key(|&i| group.columns[i].1);

    let columns = order.into_iter().map(|i| {
        let (suffix, col_idx) = &group.columns[i];
        let values: Vec<Option<String>> = perm
            .iter()
            .map(|&fi| match features[fi].properties.get(*col_idx) {
                Some(PropValue::Str(s)) => s.clone(),
                _ => None,
            })
            .collect();
        (suffix.clone(), values)
    });

    StagedProperty::SharedDict(
        StagedSharedDict::new(group.prefix.clone(), columns).expect("StagedSharedDict succeed"),
    )
}

#[cfg(test)]
mod tests {
    use geo_types::Point;

    use super::*;
    use crate::Layer;
    use crate::decoder::GeometryValues;
    use crate::encoder::{Encoder, StagedLayer};
    use crate::geojson::Geom32;
    use crate::test_helpers::{dec, parser};

    fn layer_tile(staged: StagedLayer01) -> TileLayer01 {
        let buf = StagedLayer::Tag01(staged)
            .encode_into(Encoder::default())
            .unwrap()
            .into_layer_bytes()
            .unwrap();
        let (_, layer) = Layer::from_bytes(&buf, &mut parser()).unwrap();
        let Layer::Tag01(lazy) = layer else { panic!() };
        let mut d = dec();
        lazy.decode_all(&mut d).unwrap().into_tile(&mut d).unwrap()
    }

    fn two_points() -> GeometryValues {
        let mut g = GeometryValues::default();
        g.push_geom(&Geom32::Point(Point::new(0, 0)));
        g.push_geom(&Geom32::Point(Point::new(1, 1)));
        g
    }

    /// `into_tile` must produce a **typed** null (e.g. `PropValue::Bool(None)`)
    /// for null slots, matching the column's actual type, even when the **first**
    /// feature is null.
    #[test]
    fn null_first_feature_preserves_later_typed_value() {
        let tile = layer_tile(StagedLayer01 {
            name: "t".into(),
            extent: 4096,
            id: None,
            geometry: two_points(),
            properties: vec![StagedProperty::opt_bool("flag", vec![None, Some(false)])],
        });

        assert_eq!(tile.property_names, vec!["flag"]);
        // Null slot → typed null matching the column type
        assert_eq!(tile.features[0].properties[0], PropValue::Bool(None));
        // Non-null value after the null must not be dropped
        assert_eq!(tile.features[1].properties[0], PropValue::Bool(Some(false)));
    }

    /// Every scalar type must produce a typed null for null slots and a typed
    /// non-null value for present slots, even when the first feature is null.
    #[test]
    fn null_first_feature_across_types() {
        let props = vec![
            StagedProperty::opt_bool("b", vec![None, Some(true)]),
            StagedProperty::opt_i8("i8", vec![None, Some(-1)]),
            StagedProperty::opt_u8("u8", vec![None, Some(2)]),
            StagedProperty::opt_i32("i32", vec![None, Some(-3)]),
            StagedProperty::opt_u32("u32", vec![None, Some(4)]),
            StagedProperty::opt_i64("i64", vec![None, Some(-5)]),
            StagedProperty::opt_u64("u64", vec![None, Some(6)]),
            StagedProperty::opt_f32("f32", vec![None, Some(7.0)]),
            StagedProperty::opt_f64("f64", vec![None, Some(8.0)]),
            StagedProperty::opt_str("s", vec![None, Some("ok")]),
        ];
        let tile = layer_tile(StagedLayer01 {
            name: "t".into(),
            extent: 4096,
            id: None,
            geometry: two_points(),
            properties: props,
        });

        // Feature 0: every column is null → typed null for each column
        let n = &tile.features[0].properties;
        assert_eq!(n[0], PropValue::Bool(None));
        assert_eq!(n[1], PropValue::I8(None));
        assert_eq!(n[2], PropValue::U8(None));
        assert_eq!(n[3], PropValue::I32(None));
        assert_eq!(n[4], PropValue::U32(None));
        assert_eq!(n[5], PropValue::I64(None));
        assert_eq!(n[6], PropValue::U64(None));
        assert_eq!(n[7], PropValue::F32(None));
        assert_eq!(n[8], PropValue::F64(None));
        assert_eq!(n[9], PropValue::Str(None));

        // Feature 1: every column has its typed non-null value
        let p = &tile.features[1].properties;
        assert_eq!(p[0], PropValue::Bool(Some(true)));
        assert_eq!(p[1], PropValue::I8(Some(-1)));
        assert_eq!(p[2], PropValue::U8(Some(2)));
        assert_eq!(p[3], PropValue::I32(Some(-3)));
        assert_eq!(p[4], PropValue::U32(Some(4)));
        assert_eq!(p[5], PropValue::I64(Some(-5)));
        assert_eq!(p[6], PropValue::U64(Some(6)));
        assert_eq!(p[7], PropValue::F32(Some(7.0)));
        assert_eq!(p[8], PropValue::F64(Some(8.0)));
        assert_eq!(p[9], PropValue::Str(Some("ok".into())));
    }
}
