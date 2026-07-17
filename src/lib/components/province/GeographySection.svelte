<!--
  GeographySection — area, continent, trade-node, climate zone, winter severity,
  impassable, and terrain override (Sprint 2.2). These are membership-list edits
  in the shared map/ files (byte-surgical splices), not per-province history —
  moving the id between lists (steal semantics). Region/superregion are read-only
  rollups from area membership. Options come from `get_geo_options`.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { SearchDropdown } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import FieldRow from "$lib/components/country/FieldRow.svelte";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import { slugify, uniqueKey } from "$lib/geonet";
  import type { ProvinceDetails, GeoOptions, GeoOption } from "./types";
  import { membershipMove } from "./fields";

  let {
    installPath,
    modPath,
    details,
    queue,
    geo,
  }: {
    installPath: string;
    modPath: string | null;
    details: ProvinceDetails;
    queue: EditQueue;
    geo: GeoOptions | null;
  } = $props();

  const id = $derived(details.id);
  const g = $derived(details.geography);
  const NEW_CONTINENT = "__new_continent__";

  function samePath(a: string[], b: string[]): boolean {
    return a.length === b.length && a.every((v, i) => v === b[i]);
  }

  /** Current membership key for a slot, folding pending list edits over `base`. */
  function foldSlot(options: GeoOption[], base: string | null): string | null {
    let current = base;
    const sid = String(id);
    const match = (file: string, path: string[]) =>
      options.find((o) => o.file === file && samePath(o.list_path, path));
    for (const e of queue.serialize()) {
      if (e.kind === "listMove" && e.id === sid) {
        if (match(e.fromFile, e.fromPath) && current === match(e.fromFile, e.fromPath)!.key) current = null;
        const to = match(e.toFile, e.toPath);
        if (to) current = to.key;
      } else if (e.kind === "addId" && e.id === sid) {
        const o = match(e.file, e.listPath);
        if (o) current = o.key;
      } else if (e.kind === "removeId" && e.id === sid) {
        const o = match(e.file, e.listPath);
        if (o && current === o.key) current = null;
      }
    }
    return current;
  }

  function items(options: GeoOption[], eraser?: string): DropdownItem[] {
    const base = options.map((o) => ({ key: o.key, label: o.name }));
    return eraser ? [{ key: "__none__", label: eraser }, ...base] : base;
  }

  function change(options: GeoOption[], base: string | null, selected: string, human: string) {
    const target = selected === "__none__" ? null : selected;
    const current = foldSlot(options, base);
    if (current === target) return;
    const from = current ? options.find((o) => o.key === current) ?? null : null;
    const to = target ? options.find((o) => o.key === target) ?? null : null;
    const edits = membershipMove(id, from, to);
    if (edits.length) queue.push({ label: `Set ${human} of #${id}`, edits });
  }

  // --- Continent create / delete / rename (S3.1) ---
  // The continent list is derived (base options + pending create/delete + loc
  // rename) so a just-created continent shows in the dropdown and is selectable
  // before save. The move machinery is the same steal (AddId/RemoveId/ListMove).
  const continentFile = $derived(geo?.continent_file ?? "map/continent.txt");
  const effectiveContinents = $derived.by<GeoOption[]>(() => {
    if (!geo) return [];
    const opts: GeoOption[] = geo.continents.map((o) => ({ ...o, list_path: [...o.list_path] }));
    const byKey = new Map(opts.map((o) => [o.key, o] as const));
    for (const e of queue.serialize()) {
      if ((e.kind === "appendText" || e.kind === "createFile") && e.file === continentFile) {
        const k = /^\s*([A-Za-z0-9_]+)\s*=/.exec(e.text)?.[1];
        if (k && !byKey.has(k)) {
          const o: GeoOption = { key: k, name: k, file: continentFile, list_path: [k] };
          opts.push(o);
          byKey.set(k, o);
        }
      } else if (e.kind === "removeStatement" && e.file === continentFile && e.blockPath.length === 0) {
        const i = opts.findIndex((o) => o.key === e.key);
        if (i >= 0) {
          opts.splice(i, 1);
          byKey.delete(e.key);
        }
      } else if (e.kind === "locOverride") {
        const o = byKey.get(e.key);
        if (o) o.name = e.value;
      }
    }
    return opts;
  });

  // Continent inline create/rename input.
  let continentEditing = $state<"create" | "rename" | null>(null);
  let continentDraft = $state("");
  function startCreateContinent() {
    continentDraft = "";
    continentEditing = "create";
  }
  function startRenameContinent() {
    const cur = effectiveContinents.find((c) => c.key === continentKey);
    continentDraft = cur?.name ?? "";
    continentEditing = "rename";
  }
  function cancelContinentEdit() {
    continentEditing = null;
    continentDraft = "";
  }
  async function confirmContinentEdit() {
    const name = continentDraft.trim();
    if (!name) return;
    if (continentEditing === "rename") {
      if (continentKey) {
        queue.push({
          label: `Rename continent ${continentKey}`,
          edits: [{ kind: "locOverride", key: continentKey, value: name }],
          coalesceKey: `continentname:${continentKey}`,
        });
      }
      cancelContinentEdit();
      return;
    }
    // Create: unique key, empty block + AddId this province (steal from current).
    const exists = (k: string) => effectiveContinents.some((c) => c.key === k);
    const ckey = uniqueKey(slugify(name) + "_continent", exists);
    let stmt: string;
    try {
      stmt = await invoke<string>("scaffold_continent_block", { key: ckey, provinces: [] });
    } catch {
      stmt = `${ckey} = {\n\t\n}`;
    }
    const edits: TypedEdit[] = [];
    const from = continentKey ? effectiveContinents.find((c) => c.key === continentKey) : null;
    if (from) edits.push({ kind: "removeId", file: from.file, listPath: from.list_path, id: String(id) });
    edits.push({ kind: "appendText", file: continentFile, text: stmt });
    edits.push({ kind: "addId", file: continentFile, listPath: [ckey], id: String(id) });
    edits.push({ kind: "locOverride", key: ckey, value: name });
    queue.push({ label: `Create continent ${name}`, edits });
    cancelContinentEdit();
  }
  function deleteContinent() {
    const o = effectiveContinents.find((c) => c.key === continentKey);
    if (!o) return;
    if (!confirm(`Delete continent "${o.name}"?\n\nAll its provinces become unassigned from any continent.`))
      return;
    queue.push({
      label: `Delete continent ${o.key}`,
      edits: [{ kind: "removeStatement", file: o.file, blockPath: [], key: o.key }],
    });
    cancelContinentEdit();
  }

  function onContinentSelect(k: string) {
    if (k === NEW_CONTINENT) {
      startCreateContinent();
      return;
    }
    change(effectiveContinents, g.continent?.key ?? null, k, "continent");
  }

  const continentItems = $derived<DropdownItem[]>([
    { key: NEW_CONTINENT, label: "＋ Create new continent…" },
    ...effectiveContinents.map((o) => ({ key: o.key, label: o.name })),
  ]);

  // Slot current values (pending-aware).
  let areaKey = $derived(geo ? foldSlot(geo.areas, g.area?.key ?? null) : g.area?.key ?? null);
  let continentKey = $derived(geo ? foldSlot(effectiveContinents, g.continent?.key ?? null) : g.continent?.key ?? null);
  let nodeKey = $derived(geo ? foldSlot(geo.trade_nodes, g.trade_node?.key ?? null) : g.trade_node?.key ?? null);
  let climateKey = $derived(geo ? foldSlot(geo.climate_zones, g.climate?.key ?? null) : g.climate?.key ?? null);
  let winterKey = $derived(geo ? foldSlot(geo.winters, g.winter?.key ?? null) : g.winter?.key ?? null);
  let terrainKey = $derived(geo ? foldSlot(geo.terrains, g.terrain_override?.key ?? null) : g.terrain_override?.key ?? null);
  let impassable = $derived.by(() => {
    if (!geo) return g.impassable;
    let v = g.impassable;
    const sid = String(id);
    for (const e of queue.serialize()) {
      if ((e.kind === "addId" || e.kind === "removeId") && e.file === geo.impassable_file && samePath(e.listPath, ["impassable"]) && e.id === sid) {
        v = e.kind === "addId";
      }
    }
    return v;
  });

  function toggleImpassable() {
    if (!geo) return;
    const next = !impassable;
    queue.push({
      label: `${next ? "Set" : "Clear"} impassable on #${id}`,
      edits: [next
        ? { kind: "addId", file: geo.impassable_file, listPath: ["impassable"], id: String(id) }
        : { kind: "removeId", file: geo.impassable_file, listPath: ["impassable"], id: String(id) }],
    });
  }
