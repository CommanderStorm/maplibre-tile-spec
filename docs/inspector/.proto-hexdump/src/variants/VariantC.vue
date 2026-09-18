<script setup lang="ts">
// C — Split: a virtualized hex map of the whole buffer on the left, one region's
// detail on the right, over a collapsible region tree. Hover links byte -> region,
// the tree links region -> bytes, and a container lights its whole span.
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";

import type { AnnotatedTile, DecodedBlob, DumpTree, Region } from "../annotated";
import { byteOwners, hex2, hex8, regionPath, type ViewState } from "../hex";
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

const ROW = 19;
const OVERSCAN = 8;
const MAX_VALUES = 64;

// ── the hex map ────────────────────────────────────────────────────────────────

const owners = computed(() => byteOwners(props.tree));
const rowCount = computed(() => Math.ceil(props.tree.bufLen / view.value.width));

const scrollTop = ref(0);
const viewportH = ref(600);
const pane = ref<HTMLElement | null>(null);

const slice = computed(() => {
  const first = Math.max(0, Math.floor(scrollTop.value / ROW) - OVERSCAN);
  const rows = Math.ceil(viewportH.value / ROW) + OVERSCAN * 2;
  return { first, last: Math.min(rowCount.value, first + rows) };
});

interface Cell {
  hex: string;
  owner: number;
  starts: boolean;
  faded: boolean;
}

/** Where a blob's bytes stop being worth reading, which is what `max blob` now means here. */
function fadedFrom(region: Region): number {
  if (view.value.dataMode === "hidden" && region.kind === "dataBlob") return region.offset;
  if (region.kind !== "dataBlob" || view.value.maxBlob === 0) return Number.POSITIVE_INFINITY;
  return region.offset + view.value.maxBlob;
}

const rows = computed(() => {
  const out: { n: number; offset: number; cells: Cell[]; ascii: string }[] = [];
  const regions = props.tree.regions;
  for (let n = slice.value.first; n < slice.value.last; n++) {
    const offset = n * view.value.width;
    const end = Math.min(offset + view.value.width, props.tree.bufLen);
    const cells: Cell[] = [];
    let ascii = "";
    for (let at = offset; at < end; at++) {
      const owner = owners.value[at];
      const region = regions[owner];
      const b = props.bytes[at];
      cells.push({
        hex: hex2(b),
        owner,
        starts: region !== undefined && at === region.offset,
        faded: region !== undefined && at >= fadedFrom(region),
      });
      ascii += b >= 0x20 && b <= 0x7e ? String.fromCharCode(b) : ".";
    }
    out.push({ n, offset, cells, ascii });
  }
  return out;
});

function onScroll(e: Event) {
  const el = e.target as HTMLElement;
  scrollTop.value = el.scrollTop;
  viewportH.value = el.clientHeight;
}

// A terminal is 80 columns; a browser is not, so the width knob defaults to filling
// the pane instead of leaving half of it blank. Unticking it leaves the fitted value.
const fit = ref(true);
let observer: ResizeObserver | null = null;

function fitWidth() {
  if (!fit.value || !pane.value) return;
  const probe = pane.value.querySelector(".hexrow .cell");
  const cell = probe ? probe.getBoundingClientRect().width : 18;
  const off = pane.value.querySelector(".hexrow .off")?.getBoundingClientRect().width ?? 70;
  // Each column costs one hex cell plus one ascii character, ~0.42 of a cell.
  const room = pane.value.clientWidth - off - 36;
  const cols = Math.floor(room / (cell * 1.42));
  const rounded = Math.max(8, Math.min(64, Math.floor(cols / 4) * 4));
  if (rounded !== view.value.width) view.value.width = rounded;
}

onMounted(() => {
  window.addEventListener("keydown", onKey);
  if (!pane.value) return;
  viewportH.value = pane.value.clientHeight;
  observer = new ResizeObserver(() => fitWidth());
  observer.observe(pane.value);
  requestAnimationFrame(fitWidth);
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKey);
  observer?.disconnect();
});
watch(fit, fitWidth);

