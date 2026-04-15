#![allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    // Diplomat requires Box<T> for opaque types — these lints conflict with the FFI model.
    clippy::unnecessary_box_returns,
    clippy::boxed_local,
    clippy::must_use_candidate,
    clippy::struct_excessive_bools,
)]

pub mod builder;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

fn encode_staged_to_buf(
    staged: mlt_core::encoder::StagedLayer01,
    cfg: mlt_core::encoder::EncoderConfig,
    buf: &mut Vec<u8>,
) -> Result<(), String> {
    static PROFILE: std::sync::LazyLock<bool> =
        std::sync::LazyLock::new(|| std::env::var("MLT_PROFILE").is_ok());

    let (profile_info, t0) = if *PROFILE {
        (
            Some((
                staged.name.clone(),
                staged.geometry.vector_types().len(),
                staged.properties.len(),
            )),
            Some(std::time::Instant::now()),
        )
    } else {
        (None, None)
    };
    let bytes = staged
        .encode_try_sort(cfg)
        .map_err(|e| format!("Encode: {e}"))?;
    buf.extend(&bytes);

    if let (Some((name, n_feat, n_props)), Some(t0)) = (profile_info, t0) {
        eprintln!(
            "[mlt-ffi] layer={name:20} feat={n_feat:5} props={n_props:2}  encode={:>8.1}ms",
            t0.elapsed().as_secs_f64() * 1000.0,
        );
    }
    Ok(())
}

fn str_from_diplomat(bytes: &[u8]) -> Result<String, String> {
    core::str::from_utf8(bytes)
        .map(String::from)
        .map_err(|e| format!("invalid UTF-8: {e}"))
}

/// Convert a diplomat `&[u8]` name to a `String`, replacing invalid UTF-8 with U+FFFD.
/// Used for column names where returning a Result would change the FFI signature.
fn lossy_string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn opt_slice<T>(s: &[T]) -> Option<&[T]> {
    if s.is_empty() { None } else { Some(s) }
}

fn to_core_config(config: &ffi::MltEncoderConfig) -> mlt_core::encoder::EncoderConfig {
    mlt_core::encoder::EncoderConfig {
        tessellate: config.tessellate,
        try_spatial_morton_sort: config.try_spatial_morton_sort,
        try_spatial_hilbert_sort: config.try_spatial_hilbert_sort,
        try_id_sort: config.try_id_sort,
        allow_fsst: config.allow_fsst,
        allow_fpf: config.allow_fpf,
        allow_shared_dict: config.allow_shared_dict,
    }
}

fn catch_panic<T>(
    label: &str,
    f: impl FnOnce() -> Result<T, String> + std::panic::UnwindSafe,
) -> Result<T, Box<ffi::MltError>> {
    match std::panic::catch_unwind(f) {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(Box::new(ffi::MltError(e))),
        Err(p) => Err(Box::new(ffi::MltError(format!(
            "panic in {label}: {}",
            panic_message(&*p)
        )))),
    }
}

fn catch_encode(
    f: impl FnOnce() -> Result<Vec<u8>, String> + std::panic::UnwindSafe,
) -> Result<Box<ffi::MltEncodedBuffer>, Box<ffi::MltError>> {
    catch_panic("encode", f).map(|buf| Box::new(ffi::MltEncodedBuffer(buf)))
}

// ---------------------------------------------------------------------------
// Diplomat bridge — the entire FFI surface
// ---------------------------------------------------------------------------

#[diplomat::bridge]
pub mod ffi {
    use std::fmt::Write as _;

    use diplomat_runtime::{DiplomatStr, DiplomatWrite};

    // -----------------------------------------------------------------------
    // Error
    // -----------------------------------------------------------------------

    /// Error returned by encoding operations.
    #[diplomat::opaque]
    pub struct MltError(pub(crate) String);

    impl MltError {
        /// Write the error message to the provided buffer.
        pub fn message(&self, write: &mut DiplomatWrite) {
            let _ = write.write_str(&self.0);
        }
    }

    // -----------------------------------------------------------------------
    // Encoder configuration
    // -----------------------------------------------------------------------

