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
    border-bottom: 1px solid #1f242c;
  }
  .lock-key {
    flex: 1;
    font-size: 0.83rem;
    color: #cfd4db;
  }
  .lock-val {
    font-size: 0.8rem;
    padding: 0.1rem 0.4rem;
    background: #21262e;
    border: 1px solid #1f242c;
    color: #9ca3af;
  }
  .lock-val.on {
    background: #35506b;
    color: #cfe3f5;
  }
  .lock-tag {
    font-size: 0.62rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: #8a919c;
    background: #262c35;
    padding: 0.08rem 0.3rem;
    border: 1px solid #1f242c;
  }
</style>
