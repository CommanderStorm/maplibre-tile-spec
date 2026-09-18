//! THROWAWAY: prints one tile's annotated dump as the JSON the `annotate` binding will return.
//!
//! Stands in for the not-yet-written `mlt-wasm` binding so the Inspector prototype
//! (wayfinder ticket #15) can run against real trees.
//! `cargo run -p mlt-core --example dump_json --features unstable-v2 -- tile.mlt > tile.json`
//!
//! `decoded` is the one place this diverges from the contract: the real `decodeBlob` is
//! lazy, per region, and returns structured values, while this emits the renderer's
//! decoded text for every blob up front.

use std::{env, fs};

use mlt_core::dump::{DataMode, DumpTree, RenderOpts, annotate_tile, render};
use serde_json::{Map, Value, json};

fn main() {
    let path = env::args().nth(1).expect("usage: dump_json <tile.mlt>");
    let buf = fs::read(&path).expect("tile is readable");
    let out = match annotate_tile(&buf) {
        Ok(tree) => json!({
            "tree": &tree,
            "error": null,
            "decoded": decoded_per_region(&tree, &buf),
        }),
        Err(e) => json!({
            "tree": {"bufLen": buf.len(), "regions": []},
            "error": e.to_string(),
            "decoded": {},
        }),
    };
    println!("{out}");
}

/// The renderer's decoded text, keyed by region index, for every blob that has stream metadata.
fn decoded_per_region(tree: &DumpTree, buf: &[u8]) -> Map<String, Value> {
    let opts = RenderOpts {
        data_mode: DataMode::Decoded,
        show_bits: false,
        ..RenderOpts::default()
    };
    let mut out = Map::new();
    for (idx, region) in tree.regions.iter().enumerate() {
        if region.blob.is_none() {
            continue;
        }
        let one = DumpTree {
            buf_len: tree.buf_len,
            regions: vec![region.clone()],
        };
        let mut sink = Vec::new();
        if render(&one, buf, &opts, &mut sink).is_err() {
            continue;
        }
        let text = String::from_utf8_lossy(&sink);
        if let Some((_, values)) = text.split_once("decoded: ") {
            out.insert(idx.to_string(), json!(values.trim_end()));
        }
    }
    out
}
