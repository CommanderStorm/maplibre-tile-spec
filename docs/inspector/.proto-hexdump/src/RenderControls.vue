<script setup lang="ts">
// The five CLI knobs. Only `layer` reaches the wasm (#14); the rest are view state,
// which is why the variants can re-render on every keystroke.
import type { ViewState } from "./hex";

const view = defineModel<ViewState>({ required: true });
const props = defineProps<{ layers: string[]; layout: "row" | "column" }>();
</script>

<template>
  <div class="controls" :class="props.layout">
    <label>
      layer
      <select v-model="view.layer">
        <option :value="null">all ({{ props.layers.length }})</option>
        <option v-for="(l, i) in props.layers" :key="l" :value="i">{{ i }} — {{ l }}</option>
      </select>
      <em class="wasm" title="the only knob the wasm sees">wasm</em>
    </label>
    <label>
      width
      <input v-model.number="view.width" type="range" min="4" max="48" step="4" />
      <output>{{ view.width }}</output>
    </label>
    <label>
      data
      <select v-model="view.dataMode">
        <option value="both">both</option>
        <option value="blob">blob</option>
        <option value="decoded">decoded</option>
        <option value="hidden">hidden</option>
      </select>
    </label>
    <label>
      max blob
      <input v-model.number="view.maxBlob" type="number" min="0" step="64" />
    </label>
    <label class="check">
      <input v-model="view.showBits" type="checkbox" />
      bits
    </label>
  </div>
</template>

<style scoped>
.controls {
  display: flex;
  gap: 0.75rem;
  align-items: center;
  flex-wrap: wrap;
  font-size: 0.78rem;
  color: #9ab;
}
.controls.column {
  flex-direction: column;
  align-items: stretch;
}
label {
  display: flex;
  gap: 0.3rem;
  align-items: center;
}
select,
input[type="number"] {
  background: #18202b;
  color: #cde;
  border: 1px solid #2a3340;
  border-radius: 3px;
  font: inherit;
  padding: 0.05rem 0.2rem;
}
input[type="number"] {
  width: 4.5rem;
}
input[type="range"] {
  width: 5.5rem;
}
output {
  color: #cde;
  width: 1.5rem;
}
.wasm {
  background: #3a2d12;
  color: #d9a441;
  border-radius: 2px;
  padding: 0 0.25rem;
  font-size: 0.68rem;
  font-style: normal;
}
</style>
