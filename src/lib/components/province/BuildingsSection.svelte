<!--
  BuildingsSection — 1444 buildings as a searchable checkbox list (Sprint 2.2).
  Each building is a `<building> = yes` scalar; toggling writes an insert/remove.
  Mutually-exclusive fort tiers (fort_15th…fort_18th) are validated inline (warn,
  never block) — the game only honours one fort per province.
-->
<script lang="ts">
  import { ListSection } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue } from "$lib/edits.svelte";
  import type { ProvinceDetails, ProvinceSnapshot } from "./types";
  import { toggleFlag, pushAtDate, type DateCtx } from "./fields";

  let {
    details,
    effective,
    file,
    queue,
    buildings,
    dateCtx,
  }: {
    details: ProvinceDetails;
    effective: ProvinceSnapshot;
    file: string;
    queue: EditQueue;
    buildings: DropdownItem[];
    /** Sprint 12.3 date context; later dates write into a dated block. */
    dateCtx?: DateCtx;
  } = $props();

  const top = $derived(details.top_level);
  let search = $state("");

  function isOn(key: string): boolean {
    const p = queue.pendingField(file, key);
    if (p !== undefined) return p.value === "yes";
    return effective.buildings.includes(key);
  }
  function toggle(key: string) {
    const next = !isOn(key);
    const label = `${next ? "Build" : "Demolish"} ${key} in #${details.id}`;
    if (next) {
      // Building at a later date = `<building> = yes` in that date's block.
      pushAtDate(queue, dateCtx, label, [toggleFlag(file, key, true, top.buildings.includes(key))], [`${key} = yes`]);
    } else {
      queue.push({ label, edits: [toggleFlag(file, key, false, top.buildings.includes(key))] });
    }
  }

  let active = $derived(buildings.filter((b) => isOn(b.key)).map((b) => b.key));
  let fortConflict = $derived(active.filter((k) => /^fort_\d/.test(k)).length > 1);

  let filtered = $derived.by(() => {
    const q = search.trim().toLowerCase();
    // Active buildings float to the top; then the rest, filtered by search.
    const rows = buildings.filter((b) => !q || b.label.toLowerCase().includes(q) || b.key.toLowerCase().includes(q));
    return rows.sort((a, b) => Number(isOn(b.key)) - Number(isOn(a.key)) || a.label.localeCompare(b.label));
  });
</script>

<section>
  <h3>Buildings</h3>
  {#if fortConflict}
    <p class="warn">⚠ More than one fort tier is active — the game only uses one per province.</p>
  {/if}
  <ListSection title="Buildings" count={active.length} maxHeight="14rem">
    {#snippet actions()}
      <input class="srch" placeholder="Search…" bind:value={search} />
    {/snippet}
    {#each filtered as b (b.key)}
      <label class="row" class:on={isOn(b.key)}>
        <input type="checkbox" checked={isOn(b.key)} onchange={() => toggle(b.key)} />
        <span class="nm">{b.label}</span>
      </label>
    {/each}
  </ListSection>
</section>

<style>
  section { margin-bottom: 1rem; }
  h3 { margin: 0 0 0.5rem; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; color: #9ca3af; }
  .warn { margin: 0 0 0.4rem; color: #fbbf24; font-size: 0.78rem; }
  .srch { width: 8rem; background: #21262e; border: 1px solid #1f242c; color: #cfd4db; font-family: inherit; font-size: 0.78rem; padding: 0.1rem 0.35rem; outline: none; }
  .row { display: flex; align-items: center; gap: 0.4rem; padding: 0.15rem 0.25rem; font-size: 0.82rem; cursor: pointer; }
  .row.on { color: #fff; }
  .row:hover { background: #2f3742; }
  .nm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