    /// Encoder configuration. Create with `new_default()`, then toggle fields.
    #[diplomat::opaque_mut]
    pub struct MltEncoderConfig {
        pub(super) tessellate: bool,
        pub(super) try_spatial_morton_sort: bool,
        pub(super) try_spatial_hilbert_sort: bool,
        pub(super) try_id_sort: bool,
        pub(super) allow_fsst: bool,
        pub(super) allow_fpf: bool,
        pub(super) allow_shared_dict: bool,
    }

    impl MltEncoderConfig {
        /// Default config: all optimizations enabled, tessellate off.
        pub fn new_default() -> Box<Self> {
            let d = mlt_core::encoder::EncoderConfig::default();
            Box::new(Self {
                tessellate: d.tessellate,
                try_spatial_morton_sort: d.try_spatial_morton_sort,
                try_spatial_hilbert_sort: d.try_spatial_hilbert_sort,
                try_id_sort: d.try_id_sort,
                allow_fsst: d.allow_fsst,
                allow_fpf: d.allow_fpf,
                allow_shared_dict: d.allow_shared_dict,
            })
        }

        pub fn set_tessellate(&mut self, value: bool) {
            self.tessellate = value;
        }

        pub fn set_try_morton_sort(&mut self, value: bool) {
            self.try_spatial_morton_sort = value;
        }

        pub fn set_try_hilbert_sort(&mut self, value: bool) {
            self.try_spatial_hilbert_sort = value;
        }

        pub fn set_try_id_sort(&mut self, value: bool) {
            self.try_id_sort = value;
        }

        pub fn set_allow_fsst(&mut self, value: bool) {
            self.allow_fsst = value;
        }

        pub fn set_allow_fast_pfor(&mut self, value: bool) {
            self.allow_fpf = value;
        }

        pub fn set_allow_shared_dict(&mut self, value: bool) {
            self.allow_shared_dict = value;
        }
    }

    // -----------------------------------------------------------------------
    // Encoded buffer
    // -----------------------------------------------------------------------

    /// Owned encoded byte buffer returned by encode operations.
    #[diplomat::opaque]
    pub struct MltEncodedBuffer(pub(crate) Vec<u8>);

    impl MltEncodedBuffer {
        /// Get the encoded bytes as a borrowed slice.
        pub fn as_bytes(&self) -> &[u8] {
            &self.0
        }

        /// Get the length in bytes.
        pub fn len(&self) -> usize {
            self.0.len()
        }

