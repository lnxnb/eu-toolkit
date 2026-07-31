<script lang="ts">
  import { onMount } from "svelte";
  import "../app.css";
  import { initializeWindowGeometry } from "$lib/windowGeometry";

  let { children } = $props();
  onMount(() => { let cleanup: (() => void) | undefined; void initializeWindowGeometry().then((fn) => cleanup = fn); return () => cleanup?.(); });
</script>

{@render children()}
