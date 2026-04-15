use crate::MltResult;
use crate::decoder::TileLayer01;
use crate::encoder::model::{StagedLayer, StagedLayer01};
use crate::encoder::property::encode::write_properties;
use crate::encoder::{
    Encoder, EncoderConfig, SortStrategy, group_string_properties, reorder_staged,
    spatial_sort_likely_to_help, spatial_sort_likely_to_help_staged,
};

/// Feature-count threshold above which the spatial trial is subject to the
/// bounding-box pruning heuristic.
const SORT_TRIAL_THRESHOLD: usize = 512;

/// Build the list of sort strategies to try based on `cfg`.
///
/// `feature_count` is the number of features in the layer.
/// `spatial_likely` should return `true` when the bounding-box heuristic
/// indicates spatial sorting is likely to help.
fn collect_sort_strategies(
    cfg: EncoderConfig,
    feature_count: usize,
    spatial_likely: impl FnOnce() -> bool,
) -> Vec<SortStrategy> {
    let mut sort_by = vec![SortStrategy::Unsorted];
    let try_spatial = cfg.try_spatial_morton_sort || cfg.try_spatial_hilbert_sort;
    if try_spatial && (feature_count < SORT_TRIAL_THRESHOLD || spatial_likely()) {
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
    sort_by
}

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
    #[allow(unused_mut, reason = "only unused if tesseate feature is inactive")]
    pub fn encode_into(mut self, mut enc: Encoder) -> MltResult<Encoder> {
        #[cfg(feature = "tessellate")]
        if enc.cfg.tessellate {
            self.geometry.compute_tessellation()?;
        }

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

    /// Encode the layer to bytes, trying multiple sort strategies and picking
    /// the smallest result.
    ///
    /// This is the columnar-path equivalent of [`TileLayer01::encode`]: it
    /// operates on an already-columnar [`StagedLayer01`] (e.g. from
    /// [`MltLayerEncoder::into_staged`](crate::encoder::model::StagedLayer01))
    /// and uses [`reorder_staged`] for sorting instead of row-level permutation.
    pub fn encode_try_sort(mut self, cfg: EncoderConfig) -> MltResult<Vec<u8>> {
        // Compute tessellation before sort trials so that `reorder_staged`
        // sees `triangles.is_some()` and preserves it through the rebuild.
        #[cfg(feature = "tessellate")]
        if cfg.tessellate {
            self.geometry.compute_tessellation()?;
        }

        let sort_by = collect_sort_strategies(cfg, self.geometry.vector_types().len(), || {
            spatial_sort_likely_to_help_staged(&self)
        });

        let (first, rest) = sort_by.split_first().expect("at least one strategy");
        if rest.is_empty() {
            reorder_staged(&mut self, *first)?;
            self.encode_into(Encoder::new(cfg))?
        } else {
            let mut best: Encoder = {
                let mut staged = self.clone();
                reorder_staged(&mut staged, *first)?;
                staged.encode_into(Encoder::new(cfg))?
            };
            for &sort in rest {
                let mut staged = self.clone();
                reorder_staged(&mut staged, sort)?;
                let enc = staged.encode_into(Encoder::new(cfg))?;
                if enc.total_len() < best.total_len() {
                    best = enc;
                }
            }
            best
        }
        .into_layer_bytes()
    }
}

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

        let sort_by = collect_sort_strategies(cfg, self.features.len(), || {
            spatial_sort_likely_to_help(&self)
        });

        let groups = if cfg.allow_shared_dict {
            group_string_properties(&self)
        } else {
            Vec::new()
        };

        let (last, init) = sort_by.split_last().expect("at least one strategy");
        if init.is_empty() {
            StagedLayer01::from_tile(self, *last, &groups).encode_into(Encoder::new(cfg))?
        } else {
            let mut enc: Encoder = {
                let first = init[0];
                StagedLayer01::from_tile(self.clone(), first, &groups)
                    .encode_into(Encoder::new(cfg))?
            };
            let mut best = enc.preserve_results();
            // Clone for all-but-last strategies
            for &sort in &init[1..] {
                let layer = StagedLayer01::from_tile(self.clone(), sort, &groups);
                enc = layer.encode_into(enc)?;
                if enc.total_len() < best.total_len() {
                    best = enc.preserve_results();
                }
            }
            // Last strategy: consume self, no clone
            let layer = StagedLayer01::from_tile(self, *last, &groups);
            enc = layer.encode_into(enc)?;
            if enc.total_len() < best.total_len() {
                best = enc.preserve_results();
            }
            best
        }
        .into_layer_bytes()
    }
}
