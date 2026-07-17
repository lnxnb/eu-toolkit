<!--
  TechnologyOverlay — View ▸ Technology… (Sprint 22).

  A full-screen OverlaySurface with tabs: ADM / DIP / MIL tech tables (rows =
  tech levels: editable year + modifiers, read-only unlock/unit chips, append
  level), Tech Groups (start_level / start_cost_modifier table), and Units (pip
  editor + create-unit + units-domain validation). All edits go through the
  shared typed edit queue, byte-surgical.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { OverlaySurface } from "$lib/components/script";
  import type { EditQueue } from "$lib/edits.svelte";
  import TechTable from "./TechTable.svelte";
  import TechGroupsTable from "./TechGroupsTable.svelte";
  import UnitsTable from "./UnitsTable.svelte";
  import type { TechData, Unit } from "$lib/technology";

  let {
    open = $bindable(false),
    installPath,
    modPath = null,
    queue,
  }: {
    open?: boolean;
    installPath: string;
    modPath?: string | null;
    queue: EditQueue;
  } = $props();

  let data = $state<TechData | null>(null);
  let units = $state<Unit[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let tab = $state<string>("mil");

  $effect(() => {
    if (!open) return;
    void load(installPath, modPath);
  });

  async function load(install: string, mod: string | null) {
    loading = true;
    error = null;
    try {
      const [td, us] = await Promise.all([
        invoke<TechData>("get_technologies", { installPath: install, modPath: mod }),
        invoke<Unit[]>("get_units", { installPath: install, modPath: mod }),
      ]);
      data = td;
      units = us;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  const tables = $derived(data?.tables ?? []);
  const currentTable = $derived(tables.find((t) => t.kind === tab) ?? null);
</script>

<OverlaySurface bind:open title="Technology & Units">
  {#snippet toolbar()}
    <div class="tabs">
      {#each tables as t (t.kind)}
        <button class="tab" class:active={tab === t.kind} onclick={() => (tab = t.kind)}>{t.label}</button>
      {/each}
      <button class="tab" class:active={tab === "groups"} onclick={() => (tab = "groups")}>Tech Groups</button>
      <button class="tab" class:active={tab === "units"} onclick={() => (tab = "units")}>Units</button>
    </div>
  {/snippet}

  <div class="body">
    {#if loading}
      <p class="msg">Loading technology…</p>
    {:else if error}
      <p class="msg err">{error}</p>
    {:else if data}
      {#if tab === "groups"}
        <TechGroupsTable groups={data.groups} {queue} />
      {:else if tab === "units"}
        <UnitsTable baseUnits={units} {tables} {installPath} {modPath} {queue} />
      {:else if currentTable}
        <TechTable table={currentTable} {queue} />
      {:else}
        <p class="msg">No tech table for this power.</p>
      {/if}
    {/if}
  </div>
</OverlaySurface>

<style>
  .tabs { display: flex; gap: 0.15rem; }
  .tab { border: 1px solid #1f242c; background: #21262e; color: #cfd4db; font-family: inherit; font-size: 0.8rem; padding: 0.22rem 0.7rem; cursor: pointer; }
  .tab:hover { background: #303844; }
  .tab.active { background: #4a6da7; color: #fff; border-color: #4a6da7; }
  .body { display: flex; flex-direction: column; gap: 0.5rem; }
  .msg { margin: 0.2rem 0; font-size: 0.85rem; color: #8a919c; }
  .msg.err { color: #d9756b; }
</style>
