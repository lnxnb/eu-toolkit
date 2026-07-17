<script lang="ts">
  // One monarch-power tech table (ADM/DIP/MIL): rows = tech levels. Each level
  // shows an editable year, editable numeric modifiers, and read-only unlock /
  // unit chips (localized). "＋ Append level" adds a new `technology = { year }`
  // block at the end of the file (mods extend the tree). Preserve-unknown per
  // level (blocks like expects_institution) shown read-only.
  import type { EditQueue } from "$lib/edits.svelte";
  import { liveLevelScalar, type TechTable } from "$lib/technology";

  let {
    table,
    queue,
  }: {
    table: TechTable;
    queue: EditQueue;
  } = $props();

  let expanded = $state<number | null>(null);
  let newYear = $state("");

  function toggle(i: number) {
    expanded = expanded === i ? null : i;
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
  <ul class="levels">
    {#each table.levels as lvl (lvl.index)}
      <li class="lvl" class:expanded={expanded === lvl.index}>
        <button class="lvlmain" onclick={() => toggle(lvl.index)}>
          <span class="caret">{expanded === lvl.index ? "▾" : "▸"}</span>
          <span class="idx">Tech {lvl.index}</span>
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
          <div class="lvlbody">
            <div class="field">
              <span class="lbl">Year</span>
              <input class="num" type="number" value={liveLevelScalar(queue, lvl, "year", lvl.year ?? "")}
                oninput={(e) => commitYear(lvl.index, lvl.file, (e.target as HTMLInputElement).value)} />
            </div>

            {#if lvl.modifiers.length}
              <div class="sec">Modifiers</div>
              <div class="grid">
                {#each lvl.modifiers as m (m.key)}
                  <div class="scalar">
                    <span class="sk" title={m.key}>{m.key}</span>
                    <input class="num" type="number" step="0.01" value={liveLevelScalar(queue, lvl, m.key, m.value)}
                      oninput={(e) => commitModifier(lvl.index, lvl.file, m.key, (e.target as HTMLInputElement).value)} />
                  </div>
                {/each}
              </div>
            {/if}

            {#if lvl.unlocks.length}
              <div class="sec">Unlocks</div>
              <div class="chips">
                {#each lvl.unlocks as u (u.key)}<span class="chip unl" title={u.key}>{u.label}</span>{/each}
              </div>
            {/if}

            {#if lvl.units.length}
              <div class="sec">Units</div>
              <div class="chips">
                {#each lvl.units as u (u.value)}<span class="chip unit" title={u.value}>{u.label}</span>{/each}
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
    <input class="num wide" type="number" placeholder="year (e.g. 1550)" bind:value={newYear}
      onkeydown={(e) => e.key === "Enter" && appendLevel()} />
    <button class="btn" onclick={appendLevel}>＋ Append level</button>
  </div>
</div>

<style>
  .techtable { display: flex; flex-direction: column; gap: 0.5rem; }
  .levels { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; }
  .lvl { border: 1px solid #1f242c; border-bottom: none; }
  .lvl:last-child { border-bottom: 1px solid #1f242c; }
  .lvl.expanded { background: #262d37; }
  .lvl.pending { background: #1c2a24; }
  .lvlmain { display: flex; align-items: center; gap: 0.6rem; width: 100%; text-align: left; border: none; background: transparent; color: #cfd4db; font-family: inherit; font-size: 0.85rem; padding: 0.32rem 0.5rem; cursor: pointer; }
  .lvlmain.static { cursor: default; }
  .lvlmain:hover:not(.static) { background: #303844; }
  .caret { color: #8a919c; width: 0.9rem; flex: none; }
  .idx { font-weight: 600; width: 5rem; flex: none; }
  .year { color: #9aecc0; width: 4rem; flex: none; }
  .counts { display: flex; gap: 0.3rem; margin-left: auto; }
  .cbadge { font-size: 0.66rem; text-transform: uppercase; letter-spacing: 0.03em; padding: 0.03rem 0.3rem; border: 1px solid #1f242c; color: #9ca3af; }
  .cbadge.unit { color: #9aecc0; }
  .pendtag { margin-left: auto; font-size: 0.68rem; color: #3f8a6d; text-transform: uppercase; }
  .lvlbody { padding: 0.3rem 0.6rem 0.5rem; display: flex; flex-direction: column; gap: 0.35rem; }
  .field { display: flex; align-items: center; gap: 0.5rem; }
  .lbl, .sk { font-size: 0.78rem; color: #9ca3af; }
  .lbl { width: 4rem; }
  .sec { margin-top: 0.35rem; font-size: 0.71rem; text-transform: uppercase; letter-spacing: 0.03em; color: #9ca3af; border-bottom: 1px solid #232a33; padding-bottom: 0.12rem; }
  .grid { display: flex; flex-wrap: wrap; gap: 0.35rem 1rem; }
  .scalar { display: flex; align-items: center; gap: 0.35rem; }
  .num { width: 6rem; background: #14181d; border: 1px solid #4b5563; color: #cfd4db; font-family: inherit; font-size: 0.8rem; padding: 0.13rem 0.35rem; }
  .num.wide { width: 12rem; }
  .chips { display: flex; flex-wrap: wrap; gap: 0.25rem; }
  .chip { color: #cfd4db; background: #16191f; padding: 0.05rem 0.4rem; font-size: 0.75rem; border: 1px solid #232a33; }
  .chip.unit { color: #9aecc0; }
  .chip.raw { color: #9ca3af; font-style: italic; }
  .appendrow { display: flex; align-items: center; gap: 0.5rem; }
  .btn { border: 1px solid #1f242c; background: #3f4855; color: #cfd4db; font-family: inherit; font-size: 0.8rem; padding: 0.24rem 0.7rem; cursor: pointer; }
  .btn:hover { background: #4a6da7; color: #fff; }
</style>