/** How many bytes the current `max blob` fades out, so the knob reports its own effect. */
const fadedBytes = computed(() =>
  props.tree.regions.reduce((n, r) => {
    if (r.container) return n;
    const from = fadedFrom(r);
    return from === Number.POSITIVE_INFINITY ? n : n + Math.max(0, r.offset + r.len - from);
  }, 0),
);

// ── selection ─────────────────────────────────────────────────────────────────

const hovered = ref<number | null>(null);
const selected = ref<number | null>(null);
const activeIndex = computed(() => selected.value ?? hovered.value);
const active = computed(() => (activeIndex.value === null ? null : props.tree.regions[activeIndex.value]));

/** A container lights its whole span, which is how the map shows structure. */
function inActive(cell: Cell, at: number): boolean {
  const a = active.value;
  if (!a) return false;
  return a.container ? at >= a.offset && at < a.offset + a.len : cell.owner === activeIndex.value;
}

const path = computed(() => (activeIndex.value === null ? [] : regionPath(props.tree.regions, activeIndex.value)));

const childCount = computed(() => {
  const at = activeIndex.value;
  if (at === null || !props.tree.regions[at].container) return 0;
  const depth = props.tree.regions[at].depth;
  let n = 0;
  for (let i = at + 1; i < props.tree.regions.length && props.tree.regions[i].depth > depth; i++) n += 1;
  return n;
});

const decoded = computed<DecodedBlob | null>(() => {
  const at = activeIndex.value;
  if (at === null) return null;
  const region = props.tree.regions[at];
  if (!region.blob) return null;
  if (view.value.dataMode === "blob" || view.value.dataMode === "hidden") return null;
  return props.tile.decodeBlob(indexInWhole(at), MAX_VALUES);
});

const decodedChips = computed(() => {
  const d = decoded.value;
  if (!d) return [];
  switch (d.kind) {
    case "numbers":
    case "bigints":
      return d.values.map(String);
    case "bools":
      return d.values.map((b) => (b ? "1" : "0"));
    case "text":
      return [d.value];
    case "binary":
      return [`${d.len} binary bytes`];
    case "error":
      return [d.message];
  }
});

const decodedNote = computed(() => {
  const d = decoded.value;
  if (!d) return "";
  if (d.kind === "error") return "undecodable";
  if (d.kind === "text" || d.kind === "binary") return d.kind;
  return d.truncatedFrom === null
    ? `${d.values.length} values`
    : `${d.values.length} of ${d.truncatedFrom} values`;
});

/** decodeBlob is indexed against the unfiltered tree, so a layer filter has to shift. */
function indexInWhole(i: number): number {
  const whole = props.tile.tree().regions;
  const r = props.tree.regions[i];
  if (whole.length === props.tree.regions.length) return i;
  return whole.findIndex((w) => w.offset === r.offset && w.label === r.label && w.len === r.len);
}

// ── the region tree ───────────────────────────────────────────────────────────

const AUTO_OPEN_DEPTH = 1;
const collapsed = ref(new Set<number>());

watch(
  () => props.tree,
  () => {
    collapsed.value = new Set(
      props.tree.regions.flatMap((r, i) => (r.container && r.depth > AUTO_OPEN_DEPTH ? [i] : [])),
    );
    selected.value = null;
    hovered.value = null;
    scrollTop.value = 0;
    if (pane.value) pane.value.scrollTop = 0;
  },
  { immediate: true },
);

interface Node {
  index: number;
  region: Region;
  hasKids: boolean;
}

const nodes = computed<Node[]>(() => {
  const out: Node[] = [];
  const regions = props.tree.regions;
  let hideBelow = Number.POSITIVE_INFINITY;
  regions.forEach((region, i) => {
    if (region.depth > hideBelow) return;
    hideBelow = Number.POSITIVE_INFINITY;
    const hasKids = region.container && (regions[i + 1]?.depth ?? 0) > region.depth;
    if (region.container && collapsed.value.has(i)) hideBelow = region.depth;
    out.push({ index: i, region, hasKids });
  });
  return out;
});

