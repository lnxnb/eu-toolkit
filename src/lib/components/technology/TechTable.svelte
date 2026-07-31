<script lang="ts">
  // One monarch-power tech table (ADM/DIP/MIL): rows = tech levels. Each level
  // shows its in-game name + flavor text, an editable year, its modifiers (game
  // icon + localized label + value, addable/removable), and read-only unlock /
  // unit chips (localized). "＋ Append level" adds a new `technology = { year }`
  // block at the end of the file (mods extend the tree). Preserve-unknown per
  // level (blocks like expects_institution) shown read-only.
  import type { EditQueue } from "$lib/edits.svelte";
  import {
    liveLevelScalar,
    liveLevelModifiers,
    liveLevelLoc,
    levelNameKey,
    levelDescKey,
    levelBlockKey,
    foldLevelDeletes,
    hasPendingLevelEdits,
    pendingLevelDeletes,
    modifierStatement,
    type TechTable,
    type TechLevel,
  } from "$lib/technology";
  import { ModifierIcon, SearchDropdown, type KnownModifier, type DropdownItem } from "$lib/components/ui";

  let {
    table,
    queue,
    installPath,
    modPath = null,
    knownModifiers = [],
    onopenunit,
  }: {
    table: TechTable;
    queue: EditQueue;
    installPath: string;
    modPath?: string | null;
    knownModifiers?: KnownModifier[];
    /** Open a unit's entry in the Units tab (the level's unit chips). */
    onopenunit?: (key: string) => void;
  } = $props();

  let expanded = $state<number | null>(null);
  let newYear = $state("");
  // Per-level add-a-modifier form state (the expanded level is the only one shown).
  let addKey = $state<string | null>(null);
  let addValue = $state("");

  const modifierItems = $derived<DropdownItem[]>(
    knownModifiers.map((m) => ({ key: m.key, label: m.key })),
  );

  function toggle(i: number) {
    expanded = expanded === i ? null : i;
    addKey = null;
    addValue = "";
  }

  function commitYear(index: number, file: string, value: string) {
    queue.push({
      label: `Edit ${table.kind} tech ${index} year`,
      edits: [{ kind: "setScalar", file, path: [`technology#${index}`, "year"], value, quoted: false }],
      coalesceKey: `techyear:${file}:${index}`,
    });
  }

  function commitModifier(index: number, file: string, key: string, value: string) {
    queue.push({
      label: `Edit ${key} (${table.kind} tech ${index})`,
      edits: [{ kind: "setScalar", file, path: [`technology#${index}`, key], value, quoted: false }],
      coalesceKey: `techmod:${file}:${index}:${key}`,
    });
  }

  // Title / flavor text are loc keys indexed by level, so these never touch the
  // technologies file.
  function commitLoc(key: string, value: string, what: string) {
    queue.push({
      label: `Edit ${what}`,
      edits: [{ kind: "locOverride", key, value }],
      coalesceKey: `techloc:${key}`,
    });
  }

  // A pending deletion renumbers every later level, so while one is queued the
  // rest of the table is frozen (see technology.ts for the full reasoning).
  const deletes = $derived.by(() => {
    queue.version;
    return pendingLevelDeletes(table.file, queue.serialize());
  });
  const frozen = $derived(deletes.length > 0);

  const levels = $derived.by(() => {
    queue.version;
    return foldLevelDeletes(table.levels, table.file, queue.serialize());
  });

  function deleteLevel(lvl: TechLevel) {
    // Index-addressed edits already queued against this file would shift under
    // the delete; make the user save first rather than silently mis-target them.
    if (hasPendingLevelEdits(table.file, queue.serialize())) {
      deleteError =
        "Save the project first — this file has other pending level edits, and deleting a level renumbers the ones after it.";
      return;
    }
    const later = table.levels.length - 1 - lvl.index;
    const warning =
      `Delete ${table.kind.toUpperCase()} tech ${lvl.index}${lvl.name ? ` (${lvl.name})` : ""}?` +
      (later > 0
        ? `\n\n${later} later level${later === 1 ? "" : "s"} will shift down by one. In game their names and flavor text come from index-based loc keys, so those shift too.`
        : "");
    if (!confirm(warning)) return;
    deleteError = null;
    queue.push({
      label: `Delete ${table.kind} tech level ${lvl.index}`,
      edits: [{ kind: "removeStatement", file: table.file, blockPath: [], key: levelBlockKey(lvl.index) }],
    });
    if (expanded === lvl.index) expanded = null;
  }

  let deleteError = $state<string | null>(null);

  // Add / remove operate on the level's block by statement, NOT as a whole-block
  // rewrite — a `setBlock` would drop the level's preserve-unknown sub-blocks
  // (expects_institution, effect, ahead_of_time).
  function addModifier(index: number, file: string) {
    const key = (addKey ?? "").trim();
    if (!key) return;
    const value = addValue.trim() || "0";
    queue.push({
      label: `Add ${key} to ${table.kind} tech ${index}`,
      edits: [
        { kind: "insertStatement", file, blockPath: [`technology#${index}`], statement: modifierStatement(key, value) },
      ],
      // Distinct from `techmod:` so a later value edit never clobbers the insert
      // (both compose on one evolving buffer at save time).
      coalesceKey: `techmodadd:${file}:${index}:${key}`,
    });
    addKey = null;
    addValue = "";
  }

  function removeModifier(index: number, file: string, key: string) {
    queue.push({
      label: `Remove ${key} from ${table.kind} tech ${index}`,
      edits: [{ kind: "removeStatement", file, blockPath: [`technology#${index}`], key }],
    });
  }

  function appendLevel() {
    const y = newYear.trim() || "1444";
    const index = table.levels.length;
    queue.push({
      label: `Append ${table.kind} tech level ${index}`,
      edits: [{ kind: "appendText", file: table.file, text: `\ntechnology = {\n\tyear = ${y}\n}\n` }],
    });
    newYear = "";
    expanded = index;
  }

  // Fold pending appended levels so they show live.
  const pendingLevels = $derived.by(() => {
    queue.version;
    const extra: { index: number; file: string; year: string | null }[] = [];
    let idx = table.levels.length;
    for (const e of queue.serialize()) {
      if (e.kind === "appendText" && e.file === table.file && /technology\s*=\s*\{/.test(e.text)) {
        const m = /year\s*=\s*(\d+)/.exec(e.text);
        extra.push({ index: idx, file: table.file, year: m ? m[1] : null });
        idx++;
      }
    }
    return extra;
  });
</script>

<div class="techtable">
  {#if frozen}
    <p class="banner">
      Level {deletes.join(", ")} queued for deletion. Later levels renumber when it applies, so
      editing is paused for this file until you save.
    </p>
  {/if}
  {#if deleteError}
    <p class="banner err">{deleteError}</p>
  {/if}

  <ul class="levels">
    {#each levels as lvl (lvl.index)}
      <li class="lvl" class:expanded={expanded === lvl.index}>
        <button class="lvlmain" onclick={() => toggle(lvl.index)}>
          <span class="caret">{expanded === lvl.index ? "▾" : "▸"}</span>
          <span class="idx">Tech {lvl.index}</span>
          <span class="lname">{liveLevelLoc(queue, levelNameKey(table.kind, lvl.index), lvl.name) || "—"}</span>
          <span class="year">
            {liveLevelScalar(queue, lvl, "year", lvl.year ?? "")}
          </span>
          <span class="counts">
            {#if lvl.modifiers.length}<span class="cbadge mod">{lvl.modifiers.length} mod</span>{/if}
            {#if lvl.unlocks.length}<span class="cbadge unl">{lvl.unlocks.length} unlock</span>{/if}
            {#if lvl.units.length}<span class="cbadge unit">{lvl.units.length} unit</span>{/if}
          </span>
        </button>
        {#if expanded === lvl.index}
          {@const mods = liveLevelModifiers(queue, lvl)}
          <div class="lvlbody">
            <div class="field">
              <span class="lbl">Name</span>
              <input class="text" disabled={frozen}
                placeholder="(no name — this level has no localisation)"
                value={liveLevelLoc(queue, levelNameKey(table.kind, lvl.index), lvl.name)}
                oninput={(e) => commitLoc(levelNameKey(table.kind, lvl.index), (e.target as HTMLInputElement).value, `${table.kind} tech ${lvl.index} name`)} />
            </div>
            <div class="field top">
              <span class="lbl">Flavor</span>
              <textarea class="text flavor-edit" rows="3" disabled={frozen}
                placeholder="Flavor text shown in the technology view"
                value={liveLevelLoc(queue, levelDescKey(table.kind, lvl.index), lvl.desc)}
                oninput={(e) => commitLoc(levelDescKey(table.kind, lvl.index), (e.target as HTMLTextAreaElement).value, `${table.kind} tech ${lvl.index} flavor text`)}
              ></textarea>
            </div>

            <div class="field">
              <span class="lbl">Year</span>
              <input class="num" type="number" disabled={frozen} value={liveLevelScalar(queue, lvl, "year", lvl.year ?? "")}
                oninput={(e) => commitYear(lvl.index, lvl.file, (e.target as HTMLInputElement).value)} />
              <button class="mini danger delete" onclick={() => deleteLevel(lvl)}>Delete level</button>
            </div>

            <div class="sec">Modifiers</div>
            <div class="grid">
              {#each mods as m (m.key)}
                <div class="scalar" class:pending={m.pending}>
                  <ModifierIcon {installPath} {modPath} key={m.key} />
                  <span class="sk" title={m.key}>{m.label}</span>
                  <input class="num" type="number" step="0.01" disabled={frozen} value={liveLevelScalar(queue, lvl, m.key, m.value)}
                    oninput={(e) => commitModifier(lvl.index, lvl.file, m.key, (e.target as HTMLInputElement).value)} />
                  <button class="mini danger" title="Remove {m.key}" disabled={frozen}
                    onclick={() => removeModifier(lvl.index, lvl.file, m.key)}>✕</button>
                </div>
              {/each}
              {#if !mods.length}
                <span class="none">No modifiers.</span>
              {/if}
            </div>
            <div class="addrow">
              <div class="picker">
                <SearchDropdown items={modifierItems} bind:value={addKey} placeholder="modifier key…" disabled={frozen} />
              </div>
              <input class="num" type="number" step="0.01" placeholder="value" disabled={frozen} bind:value={addValue}
                onkeydown={(e) => e.key === "Enter" && addModifier(lvl.index, lvl.file)} />
              <button class="btn" disabled={!addKey || frozen} onclick={() => addModifier(lvl.index, lvl.file)}>＋ add modifier</button>
            </div>

            {#if lvl.unlocks.length}
              <div class="sec">Unlocks</div>
              <div class="chips">
                {#each lvl.unlocks as u (u.key)}<span class="chip unl" title={u.key}>{u.label}</span>{/each}
              </div>
            {/if}

            {#if lvl.units.length}
              <div class="sec">Units</div>
              <div class="chips">
                {#each lvl.units as u (u.value)}
                  <button class="chip unit link" title="Open {u.value}" onclick={() => onopenunit?.(u.value)}>
                    {u.label}
                  </button>
                {/each}
              </div>
            {/if}

            {#if lvl.rawExtra.length}
              <div class="sec">Advanced (read-only)</div>
              <div class="chips">
                {#each lvl.rawExtra as r (r)}<code class="chip raw">{r}</code>{/each}
              </div>
            {/if}
          </div>
        {/if}
      </li>
    {/each}

    {#each pendingLevels as p (p.index)}
      <li class="lvl pending">
        <div class="lvlmain static">
          <span class="caret">＋</span>
          <span class="idx">Tech {p.index}</span>
          <span class="year">{p.year ?? "?"}</span>
          <span class="pendtag">pending</span>
        </div>
      </li>
    {/each}
  </ul>

  <div class="appendrow">
    <input class="num wide" type="number" placeholder="year (e.g. 1550)" disabled={frozen} bind:value={newYear}
      onkeydown={(e) => e.key === "Enter" && appendLevel()} />
    <button class="btn" disabled={frozen} onclick={appendLevel}>＋ Append level</button>
  </div>
</div>

<style>
  .techtable { display: flex; flex-direction: column; gap: 0.5rem; }
  .levels { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; }
  .lvl { border: 1px solid var(--border); border-bottom: none; }
  .lvl:last-child { border-bottom: 1px solid var(--border); }
  .lvl.expanded { background: var(--bg-2); }
  .lvl.pending { background: var(--bg-1); }
  .lvlmain { display: flex; align-items: center; gap: 0.6rem; width: 100%; text-align: left; border: none; background: transparent; color: var(--text-1); font-family: inherit; font-size: 0.85rem; padding: 0.32rem 0.5rem; cursor: pointer; }
  .lvlmain.static { cursor: default; }
  .lvlmain:hover:not(.static) { background: var(--bg-3); }
  .caret { color: var(--text-2); width: 0.9rem; flex: none; }
  .idx { font-weight: 600; width: 5rem; flex: none; }
  .lname { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .year { color: var(--ok); width: 4rem; flex: none; }
  .field.top { align-items: flex-start; }
  .text { flex: 1; max-width: 44ch; background: var(--bg-0); border: 1px solid var(--border-strong); color: var(--text-1); font-family: inherit; font-size: 0.8rem; padding: 0.13rem 0.35rem; }
  .flavor-edit { resize: vertical; line-height: 1.4; }
  .text:disabled, .num:disabled { opacity: 0.5; }
  .delete { margin-left: auto; }
  .banner { margin: 0; padding: 0.3rem 0.5rem; font-size: 0.78rem; background: var(--bg-2); border: 1px solid var(--warn); color: var(--warn); }
  .banner.err { border-color: var(--err); color: var(--err); }
  .none { font-size: 0.78rem; color: var(--text-3); }
  .scalar.pending .sk { color: var(--ok); }
  .picker { width: 14rem; }
  .addrow { display: flex; align-items: center; gap: 0.4rem; margin-top: 0.2rem; }
  .mini { border: 1px solid var(--border); background: var(--bg-3); color: var(--text-2); font-family: inherit; font-size: 0.7rem; line-height: 1; padding: 0.15rem 0.3rem; cursor: pointer; }
  .mini.danger:hover { background: var(--danger-bg); color: var(--text-1); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .btn:disabled:hover { background: var(--bg-3); color: var(--text-1); }
  .counts { display: flex; gap: 0.3rem; margin-left: auto; }
  .cbadge { font-size: 0.66rem; text-transform: uppercase; letter-spacing: 0.03em; padding: 0.03rem 0.3rem; border: 1px solid var(--border); color: var(--text-2); }
  .cbadge.unit { color: var(--ok); }
  .pendtag { margin-left: auto; font-size: 0.68rem; color: var(--ok); text-transform: uppercase; }
  .lvlbody { padding: 0.3rem 0.6rem 0.5rem; display: flex; flex-direction: column; gap: 0.35rem; }
  .field { display: flex; align-items: center; gap: 0.5rem; }
  .lbl, .sk { font-size: 0.78rem; color: var(--text-2); }
  .lbl { width: 4rem; }
  .sec { margin-top: 0.35rem; font-size: 0.71rem; text-transform: uppercase; letter-spacing: 0.03em; color: var(--text-2); border-bottom: 1px solid var(--bg-1); padding-bottom: 0.12rem; }
  .grid { display: flex; flex-wrap: wrap; gap: 0.35rem 1rem; }
  .scalar { display: flex; align-items: center; gap: 0.35rem; }
  .num { width: 6rem; background: var(--bg-0); border: 1px solid var(--border-strong); color: var(--text-1); font-family: inherit; font-size: 0.8rem; padding: 0.13rem 0.35rem; }
  .num.wide { width: 12rem; }
  .chips { display: flex; flex-wrap: wrap; gap: 0.25rem; }
  .chip { color: var(--text-1); background: var(--bg-0); padding: 0.05rem 0.4rem; font-size: 0.75rem; border: 1px solid var(--bg-1); }
  .chip.unit { color: var(--ok); }
  .chip.link { font-family: inherit; cursor: pointer; }
  .chip.link:hover { border-color: var(--accent); background: var(--bg-3); }
  .chip.raw { color: var(--text-2); font-style: italic; }
  .appendrow { display: flex; align-items: center; gap: 0.5rem; }
  .btn { border: 1px solid var(--border); background: var(--bg-3); color: var(--text-1); font-family: inherit; font-size: 0.8rem; padding: 0.24rem 0.7rem; cursor: pointer; }
  .btn:hover { background: var(--accent); color: var(--text-inverse); }
</style>
