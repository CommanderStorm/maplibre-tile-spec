<script setup lang="ts">
// Prototype-only variant bar. Never ships.
import { onMounted, onUnmounted } from "vue";

const props = defineProps<{ variants: string[]; names: Record<string, string>; current: string }>();
const emit = defineEmits<{ pick: [key: string] }>();

function cycle(step: number) {
  const at = props.variants.indexOf(props.current);
  const next = (at + step + props.variants.length) % props.variants.length;
  emit("pick", props.variants[next]);
}

function onKey(e: KeyboardEvent) {
  const el = document.activeElement;
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) return;
  if (el instanceof HTMLElement && el.isContentEditable) return;
  if (e.key === "ArrowLeft") cycle(-1);
  if (e.key === "ArrowRight") cycle(1);
}

onMounted(() => window.addEventListener("keydown", onKey));
onUnmounted(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="switcher">
    <button @click="cycle(-1)">←</button>
    <span class="label">{{ current }} — {{ names[current] }}</span>
    <button @click="cycle(1)">→</button>
  </div>
</template>

<style scoped>
.switcher {
  position: fixed;
  bottom: 2.4rem;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 0.4rem;
  background: #f7f3e8;
  color: #1a1a1a;
  border-radius: 999px;
  padding: 0.3rem 0.5rem;
  box-shadow: 0 4px 18px rgb(0 0 0 / 55%);
  font-family: system-ui, sans-serif;
  font-size: 0.82rem;
  z-index: 100;
}
.label {
  padding: 0 0.3rem;
  white-space: nowrap;
}
button {
  border: none;
  background: #1a1a1a;
  color: #f7f3e8;
  border-radius: 999px;
  width: 1.5rem;
  height: 1.5rem;
  cursor: pointer;
  font: inherit;
}
</style>
