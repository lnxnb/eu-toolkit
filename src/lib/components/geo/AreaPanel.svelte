<!--
  AreaPanel — Sprint 10.1 thin area panel.

  Reads the *effective* area (base + pending) passed by MapView, so name/members
  reflect queued edits and undo/redo. Membership painting happens on the map via
  the brush; this panel handles the name loc-override, read-only rollups (parent
  region + superregion breadcrumb), the optional area.txt color note, and delete.
-->
<script lang="ts">
  import { SidePanel } from "$lib/components/ui";
  import FieldRow from "../country/FieldRow.svelte";
  import ValidationStrip, { type ValidationIssue, type JumpTarget } from "../ValidationStrip.svelte";
  import type { EditQueue } from "$lib/edits.svelte";
  import type { GeoNetwork, GeoArea } from "$lib/geonet";

  let {
    queue,
    network,
    area,
    issues,
    onclose,
    onjump,
    ondeleted,
  }: {
    queue: EditQueue;
    network: GeoNetwork;
    area: GeoArea;
    issues: ValidationIssue[];
    onclose: () => void;
    onjump: (j: JumpTarget) => void;
    ondeleted: () => void;
  } = $props();

  const key = $derived(area.key);
  const file = $derived(area.source_file || network.area_file);

  // --- Name (loc override; loc key IS the area key) ---
  const pendingName = $derived(queue.pendingLocOverride(key));
  const titleName = $derived(pendingName ?? area.name ?? key);
  function commitName(v: string) {
    queue.push({
      label: `Rename ${key}`,
      edits: [{ kind: "locOverride", key, value: v }],
      coalesceKey: `areaname:${key}`,
    });
  }

  // --- Rollups (read-only here; region editable from Regions mode) ---
  const region = $derived(area.region ? network.regions.find((r) => r.key === area.region) ?? null : null);
  const superregion = $derived(
    region?.superregion ? network.superregions.find((s) => s.key === region!.superregion) ?? null : null,
  );

  const swatch = $derived(area.color ?? area.hash_color);
  function css(c: [number, number, number]): string {
    return `rgb(${c[0]}, ${c[1]}, ${c[2]})`;
  }

  // --- Delete area ---
  function deleteArea() {
    const msg = area.region
      ? `Delete area "${titleName}"?\n\nIts ${area.provinces.length} province(s) become unassigned, and it is removed from region "${area.region}".`
      : `Delete area "${titleName}"?\n\nIts ${area.provinces.length} province(s) become unassigned.`;
    if (!confirm(msg)) return;
    const edits = [];
    // Must first be removed from any region (region.areas is in region_file).
    if (area.region) {
      edits.push({
        kind: "removeId" as const,
        file: network.region_file,
        listPath: [area.region, "areas"],
        id: key,
      });
    }
    edits.push({
      kind: "removeStatement" as const,
      file,
      blockPath: [],
      key,
    });
    queue.push({ label: `Delete area ${key}`, edits });
    ondeleted();
  }
</script>

<SidePanel title={titleName} {onclose}>
  {#snippet header()}
    <div class="head">
      <span class="swatch" style="background: {css(swatch)}"></span>
      <span class="key-chip">{key}</span>
    </div>
  {/snippet}

  <div class="strip-wrap">
    <ValidationStrip {issues} {onjump} title="Geography" />
  </div>

  <section>
    <h3>Area</h3>
    <FieldRow label="Name" edited={pendingName !== undefined}>
      <input class="text" value={titleName} oninput={(e) => commitName((e.target as HTMLInputElement).value)} />
    </FieldRow>
    <FieldRow label="Key"><span class="mono">{key}</span></FieldRow>
    <FieldRow label="Color">
      <span class="swatch small" style="background: {css(swatch)}"></span>
      <span class="mono">{area.color ? `${area.color.join(" ")} (area.txt)` : "hash (no color in area.txt)"}</span>
    </FieldRow>
    <FieldRow label="Provinces"><span>{area.provinces.length}</span></FieldRow>
    <FieldRow label="Region">
      {#if region}
        <span class="mono">{region.name}</span>
        <span class="tag">(edit membership in Regions mode)</span>
      {:else}
        <span class="warn">— none —</span>
      {/if}
    </FieldRow>
    <FieldRow label="Superregion">
      <span class="mono">{superregion ? superregion.name : "—"}</span>
    </FieldRow>
  </section>

  <section class="hint">
    <p class="dim small">
      Paint provinces into this area with the Add / Remove brush below. A province belongs to exactly one area — painting steals it from its previous area.
    </p>
  </section>

  <section>
    <button class="btn danger wide" onclick={deleteArea}>Delete area…</button>
  </section>
</SidePanel>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .swatch {
    width: 1rem;
    height: 1rem;
    display: inline-block;
    border: 1px solid #1f242c;
  }
  .swatch.small {
    width: 0.8rem;
    height: 0.8rem;
  }
  .key-chip {
    font-size: 0.8rem;
    color: #9ca3af;
  }
  .strip-wrap {
    margin: -0.2rem 0 0.4rem;
  }
  section {
    padding: 0.4rem 0 0.6rem;
    border-bottom: 1px solid #232a33;
  }
  section.hint {
    border-bottom: none;
  }
  h3 {
    margin: 0 0 0.4rem;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #9ca3af;
  }
  .text {
    width: 100%;
    background: #14181d;
    border: 1px solid #4b5563;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.2rem 0.4rem;
  }
  .mono {
    font-family: ui-monospace, monospace;
    color: #9ca3af;
    font-size: 0.82rem;
  }
  .warn {
    color: #d8a020;
    font-size: 0.82rem;
  }
  .tag {
    font-size: 0.7rem;
    color: #6b7280;
  }
  .btn {
    border: 1px solid #4b5563;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.2rem 0.6rem;
    cursor: pointer;
  }
  .btn.wide {
    width: 100%;
  }
  .btn.danger {
    color: #fca5a5;
    border-color: #6b3630;
  }
  .btn.danger:hover {
    background: #7a2820;
    border-color: #9a3226;
    color: #fff;
  }
  .dim {
    color: #9ca3af;
  }
  .small {
    font-size: 0.76rem;
  }
</style>
