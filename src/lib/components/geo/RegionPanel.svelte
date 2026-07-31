<!--
  RegionPanel — Sprint 10.2 thin region panel.

  Reads the *effective* region (base + pending). Membership painting (which areas
  belong to this region) happens on the map with the area-granularity brush; this
  panel handles the name loc-override, the member-area list (jump links → select
  that area in Areas mode), the superregion dropdown (writes superregion.txt
  membership), the read-only monsoon note, and delete.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { SidePanel } from "$lib/components/ui";
  import FieldRow from "../country/FieldRow.svelte";
  import ValidationStrip, { type ValidationIssue, type JumpTarget } from "../ValidationStrip.svelte";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import { slugify, uniqueKey, type GeoNetwork, type GeoRegion, type MonsoonRange } from "$lib/geonet";

  let {
    installPath,
    modPath,
    queue,
    network,
    region,
    issues,
    onclose,
    onjumparea,
    onjump,
    ondeleted,
  }: {
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
    network: GeoNetwork;
    region: GeoRegion;
    issues: ValidationIssue[];
    onclose: () => void;
    /** Jump to an area (switch to Areas mode + select it). */
    onjumparea: (areaKey: string) => void;
    onjump: (j: JumpTarget) => void;
    ondeleted: () => void;
  } = $props();

  const key = $derived(region.key);
  const file = $derived(region.source_file || network.region_file);

  // --- Name (loc override; loc key IS the region key) ---
  const pendingName = $derived(queue.pendingLocOverride(key));
  const titleName = $derived(pendingName ?? region.name ?? key);
  function commitName(v: string) {
    queue.push({
      label: `Rename ${key}`,
      edits: [{ kind: "locOverride", key, value: v }],
      coalesceKey: `regionname:${key}`,
    });
  }

  const memberAreas = $derived(
    region.areas.map((ak) => ({ key: ak, area: network.areas.find((a) => a.key === ak) ?? null })),
  );

  // --- Superregion membership (writes superregion.txt) ---
  function setSuperregion(next: string) {
    const cur = region.superregion ?? "";
    if (next === cur) return;
    const sf = network.superregion_file;
    let edits;
    if (!cur) {
      edits = [{ kind: "addId" as const, file: sf, listPath: [next], id: key }];
    } else if (!next) {
      edits = [{ kind: "removeId" as const, file: sf, listPath: [cur], id: key }];
    } else {
      edits = [
        {
          kind: "listMove" as const,
          fromFile: sf,
          fromPath: [cur],
          toFile: sf,
          toPath: [next],
          id: key,
        },
      ];
    }
    queue.push({ label: `Set superregion of ${key}`, edits });
  }

  // --- Create / delete superregion (S3.1) ---
  // Mirrors the region create/delete UX: name prompt → scaffold a block into the
  // project's superregion.txt (+ loc override), with THIS region as first member;
  // delete removes the block so its regions become unassigned.
  let creatingSuper = $state(false);
  let superName = $state("");
  function startCreateSuper() {
    superName = "";
    creatingSuper = true;
  }
  function cancelCreateSuper() {
    creatingSuper = false;
    superName = "";
  }
  async function confirmCreateSuper() {
    const name = superName.trim();
    if (!name) return;
    const exists = (k: string) => network.superregions.some((s) => s.key === k);
    const superKey = uniqueKey(slugify(name) + "_superregion", exists);
    let stmt: string;
    try {
      stmt = await invoke<string>("scaffold_superregion_block", {
        key: superKey,
        firstRegion: key,
      });
    } catch {
      // Same formatting the backend scaffold produces (keeps save byte-clean).
      stmt = `${superKey} = {\n\t${key}\n}`;
    }
    const sf = network.superregion_file;
    const edits: TypedEdit[] = [];
    // Steal this region out of its current superregion first, if any.
    if (region.superregion) {
      edits.push({ kind: "removeId", file: sf, listPath: [region.superregion], id: key });
    }
    edits.push({ kind: "appendText", file: sf, text: stmt });
    edits.push({ kind: "locOverride", key: superKey, value: name });
    queue.push({ label: `Create superregion ${name}`, edits });
    cancelCreateSuper();
  }

  function deleteSuperregion() {
    const sr = region.superregion;
    if (!sr) return;
    const srObj = network.superregions.find((s) => s.key === sr);
    const cnt = srObj?.regions.length ?? 0;
    if (
      !confirm(
        `Delete superregion "${srObj?.name ?? sr}"?\n\nIts ${cnt} region(s) become unassigned.`,
      )
    )
      return;
    queue.push({
      label: `Delete superregion ${sr}`,
      edits: [{ kind: "removeStatement", file: network.superregion_file, blockPath: [], key: sr }],
    });
  }

  // --- Monsoon date-range editing (S2.6) ---
  // region.monsoon is the *effective* list (foldGeo already applied pending
  // edits), so occurrence indices below address the same order the backend sees.
  const monsoon = $derived(region.monsoon);

  function pad2(n: number): string {
    return String(Math.max(0, Math.floor(n))).padStart(2, "0");
  }
  /** month/day → game date "00.MM.DD" (season is year-agnostic; year is always 00). */
  function fmtDate(month: number, day: number): string {
    return `00.${pad2(month)}.${pad2(day)}`;
  }
  /** "00.MM.DD" (or "MM.DD") → {month, day}. */
  function parseMD(date: string): { month: number; day: number } {
    const parts = date.split(".").filter((p) => p.length > 0);
    if (parts.length >= 3) {
      return { month: parseInt(parts[1], 10) || 0, day: parseInt(parts[2], 10) || 0 };
    }
    if (parts.length === 2) {
      return { month: parseInt(parts[0], 10) || 0, day: parseInt(parts[1], 10) || 0 };
    }
    return { month: 0, day: 0 };
  }
  /** A month/day ordinal for ordering + overlap checks (monotonic within a year). */
  function ord(date: string): number {
    const { month, day } = parseMD(date);
    return month * 100 + day;
  }

  function commitMonsoon(idx: number, next: MonsoonRange) {
    queue.push({
      label: `Edit monsoon of ${key}`,
      edits: [{ kind: "setBlock", file, path: [key, `monsoon#${idx}`], value: `${next.start} ${next.end}` }],
      coalesceKey: `monsoon:${key}:${idx}`,
    });
  }
  function setStartMonth(idx: number, v: number) {
    const r = monsoon[idx];
    const d = parseMD(r.start);
    commitMonsoon(idx, { start: fmtDate(v, d.day), end: r.end });
  }
  function setStartDay(idx: number, v: number) {
    const r = monsoon[idx];
    const d = parseMD(r.start);
    commitMonsoon(idx, { start: fmtDate(d.month, v), end: r.end });
  }
  function setEndMonth(idx: number, v: number) {
    const r = monsoon[idx];
    const d = parseMD(r.end);
    commitMonsoon(idx, { start: r.start, end: fmtDate(v, d.day) });
  }
  function setEndDay(idx: number, v: number) {
    const r = monsoon[idx];
    const d = parseMD(r.end);
    commitMonsoon(idx, { start: r.start, end: fmtDate(d.month, v) });
  }
  function addMonsoon() {
    // Authored at column 0; InsertStatement re-indents to the region's depth.
    const stmt = "monsoon = {\n\t00.06.01\n\t00.09.30\n}";
    queue.push({
      label: `Add monsoon to ${key}`,
      edits: [{ kind: "insertStatement", file, blockPath: [key], statement: stmt }],
    });
  }
  function removeMonsoon(idx: number) {
    queue.push({
      label: `Remove monsoon from ${key}`,
      edits: [{ kind: "removeStatement", file, blockPath: [key], key: `monsoon#${idx}` }],
    });
  }

  // Validation (warn only — never blocks save): end after start per range, and
  // no two ranges overlap. Wrapping seasons are modeled as two separate blocks.
  const monsoonWarnings = $derived.by(() => {
    const w: string[] = [];
    monsoon.forEach((r, i) => {
      if (ord(r.end) < ord(r.start)) w.push(`Range ${i + 1}: end date is before its start.`);
    });
    for (let i = 0; i < monsoon.length; i++) {
      for (let j = i + 1; j < monsoon.length; j++) {
        const lo1 = Math.min(ord(monsoon[i].start), ord(monsoon[i].end));
        const hi1 = Math.max(ord(monsoon[i].start), ord(monsoon[i].end));
        const lo2 = Math.min(ord(monsoon[j].start), ord(monsoon[j].end));
        const hi2 = Math.max(ord(monsoon[j].start), ord(monsoon[j].end));
        if (lo1 <= hi2 && lo2 <= hi1) w.push(`Ranges ${i + 1} and ${j + 1} overlap.`);
      }
    }
    return w;
  });

  // --- Delete region ---
  function deleteRegion() {
    const msg = region.superregion
      ? `Delete region "${titleName}"?\n\nIts ${region.areas.length} area(s) become region-less, and it is removed from superregion "${region.superregion}".`
      : `Delete region "${titleName}"?\n\nIts ${region.areas.length} area(s) become region-less.`;
    if (!confirm(msg)) return;
    const edits = [];
    if (region.superregion) {
      edits.push({
        kind: "removeId" as const,
        file: network.superregion_file,
        listPath: [region.superregion],
        id: key,
      });
    }
    edits.push({ kind: "removeStatement" as const, file, blockPath: [], key });
    queue.push({ label: `Delete region ${key}`, edits });
    ondeleted();
  }
