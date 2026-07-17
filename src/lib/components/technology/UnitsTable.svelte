<script lang="ts">
  // Unit stats table: search + category filter; rows show name/category/graphical
  // group/arrival tech level/total pips. Expand a row to edit its pips (land) or
  // ship stats (byte-surgical SetScalar). "＋ New unit" scaffolds a project
  // common/units/<key>.txt + an `enable` registration into the chosen tech level
  // (ONE composite, atomic) + loc. A units-domain validation strip surfaces
  // pip-budget outliers and units enabled by no tech level.
  import { invoke } from "@tauri-apps/api/core";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import {
    foldUnits,
    allUnitKeys,
    isValidKey,
    slugify,
    liveUnitStat,
    LAND_CATEGORIES,
    SHIP_CATEGORIES,
    type Unit,
    type TechTable,
    type UnitScaffold,
  } from "$lib/technology";

  let {
    baseUnits,
    tables,
    installPath,
    modPath = null,
    queue,
  }: {
    baseUnits: Unit[];
    tables: TechTable[];
    installPath: string;
    modPath?: string | null;
    queue: EditQueue;
  } = $props();

  interface Issue {
    severity: string;
    message: string;
  }

  let search = $state("");
  let catFilter = $state<string>("all");
  let expanded = $state<string | null>(null);
  let issues = $state<Issue[]>([]);

  // Create form state.
  let newName = $state("");
  let newCategory = $state("infantry");
  let newUnitType = $state("western");
  let newLevel = $state<number>(0);
  let newError = $state<string | null>(null);

  const units = $derived<Unit[]>(((queue.version, foldUnits(baseUnits, queue.serialize()))));
  const keys = $derived(allUnitKeys(units));

  const shown = $derived(
    units.filter((u) => {
      if (catFilter !== "all" && u.category !== catFilter) return false;
      const q = search.trim().toLowerCase();
      if (!q) return true;
      return u.key.toLowerCase().includes(q) || u.name.toLowerCase().includes(q);
    }),
  );

  const categories = $derived([...new Set(units.map((u) => u.category))].filter(Boolean).sort());

  // The tech table a new unit of the current category registers into.
  const targetTable = $derived<TechTable | undefined>(
    LAND_CATEGORIES.includes(newCategory)
      ? tables.find((t) => t.kind === "mil")
      : tables.find((t) => t.kind === "dip"),
  );

  // Validation reads the SAVED files (disk), so it only needs to run when the
  // session changes — not on every pending keystroke.
  $effect(() => {
    void runValidation(installPath, modPath);
  });

  async function runValidation(install: string, mod: string | null) {
    try {
      issues = await invoke<Issue[]>("validate", { domain: "units", installPath: install, modPath: mod, date: null });
    } catch {
      issues = [];
    }
  }

  function toggle(k: string) {
    expanded = expanded === k ? null : k;
  }

  function arrivalLabel(u: Unit): string {
    if (u.arrivesLevel == null) return "—";
    return `${(u.arrivesTech ?? "").toUpperCase()} ${u.arrivesLevel}`;
  }

  function commitStat(u: Unit, key: string, value: string) {
    queue.push({
      label: `Edit ${key} of ${u.key}`,
      edits: [{ kind: "setScalar", file: u.file, path: [key], value, quoted: false }],
      coalesceKey: `unitstat:${u.file}:${key}`,
    });
  }

  function liveTotal(u: Unit): number {
    if (!u.isLand) return 0;
    return u.pips.reduce((a, p) => a + (Number(liveUnitStat(queue, u, p.key, p.value)) || 0), 0);
  }

  async function createUnit() {
    newError = null;
    const key = slugify(newName.trim());
    if (!isValidKey(key)) {
      newError = "Use lowercase letters, digits and underscores (start with a letter).";
      return;
    }
    if (keys.has(key)) {
      newError = `A unit named "${key}" already exists.`;
      return;
    }
    if (!targetTable) {
      newError = `No ${LAND_CATEGORIES.includes(newCategory) ? "MIL" : "DIP"} tech file to register into.`;
      return;
    }
    let scaffold: UnitScaffold;
    try {
      scaffold = await invoke<UnitScaffold>("scaffold_unit_file", {
        key,
        category: newCategory,
        unitType: LAND_CATEGORIES.includes(newCategory) ? newUnitType.trim() : "",
      });
    } catch (e) {
      newError = String(e);
      return;
    }
    const edits: TypedEdit[] = [
      { kind: "createFile", file: scaffold.file, text: scaffold.text },
      {
        kind: "insertStatement",
        file: targetTable.file,
        blockPath: [`technology#${newLevel}`],
        statement: `enable = ${key}`,
      },
    ];
    for (const le of scaffold.locEntries) edits.push({ kind: "locOverride", key: le.key, value: le.value });
    queue.push({ label: `Create unit ${key}`, edits });
    newName = "";
    expanded = key;
  }
