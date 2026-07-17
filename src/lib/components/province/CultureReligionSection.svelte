<!--
  CultureReligionSection — culture, religion, and reformation-centre toggle
  (Sprint 2.2). Verified key: `reformation_center = <religion>` (a top-level
  scalar naming the religion whose reformation this province seeds, e.g.
  `reformation_center = protestant`).
-->
<script lang="ts">
  import { SearchDropdown } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import FieldRow from "$lib/components/country/FieldRow.svelte";
  import type { EditQueue } from "$lib/edits.svelte";
  import type { ProvinceDetails, ProvinceSnapshot } from "./types";
  import { fieldOps, type DateCtx } from "./fields";

  let {
    details,
    effective,
    file,
    queue,
    cultures,
    religions,
    dateCtx,
  }: {
    details: ProvinceDetails;
    effective: ProvinceSnapshot;
    file: string;
    queue: EditQueue;
    cultures: DropdownItem[];
    religions: DropdownItem[];
    /** Sprint 12.3 date context; later dates write into a dated block. */
    dateCtx?: DateCtx;
  } = $props();

  const ops = $derived(fieldOps(queue, file, dateCtx));
  const top = $derived(details.top_level);

  let cultureVal = $derived(ops.val("culture", effective.culture));
  let religionVal = $derived(ops.val("religion", effective.religion));

  let reformField = $derived(queue.pendingField(file, "reformation_center"));
  let reformVal = $derived(reformField !== undefined ? reformField.value : effective.reformation_center);
  let reformOn = $derived(reformVal != null && reformVal !== "");

  function toggleReform() {
    if (reformOn) {
      queue.push({ label: `Clear reformation centre of #${details.id}`, edits: [{ kind: "removeStatement", file, blockPath: [], key: "reformation_center" }] });
    } else {
      const rel = religionVal ?? "protestant";
      ops.set("reformation_center", top.reformation_center != null, rel, `Set reformation centre of #${details.id}`);
    }
  }
</script>

<section>
  <h3>Culture &amp; Religion</h3>

  <FieldRow label="Culture" edited={ops.edited("culture", effective.culture)}>
    <SearchDropdown items={cultures} value={cultureVal} placeholder="Culture…" onselect={(k) => ops.set("culture", top.culture != null, k, `Set culture of #${details.id}`)} />
  </FieldRow>

  <FieldRow label="Religion" edited={ops.edited("religion", effective.religion)}>
    <SearchDropdown items={religions} value={religionVal} placeholder="Religion…" onselect={(k) => ops.set("religion", top.religion != null, k, `Set religion of #${details.id}`)} />
  </FieldRow>

  <FieldRow label="Reformation Centre" edited={reformField !== undefined}>
    <label class="check"><input type="checkbox" checked={reformOn} onchange={toggleReform} /><span>{reformOn ? "Centre of Reformation" : "No"}</span></label>
  </FieldRow>
  {#if reformOn}
    <FieldRow label="Reformation Religion">
      <SearchDropdown items={religions} value={reformVal} placeholder="Religion…" onselect={(k) => ops.set("reformation_center", top.reformation_center != null, k, `Set reformation religion of #${details.id}`)} />
    </FieldRow>
  {/if}
</section>

<style>
  section { margin-bottom: 1rem; }
  h3 { margin: 0 0 0.5rem; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; color: #9ca3af; }
  .check { display: flex; align-items: center; gap: 0.4rem; font-size: 0.85rem; cursor: pointer; }
</style>
