#![no_main]
#![allow(clippy::cast_possible_truncation)]

use libfuzzer_sys::fuzz_target;
use mlt_core::encoder::{EncoderConfig, StagedLayer01, StagedProperty};

fuzz_target!(|layer: StagedLayer01| {
    fuzz_encode_compare(layer);
});

/// Encode via `mlt-core` directly and via the diplomat FFI bridge,
/// then assert bit-for-bit identical output.
fn fuzz_encode_compare(layer: StagedLayer01) {
    // Skip layers whose name contains interior NUL bytes.
    if layer.name.contains('\0') {
        return;
    }
    // Skip if any property name contains interior NUL bytes.
    if layer.properties.iter().any(|p| p.name().contains('\0')) {
        return;
    }
    // Skip Str/SharedDict properties — their internal fields are pub(crate).
    for prop in &layer.properties {
        if matches!(prop, StagedProperty::Str(_) | StagedProperty::SharedDict(_)) {
            return;
        }
    }

    // Deterministic config: no sorting, no advanced codecs.
    let cfg = EncoderConfig {
        tessellate: false,
        try_spatial_morton_sort: false,
        try_spatial_hilbert_sort: false,
        try_id_sort: false,
        allow_fsst: false,
        allow_fpf: false,
        allow_shared_dict: false,
    };

    // ── FFI path first (borrows layer) ──
    let Some(ffi_bytes) = encode_via_ffi(&layer) else {
        return;
    };

    // ── Rust path (consumes layer) ──
    let Ok(rust_bytes) = layer.encode_try_sort(cfg) else {
        return;
    };

    assert_eq!(
        rust_bytes, ffi_bytes,
        "FFI encode diverged from Rust encode"
    );
}

// ---------------------------------------------------------------------------
// FFI encode path — uses the diplomat bridge types directly from Rust
// ---------------------------------------------------------------------------

fn encode_via_ffi(layer: &StagedLayer01) -> Option<Vec<u8>> {
    use mlt_ffi::ffi::{MltColumnType, MltEncoderConfig, MltLayerEncoder};

    let mut enc = MltLayerEncoder::new(layer.name.as_bytes(), layer.extent).ok()?;

    // ── IDs ──
    if let Some(ref id_vals) = layer.id {
        let (ids, present) = decompose_options(&id_vals.0);
        enc.set_ids(&ids, &present).ok()?;
    }

    // ── Geometry ──
    {
        let types: Vec<u8> = layer
            .geometry
            .vector_types()
            .iter()
            .map(|t| *t as u8)
            .collect();

        let verts = layer.geometry.vertices().unwrap_or(&[]);
        let geo_off = layer.geometry.geometry_offsets().unwrap_or(&[]);
        let part_off = layer.geometry.part_offsets().unwrap_or(&[]);
        let ring_off = layer.geometry.ring_offsets().unwrap_or(&[]);

        enc.set_geometry(&types, verts, geo_off, part_off, ring_off)
            .ok()?;
    }

    // ── Properties ──
    for prop in &layer.properties {
        let name = prop.name().as_bytes();
        match prop {
            StagedProperty::Bool(v) => {
                let (vals, pres) = decompose_options(&v.values);
                let data: Vec<u8> = vals.iter().map(|&b| b as u8).collect();
                enc.add_column(name, MltColumnType::Bool, &data, vals.len() as u32, &pres)
                    .ok()?;
            }
            StagedProperty::I8(v) => {
                let (vals, pres) = decompose_options(&v.values);
                enc.add_column(
                    name,
                    MltColumnType::I8,
                    bytemuck::cast_slice(&vals),
                    vals.len() as u32,
                    &pres,
                )
                .ok()?;
            }
            StagedProperty::U8(v) => {
                let (vals, pres) = decompose_options(&v.values);
                enc.add_column(name, MltColumnType::U8, &vals, vals.len() as u32, &pres)
                    .ok()?;
            }
            StagedProperty::I32(v) => {
                let (vals, pres) = decompose_options(&v.values);
                enc.add_column(
                    name,
                    MltColumnType::I32,
                    bytemuck::cast_slice(&vals),
                    vals.len() as u32,
                    &pres,
                )
                .ok()?;
            }
            StagedProperty::U32(v) => {
                let (vals, pres) = decompose_options(&v.values);
                enc.add_column(
                    name,
                    MltColumnType::U32,
                    bytemuck::cast_slice(&vals),
                    vals.len() as u32,
                    &pres,
                )
                .ok()?;
            }
            StagedProperty::I64(v) => {
                let (vals, pres) = decompose_options(&v.values);
                enc.add_column(
                    name,
                    MltColumnType::I64,
                    bytemuck::cast_slice(&vals),
                    vals.len() as u32,
                    &pres,
                )
                .ok()?;
            }
            StagedProperty::U64(v) => {
                let (vals, pres) = decompose_options(&v.values);
                enc.add_column(
                    name,
                    MltColumnType::U64,
                    bytemuck::cast_slice(&vals),
                    vals.len() as u32,
                    &pres,
                )
                .ok()?;
            }
            StagedProperty::F32(v) => {
                let (vals, pres) = decompose_options(&v.values);
                enc.add_column(
                    name,
                    MltColumnType::F32,
                    bytemuck::cast_slice(&vals),
                    vals.len() as u32,
                    &pres,
                )
                .ok()?;
            }
            StagedProperty::F64(v) => {
                let (vals, pres) = decompose_options(&v.values);
                enc.add_column(
                    name,
                    MltColumnType::F64,
                    bytemuck::cast_slice(&vals),
                    vals.len() as u32,
                    &pres,
                )
                .ok()?;
            }
            // Guarded by early return above; unreachable.
            StagedProperty::Str(_) | StagedProperty::SharedDict(_) => unreachable!(),
        }
    }

    // ── Encode ──
    // Build config matching the core one (all optimizations off).
    let mut config = MltEncoderConfig::new_default();
    config.set_tessellate(false);
    config.set_try_morton_sort(false);
    config.set_try_hilbert_sort(false);
    config.set_try_id_sort(false);
    config.set_allow_fsst(false);
    config.set_allow_fast_pfor(false);
    config.set_allow_shared_dict(false);

    let buf = enc.encode(&config).ok()?;
    Some(buf.as_bytes().to_vec())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Split `&[Option<T>]` into (values, presence) arrays.
/// Missing values are filled with `T::default()`.
fn decompose_options<T: Copy + Default>(opts: &[Option<T>]) -> (Vec<T>, Vec<bool>) {
    let vals: Vec<T> = opts.iter().map(|o| o.unwrap_or_default()).collect();
    let pres: Vec<bool> = opts.iter().map(Option::is_some).collect();
    (vals, pres)
}
