<script setup lang="ts">
// Source widget: upload + drag-and-drop + the fixture index, grouped by tag.
// Each variant places it differently; only the widget itself is shared.
import { ref } from "vue";

export interface FixtureEntry {
  name: string;
  path: string;
}

const props = defineProps<{
  index: Record<string, FixtureEntry[]> | null;
  current: string | null;
  layout: "bar" | "panel";
}>();

const emit = defineEmits<{
  pickFixture: [path: string];
  pickUpload: [file: File];
}>();

const dragging = ref(false);

function onDrop(e: DragEvent) {
  dragging.value = false;
  const file = e.dataTransfer?.files?.[0];
  if (file) emit("pickUpload", file);
}

function onFile(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0];
  if (file) emit("pickUpload", file);
}
</script>

<template>
  <div
    class="source"
    :class="[layout, { dragging }]"
    @dragover.prevent="dragging = true"
    @dragleave="dragging = false"
    @drop.prevent="onDrop"
  >
    <label class="upload">
      <input type="file" accept=".mlt" @change="onFile" />
      <span>{{ dragging ? "drop to load" : "upload .mlt" }}</span>
    </label>
    <span class="hint">or drag a tile anywhere</span>
    <template v-for="(entries, group) in props.index ?? {}" :key="group">
      <fieldset>
        <legend>{{ group }}</legend>
        <button
          v-for="f in entries"
          :key="f.path"
          :class="{ on: f.path === props.current }"
          @click="emit('pickFixture', f.path)"
        >
          {{ f.name }}
        </button>
      </fieldset>
    </template>
  </div>
</template>

<style scoped>
.source {
  display: flex;
  gap: 0.6rem;
  align-items: center;
  flex-wrap: wrap;
}
.source.panel {
  flex-direction: column;
  align-items: stretch;
}
.dragging {
  outline: 2px dashed #7c6;
}
.upload input {
  display: none;
}
.upload span {
  border: 1px solid #567;
  border-radius: 3px;
  padding: 0.15rem 0.5rem;
  cursor: pointer;
  white-space: nowrap;
}
.hint {
  color: #789;
  font-size: 0.78rem;
}
fieldset {
  border: 1px solid #2a3340;
  border-radius: 3px;
  padding: 0.1rem 0.4rem 0.3rem;
  display: flex;
  gap: 0.25rem;
  flex-wrap: wrap;
}
legend {
  color: #789;
  font-size: 0.72rem;
  padding: 0 0.25rem;
}
button {
  background: #18202b;
  color: #cde;
  border: 1px solid #2a3340;
  border-radius: 3px;
  padding: 0.1rem 0.4rem;
  font: inherit;
  font-size: 0.78rem;
  cursor: pointer;
}
button.on {
  background: #2d4;
  color: #062;
  border-color: #2d4;
}
</style>
