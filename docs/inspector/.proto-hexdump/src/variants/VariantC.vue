<script setup lang="ts">
// C — Split: a virtualized hex map of the whole buffer on the left, one region's
// detail on the right. Hover links byte -> region, the tree list links region -> bytes.
import { computed, ref, watch } from "vue";

import type { AnnotatedTile, DumpTree } from "../annotated";
import { byteOwners, decodedValues, hex2, hex8, regionPath, type ViewState } from "../hex";
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

const owners = computed(() => byteOwners(props.tree));
const rowCount = computed(() => Math.ceil(props.tree.bufLen / view.value.width));

const scrollTop = ref(0);
const viewportH = ref(600);
const pane = ref<HTMLElement | null>(null);

const window_ = computed(() => {
  const first = Math.max(0, Math.floor(scrollTop.value / ROW) - OVERSCAN);
  const count = Math.ceil(viewportH.value / ROW) + OVERSCAN * 2;
  return { first, last: Math.min(rowCount.value, first + count) };
});

const rows = computed(() => {
  const out: { n: number; offset: number; cells: { b: string; owner: number }[]; ascii: string }[] = [];
  for (let n = window_.value.first; n < window_.value.last; n++) {
    const offset = n * view.value.width;
    const cells = [];
    let ascii = "";
    for (let at = offset; at < Math.min(offset + view.value.width, props.tree.bufLen); at++) {
      const b = props.bytes[at];
      cells.push({ b: hex2(b), owner: owners.value[at] });
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

const hovered = ref<number | null>(null);
const selected = ref<number | null>(null);
const active = computed(() => selected.value ?? hovered.value);

watch(
  () => props.tree,
  () => {
    selected.value = null;
    hovered.value = null;
    scrollTop.value = 0;
    if (pane.value) pane.value.scrollTop = 0;
  },
);

const detail = computed(() => (active.value === null ? null : props.tree.regions[active.value]));
const path = computed(() => (active.value === null ? [] : regionPath(props.tree.regions, active.value)));

const values = computed(() => {
  const at = active.value;
  if (at === null) return null;
  const region = props.tree.regions[at];
  if (!region.blob || view.value.dataMode === "blob" || view.value.dataMode === "hidden") return null;
  return decodedValues(props.tile.decodeBlob(indexInWhole(at), Math.max(8, view.value.maxBlob / 4)));
});

function indexInWhole(i: number): number {
  const whole = props.tile.tree().regions;
  const r = props.tree.regions[i];
  if (whole.length === props.tree.regions.length) return i;
  return whole.findIndex((w) => w.offset === r.offset && w.label === r.label && w.len === r.len);
}

/** Leaves only: the map is a partition of the buffer, so containers have no cells. */
const leaves = computed(() =>
  props.tree.regions.flatMap((r, i) => (r.container ? [] : [{ i, label: r.label, depth: r.depth, kind: r.kind }])),
);

function reveal(i: number) {
  selected.value = i;
  const row = Math.floor(props.tree.regions[i].offset / view.value.width);
  if (pane.value) pane.value.scrollTop = Math.max(0, row * ROW - viewportH.value / 3);
}
</script>

<template>
  <div class="split">
    <header>
      <SourcePicker
        layout="bar"
        :index="props.index"
        :current="props.current"
        @pick-fixture="emit('pickFixture', $event)"
        @pick-upload="emit('pickUpload', $event)"
      />
      <span class="status">{{ props.status }}</span>
    </header>

    <div class="panes">
      <div ref="pane" class="map" @scroll="onScroll">
        <div class="spacer" :style="{ height: `${rowCount * ROW}px` }">
          <div
            v-for="row in rows"
            :key="row.n"
            class="row"
            :style="{ top: `${row.n * ROW}px` }"
          >
            <span class="off">{{ hex8(row.offset) }}</span>
            <span
              v-for="(cell, n) in row.cells"
              :key="n"
              class="cell"
              :class="[
                props.tree.regions[cell.owner]?.kind,
                {
                  on: cell.owner === active,
                  alt: cell.owner % 2 === 1,
                },
              ]"
              @mouseenter="hovered = cell.owner"
              @click="selected = cell.owner"
              >{{ cell.b }}</span
            >
            <span class="ascii">{{ row.ascii }}</span>
          </div>
        </div>
      </div>

      <aside>
        <RenderControls v-model="view" :layers="props.layers" layout="column" />
        <hr />
        <template v-if="detail">
          <nav>
            <span v-for="p in path" :key="p">{{ p }}</span>
          </nav>
          <h2>{{ detail.label }}</h2>
          <dl>
            <dt>offset</dt>
            <dd>{{ hex8(detail.offset) }} · {{ detail.len }} B</dd>
            <dt>kind</dt>
            <dd>{{ detail.kind }}{{ detail.container ? " (container)" : "" }}</dd>
            <template v-if="detail.value">
              <dt>value</dt>
              <dd class="val">{{ detail.value }}</dd>
            </template>
            <template v-if="detail.blob">
              <dt>stream</dt>
              <dd>{{ detail.blob.streamType }}</dd>
              <dt>encoding</dt>
              <dd>{{ detail.blob.logical }} / {{ detail.blob.physical }}</dd>
              <dt>values</dt>
              <dd>{{ detail.blob.numValues }} · hint {{ detail.blob.hint.kind }}</dd>
            </template>
          </dl>

          <template v-if="detail.bits.length && view.showBits">
            <h3>bits</h3>
            <div v-for="bf in detail.bits" :key="bf.hi" class="bitrow">
              <span class="bitcells">
                <i
                  v-for="n in 8"
                  :key="n"
                  :class="{ in: 8 - n <= bf.hi && 8 - n >= bf.lo }"
                >{{ (props.bytes[detail.offset] >> (8 - n)) & 1 }}</i>
              </span>
              <span class="meaning">{{ bf.meaning }}</span>
            </div>
          </template>

          <template v-if="values">
            <h3>decoded</h3>
            <div class="values">
              <i v-for="(v, n) in values" :key="n">{{ v }}</i>
            </div>
          </template>
        </template>
        <p v-else class="idle">Hover or click a byte. Or pick a region:</p>

        <h3>regions</h3>
        <ul class="list">
          <li
            v-for="leaf in leaves"
            :key="leaf.i"
            :class="[leaf.kind, { on: leaf.i === active }]"
            :style="{ paddingLeft: `${leaf.depth * 0.6}rem` }"
            @click="reveal(leaf.i)"
          >
            {{ leaf.label }}
          </li>
        </ul>
      </aside>
    </div>
  </div>
</template>

<style scoped>
.split {
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
.panes {
  flex: 1;
  display: grid;
  grid-template-columns: 1fr 21rem;
  min-height: 0;
}
.map {
  overflow: auto;
  position: relative;
  padding: 0.4rem 0.75rem 3rem;
  font-size: 0.74rem;
}
.spacer {
  position: relative;
}
.row {
  position: absolute;
  left: 0;
  right: 0;
  height: 19px;
  display: flex;
  align-items: center;
  gap: 0.15rem;
  white-space: pre;
}
.off {
  color: #4e5f74;
  margin-right: 0.4rem;
}
.cell {
  color: #8fa8c0;
  cursor: pointer;
  padding: 0 1px;
  border-radius: 2px;
}
.cell.meta {
  color: #cde;
}
.cell.dataBlob {
  color: #7f94a8;
}
.cell.alt {
  background: #161d26;
}
.cell.on {
  background: #2d6;
  color: #042;
}
.ascii {
  margin-left: 0.6rem;
  color: #4e5f74;
}
aside {
  border-left: 1px solid #2a3340;
  background: #121821;
  padding: 0.6rem 0.7rem 3rem;
  overflow: auto;
  font-size: 0.76rem;
}
hr {
  border: none;
  border-top: 1px solid #2a3340;
  margin: 0.6rem 0;
}
nav {
  color: #5d708a;
  font-size: 0.7rem;
}
nav span::after {
  content: " › ";
}
h2 {
  font: 600 0.9rem/1.3 ui-monospace, monospace;
  color: #e8d9a0;
  margin: 0.1rem 0 0.4rem;
}
h3 {
  font: 600 0.72rem/1.4 system-ui, sans-serif;
  color: #789;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  margin: 0.8rem 0 0.25rem;
}
dl {
  display: grid;
  grid-template-columns: 4.6rem 1fr;
  gap: 0.1rem 0.4rem;
  margin: 0;
}
dt {
  color: #6b7f93;
}
dd {
  margin: 0;
  color: #cde;
}
dd.val {
  color: #7fd6a0;
}
.bitrow {
  display: flex;
  gap: 0.4rem;
  align-items: baseline;
}
.bitcells i {
  font-style: normal;
  color: #3f4d5e;
}
.bitcells i.in {
  color: #d9a441;
  font-weight: 600;
}
.meaning {
  color: #cde;
}
.values i {
  display: inline-block;
  font-style: normal;
  color: #7fd6a0;
  margin: 0 0.3rem 0.1rem 0;
}
.idle {
  color: #6b7f93;
}
.list {
  list-style: none;
  margin: 0;
  padding: 0;
}
.list li {
  cursor: pointer;
  border-radius: 2px;
  padding: 0 0.2rem;
  color: #9ab;
}
.list li.dataBlob {
  color: #7f94a8;
}
.list li:hover {
  background: #1b2430;
}
.list li.on {
  background: #23405c;
  color: #fff;
}
</style>
