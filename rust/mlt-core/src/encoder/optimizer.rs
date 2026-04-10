use std::collections::HashMap;

use crate::MltResult;
use crate::decoder::{TileFeature, TileLayer01};
use crate::encoder::model::{StagedLayer, StagedLayer01};
use crate::encoder::property::encode::write_properties;
use crate::encoder::{
    Encoder, EncoderConfig, SortStrategy, StagedProperty, StringGroup, compute_permutation,
    group_string_properties, spatial_sort_likely_to_help,
};

impl StagedLayer {
    /// Automatically encode and write `self` to `enc`.
    #[hotpath::measure]
    pub fn encode_into(self, enc: Encoder) -> MltResult<Encoder> {
        match self {
            Self::Tag01(t) => t.encode_into(enc),
            Self::Unknown(u) => u.write_to(enc),
        }
    }
}

impl StagedLayer01 {
    /// Encode and serialize the layer directly into `enc`, without creating any
    /// intermediate representation.
    ///
    /// This is the hot path inside `TileLayer01::encode`: each sort-strategy
    /// trial calls this method on its own fresh `Encoder`, and only the
    /// `Encoder` with the smallest `total_len()` is kept.
    #[hotpath::measure]
    pub fn encode_into(self, mut enc: Encoder) -> MltResult<Encoder> {
        let Self {
            name,
            extent,
            id,
            geometry,
            properties,
        } = self;

        if let Some(ids) = id {
            ids.write_to(&mut enc)?;
        }
        geometry.write_to(&mut enc)?;
        write_properties(&properties, &mut enc)?;
        enc.write_header(&name, extent)?;

        Ok(enc)
    }
}

/// Feature-count threshold above which the spatial trial is subject to the
/// bounding-box pruning heuristic.
const SORT_TRIAL_THRESHOLD: usize = 512;

impl TileLayer01 {
    /// Encode a [`TileLayer01`] to bytes, automatically optimizing all encoding choices.
    ///
    /// This is the primary encoding entry point. It:
    /// 1. Determines which sort strategies to try based on `cfg`
    /// 2. Tries each sort strategy, encoding and measuring the output size
    /// 3. Returns the smallest encoding as a complete layer record (including tag and length prefix)
    ///
    /// All encoding choices — sort order, per-stream integer encodings, string compression,
    /// vertex buffer layout — are selected automatically to minimize output size.
    #[hotpath::measure]
    pub fn encode(self, cfg: EncoderConfig) -> MltResult<Vec<u8>> {
        if self.features.is_empty() {
            return Ok(Vec::new());
        }

        let mut sort_by = vec![SortStrategy::Unsorted];
        let try_spatial_sort = cfg.try_spatial_morton_sort || cfg.try_spatial_hilbert_sort;
        if try_spatial_sort
            && (self.features.len() < SORT_TRIAL_THRESHOLD || spatial_sort_likely_to_help(&self))
        {
            if cfg.try_spatial_morton_sort {
                sort_by.push(SortStrategy::SpatialMorton);
            }
            if cfg.try_spatial_hilbert_sort {
                sort_by.push(SortStrategy::SpatialHilbert);
            }
        }
        if cfg.try_id_sort {
            sort_by.push(SortStrategy::Id);
        }

        let groups = if cfg.allow_shared_dict {
            group_string_properties(&self)
        } else {
            Vec::new()
        };

        // Pre-build sort-invariant property columns (all values identical
        // across features) so they aren't rebuilt on every trial.
        let invariant_cols = if sort_by.len() > 1 {
            detect_invariant_columns(&self.features, &self.property_names, &groups)
        } else {
            HashMap::new()
        };

        // Permutation-based trials: borrow `self` immutably, compute a sorted
        // index array for each strategy, and build the staged layer by reading
        // features in permuted order — no deep-clones of the tile.
        let trial = |sort: SortStrategy| -> MltResult<Encoder> {
            let perm = compute_permutation(&self.features, sort);
            let staged = self.stage_permuted(&perm, &groups, &invariant_cols);
            staged.encode_into(Encoder::new(cfg))
        };

        // When the `rayon` feature is enabled (default), trials run on the
        // rayon thread pool.  Each trial gets its own `Encoder`.
        #[cfg(feature = "rayon")]
        let results: Vec<MltResult<Encoder>> = {
            use rayon::prelude::*;
            sort_by.into_par_iter().map(&trial).collect()
        };
        #[cfg(not(feature = "rayon"))]
        let results: Vec<MltResult<Encoder>> = sort_by.into_iter().map(trial).collect();

        let best = pick_smallest(results)?.expect("at least one strategy");

        best.into_layer_bytes()
    }
}

/// Reduce a sequence of trial results to the `Encoder` with the smallest output.
fn pick_smallest(
    results: impl IntoIterator<Item = MltResult<Encoder>>,
) -> MltResult<Option<Encoder>> {
    let mut best: Option<Encoder> = None;
    for result in results {
        let enc = result?;
        best = Some(match best {
            Some(prev) if prev.total_len() <= enc.total_len() => prev,
            _ => enc,
        });
    }
    Ok(best)
}

/// Return `true` if column `col` has the same `PropValue` in every feature.
///
/// A column is sort-invariant when every feature holds the same value at
/// index `col` (including all-null). Reordering features never changes the
/// encoded output for such a column, so it can be built once and reused
/// across sort trials.
fn is_column_invariant(features: &[TileFeature], col: usize) -> bool {
    let first = features[0].properties.get(col);
    features[1..].iter().all(|f| f.properties.get(col) == first)
}

/// Detect property columns that are sort-invariant and build their
/// `StagedProperty` once.  Returns a map from column index to the
/// pre-built property.
fn detect_invariant_columns(
    features: &[TileFeature],
    property_names: &[String],
    groups: &[StringGroup],
) -> HashMap<usize, StagedProperty> {
    // Skip columns that are part of a shared-dict group — those are built
    // together from multiple columns and the invariance logic is more
    // complex; the payoff is low since shared-dict columns are rare.
    let grouped: std::collections::HashSet<usize> = groups
        .iter()
        .flat_map(|g| g.columns.iter().map(|(_, i)| *i))
        .collect();

    let identity: Vec<usize> = (0..features.len()).collect();

    let mut out = HashMap::new();
    for (col_idx, name) in property_names.iter().enumerate() {
        if grouped.contains(&col_idx) {
            continue;
        }
        if is_column_invariant(features, col_idx) {
            // Build once using the identity permutation (order doesn't matter
            // since all values are the same).
            out.insert(
                col_idx,
                super::tile::build_column_permuted(name.clone(), col_idx, features, &identity),
            );
        }
    }
    out
}
