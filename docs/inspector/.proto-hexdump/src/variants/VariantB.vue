<script setup lang="ts">
// B — Outline: the region tree is the page. No global hex column; every leaf carries
// its own bytes inline, and Source + controls live in a left panel.
import { computed, ref, watch } from "vue";

import type { AnnotatedTile, DumpTree, Region } from "../annotated";
import { decodedValues, hex2, hex8, shownLen, type ViewState } from "../hex";
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

/** Containers deeper than this start collapsed, so a real tile opens readable. */
const AUTO_OPEN_DEPTH = 1;

const collapsed = ref(new Set<number>());
const selected = ref<number | null>(null);

watch(
  () => props.tree,
  () => {
    collapsed.value = new Set(
      props.tree.regions.flatMap((r, i) => (r.container && r.depth > AUTO_OPEN_DEPTH ? [i] : [])),
    );
    selected.value = null;
  },
  { immediate: true },
);

interface Node {
  index: number;
  region: Region;
  hidden: boolean;
  hasKids: boolean;
}

/** The pre-order list with collapsed subtrees dropped. */
const nodes = computed<Node[]>(() => {
  const out: Node[] = [];
  const regions = props.tree.regions;
  let hideBelow = Number.POSITIVE_INFINITY;
  regions.forEach((region, i) => {
    if (region.depth > hideBelow) return;
    hideBelow = Number.POSITIVE_INFINITY;
    const hasKids = region.container && (regions[i + 1]?.depth ?? 0) > region.depth;
    if (region.container && collapsed.value.has(i)) hideBelow = region.depth;
    out.push({ index: i, region, hidden: false, hasKids });
  });
  return out;
});

function toggle(i: number) {
  const next = new Set(collapsed.value);
  if (next.has(i)) next.delete(i);
  else next.add(i);
  collapsed.value = next;
}

function expandAll() {
  collapsed.value = new Set();
}

function collapseAll() {
  collapsed.value = new Set(props.tree.regions.flatMap((r, i) => (r.container ? [i] : [])));
}

/** Inline bytes for a leaf, capped so a 40 kB blob cannot blow up the row. */
function inlineBytes(region: Region): string[] {
  const len = Math.min(shownLen(region, view.value), view.value.width * 2);
  const out: string[] = [];
  for (let at = 0; at < len; at++) out.push(hex2(props.bytes[region.offset + at]));
  return out;
}

function values(i: number): string[] {
  return decodedValues(props.tile.decodeBlob(indexInWhole(i), 48));
}

function indexInWhole(i: number): number {
  const whole = props.tile.tree().regions;
  const r = props.tree.regions[i];
  if (whole.length === props.tree.regions.length) return i;
  return whole.findIndex((w) => w.offset === r.offset && w.label === r.label && w.len === r.len);
}
</script>

<template>
  <div class="outline">
    <aside>
      <h1>Inspector</h1>
      <p class="status">{{ props.status }}</p>
      <SourcePicker
        layout="panel"
        :index="props.index"
        :current="props.current"
        @pick-fixture="emit('pickFixture', $event)"
        @pick-upload="emit('pickUpload', $event)"
      />
      <hr />
      <RenderControls v-model="view" :layers="props.layers" layout="column" />
      <hr />
      <div class="folds">
        <button @click="expandAll">expand all</button>
        <button @click="collapseAll">collapse all</button>
      </div>
    </aside>

    <main>
      <div
        v-for="node in nodes"
        :key="node.index"
        class="node"
        :class="[node.region.kind, { container: node.region.container, sel: selected === node.index }]"
        :style="{ paddingLeft: `${node.region.depth * 1.1 + 0.5}rem` }"
        @click="selected = selected === node.index ? null : node.index"
      >
        <button v-if="node.hasKids" class="caret" @click.stop="toggle(node.index)">
          {{ collapsed.has(node.index) ? "▸" : "▾" }}
        </button>
        <span v-else class="caret spacer"></span>

        <span class="label">{{ node.region.label }}</span>
        <span v-if="node.region.value" class="value">{{ node.region.value }}</span>
        <span v-if="node.region.container" class="span">{{ node.region.len }} B</span>

        <template v-if="!node.region.container">
          <span class="off">@{{ hex8(node.region.offset) }}</span>
          <span v-if="node.region.blob" class="badge">
            {{ node.region.blob.streamType }} · {{ node.region.blob.logical }}/{{ node.region.blob.physical }} ·
            {{ node.region.blob.numValues }}v
          </span>
          <span v-if="view.dataMode !== 'hidden'" class="bytes">
            <i v-for="(b, n) in inlineBytes(node.region)" :key="n">{{ b }}</i>
            <em v-if="inlineBytes(node.region).length < node.region.len">
              +{{ node.region.len - inlineBytes(node.region).length }}
            </em>
          </span>
        </template>

        <div v-if="selected === node.index && node.region.bits.length && view.showBits" class="bits" @click.stop>
          <div v-for="bf in node.region.bits" :key="bf.hi" class="bit">
            <code>{{ bf.hi === bf.lo ? `bit ${bf.hi}` : `bits ${bf.hi}-${bf.lo}` }}</code>
            <b>{{ bf.raw.toString(2).padStart(bf.hi - bf.lo + 1, "0") }}</b>
            <span>{{ bf.meaning }}</span>
          </div>
        </div>

        <div
          v-if="selected === node.index && node.region.blob && view.dataMode !== 'hidden' && view.dataMode !== 'blob'"
          class="decoded"
          @click.stop
        >
          <i v-for="(v, n) in values(node.index)" :key="n">{{ v }}</i>
        </div>
      </div>
    </main>
  </div>
