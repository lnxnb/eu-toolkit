<!--
  DiscoverySection — `discovered_by` tech groups (Sprint 2.2). Repeated top-level
  statements; add/remove each. Options come from the technology_groups registry,
  but any base value not in the registry (e.g. a mod's custom group, or a tag from
  a dated grant) still renders so it can be removed.
-->
<script lang="ts">
  import { SearchDropdown } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue } from "$lib/edits.svelte";
  import type { ProvinceDetails, ProvinceSnapshot } from "./types";
  import { listAdd, listRemove, pushAtDate, type DateCtx } from "./fields";

  let {
    details,
    effective,
    file,
    queue,
    techGroups,
    dateCtx,
  }: {
    details: ProvinceDetails;
    effective: ProvinceSnapshot;
    file: string;
    queue: EditQueue;
    techGroups: DropdownItem[];
    /** Sprint 12.3 date context; later dates write into a dated block. */
    dateCtx?: DateCtx;
  } = $props();

  let discovered = $derived(queue.pendingList(file, "discovered_by", effective.discovered_by));
  function label(key: string): string {
    return techGroups.find((t) => t.key === key)?.label ?? key;
  }
  function add(key: string) {
    if (!key || discovered.includes(key)) return;
    pushAtDate(queue, dateCtx, `Add discovered_by ${key} to #${details.id}`, [listAdd(file, "discovered_by", key)], [`discovered_by = ${key}`]);
  }
  function remove(key: string) {
    queue.push({ label: `Remove discovered_by ${key} from #${details.id}`, edits: [listRemove(file, "discovered_by", key)] });
  }
</script>

<section>
  <h3>Discovery</h3>
  <div class="list-field">
    {#each discovered as t (t)}
      <span class="chip">{label(t)}<button class="x" onclick={() => remove(t)}>×</button></span>
    {/each}
    <SearchDropdown items={techGroups} value={null} placeholder="Add tech group…" onselect={add} />
  </div>
</section>

<style>
  section { margin-bottom: 1rem; }
  h3 { margin: 0 0 0.5rem; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; color: #9ca3af; }
  .list-field { display: flex; flex-direction: column; gap: 0.3rem; }
  .chip { display: inline-flex; align-items: center; gap: 0.3rem; align-self: flex-start; background: #21262e; border: 1px solid #1f242c; color: #cfd4db; font-size: 0.8rem; padding: 0.12rem 0.2rem 0.12rem 0.45rem; }
  .x { border: none; background: transparent; color: #9ca3af; cursor: pointer; font-size: 0.95rem; line-height: 1; padding: 0 0.2rem; }
  .x:hover { color: #fca5a5; }
</style>