function toggle(i: number) {
  const next = new Set(collapsed.value);
  if (next.has(i)) next.delete(i);
  else next.add(i);
  collapsed.value = next;
}

/** Opens every container on the way to `i`, so revealing from the map cannot land nowhere. */
function revealInTree(i: number) {
  const next = new Set(collapsed.value);
  let depth = props.tree.regions[i].depth;
  for (let at = i - 1; at >= 0 && depth > 0; at--) {
    const r = props.tree.regions[at];
    if (r.container && r.depth < depth) {
      next.delete(at);
      depth = r.depth;
    }
  }
  collapsed.value = next;
}

function pick(i: number, scroll = true) {
  selected.value = i;
  revealInTree(i);
  if (!scroll || !pane.value) return;
  const row = Math.floor(props.tree.regions[i].offset / view.value.width);
  const top = row * ROW;
  const seen = pane.value.scrollTop;
  if (top < seen || top > seen + pane.value.clientHeight - ROW * 2) {
    pane.value.scrollTop = Math.max(0, top - pane.value.clientHeight / 3);
  }
}

/** ↑/↓ walk the leaves, since ←/→ belong to the prototype switcher. On the window,
 * because a focusable pane would make the keys work only after a click. */
function onKey(e: KeyboardEvent) {
  const el = document.activeElement;
  if (el instanceof HTMLInputElement || el instanceof HTMLSelectElement) return;
  if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
  e.preventDefault();
  const step = e.key === "ArrowDown" ? 1 : -1;
  const regions = props.tree.regions;
  let at = (selected.value ?? hovered.value ?? -1) + step;
  while (at >= 0 && at < regions.length && regions[at].container) at += step;
  if (at >= 0 && at < regions.length) pick(at);
}
</script>

