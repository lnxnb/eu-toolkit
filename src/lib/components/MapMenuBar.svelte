<script lang="ts">
  import type { Snippet } from "svelte";
  let { children }: { children: Snippet } = $props();
</script>

<nav class="toolbar" aria-label="Application menu">{@render children()}</nav>

<style>
  .toolbar { position: absolute; inset: 0 0 auto; z-index: 10; display: flex; align-items: center; gap: var(--sp-2); padding: var(--sp-1) var(--sp-2); background: var(--bg-3); border-bottom: 1px solid var(--bg-2); color: var(--text-1); font-size: var(--fs-md); }
</style>
