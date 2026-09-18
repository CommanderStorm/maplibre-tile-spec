<script setup lang="ts">
// A — Dump: the CLI's shape, one monospace stream of `offset hex ascii | annotation`.
// Controls and Source sit in a single dense toolbar, like the flags they mirror.
import { computed, ref, watch } from "vue";

import type { AnnotatedTile, DumpTree } from "../annotated";
import { decodedSummary, hex8, hexRows, shownLen, type ViewState } from "../hex";
import RenderControls from "../RenderControls.vue";
import SourcePicker, { type FixtureEntry } from "../SourcePicker.vue";

const props = defineProps<{
  tile: AnnotatedTile;
  tree: DumpTree;
  bytes: Uint8Array;
  index: Record<string, FixtureEntry[]> | null;
  current: string | null;
  layers: string[];
  status: string;
}>();
const view = defineModel<ViewState>("view", { required: true });
const emit = defineEmits<{ pickFixture: [path: string]; pickUpload: [file: File] }>();

// ?chunk=N renders N regions up front, so the unwindowed cost can be measured.
const CHUNK = Number(new URLSearchParams(location.search).get("chunk") ?? 500);
const shown = ref(CHUNK);
watch(
  () => [props.tree, view.value.width, view.value.maxBlob],
  () => {
    shown.value = CHUNK;
  },
);

const selected = ref<number | null>(null);

const leftWidth = computed(() => 8 + 2 + view.value.width * 3 - 1 + 2 + view.value.width);

interface Line {
  region: number;
  left: string;
  annot: string;
  tone: "container" | "meta" | "blob" | "bits" | "decoded" | "note";
}

/** The whole stream, flattened the way the renderer writes it. */
const lines = computed<Line[]>(() => {
  const out: Line[] = [];
  const regions = props.tree.regions.slice(0, shown.value);
  regions.forEach((region, i) => {
    const indent = "  ".repeat(region.depth);
    if (region.container) {
      out.push({
        region: i,
        left: hex8(region.offset),
        annot: `${indent}${region.label} (${region.len} B)`,
        tone: "container",
      });
      return;
    }
    const isBlob = region.kind === "dataBlob";
    const head = isBlob ? blobSummary(i) : metaSummary(i);
    if (isBlob && view.value.dataMode === "hidden") {
      out.push({ region: i, left: hex8(region.offset), annot: `${indent}${head}`, tone: "blob" });
      return;
    }
    const wantHex = !isBlob || view.value.dataMode === "both" || view.value.dataMode === "blob";
    if (wantHex) {
      const len = shownLen(region, view.value);
      const rows = hexRows(props.bytes, region.offset, len, view.value.width);
      if (rows.length === 0) {
        out.push({ region: i, left: hex8(region.offset), annot: `${indent}${head}`, tone: isBlob ? "blob" : "meta" });
      }
      rows.forEach((row, r) => {
        out.push({
          region: i,
          left: `${hex8(row.offset)}  ${row.hex.join(" ").padEnd(view.value.width * 3 - 1)}  ${row.ascii}`,
          annot: r === 0 ? `${indent}${head}` : "",
          tone: isBlob ? "blob" : "meta",
        });
      });
      if (len < region.len) {
        out.push({
          region: i,
          left: "",
          annot: `${indent}  … ${region.len - len} more bytes omitted (max blob to change)`,
          tone: "note",
        });
      }
    }
    if (!isBlob && view.value.showBits) {
      region.bits.forEach((bf) => {
        const label = bf.hi === bf.lo ? `bit ${bf.hi}` : `bits ${bf.hi}-${bf.lo}`;
        const raw = bf.raw.toString(2).padStart(bf.hi - bf.lo + 1, "0");
        out.push({ region: i, left: "", annot: `${indent}  └ ${label} = ${raw} -> ${bf.meaning}`, tone: "bits" });
      });
    }
    if (isBlob && (view.value.dataMode === "both" || view.value.dataMode === "decoded")) {
      out.push({ region: i, left: "", annot: `${indent}  decoded: ${decoded(i)}`, tone: "decoded" });
    }
  });
  return out;
});

function metaSummary(i: number): string {
  const r = props.tree.regions[i];
  return r.value === null ? r.label : `${r.label}: ${r.value}`;
}

function blobSummary(i: number): string {
  const r = props.tree.regions[i];
  if (!r.blob) return `${r.label} [${r.len} B]`;
  const b = r.blob;
  return `${r.label} [${b.streamType} ${b.logical}/${b.physical}, ${b.numValues} values, ${r.len} B]`;
}

function decoded(i: number): string {
  return decodedSummary(props.tile.decodeBlob(indexInWhole(i), 48));
}

/** decodeBlob is indexed against the unfiltered tree, so a layer filter has to shift. */
function indexInWhole(i: number): number {
  const whole = props.tile.tree().regions;
  const r = props.tree.regions[i];
  if (whole.length === props.tree.regions.length) return i;
  return whole.findIndex((w) => w.offset === r.offset && w.label === r.label && w.len === r.len);
}
</script>

<template>
  <div class="dump">
    <header>
      <SourcePicker
        layout="bar"
        :index="props.index"
        :current="props.current"
        @pick-fixture="emit('pickFixture', $event)"
        @pick-upload="emit('pickUpload', $event)"
      />
      <RenderControls v-model="view" :layers="props.layers" layout="row" />
      <span class="status">{{ props.status }}</span>
    </header>

    <pre class="stream"><template v-for="(line, n) in lines" :key="n"><span
      class="line"
      :class="[line.tone, { sel: selected === line.region }]"
      @click="selected = selected === line.region ? null : line.region"
    >{{ line.left.padEnd(leftWidth) }} | {{ line.annot }}
</span></template></pre>

    <footer v-if="shown < props.tree.regions.length">
      showing the first {{ shown }} of {{ props.tree.regions.length }} regions
      <button @click="shown += 2000">render 2000 more</button>
    </footer>
  </div>
</template>

<style scoped>
.dump {
  display: flex;
  flex-direction: column;
  height: 100vh;
}
header {
  display: flex;
  gap: 1rem;
  align-items: center;
  flex-wrap: wrap;
  padding: 0.5rem 0.75rem;
  border-bottom: 1px solid #2a3340;
  background: #121821;
}
.status {
  margin-left: auto;
  color: #789;
  font-size: 0.78rem;
}
.stream {
  flex: 1;
  overflow: auto;
  margin: 0;
  padding: 0.5rem 0.75rem 3rem;
  font-size: 0.74rem;
  line-height: 1.35;
}
.line {
  display: block;
  white-space: pre;
}
.line:hover {
  background: #1b2430;
}
.line.sel {
  background: #23405c;
}
.container {
  color: #e8d9a0;
  font-weight: 600;
}
.meta {
  color: #cde;
}
.blob {
  color: #8fa8c0;
}
.bits,
.note {
  color: #6b7f93;
}
.decoded {
  color: #7fd6a0;
}
footer {
  padding: 0.4rem 0.75rem;
  border-top: 1px solid #2a3340;
  color: #d9a441;
  font-size: 0.78rem;
}
footer button {
  margin-left: 0.5rem;
  background: #18202b;
  color: #cde;
  border: 1px solid #2a3340;
  border-radius: 3px;
  font: inherit;
  cursor: pointer;
}
</style>