<template>
  <div class="split">
    <header>
      <SourcePicker
        layout="compact"
        :index="props.index"
        :current="props.current"
        @pick-fixture="emit('pickFixture', $event)"
        @pick-upload="emit('pickUpload', $event)"
      />
      <RenderControls v-model="view" :layers="props.layers" layout="row" />
    </header>

    <div class="panes">
      <section class="left">
        <div ref="pane" class="map" @scroll="onScroll" @mouseleave="hovered = null">
          <div class="spacer" :style="{ height: `${rowCount * ROW}px` }">
            <div v-for="row in rows" :key="row.n" class="hexrow" :style="{ top: `${row.n * ROW}px` }">
              <span class="off">{{ hex8(row.offset) }}</span>
              <span
                v-for="(cell, n) in row.cells"
                :key="n"
                class="cell"
                :class="[
                  props.tree.regions[cell.owner]?.kind,
                  { on: inActive(cell, row.offset + n), starts: cell.starts, faded: cell.faded },
                ]"
                @mouseenter="hovered = cell.owner"
                @click="pick(cell.owner, false)"
                >{{ cell.hex }}</span
              >
              <span class="ascii">{{ row.ascii }}</span>
            </div>
          </div>
        </div>
        <footer>
          <label class="fit"><input v-model="fit" type="checkbox" /> fit width</label>
          <span class="status">{{ props.status }}</span>
          <span v-if="fadedBytes" class="faded-note">
            max blob fades {{ fadedBytes.toLocaleString() }} B
          </span>
        </footer>
      </section>

      <aside>
        <div class="detail">
          <template v-if="active">
            <nav v-if="path.length">{{ path.join(" › ") }}</nav>
            <h2>
              {{ active.label }}
              <em v-if="selected === null">hovering</em>
            </h2>
            <dl>
              <dt>bytes</dt>
              <dd>{{ hex8(active.offset) }} … {{ hex8(active.offset + active.len - 1) }} · {{ active.len }} B</dd>
              <template v-if="active.container">
                <dt>holds</dt>
                <dd>{{ childCount }} regions</dd>
              </template>
              <template v-if="active.value">
                <dt>value</dt>
                <dd class="val">{{ active.value }}</dd>
              </template>
              <template v-if="active.blob">
                <dt>stream</dt>
                <dd>{{ active.blob.streamType }}</dd>
                <dt>encoding</dt>
                <dd>{{ active.blob.logical }} / {{ active.blob.physical }}</dd>
                <dt>values</dt>
                <dd>{{ active.blob.numValues }} · {{ active.blob.hint.kind }}</dd>
              </template>
            </dl>

            <template v-if="active.bits.length && view.showBits">
              <h3>bits of {{ hex2(props.bytes[active.offset]) }}</h3>
              <div v-for="bf in active.bits" :key="bf.hi" class="bitrow">
                <span class="bitcells">
                  <i v-for="n in 8" :key="n" :class="{ in: 8 - n <= bf.hi && 8 - n >= bf.lo }">{{
                    (props.bytes[active.offset] >> (8 - n)) & 1
                  }}</i>
                </span>
                <span class="meaning">{{ bf.meaning }}</span>
              </div>
            </template>

            <template v-if="decoded">
              <h3>decoded <small>{{ decodedNote }}</small></h3>
              <div class="values" :class="decoded.kind">
                <i v-for="(v, n) in decodedChips" :key="n">{{ v }}</i>
              </div>
            </template>
          </template>
          <p v-else class="idle">Hover a byte, or walk the tree with ↑ / ↓.</p>
        </div>

        <div class="tree">
          <div class="treebar">
            <span>regions</span>
            <button @click="collapsed = new Set()">expand</button>
            <button @click="collapsed = new Set(props.tree.regions.flatMap((r, i) => (r.container ? [i] : [])))">
              collapse
            </button>
          </div>
          <div class="treescroll">
            <div
              v-for="node in nodes"
              :key="node.index"
              class="node"
              :class="[node.region.kind, { container: node.region.container, on: node.index === activeIndex }]"
              :style="{ paddingLeft: `${node.region.depth * 0.7 + 0.3}rem` }"
              @mouseenter="hovered = node.index"
              @click="pick(node.index)"
            >
              <button v-if="node.hasKids" class="caret" @click.stop="toggle(node.index)">
                {{ collapsed.has(node.index) ? "▸" : "▾" }}
              </button>
              <span v-else class="caret" />
              <span class="label">{{ node.region.label }}</span>
              <span class="size">{{ node.region.len }} B</span>
            </div>
          </div>
        </div>
      </aside>
    </div>
  </div>
</template>

