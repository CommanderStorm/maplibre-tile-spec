// Hex-column helpers. All of this is JS-side because the tree carries only offset/len.

import type { DecodedBlob, DumpTree, Region } from "./annotated";

export interface ViewState {
  width: number;
  showBits: boolean;
  dataMode: "both" | "blob" | "decoded" | "hidden";
  maxBlob: number;
  layer: number | null;
}

/** The CLI's defaults, overridable from the URL so a view can be shared or scripted. */
export function defaultView(): ViewState {
  const q = new URLSearchParams(location.search);
  const num = (key: string, fallback: number) => (q.has(key) ? Number(q.get(key)) : fallback);
  return {
    width: num("width", 16),
    showBits: q.get("bits") !== "0",
    dataMode: (q.get("data") as ViewState["dataMode"]) ?? "both",
    maxBlob: num("maxBlob", 256),
    layer: q.has("layer") ? Number(q.get("layer")) : null,
  };
}

export interface HexRow {
  offset: number;
  hex: string[];
  ascii: string;
}

export function hexRows(bytes: Uint8Array, offset: number, len: number, width: number): HexRow[] {
  const rows: HexRow[] = [];
  for (let at = 0; at < len; at += width) {
    const chunk = bytes.subarray(offset + at, offset + Math.min(at + width, len));
    rows.push({
      offset: offset + at,
      hex: [...chunk].map(hex2),
      ascii: [...chunk].map(printable).join(""),
    });
  }
  return rows;
}

export function hex2(b: number): string {
  return b.toString(16).padStart(2, "0");
}

export function hex8(n: number): string {
  return n.toString(16).padStart(8, "0");
}

function printable(b: number): string {
  return b >= 0x20 && b <= 0x7e ? String.fromCharCode(b) : ".";
}

/** How many bytes of a blob the current maxBlob shows. */
export function shownLen(region: Region, view: ViewState): number {
  if (region.kind === "meta" || view.maxBlob === 0) return region.len;
  return Math.min(region.len, view.maxBlob);
}

/** Leaf region index per byte. Leaves partition the buffer, so containers are skipped. */
export function byteOwners(tree: DumpTree): Int32Array {
  const owners = new Int32Array(tree.bufLen).fill(-1);
  tree.regions.forEach((r, i) => {
    if (r.container) return;
    owners.fill(i, r.offset, r.offset + r.len);
  });
  return owners;
}

/** Enclosing container labels, innermost last, found by walking back up the depths. */
export function regionPath(regions: Region[], index: number): string[] {
  const path: string[] = [];
  let depth = regions[index].depth;
  for (let i = index - 1; i >= 0 && depth > 0; i--) {
    const r = regions[i];
    if (r.container && r.depth < depth) {
      path.unshift(r.label);
      depth = r.depth;
    }
  }
  return path;
}

/** Index of the layer container each region belongs to, for the layer filter and colouring. */
export function layerOf(regions: Region[], index: number): number {
  let layer = -1;
  for (let i = 0; i <= index; i++) {
    if (regions[i].depth === 0 && regions[i].container) layer += 1;
  }
  return layer;
}

export function layerLabels(tree: DumpTree): string[] {
  return tree.regions.filter((r) => r.depth === 0 && r.container).map((r) => r.label);
}

/** One line of decoded values, in the shape the union hands over. */
export function decodedSummary(blob: DecodedBlob): string {
  switch (blob.kind) {
    case "numbers":
    case "bigints":
      return `[${blob.values.join(", ")}]${more(blob.truncatedFrom, blob.values.length)}`;
    case "bools":
      return `${blob.values.map((b) => (b ? "1" : "0")).join("")}${more(blob.truncatedFrom, blob.values.length)}`;
    case "text":
      return `utf-8 ${blob.value}`;
    case "binary":
      return `${blob.len} binary bytes`;
    case "error":
      return `undecodable: ${blob.message}`;
  }
}

function more(truncatedFrom: number | null, shown: number): string {
  return truncatedFrom === null ? "" : ` … ${truncatedFrom - shown} more of ${truncatedFrom}`;
}

/** Value chips, so a variant can render values as a grid rather than a line. */
export function decodedValues(blob: DecodedBlob): string[] {
  switch (blob.kind) {
    case "numbers":
    case "bigints":
      return blob.values.map(String);
    case "bools":
      return blob.values.map((b) => (b ? "1" : "0"));
    case "text":
      return [blob.value];
    case "binary":
      return [`${blob.len} binary bytes`];
    case "error":
      return [`undecodable: ${blob.message}`];
  }
}
