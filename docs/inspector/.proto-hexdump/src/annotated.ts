// The #14 wire contract, verbatim, plus a stand-in handle over dump_json output.
// The real handle comes from @maplibre/mlt-wasm; nothing here survives into the app.

export interface DumpTree {
  bufLen: number;
  regions: Region[];
}

export interface Region {
  offset: number;
  len: number;
  depth: number;
  label: string;
  value: string | null;
  bits: BitField[];
  kind: "meta" | "dataBlob";
  container: boolean;
  blob: BlobInfo | null;
}

export interface BitField {
  hi: number;
  lo: number;
  raw: number;
  meaning: string;
}

export interface BlobInfo {
  streamType: string;
  logical: string;
  physical: string;
  numValues: number;
  hint: DecodeHint;
}

export type DecodeHint =
  | { kind: "presence" }
  | { kind: "bool" }
  | { kind: "i32" }
  | { kind: "u32" }
  | { kind: "i64" }
  | { kind: "u64" }
  | { kind: "f32" }
  | { kind: "f64" }
  | { kind: "bytes" }
  | { kind: "packedBits" }
  | { kind: "alp"; e: number; f: number; base: bigint };

export type DecodedBlob =
  | { kind: "numbers"; values: number[]; truncatedFrom: number | null }
  | { kind: "bigints"; values: bigint[]; truncatedFrom: number | null }
  | { kind: "bools"; values: boolean[]; truncatedFrom: number | null }
  | { kind: "text"; value: string }
  | { kind: "binary"; len: number }
  | { kind: "error"; message: string };

export interface AnnotatedTile {
  tree(layer?: number): DumpTree;
  readonly error: string | null;
  decodeBlob(regionIndex: number, maxValues: number): DecodedBlob;
  free(): void;
}

/** What `cargo run --example dump_json` writes. */
interface DumpJson {
  tree: DumpTree;
  error: string | null;
  decoded: Record<string, string>;
}

/** Stands in for `annotateTile`, memoizing per layer the way the handle does. */
class StubTile implements AnnotatedTile {
  readonly error: string | null;
  private readonly whole: DumpTree;
  private readonly decoded: Record<string, string>;
  private readonly perLayer = new Map<number, DumpTree>();

  constructor(json: DumpJson) {
    this.whole = json.tree;
    this.error = json.error;
    this.decoded = json.decoded;
  }

  tree(layer?: number): DumpTree {
    if (layer === undefined) return this.whole;
    const cached = this.perLayer.get(layer);
    if (cached) return cached;
    const filtered = filterLayer(this.whole, layer);
    this.perLayer.set(layer, filtered);
    return filtered;
  }

  decodeBlob(regionIndex: number, maxValues: number): DecodedBlob {
    const text = this.decoded[String(regionIndex)];
    if (text === undefined) return { kind: "error", message: "no stream metadata" };
    const region = this.whole.regions[regionIndex];
    return parseDecoded(text, region?.blob?.hint.kind ?? "bytes", maxValues);
  }

  free(): void {
    this.perLayer.clear();
  }
}

export async function loadStubTile(jsonUrl: string): Promise<AnnotatedTile> {
  const res = await fetch(jsonUrl);
  if (!res.ok) throw new Error(`${jsonUrl}: ${res.status}`);
  return new StubTile((await res.json()) as DumpJson);
}

/** Mirrors `filter_layer` (rust/mlt/src/hexdump.rs:104), which #14 moves into mlt-core. */
export function filterLayer(tree: DumpTree, idx: number): DumpTree {
  const layers = tree.regions.filter((r) => r.depth === 0 && r.container);
  const layer = layers[idx];
  if (!layer) return { bufLen: tree.bufLen, regions: [] };
  const start = layer.offset;
  const end = layer.offset + layer.len;
  return {
    bufLen: tree.bufLen,
    regions: tree.regions.filter((r) => r.offset >= start && r.offset + r.len <= end),
  };
}

// The renderer's decoded text put back into the contract's shape. The real binding
// returns these arrays directly, so only this function is prototype scaffolding.
function parseDecoded(text: string, hint: DecodeHint["kind"], maxValues: number): DecodedBlob {
  const undecodable = /^<undecodable: (.*)>$/.exec(text);
  if (undecodable) return { kind: "error", message: undecodable[1] };

  const binary = /^<(\d+) binary bytes>$/.exec(text);
  if (binary) return { kind: "binary", len: Number(binary[1]) };

  if (text.startsWith("utf-8 ")) return { kind: "text", value: text.slice(6) };

  const bits = /^(\d+) present-bits: ([01]*)(…?)$/.exec(text);
  if (bits) {
    const all = [...bits[2]].map((c) => c === "1");
    const values = all.slice(0, maxValues);
    const total = Number(bits[1]);
    return { kind: "bools", values, truncatedFrom: values.length < total ? total : null };
  }

  const list = /^\[(.*?)(?:, … ] \((\d+) total\)|])$/.exec(text);
  if (!list) return { kind: "error", message: `unparsed: ${text}` };
  const parts = list[1].length ? list[1].split(", ") : [];
  const total = list[2] ? Number(list[2]) : parts.length;
  const shown = parts.slice(0, maxValues);
  const truncatedFrom = shown.length < total ? total : null;

  if (hint === "bool" || hint === "packedBits" || hint === "presence") {
    return { kind: "bools", values: shown.map((v) => v === "true"), truncatedFrom };
  }
  if (hint === "i64" || hint === "u64" || hint === "alp") {
    return { kind: "bigints", values: shown.map((v) => BigInt(v)), truncatedFrom };
  }
  return { kind: "numbers", values: shown.map(Number), truncatedFrom };
}