<style scoped>
.split {
  display: flex;
  flex-direction: column;
  height: 100vh;
  outline: none;
}
header {
  display: flex;
  gap: 1.25rem;
  align-items: center;
  flex-wrap: wrap;
  padding: 0.45rem 0.75rem;
  border-bottom: 1px solid #222c38;
  background: #111823;
}
/* Both children wrap internally, so they must not be shrunk below their content. */
header > * {
  flex: 0 0 auto;
  min-width: 0;
}
.panes {
  flex: 1;
  display: grid;
  grid-template-columns: 1fr 22rem;
  min-height: 0;
}
.left {
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.map {
  flex: 1;
  overflow: auto;
  position: relative;
  padding: 0.4rem 0.75rem 1rem;
  font-size: 0.74rem;
}
.spacer {
  position: relative;
}
.hexrow {
  position: absolute;
  left: 0;
  right: 0;
  height: 19px;
  display: flex;
  align-items: center;
  white-space: pre;
}
.off {
  color: #44535f;
  margin-right: 0.55rem;
}
.cell {
  color: #96aec4;
  cursor: pointer;
  padding: 1px 2px 1px 1px;
  border-left: 1px solid transparent;
  border-radius: 2px;
}
.cell.meta {
  color: #d3e2ef;
}
.cell.starts {
  border-left-color: #3b4c5e;
}
.cell.faded {
  color: #3f4b57;
}
.cell.on {
  background: #2e4f70;
  color: #eaf4ff;
  border-left-color: #2e4f70;
}
.cell.starts.on {
  border-left-color: #6fa8dc;
}
.ascii {
  margin-left: 0.7rem;
  color: #44535f;
}
.left footer {
  display: flex;
  gap: 1rem;
  padding: 0.3rem 0.75rem;
  border-top: 1px solid #222c38;
  background: #111823;
  color: #6b7f93;
  font-size: 0.72rem;
}
.faded-note {
  margin-left: auto;
  color: #8a7a4a;
}
.fit {
  display: flex;
  gap: 0.25rem;
  align-items: center;
  color: #9ab;
}
aside {
  border-left: 1px solid #222c38;
  background: #111823;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
/* flex-shrink stays 0: the tree must not squeeze the detail card down to its first line. */
.detail {
  padding: 0.55rem 0.7rem 0.7rem;
  overflow: auto;
  flex: 0 0 auto;
  max-height: 58%;
  font-size: 0.76rem;
}
nav {
  color: #55697f;
  font-size: 0.7rem;
}
h2 {
  font: 600 0.92rem/1.3 ui-monospace, monospace;
  color: #e8d9a0;
  margin: 0.05rem 0 0.45rem;
}
h2 em {
  color: #55697f;
  font: 400 0.68rem/1 system-ui, sans-serif;
  font-style: normal;
  vertical-align: middle;
  margin-left: 0.35rem;
}
h3 {
  font: 600 0.7rem/1.4 system-ui, sans-serif;
  color: #6b7f93;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 0.85rem 0 0.3rem;
}
h3 small {
  text-transform: none;
  letter-spacing: 0;
  color: #55697f;
  font-weight: 400;
}
dl {
  display: grid;
  grid-template-columns: 4.4rem 1fr;
  gap: 0.12rem 0.45rem;
  margin: 0;
}
dt {
  color: #6b7f93;
}
dd {
  margin: 0;
  color: #d3e2ef;
}
dd.val {
  color: #7fd6a0;
}
.bitrow {
  display: flex;
  gap: 0.55rem;
  align-items: baseline;
  line-height: 1.5;
}
.bitcells i {
  font-style: normal;
  color: #39485a;
}
.bitcells i.in {
  color: #d9a441;
  font-weight: 600;
}
.meaning {
  color: #d3e2ef;
}
.values i {
  display: inline-block;
  font-style: normal;
  color: #7fd6a0;
  margin: 0 0.35rem 0.12rem 0;
}
.values.error i,
.values.binary i {
  color: #d98f8f;
}
.idle {
  color: #55697f;
  margin: 0.2rem 0;
}
.tree {
  border-top: 1px solid #222c38;
  display: flex;
  flex-direction: column;
  min-height: 0;
  flex: 1 1 auto;
}
.treebar {
  display: flex;
  gap: 0.3rem;
  align-items: center;
  padding: 0.3rem 0.7rem;
  color: #6b7f93;
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}
.treebar span {
  flex: 1;
}
.treebar button {
  background: #18202b;
  color: #9ab;
  border: 1px solid #2a3340;
  border-radius: 3px;
  font: inherit;
  text-transform: none;
  letter-spacing: 0;
  cursor: pointer;
}
.treescroll {
  overflow: auto;
  padding: 0 0.75rem 2.5rem 0.4rem;
  font-size: 0.74rem;
}
.node {
  display: flex;
  align-items: baseline;
  gap: 0.3rem;
  border-radius: 2px;
  cursor: pointer;
  padding: 0.02rem 0.2rem;
}
.node:hover {
  background: #18212c;
}
.node.on {
  background: #2e4f70;
}
.node .caret {
  width: 0.9rem;
  flex: none;
  background: none;
  border: none;
  color: #6b7f93;
  cursor: pointer;
  font: inherit;
  padding: 0;
  text-align: left;
}
.label {
  color: #b9cbdb;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.node.container .label {
  color: #e8d9a0;
}
.node.dataBlob .label {
  color: #8fa8c0;
}
.node.on .label {
  color: #eaf4ff;
}
.size {
  margin-left: auto;
  flex: none;
  color: #4e5f74;
  font-size: 0.68rem;
}
</style>