</script>

<section>
  <h3>Geography</h3>
  {#if !geo}
    <p class="dim">Geography options unavailable.</p>
  {:else}
    <FieldRow label="Area" edited={areaKey !== (g.area?.key ?? null)}>
      <SearchDropdown items={items(geo.areas)} value={areaKey} placeholder="Area…" onselect={(k) => change(geo.areas, g.area?.key ?? null, k, "area")} />
    </FieldRow>
    <FieldRow label="Region"><span class="ro">{g.region?.name ?? "—"}</span></FieldRow>
    <FieldRow label="Superregion"><span class="ro">{g.superregion?.name ?? "—"}</span></FieldRow>

    <FieldRow label="Trade Node" edited={nodeKey !== (g.trade_node?.key ?? null)}>
      <SearchDropdown items={items(geo.trade_nodes, "(none)")} value={nodeKey ?? "__none__"} placeholder="Trade node…" onselect={(k) => change(geo.trade_nodes, g.trade_node?.key ?? null, k, "trade node")} />
    </FieldRow>

    <FieldRow label="Continent" edited={continentKey !== (g.continent?.key ?? null)}>
      <div class="cont-ctl">
        <div class="cont-dd">
          <SearchDropdown items={continentItems} value={continentKey} placeholder="Continent…" onselect={onContinentSelect} />
        </div>
        {#if continentKey}
          <button class="ico" title="Rename continent" aria-label="Rename continent" onclick={startRenameContinent}>✎</button>
          <button class="ico danger" title="Delete continent (its provinces become unassigned)" aria-label="Delete continent" onclick={deleteContinent}>🗑</button>
        {/if}
      </div>
    </FieldRow>
    {#if continentEditing}
      <div class="cont-new">
        <span class="cont-lead">{continentEditing === "create" ? "New continent" : "Rename"}</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="cont-in" bind:value={continentDraft} placeholder="Continent name"
          onkeydown={(e) => { if (e.key === 'Enter') confirmContinentEdit(); else if (e.key === 'Escape') cancelContinentEdit(); }} autofocus />
        <button class="ico" title="Apply" aria-label="Apply" onclick={confirmContinentEdit}>✓</button>
        <button class="ico" title="Cancel" aria-label="Cancel" onclick={cancelContinentEdit}>×</button>
      </div>
    {/if}

    <FieldRow label="Climate Zone" edited={climateKey !== (g.climate?.key ?? null)}>
      <SearchDropdown items={items(geo.climate_zones, "Temperate (none)")} value={climateKey ?? "__none__"} placeholder="Climate…" onselect={(k) => change(geo.climate_zones, g.climate?.key ?? null, k, "climate")} />
    </FieldRow>

    <FieldRow label="Winter Severity" edited={winterKey !== (g.winter?.key ?? null)}>
      <SearchDropdown items={items(geo.winters, "No winter (none)")} value={winterKey ?? "__none__"} placeholder="Winter…" onselect={(k) => change(geo.winters, g.winter?.key ?? null, k, "winter")} />
    </FieldRow>

    <FieldRow label="Impassable" edited={impassable !== g.impassable}>
      <label class="check"><input type="checkbox" checked={impassable} onchange={toggleImpassable} /><span>{impassable ? "Impassable (wasteland)" : "Passable"}</span></label>
    </FieldRow>

    <FieldRow label="Terrain Override" edited={terrainKey !== (g.terrain_override?.key ?? null)}>
      <SearchDropdown items={items(geo.terrains, "Auto (from terrain.bmp)")} value={terrainKey ?? "__none__"} placeholder="Terrain…" onselect={(k) => change(geo.terrains, g.terrain_override?.key ?? null, k, "terrain override")} />
    </FieldRow>
  {/if}
</section>

<style>
  section { margin-bottom: 1rem; }
  h3 { margin: 0 0 0.5rem; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; color: #9ca3af; }
  .ro { font-size: 0.85rem; color: #cfd4db; }
  .dim { color: #8a919c; font-size: 0.82rem; }
  .check { display: flex; align-items: center; gap: 0.4rem; font-size: 0.85rem; cursor: pointer; }
  .cont-ctl { display: flex; align-items: center; gap: 0.25rem; width: 100%; }
  .cont-dd { flex: 1; min-width: 0; }
  .cont-new { display: flex; align-items: center; gap: 0.25rem; margin: 0.3rem 0 0; }
  .cont-lead { font-size: 0.72rem; color: #9ca3af; white-space: nowrap; }
  .cont-in { flex: 1; min-width: 0; background: #14181d; border: 1px solid #4b5563; color: #cfd4db; font-family: inherit; font-size: 0.85rem; padding: 0.2rem 0.4rem; outline: none; }
  .cont-in:focus { border-color: #4a6da7; }
  .ico { border: 1px solid #4b5563; background: #2b323d; color: #cfd4db; font-family: inherit; font-size: 0.85rem; line-height: 1; padding: 0.2rem 0.4rem; cursor: pointer; }
  .ico:hover { border-color: #4a6da7; background: #4a6da7; color: #fff; }
  .ico.danger { color: #fca5a5; border-color: #6b3630; }
  .ico.danger:hover { background: #7a2820; border-color: #9a3226; color: #fff; }
</style>
