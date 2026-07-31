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
  import { TabStrip } from "$lib/components/workspace";
  import type { EditQueue } from "$lib/edits.svelte";
  import TechTable from "./TechTable.svelte";
  import TechGroupsTable from "./TechGroupsTable.svelte";
  import UnitsTable from "./UnitsTable.svelte";
  import type { TechData, Unit } from "$lib/technology";
  import type { KnownModifier } from "$lib/components/ui";

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
  let knownModifiers = $state<KnownModifier[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let tab = $state<string>("mil");
  // Set when a tech level's unit chip is clicked: switch to Units and reveal it.
  let unitFocus = $state<string | null>(null);

  function openUnit(key: string) {
    unitFocus = key;
    tab = "units";
  }

  $effect(() => {
    if (!open) return;
    void load(installPath, modPath);
  });

  async function load(install: string, mod: string | null) {
    loading = true;
    error = null;
    try {
      const [td, us, km] = await Promise.all([
        invoke<TechData>("get_technologies", { installPath: install, modPath: mod }),
        invoke<Unit[]>("get_units", { installPath: install, modPath: mod }),
        // Fetched once here and passed down: the modifier picker is the same
        // registry every other modifier editor uses.
        invoke<KnownModifier[]>("get_known_modifiers"),
      ]);
      data = td;
      units = us;
      knownModifiers = km;
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
    <TabStrip tier="content" tabs={[...tables.map((t) => ({id:t.kind,label:t.label})),{id:"groups",label:"Tech Groups"},{id:"units",label:"Units"}]} activeId={tab} onselect={(id) => tab = id} />
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
        <UnitsTable baseUnits={units} {tables} {installPath} {modPath} {queue}
          focusKey={unitFocus} onfocused={() => (unitFocus = null)} />
      {:else if currentTable}
        <TechTable table={currentTable} {queue} {installPath} {modPath} {knownModifiers} onopenunit={openUnit} />
      {:else}
        <p class="msg">No tech table for this power.</p>
      {/if}
    {/if}
  </div>
</OverlaySurface>

<style>
  .body { display: flex; flex-direction: column; gap: 0.5rem; }
  .msg { margin: 0.2rem 0; font-size: 0.85rem; color: var(--text-2); }
  .msg.err { color: var(--err); }
</style>