</script>

<SidePanel title={titleName} {onclose}>
  {#snippet header()}
    <div class="head">
      <span class="swatch" style="background: rgb({region.hash_color.join(',')})"></span>
      <span class="key-chip">{key}</span>
      {#if region.has_monsoon}<span class="badge">monsoon</span>{/if}
    </div>
  {/snippet}

  <div class="strip-wrap">
    <ValidationStrip {issues} {onjump} title="Geography" />
  </div>

  <section>
    <h3>Region</h3>
    <FieldRow label="Name" edited={pendingName !== undefined}>
      <input class="text" value={titleName} oninput={(e) => commitName((e.target as HTMLInputElement).value)} />
    </FieldRow>
    <FieldRow label="Key"><span class="mono">{key}</span></FieldRow>
    <FieldRow label="Superregion">
      <div class="sr-ctl">
        <select class="text" value={region.superregion ?? ""} onchange={(e) => setSuperregion((e.target as HTMLSelectElement).value)}>
          <option value="">— none —</option>
          {#each network.superregions as s (s.key)}
            <option value={s.key}>{s.name}</option>
          {/each}
        </select>
        <button class="ico" title="Create a new superregion with this region as its first member" onclick={startCreateSuper}>＋</button>
        {#if region.superregion}
          <button class="ico danger" title="Delete this superregion (its regions become unassigned)" aria-label="Delete superregion" onclick={deleteSuperregion}>🗑</button>
        {/if}
      </div>
    </FieldRow>
    {#if creatingSuper}
      <div class="sr-new">
        <span class="sr-lead">New superregion</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="text" bind:value={superName} placeholder="Superregion name"
          onkeydown={(e) => { if (e.key === 'Enter') confirmCreateSuper(); else if (e.key === 'Escape') cancelCreateSuper(); }} autofocus />
        <button class="ico" title="Create" aria-label="Create superregion" onclick={confirmCreateSuper}>✓</button>
        <button class="ico" title="Cancel" aria-label="Cancel" onclick={cancelCreateSuper}>×</button>
      </div>
    {/if}
  </section>

  <section>
    <h3>Member areas ({memberAreas.length})</h3>
    {#if memberAreas.length === 0}
      <p class="dim small">No areas — paint provinces to add their areas to this region.</p>
    {:else}
      <ul class="areas">
        {#each memberAreas as m (m.key)}
          <li class="area">
            <button class="link grow" onclick={() => onjumparea(m.key)} title="Select in Areas mode">{m.area?.name ?? m.key}</button>
            {#if m.area}<span class="tag">{m.area.provinces.length} prov</span>{/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section>
    <h3>Monsoon ({monsoon.length})</h3>
    <p class="dim small">Seasonal date ranges (month/day). A wrapping season uses two ranges.</p>
    {#if monsoon.length === 0}
      <p class="dim small">No monsoon season.</p>
    {:else}
      <ul class="monsoon">
        {#each monsoon as r, i (i)}
          {@const s = parseMD(r.start)}
          {@const e = parseMD(r.end)}
          <li class="mrow">
            <span class="mlead">#{i + 1}</span>
            <span class="date">
              <input class="md" type="number" min="1" max="12" value={s.month}
                oninput={(ev) => setStartMonth(i, parseInt((ev.target as HTMLInputElement).value, 10) || 0)} title="start month" />
              <span class="dot">.</span>
              <input class="md" type="number" min="1" max="31" value={s.day}
                oninput={(ev) => setStartDay(i, parseInt((ev.target as HTMLInputElement).value, 10) || 0)} title="start day" />
            </span>
            <span class="arrow">→</span>
            <span class="date">
              <input class="md" type="number" min="1" max="12" value={e.month}
                oninput={(ev) => setEndMonth(i, parseInt((ev.target as HTMLInputElement).value, 10) || 0)} title="end month" />
              <span class="dot">.</span>
              <input class="md" type="number" min="1" max="31" value={e.day}
                oninput={(ev) => setEndDay(i, parseInt((ev.target as HTMLInputElement).value, 10) || 0)} title="end day" />
            </span>
            <button class="mdel" aria-label="Remove range" title="Remove range" onclick={() => removeMonsoon(i)}>×</button>
          </li>
        {/each}
      </ul>
    {/if}
    {#if monsoonWarnings.length > 0}
      <ul class="warn">
        {#each monsoonWarnings as msg (msg)}<li>⚠ {msg}</li>{/each}
      </ul>
    {/if}
    <button class="btn wide" onclick={addMonsoon}>＋ Add range</button>
  </section>

  {#if region.raw_extra.length > 0}
    <section>
      <h3>Advanced (read-only)</h3>
      <p class="dim small">Preserved untouched on save.</p>
      <ul class="raw">
        {#each region.raw_extra as r (r)}<li><span class="mono">{r}</span></li>{/each}
      </ul>
    </section>
  {/if}

  <section class="hint">
    <p class="dim small">
      Paint provinces with the Add / Remove brush below to move their whole area in or out of this region. An area belongs to one region — painting steals it.
    </p>
  </section>

  <section>
    <button class="btn danger wide" onclick={deleteRegion}>Delete region…</button>
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
    border: 1px solid var(--border);
  }
  .key-chip {
    font-size: 0.8rem;
    color: var(--text-2);
  }
  .badge {
    font-size: 0.68rem;
    padding: 0.05rem 0.35rem;
    border: 1px solid var(--border);
    color: var(--text-inverse);
    background: var(--warn);
  }
  .strip-wrap {
    margin: -0.2rem 0 0.4rem;
  }
  section {
    padding: 0.4rem 0 0.6rem;
    border-bottom: 1px solid var(--bg-1);
  }
  section.hint {
    border-bottom: none;
  }
  h3 {
    margin: 0 0 0.4rem;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-2);
  }
  .text {
    width: 100%;
    background: var(--bg-0);
    border: 1px solid var(--border-strong);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.2rem 0.4rem;
  }
  .mono {
    font-family: ui-monospace, monospace;
    color: var(--text-2);
    font-size: 0.82rem;
  }
  .sr-ctl {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    width: 100%;
  }
  .sr-ctl .text {
    flex: 1;
  }
  .sr-new {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    margin-top: 0.3rem;
  }
  .sr-new .text {
    flex: 1;
  }
  .sr-lead {
    font-size: 0.72rem;
    color: var(--text-2);
    white-space: nowrap;
  }
  .ico {
    border: 1px solid var(--border-strong);
    background: var(--bg-2);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    line-height: 1;
    padding: 0.2rem 0.4rem;
    cursor: pointer;
  }
  .ico:hover {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--text-inverse);
  }
  .ico.danger {
    color: var(--err);
    border-color: var(--danger-bg);
  }
  .ico.danger:hover {
    background: var(--danger-bg);
    border-color: var(--danger-bg);
    color: var(--text-inverse);
  }
  .areas {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .area {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.12rem 0;
  }
  .grow {
    flex: 1;
    text-align: left;
  }
  .link {
    border: 1px solid var(--border-strong);
    background: var(--bg-2);
    color: var(--accent-text);
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.1rem 0.4rem;
    cursor: pointer;
  }
  .link:hover {
    border-color: var(--accent);
    color: var(--text-inverse);
  }
  .tag {
    font-size: 0.7rem;
    color: var(--text-2);
  }
  .raw {
    list-style: none;
    margin: 0;
    padding: 0;
    font-size: 0.8rem;
    color: var(--text-1);
  }
  .monsoon {
    list-style: none;
    margin: 0 0 0.4rem;
    padding: 0;
  }
  .mrow {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.12rem 0;
  }
  .mlead {
    font-size: 0.72rem;
    color: var(--text-2);
    width: 1.6rem;
  }
  .date {
    display: inline-flex;
    align-items: center;
    gap: 0.1rem;
  }
  .md {
    width: 2.6rem;
    background: var(--bg-0);
    border: 1px solid var(--border-strong);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.15rem 0.25rem;
    text-align: center;
  }
  .dot {
    color: var(--text-2);
  }
  .arrow {
    color: var(--text-2);
    font-size: 0.8rem;
  }
  .mdel {
    margin-left: auto;
    border: none;
    background: transparent;
    color: var(--text-2);
    font-size: 1.1rem;
    line-height: 1;
    cursor: pointer;
    padding: 0 0.2rem;
  }
  .mdel:hover {
    color: var(--err);
  }
  .warn {
    list-style: none;
    margin: 0 0 0.4rem;
    padding: 0;
    font-size: 0.74rem;
    color: var(--warn);
  }
  .warn li {
    padding: 0.05rem 0;
  }
  .btn {
    border: 1px solid var(--border-strong);
    background: transparent;
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.2rem 0.6rem;
    cursor: pointer;
  }
  .btn.wide {
    width: 100%;
  }
  .btn.danger {
    color: var(--err);
    border-color: var(--danger-bg);
  }
  .btn.danger:hover {
    background: var(--danger-bg);
    border-color: var(--danger-bg);
    color: var(--text-inverse);
  }
  .dim {
    color: var(--text-2);
  }
  .small {
    font-size: 0.76rem;
  }
</style>