        /// Whether the buffer is empty.
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }
    }

    // -----------------------------------------------------------------------
    // Column type enum
    // -----------------------------------------------------------------------

    /// Data type for scalar property columns.
    pub enum MltColumnType {
        Bool = 0,
        I8 = 1,
        U8 = 2,
        I32 = 3,
        U32 = 4,
        I64 = 5,
        U64 = 6,
        F32 = 7,
        F64 = 8,
    }

    // -----------------------------------------------------------------------
    // Layer encoder
    // -----------------------------------------------------------------------

    /// Columnar layer encoder. Set columns, then call `encode` or pass to a
    /// tile encoder via `add_layer`.
    ///
    /// After `encode` or being passed to `add_layer`, the encoder is consumed
    /// and further calls will return errors.
    #[diplomat::opaque_mut]
    pub struct MltLayerEncoder(Option<super::builder::MltLayerEncoder>);

    impl MltLayerEncoder {
        /// Create a new layer encoder with the given name (UTF-8) and extent.
        pub fn new(name: &DiplomatStr, extent: u32) -> Result<Box<Self>, Box<MltError>> {
            let name = super::str_from_diplomat(name).map_err(|e| Box::new(MltError(e)))?;
            Ok(Box::new(Self(Some(super::builder::MltLayerEncoder::new(
                name, extent,
            )))))
        }

        /// Set the ID column. Pass an empty `present` slice for "all present".
        pub fn set_ids(&mut self, ids: &[u64], present: &[bool]) -> Result<(), Box<MltError>> {
            let enc = self
                .0
                .as_mut()
                .ok_or_else(|| Box::new(MltError("encoder already consumed".into())))?;
            enc.set_ids(ids, super::opt_slice(present))
                .map_err(|e| Box::new(MltError(e)))
        }

        /// Set geometry from flat coordinate + meta arrays (row-oriented input).
        pub fn set_geometries(
            &mut self,
            coords: &[i32],
            meta: &[u32],
        ) -> Result<(), Box<MltError>> {
            let enc = self
                .0
                .as_mut()
                .ok_or_else(|| Box::new(MltError("encoder already consumed".into())))?;
            enc.set_geometries(coords, meta)
                .map_err(|e| Box::new(MltError(e)))
        }

        /// Add a scalar property column. `data` contains the raw values in
        /// native endianness, `count` is the number of elements.
        pub fn add_column(
            &mut self,
            name: &DiplomatStr,
            column_type: MltColumnType,
            data: &[u8],
            count: u32,
            present: &[bool],
        ) -> Result<(), Box<MltError>> {
            let enc = self
                .0
                .as_mut()
                .ok_or_else(|| Box::new(MltError("encoder already consumed".into())))?;
            enc.add_column(
                super::lossy_string(name),
                column_type,
                data,
                count as usize,
                super::opt_slice(present),
            )
            .map_err(|e| Box::new(MltError(e)))
        }

        /// Add a string property column.
        /// `data` = concatenated UTF-8 bytes, `offsets` = count+1 delimiters,
        /// `present` = presence bitmap (empty = all present).
        pub fn add_string_column(
            &mut self,
            name: &DiplomatStr,
            data: &DiplomatStr,
            offsets: &[u32],
            present: &[bool],
        ) -> Result<(), Box<MltError>> {
            let enc = self
                .0
                .as_mut()
                .ok_or_else(|| Box::new(MltError("encoder already consumed".into())))?;
            enc.add_string_column_raw(
                super::lossy_string(name),
                data,
                offsets,
                super::opt_slice(present),
            );
            Ok(())
        }

        /// Encode a single layer. Consumes the encoder (further calls will error).
        pub fn encode(
            &mut self,
            config: &MltEncoderConfig,
        ) -> Result<Box<MltEncodedBuffer>, Box<MltError>> {
            let inner = self
                .0
                .take()
                .ok_or_else(|| Box::new(MltError("encoder already consumed".into())))?;
            let cfg = super::to_core_config(config);
            super::catch_encode(std::panic::AssertUnwindSafe(|| {
                let staged = inner.into_staged()?;
                let mut buf = Vec::with_capacity(4096);
                super::encode_staged_to_buf(staged, cfg, &mut buf)?;
                Ok(buf)
            }))
        }
    }

    // -----------------------------------------------------------------------
    // Tile encoder
    // -----------------------------------------------------------------------

    /// Tile encoder for combining multiple layers.
    ///
    /// After `encode` is called, the encoder is consumed and further calls
    /// will return errors.
    #[diplomat::opaque_mut]
    pub struct MltTileEncoder(Option<super::builder::MltTileEncoder>);

    impl MltTileEncoder {
        /// Create a new tile encoder.
        pub fn new() -> Box<Self> {
            Box::new(Self(Some(super::builder::MltTileEncoder::new())))
        }

        /// Add a completed layer, consuming the layer encoder.
        pub fn add_layer(&mut self, layer: &mut MltLayerEncoder) -> Result<(), Box<MltError>> {
            let tile = self
                .0
                .as_mut()
                .ok_or_else(|| Box::new(MltError("tile encoder already consumed".into())))?;
            let inner = layer
                .0
                .take()
                .ok_or_else(|| Box::new(MltError("layer encoder already consumed".into())))?;
            let staged = super::catch_panic(
                "add_layer",
                std::panic::AssertUnwindSafe(|| inner.into_staged()),
            )?;
            tile.add_layer(staged);
            Ok(())
        }

        /// Encode all layers. Consumes the encoder (further calls will error).
        pub fn encode(
            &mut self,
            config: &MltEncoderConfig,
        ) -> Result<Box<MltEncodedBuffer>, Box<MltError>> {
            let inner = self
                .0
                .take()
                .ok_or_else(|| Box::new(MltError("tile encoder already consumed".into())))?;
            let cfg = super::to_core_config(config);
            super::catch_encode(std::panic::AssertUnwindSafe(|| {
                let layers = inner.into_layers();
                let mut buf = Vec::with_capacity(4096 * layers.len());
                for (i, staged) in layers.into_iter().enumerate() {
                    super::encode_staged_to_buf(staged, cfg, &mut buf)
                        .map_err(|e| format!("layer {i}: {e}"))?;
                }
                Ok(buf)
            }))
        }
    }
}

