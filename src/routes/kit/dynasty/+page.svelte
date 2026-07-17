<!--
  /kit/dynasty — DEV BENCH for the SPRINT 1.3 dynasty picker modal. Not shipped
  UI. It loads real dynasties via the `scan_dynasties` backend command WHEN it is
  registered (the orchestrator registers dynasties.rs commands), falling back to
  sample data with a "not yet registered" note otherwise, then exercises every
  path of DynastyModal:

    • pick mode   — search, single-pick (check one + Use), and "New dynasty…"
                    free-text. The picked name is shown below.
    • manage mode — multi-select + mass delete with the confirm dialog. A
                    THROWAWAY EditQueue captures the generated composite; its
                    serialized TypedEdit[] is rendered as JSON so correctness can
                    be eyeballed.

  A separate sub-route (not kit/+page.svelte) so parallel agents extending the
  main bench don't collide here.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import DynastyModal, {
    type DynastyEntry,
  } from "$lib/components/DynastyModal.svelte";
  import { EditQueue } from "$lib/edits.svelte";

  // Sample data used when the backend command isn't available. Mirrors the
  // real payload shape (top-level + dated, monarch/heir/queen, path segments).
  const SAMPLE: DynastyEntry[] = [
    {
      name: "von Habsburg",
      count: 3,
      usages: [
        {
          tag: "HAB",
          file: "history/countries/HAB - Austria.txt",
          date: "1440.1.1",
          holder: "monarch",
          holderName: "Friedrich V",
          path: ["1440.1.1", "monarch"],
        },
        {
          tag: "HAB",
          file: "history/countries/HAB - Austria.txt",
          date: "1440.1.1",
          holder: "heir",
          holderName: "Maximilian",
          path: ["1440.1.1", "heir"],
        },
        {
          tag: "BOH",
          file: "history/countries/BOH - Bohemia.txt",
          date: "1526.1.1",
          holder: "monarch",
          holderName: "Ferdinand I",
          path: ["1526.1.1", "monarch"],
        },
      ],
    },
    {
      name: "de Valois",
      count: 2,
      usages: [
        {
          tag: "FRA",
          file: "history/countries/FRA - France.txt",
          date: "1422.10.21",
          holder: "monarch",
          holderName: "Charles VII",
          path: ["1422.10.21", "monarch"],
        },
        {
          tag: "FRA",
          file: "history/countries/FRA - France.txt",
          date: "1422.10.21",
          holder: "queen",
          holderName: "Marie",
          path: ["1422.10.21", "queen"],
        },
      ],
    },
    {
      name: "Jagiellon",
      count: 1,
      usages: [
        {
          tag: "POL",
          file: "history/countries/POL - Poland.txt",
          date: null,
          holder: "monarch",
          holderName: "Kazimierz IV",
          path: ["monarch"],
        },
      ],
    },
    {
      name: "de Avis",
      count: 1,
      usages: [
        {
          tag: "POR",
          file: "history/countries/POR - Portugal.txt",
          date: "1438.9.13",
          holder: "monarch",
          holderName: "Afonso V",
          path: ["1438.9.13", "monarch"],
        },
      ],
    },
  ];

  let installPath = $state("");
  let modPath = $state<string | null>(null);
  let dynasties = $state<DynastyEntry[]>(SAMPLE);
  let source = $state("loading…");

  // pick-mode bench
  let pickOpen = $state(false);
  let pickedName = $state<string | null>(null);

  // manage-mode bench: a throwaway queue we can inspect. serialize() reads the
  // queue's internal $state, so this $derived recomputes when a composite lands.
  let manageOpen = $state(false);
  const queue = new EditQueue();
  let editJson = $derived(JSON.stringify(queue.serialize(), null, 2));

  onMount(async () => {
    try {
      installPath = (await invoke<string | null>("get_saved_installation")) ?? "";
    } catch {
      installPath = "";
    }
    if (!installPath) {
      source = "no saved installation — using SAMPLE data";
      dynasties = SAMPLE;
      return;
    }
    try {
      const real = await invoke<DynastyEntry[]>("scan_dynasties", {
        installPath,
        modPath,
      });
      dynasties = real;
      source = `scan_dynasties: ${real.length} dynasties from the real install`;
    } catch (e) {
      dynasties = SAMPLE;
      source = `scan_dynasties not yet registered (${String(e)}) — using SAMPLE data`;
    }
  });

  function clearQueue() {
    queue.clear();
  }
