<!--
  TerrainPropertiesEditor — S2.7 editable terrain-category properties. Mirrors
  religion/ModifierBlock: owns the ModifierEditor row array, initialized ONCE
  from the on-disk category so pending edits aren't clobbered, and commits the
  whole row set on every change. `is_water` is shown but LOCKED (flipping it
  breaks the water/land map-classification invariants). Everything else in the
  category block (type, sound_type, terrain_override, AI keys) is preserve-unknown
  and never touched by the diff. Remount via {#key category.key} in the parent so
  a new selection re-seeds the rows.
-->
<script lang="ts">
  import { ModifierEditor } from "$lib/components/ui";
  import type { ModifierRow } from "$lib/components/ui";
  import { TERRAIN_MODIFIERS, terrainModifierRows, type TerrainCategory } from "./types";

  let {
    category,
    oncommit,
  }: {
    category: TerrainCategory;
    oncommit: (rows: ModifierRow[]) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let rows = $state<ModifierRow[]>(terrainModifierRows(category));
</script>

<div class="props">
  <div class="lock-row" title="Water/land classification is fixed — editing it would break the map">
    <span class="lock-key">is_water</span>
    <span class="lock-val" class:on={category.isWater}>{category.isWater ? "yes" : "no"}</span>
    <span class="lock-tag">locked</span>
  </div>
  <ModifierEditor bind:modifiers={rows} known={TERRAIN_MODIFIERS} onchange={(r) => oncommit(r)} />
</div>

<style>
  .props {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .lock-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding-bottom: 0.35rem;
    border-bottom: 1px solid var(--border);
  }
  .lock-key {
    flex: 1;
    font-size: 0.83rem;
    color: var(--text-1);
  }
  .lock-val {
    font-size: 0.8rem;
    padding: 0.1rem 0.4rem;
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-2);
  }
  .lock-val.on {
    background: var(--accent);
    color: var(--accent-text);
  }
  .lock-tag {
    font-size: 0.62rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
    background: var(--bg-2);
    padding: 0.08rem 0.3rem;
    border: 1px solid var(--border);
  }
</style>
