<script setup lang="ts">
// Three variants of the annotated hexdump view on one route, switched by ?variant=.
// The host owns the Source and the handle; each variant owns its own chrome.
import { computed, nextTick, onMounted, ref, watch } from "vue";

import { type AnnotatedTile, filterLayer, loadStubTile } from "./annotated";
import { defaultView, layerLabels, type ViewState } from "./hex";
import PrototypeSwitcher from "./PrototypeSwitcher.vue";
import SourcePicker, { type FixtureEntry } from "./SourcePicker.vue";
import VariantA from "./variants/VariantA.vue";
import VariantB from "./variants/VariantB.vue";
import VariantC from "./variants/VariantC.vue";

const NAMES: Record<string, string> = {
  A: "Dump — CLI stream",
  B: "Outline — tree with inline bytes",
  C: "Split — hex map + inspector",
};
const VARIANTS = Object.keys(NAMES);

const variant = ref(new URLSearchParams(location.search).get("variant") ?? "A");
watch(variant, (v) => {
  const url = new URL(location.href);
  url.searchParams.set("variant", v);
  history.replaceState(null, "", url);
});

const view = ref<ViewState>(defaultView());
const index = ref<Record<string, FixtureEntry[]> | null>(null);
const current = ref<string | null>(null);
const tile = ref<AnnotatedTile | null>(null);
const bytes = ref<Uint8Array>(new Uint8Array());
const loading = ref<string | null>(null);
const failure = ref<string | null>(null);

onMounted(async () => {
  index.value = await (await fetch("/fixtures/index.json")).json();
  const asked = new URLSearchParams(location.search).get("fixture");
  const fallback = index.value?.["0x02"]?.[0]?.path;
  const path = asked ?? fallback;
  if (path) await pickFixture(path);
});

async function pickFixture(path: string) {
  loading.value = path;
  failure.value = null;
  try {
    const [next, raw] = await Promise.all([
      loadStubTile(`/fixtures/${path}.json`),
      fetch(`/fixtures/${path}.mlt`).then((r) => r.arrayBuffer()),
    ]);
    tile.value?.free();
    tile.value = next;
    bytes.value = new Uint8Array(raw);
    const isSwitch = current.value !== null;
    current.value = path;
    if (isSwitch) view.value.layer = null;
    const url = new URL(location.href);
    url.searchParams.set("fixture", path);
    history.replaceState(null, "", url);
    await timeRerender();
  } catch (e) {
    failure.value = `could not load "${path}": ${e instanceof Error ? e.message : String(e)}`;
  } finally {
    loading.value = null;
  }
}

/** An upload has no precomputed dump, so the prototype says so instead of guessing. */
async function pickUpload(file: File) {
  failure.value =
    `${file.name}: an upload needs the real wasm binding to annotate. ` +
    `Add it to public/fixtures via gen-fixtures.sh to see it here.`;
}

const bench = ref<number | null>(null);

/** Nudges a view-only knob and times the frame, since "instant re-render" is a #15 question. */
async function timeRerender() {
  bench.value = null;
  await nextTick();
  const was = view.value.width;
  const t0 = performance.now();
  view.value.width = was === 16 ? 15 : 16;
  await nextTick();
  await new Promise((done) => requestAnimationFrame(() => done(null)));
  bench.value = performance.now() - t0;
  view.value.width = was;
}

const whole = computed(() => tile.value?.tree() ?? null);
const layers = computed(() => (whole.value ? layerLabels(whole.value) : []));
const tree = computed(() => {
  if (!whole.value) return null;
  return view.value.layer === null ? whole.value : filterLayer(whole.value, view.value.layer);
});

const status = computed(() => {
  if (!tree.value) return "";
  const t = tree.value;
  const blobs = t.regions.filter((r) => r.kind === "dataBlob");
  const blobBytes = blobs.reduce((n, r) => n + r.len, 0);
  const leaves = t.regions.filter((r) => !r.container);
  const span = leaves.reduce((n, r) => n + r.len, 0);
  return [
    `${t.bufLen.toLocaleString()} B`,
    `${t.regions.length.toLocaleString()} regions`,
    `${blobs.length} blobs (${Math.round((100 * blobBytes) / Math.max(1, span))}% of bytes)`,
    `${Math.ceil(span / view.value.width).toLocaleString()} hex rows`,
    bench.value === null ? "timing…" : `width re-render ${bench.value.toFixed(0)} ms`,
  ].join(" · ");
});

const component = computed(() => ({ A: VariantA, B: VariantB, C: VariantC })[variant.value] ?? VariantA);
</script>

<template>
  <div v-if="failure" class="failure">{{ failure }}</div>
  <p v-if="loading" class="loading">loading {{ loading }}…</p>

  <component
    :is="component"
    v-if="tile && tree"
    v-model:view="view"
    :tile="tile"
    :tree="tree"
    :bytes="bytes"
    :index="index"
    :current="current"
    :layers="layers"
    :status="status"
    @pick-fixture="pickFixture"
    @pick-upload="pickUpload"
  />

  <!-- The empty state cannot belong to a variant: there is no tile to render one around. -->
  <section v-else-if="!loading" class="empty">
    <h1>Annotated hexdump</h1>
    <p>Pick a synthetic fixture, or drop a <code>.mlt</code> tile here.</p>
    <SourcePicker
      layout="bar"
      :index="index"
      :current="current"
      @pick-fixture="pickFixture"
      @pick-upload="pickUpload"
    />
  </section>

  <PrototypeSwitcher :variants="VARIANTS" :names="NAMES" :current="variant" @pick="variant = $event" />
</template>

<style>
:root {
  color-scheme: dark;
}
body {
  margin: 0;
  background: #0d1218;
  color: #cde;
  font-family: ui-monospace, "SF Mono", "JetBrains Mono", monospace;
}
.failure {
  background: #3a1b1b;
  color: #ffb4a8;
  padding: 0.4rem 0.75rem;
  font-size: 0.78rem;
}
.loading {
  padding: 0.4rem 0.75rem;
  color: #789;
  font-size: 0.78rem;
  margin: 0;
}
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.6rem;
  height: 90vh;
  text-align: center;
}
.empty h1 {
  font: 600 1.1rem/1.2 system-ui, sans-serif;
  margin: 0;
}
.empty p {
  color: #789;
  margin: 0 0 0.6rem;
}
</style>