</script>

<div class="bench">
  <h1>DynastyModal bench (SPRINT 1.3)</h1>
  <p class="source">{source}</p>
  <p class="meta">
    install: <code>{installPath || "(none)"}</code> · dynasties loaded:
    <code>{dynasties.length}</code>
  </p>

  <section>
    <h2>Pick mode</h2>
    <button onclick={() => (pickOpen = true)}>Open picker</button>
    <span class="picked">
      picked: <strong>{pickedName ?? "—"}</strong>
    </span>
    <p class="hint">
      Search, check a dynasty and press “Use selected”, or type a name and press
      “+ New”. Result flows to onpick (the caller would make the pending edit).
    </p>
  </section>

  <section>
    <h2>Manage mode (multi-select + mass delete)</h2>
    <button onclick={() => (manageOpen = true)}>Open manager</button>
    <button onclick={clearQueue} disabled={!queue.dirty}>Clear queue</button>
    <span class="picked">queued composites: <strong>{queue.canUndo ? "yes" : "none"}</strong></span>
    <p class="hint">
      Check several dynasties, press Delete, confirm. The generated composite
      lands in a throwaway EditQueue; its serialized edits appear below.
    </p>
    <h3>queue.serialize() → TypedEdit[]</h3>
    <pre>{editJson}</pre>
  </section>
</div>

<DynastyModal
  bind:open={pickOpen}
  mode="pick"
  {installPath}
  {modPath}
  {dynasties}
  onpick={(name) => (pickedName = name)}
/>

<DynastyModal
  bind:open={manageOpen}
  mode="manage"
  {installPath}
  {modPath}
  {dynasties}
  {queue}
  onpick={() => {}}
/>

<style>
  .bench {
    padding: 1.5rem;
    color: #cfd4db;
    background: #2b323d;
    min-height: 100vh;
    font-family:
      system-ui,
      -apple-system,
      sans-serif;
  }

  h1 {
    font-size: 1.2rem;
    margin: 0 0 0.5rem;
  }

  h2 {
    font-size: 1rem;
    margin: 1.4rem 0 0.5rem;
    border-bottom: 1px solid #3f4855;
    padding-bottom: 0.25rem;
  }

  h3 {
    font-size: 0.85rem;
    margin: 0.8rem 0 0.3rem;
    color: #8a919c;
  }

  .source {
    color: #9fd0a0;
    font-size: 0.85rem;
    margin: 0 0 0.25rem;
  }

  .meta {
    font-size: 0.82rem;
    color: #8a919c;
    margin: 0 0 0.5rem;
  }

  code {
    background: #21262e;
    padding: 0 0.25rem;
  }

  button {
    border: 1px solid #1f242c;
    background: #3f4855;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.35rem 0.8rem;
    cursor: pointer;
    margin-right: 0.4rem;
  }

  button:hover:not(:disabled) {
    background: #4a6da7;
    color: #ffffff;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .picked {
    font-size: 0.85rem;
    color: #8a919c;
  }

  .hint {
    font-size: 0.8rem;
    color: #8a919c;
    margin: 0.4rem 0 0;
  }

  pre {
    background: #21262e;
    border: 1px solid #1f242c;
    padding: 0.6rem;
    font-size: 0.78rem;
    overflow-x: auto;
    max-height: 24rem;
    overflow-y: auto;
    white-space: pre;
  }
</style>