</template>

<style scoped>
.outline {
  display: grid;
  grid-template-columns: 15rem 1fr;
  height: 100vh;
}
aside {
  border-right: 1px solid #2a3340;
  background: #121821;
  padding: 0.75rem;
  overflow: auto;
}
h1 {
  font: 600 0.95rem/1.2 system-ui, sans-serif;
  margin: 0 0 0.2rem;
  color: #cde;
}
.status {
  color: #789;
  font-size: 0.74rem;
  margin: 0 0 0.6rem;
}
hr {
  border: none;
  border-top: 1px solid #2a3340;
  margin: 0.7rem 0;
}
.folds {
  display: flex;
  gap: 0.3rem;
}
.folds button {
  flex: 1;
  background: #18202b;
  color: #cde;
  border: 1px solid #2a3340;
  border-radius: 3px;
  font: inherit;
  font-size: 0.74rem;
  cursor: pointer;
}
main {
  overflow: auto;
  padding: 0.5rem 0.75rem 3.5rem;
  font-size: 0.76rem;
}
.node {
  display: flex;
  align-items: baseline;
  gap: 0.4rem;
  flex-wrap: wrap;
  padding: 0.08rem 0;
  border-radius: 3px;
  cursor: pointer;
}
.node:hover {
  background: #1b2430;
}
.node.sel {
  background: #23405c;
}
.caret {
  width: 1rem;
  background: none;
  border: none;
  color: #8fa8c0;
  cursor: pointer;
  font: inherit;
  padding: 0;
}
.caret.spacer {
  display: inline-block;
}
.label {
  color: #cde;
}
.node.container .label {
  color: #e8d9a0;
  font-weight: 600;
}
.value {
  color: #7fd6a0;
}
.span,
.off {
  color: #5d708a;
  font-size: 0.7rem;
}
.badge {
  color: #d9a441;
  background: #2a2214;
  border-radius: 2px;
  padding: 0 0.25rem;
  font-size: 0.68rem;
}
.bytes i {
  color: #8fa8c0;
  font-style: normal;
  margin-right: 0.25rem;
}
.bytes em {
  color: #5d708a;
  font-style: normal;
}
.bits,
.decoded {
  flex-basis: 100%;
  margin: 0.2rem 0 0.3rem 1.4rem;
  padding: 0.3rem 0.4rem;
  background: #10161e;
  border-left: 2px solid #3a4a5e;
  border-radius: 2px;
}
.bit {
  display: flex;
  gap: 0.5rem;
}
.bit code {
  color: #6b7f93;
  width: 5.5rem;
}
.bit b {
  color: #d9a441;
  width: 3rem;
}
.bit span {
  color: #cde;
}
.decoded i {
  display: inline-block;
  color: #7fd6a0;
  font-style: normal;
  margin: 0 0.35rem 0.15rem 0;
}
</style>