</script>

<div class="units">
  <!-- Create -->
  <div class="createbox">
    <div class="crow">
      <input class="txt" placeholder="New unit name…" bind:value={newName}
        onkeydown={(e) => e.key === 'Enter' && createUnit()} />
      <select class="sel" bind:value={newCategory}>
        <optgroup label="Land">
          {#each LAND_CATEGORIES as c (c)}<option value={c}>{c}</option>{/each}
        </optgroup>
        <optgroup label="Naval">
          {#each SHIP_CATEGORIES as c (c)}<option value={c}>{c}</option>{/each}
        </optgroup>
      </select>
      {#if LAND_CATEGORIES.includes(newCategory)}
        <input class="txt narrow" placeholder="unit_type (graphical)" bind:value={newUnitType} />
      {/if}
      {#if targetTable}
        <select class="sel" bind:value={newLevel}>
          {#each targetTable.levels as lvl (lvl.index)}
            <option value={lvl.index}>{targetTable.kind.toUpperCase()} Tech {lvl.index}{lvl.year ? ` (${lvl.year})` : ""}</option>
          {/each}
        </select>
      {/if}
      <button class="btn" onclick={createUnit}>＋ New unit</button>
    </div>
    {#if newError}<div class="err">{newError}</div>{/if}
  </div>

  <!-- Validation strip -->
  {#if issues.length}
    <div class="valid">
      {#each issues as iss, i (i)}
        <div class="issue {iss.severity}"><span class="sev">{iss.severity}</span>{iss.message}</div>
      {/each}
    </div>
  {/if}

  <!-- Filters -->
  <div class="filters">
    <input class="txt" placeholder="Search units…" bind:value={search} />
    <select class="sel" bind:value={catFilter}>
      <option value="all">All categories</option>
      {#each categories as c (c)}<option value={c}>{c}</option>{/each}
    </select>
    <span class="counter">{shown.length}</span>
  </div>

  <table class="tbl">
    <thead>
      <tr><th></th><th>Unit</th><th>Category</th><th>Graphical</th><th>Arrives</th><th>Total pips</th></tr>
    </thead>
    <tbody>
      {#each shown as u (u.file + "::" + u.key)}
        <tr class="row" class:expanded={expanded === u.key} onclick={() => toggle(u.key)}>
          <td class="caret">{expanded === u.key ? "▾" : "▸"}</td>
          <td class="uname">
            {u.name}
            <span class="badge {u.origin}">{u.origin}</span>
          </td>
          <td>{u.category}</td>
          <td class="dim">{u.unitType ?? "—"}</td>
          <td>{arrivalLabel(u)}</td>
          <td>{u.isLand ? liveTotal(u) : "—"}</td>
        </tr>
        {#if expanded === u.key}
          <tr class="editrow">
            <td colspan="6">
              <div class="editor">
                <div class="meta"><code class="key">{u.key}</code> <span class="dim">{u.file}</span></div>
                <div class="grid">
                  {#each u.pips as p (p.key)}
                    <div class="scalar">
                      <span class="sk" title={p.key}>{p.key}</span>
                      <input class="num" type="number" step="1" value={liveUnitStat(queue, u, p.key, p.value)}
                        oninput={(e) => commitStat(u, p.key, (e.target as HTMLInputElement).value)} />
                    </div>
                  {/each}
                </div>
                {#if u.rawExtra.length}
                  <div class="raw">Advanced (read-only): {#each u.rawExtra as r (r)}<code class="rawchip">{r}</code>{/each}</div>
                {/if}
              </div>
            </td>
          </tr>
        {/if}
      {/each}
    </tbody>
  </table>
</div>

<style>
  .units { display: flex; flex-direction: column; gap: 0.5rem; }
  .createbox { border: 1px solid #232a33; background: #1c2129; padding: 0.4rem 0.5rem; }
  .crow { display: flex; flex-wrap: wrap; align-items: center; gap: 0.4rem; }
  .filters { display: flex; align-items: center; gap: 0.5rem; }
  .counter { font-size: 0.8rem; color: #8a919c; }
  .txt { background: #14181d; border: 1px solid #4b5563; color: #cfd4db; font-family: inherit; font-size: 0.8rem; padding: 0.16rem 0.35rem; width: 14rem; }
  .txt.narrow { width: 10rem; }
  .sel { background: #14181d; border: 1px solid #4b5563; color: #cfd4db; font-family: inherit; font-size: 0.8rem; padding: 0.16rem 0.25rem; }
  .btn { border: 1px solid #1f242c; background: #3f4855; color: #cfd4db; font-family: inherit; font-size: 0.8rem; padding: 0.2rem 0.7rem; cursor: pointer; }
  .btn:hover { background: #4a6da7; color: #fff; }
  .err { color: #d9756b; font-size: 0.78rem; margin-top: 0.3rem; }
  .valid { display: flex; flex-direction: column; gap: 0.15rem; max-height: 9rem; overflow-y: auto; border: 1px solid #232a33; padding: 0.3rem; }
  .issue { font-size: 0.76rem; color: #cfd4db; }
  .issue .sev { display: inline-block; width: 4.5rem; text-transform: uppercase; font-size: 0.66rem; color: #9ca3af; }
  .issue.warning .sev { color: #e0b050; }
  .issue.error .sev { color: #d9756b; }
  .tbl { border-collapse: collapse; width: 100%; font-size: 0.82rem; }
  th { text-align: left; color: #9ca3af; font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.03em; padding: 0.3rem 0.5rem; border-bottom: 1px solid #232a33; }
  td { padding: 0.24rem 0.5rem; border-bottom: 1px solid #1f242c; color: #cfd4db; }
  .row { cursor: pointer; }
  .row:hover { background: #262d37; }
  .row.expanded { background: #262d37; }
  .caret { color: #8a919c; width: 1rem; }
  .uname { font-weight: 600; }
  .dim { color: #8a919c; }
  .badge { font-size: 0.64rem; text-transform: uppercase; padding: 0.02rem 0.28rem; border: 1px solid #1f242c; margin-left: 0.3rem; }
  .badge.base { background: #3f4855; color: #cfd4db; }
  .badge.mod { background: #3f8a6d; color: #fff; }
  .editrow td { background: #14181d; }
  .editor { display: flex; flex-direction: column; gap: 0.35rem; padding: 0.3rem 0.2rem; }
  .meta { font-size: 0.76rem; }
  .key { color: #9aecc0; background: #16191f; padding: 0 0.3rem; }
  .grid { display: flex; flex-wrap: wrap; gap: 0.35rem 1rem; }
  .scalar { display: flex; align-items: center; gap: 0.35rem; }
  .sk { font-size: 0.76rem; color: #9ca3af; width: 8.5rem; }
  .num { width: 5rem; background: #14181d; border: 1px solid #4b5563; color: #cfd4db; font-family: inherit; font-size: 0.8rem; padding: 0.13rem 0.35rem; }
  .raw { font-size: 0.74rem; color: #9ca3af; }
  .rawchip { color: #9ca3af; background: #16191f; padding: 0 0.3rem; margin: 0 0.15rem; font-style: italic; }
</style>