impl std::fmt::Debug for ffi::MltError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MltError").field(&self.0).finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::ffi::*;

    #[test]
    fn test_encode_single_layer() {
        let mut encoder = MltLayerEncoder::new(b"test", 4096).unwrap();
        encoder.set_ids(&[1, 2, 3], &[]).unwrap();
        encoder
            .set_geometries(&[10, 20, 30, 40, 50, 60], &[0, 0, 0])
            .unwrap();
        let values: [i32; 3] = [100, 200, 300];
        let data = bytemuck::cast_slice::<i32, u8>(&values);
        encoder
            .add_column(b"pop", MltColumnType::I32, data, 3, &[])
            .unwrap();

        let config = MltEncoderConfig::new_default();
        let buf = encoder.encode(&config).unwrap();
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_encode_after_consume_errors() {
        let mut encoder = MltLayerEncoder::new(b"test", 4096).unwrap();
        encoder.set_geometries(&[10, 20], &[0]).unwrap();

        let config = MltEncoderConfig::new_default();
        let _buf = encoder.encode(&config).unwrap();

        // Second encode should error
        let result = encoder.encode(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_encode_tile_multi_layer() {
        let mut l1 = MltLayerEncoder::new(b"a", 4096).unwrap();
        l1.set_geometries(&[1, 2], &[0]).unwrap();

        let mut l2 = MltLayerEncoder::new(b"b", 4096).unwrap();
        l2.set_geometries(&[3, 4], &[0]).unwrap();

        let mut tile = MltTileEncoder::new();
        tile.add_layer(&mut l1).unwrap();
        tile.add_layer(&mut l2).unwrap();

        let config = MltEncoderConfig::new_default();
        let buf = tile.encode(&config).unwrap();
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_string_column() {
        let mut encoder = MltLayerEncoder::new(b"test", 4096).unwrap();
        encoder
            .set_geometries(&[10, 20, 30, 40, 50, 60], &[0, 0, 0])
            .unwrap();

        // "hello" + "world" concatenated, with offsets and presence bitmap
        let data = b"helloworld";
        let offsets: &[u32] = &[0, 5, 5, 10]; // 3 strings: [0..5], [5..5] (null), [5..10]
        let present: &[bool] = &[true, false, true];
        encoder
            .add_string_column(b"name", data, offsets, present)
            .unwrap();

        let config = MltEncoderConfig::new_default();
        let buf = encoder.encode(&config).unwrap();
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_presence_bitmap() {
        let mut encoder = MltLayerEncoder::new(b"test", 4096).unwrap();
        encoder
            .set_ids(&[10, 20, 30], &[true, false, true])
            .unwrap();
        encoder
            .set_geometries(&[10, 20, 30, 40, 50, 60], &[0, 0, 0])
            .unwrap();
        let pop_values: [i32; 3] = [100, 200, 300];
        encoder
            .add_column(
                b"pop",
                MltColumnType::I32,
                bytemuck::cast_slice(&pop_values),
                3,
                &[true, true, false],
            )
            .unwrap();

        let config = MltEncoderConfig::new_default();
        let buf = encoder.encode(&config).unwrap();
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_missing_geometry_errors() {
        let mut encoder = MltLayerEncoder::new(b"test", 4096).unwrap();
        let config = MltEncoderConfig::new_default();
        let result = encoder.encode(&config);
        assert!(result.is_err());
    }
}
