<script lang="ts">
  // One expanded rebel faction: loc name/title/desc, color, typed scalars
  // (enum dropdowns, bool toggles, number steppers), trigger/effect/weight
  // blocks (14.2 tree via the shared EstateScriptBlock), start-date revolt
  // context (RebelContext), and preserve-unknown keys (read-only). Edits use
  // only the existing typed-edit vocabulary and are byte-surgical.
  import EstateScriptBlock from "$lib/components/estates/EstateScriptBlock.svelte";
  import type { KnownKey } from "$lib/components/script";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import type { RebelFaction, Scalar } from "$lib/rebels";
  import RebelContext from "./RebelContext.svelte";

  let {
    installPath,
    modPath,
    date = null,
    queue,
    obj,
    triggers,
    effects,
    countries = [],
    onremove,
    onopenprovince,
  }: {
    installPath: string;
    modPath: string | null;
    date?: string | null;
    queue: EditQueue;
    obj: RebelFaction;
    triggers: KnownKey[];
    effects: KnownKey[];
    countries?: DropdownItem[];
    onremove: () => void;
    onopenprovince?: (id: number) => void;
  } = $props();

  const file = $derived(obj.file);
  const key = $derived(obj.key);

  // --- Loc name / title / description (LocOverride) ---
  function liveLoc(locKey: string, base: string | null): string {
    return queue.pendingLocOverride(locKey) ?? base ?? "";
  }
  function commitLoc(locKey: string, label: string, v: string) {
    queue.push({ label, edits: [{ kind: "locOverride", key: locKey, value: v }], coalesceKey: `rebloc:${locKey}` });
  }

  // --- Scalar helpers (SetScalar when present, InsertStatement when absent) ---
  function liveScalar(s: Scalar): string {
    queue.version;
    const ps = queue.pendingScalar(file, [key, s.key]);
    if (ps !== undefined) return ps;
    const ins = queue.findLast(
      (e) =>
        e.kind === "insertStatement" &&
        e.file === file &&
        e.blockPath.length === 1 &&
        e.blockPath[0] === key &&
        e.statement.split("=")[0].trim() === s.key,
    );
    if (ins?.kind === "insertStatement") return ins.statement.split("=").slice(1).join("=").trim();
    return s.value;
  }
  function commitScalar(s: Scalar, value: string) {
    const quoted = s.kind === "str" && s.key === "demands_description";
    const written = quoted ? `"${value}"` : value;
    const edit: TypedEdit = s.present
      ? { kind: "setScalar", file, path: [key, s.key], value, quoted }
      : { kind: "insertStatement", file, blockPath: [key], statement: `${s.key} = ${written}` };
    queue.push({ label: `Edit ${s.key} of ${key}`, edits: [edit], coalesceKey: `rebsc:${file}:${key}:${s.key}` });
  }

  // --- Color (SetBlock on [key,"color"]) ---
  function currentColor(): [number, number, number] {
    queue.version;
    const pend = queue.pendingBlockValue(file, [key, "color"]);
    if (pend !== undefined) {
      const p = pend.trim().split(/\s+/).map(Number);
      if (p.length === 3 && p.every((n) => Number.isFinite(n))) return [p[0], p[1], p[2]];
    }
    return obj.color ?? [128, 128, 128];
  }
  function setColorComponent(i: number, v: number) {
    const c = currentColor();
    c[i] = Math.max(0, Math.min(255, v | 0));
    queue.push({
      label: `Edit color of ${key}`,
      edits: [{ kind: "setBlock", file, path: [key, "color"], value: `${c[0]} ${c[1]} ${c[2]}` }],
      coalesceKey: `rebcolor:${file}:${key}`,
    });
  }

  const liveColor = $derived(currentColor());

  // Group the scalars for a tidier layout.
  const enums = $derived(obj.scalars.filter((s) => s.kind === "enum" || s.key === "defect_delay"));
  const flags = $derived(obj.scalars.filter((s) => s.kind === "bool"));
  const nums = $derived(obj.scalars.filter((s) => s.kind === "num"));
  const strs = $derived(obj.scalars.filter((s) => s.kind === "str"));
</script>

<div class="editor">
  <!-- Loc name / title / description -->
  <div class="field">
    <label for={`reb-name-${key}`}>Name</label>
    <input id={`reb-name-${key}`} class="txt" value={liveLoc(obj.nameKey, obj.nameLoc)}
      oninput={(e) => commitLoc(obj.nameKey, `Rename ${key}`, (e.target as HTMLInputElement).value)} />
  </div>
  <div class="field">
    <label for={`reb-title-${key}`}>Title</label>
    <input id={`reb-title-${key}`} class="txt" value={liveLoc(obj.titleKey, obj.titleLoc)}
      oninput={(e) => commitLoc(obj.titleKey, `Edit title of ${key}`, (e.target as HTMLInputElement).value)} />
  </div>
  <div class="field">
    <label for={`reb-desc-${key}`}>Description</label>
    <input id={`reb-desc-${key}`} class="txt" placeholder="(loc description)" value={liveLoc(obj.descKey, obj.descLoc)}
      oninput={(e) => commitLoc(obj.descKey, `Edit description of ${key}`, (e.target as HTMLInputElement).value)} />
  </div>

  <!-- Color -->
  <div class="field">
    <span class="lbl">Color</span>
    <div class="colorrow">
      <span class="swatch" style={`background: rgb(${liveColor[0]}, ${liveColor[1]}, ${liveColor[2]})`}></span>
      {#each [0, 1, 2] as i (i)}
        <input class="num" type="number" min="0" max="255" value={liveColor[i]}
          oninput={(e) => setColorComponent(i, Number((e.target as HTMLInputElement).value))} />
      {/each}
    </div>
  </div>

  <!-- Target / behavior (enum dropdowns + defect_delay) -->
  <div class="section-title">Behavior</div>
  <div class="grid">
    {#each enums as s (s.key)}
      <div class="scalar">
        <span class="sk" title={s.key}>{s.key}</span>
        {#if s.kind === "enum"}
          <select class="sel" value={liveScalar(s)} onchange={(e) => commitScalar(s, (e.target as HTMLSelectElement).value)}>
            {#if !s.present && !s.options.includes(liveScalar(s))}<option value="">(unset)</option>{/if}
            {#each s.options as o (o)}<option value={o}>{o}</option>{/each}
          </select>
        {:else}
          <input class="num" type="number" step="1" value={liveScalar(s)}
            oninput={(e) => commitScalar(s, (e.target as HTMLInputElement).value)} />
        {/if}
      </div>
    {/each}
  </div>

  <!-- Composition + morale -->
  <div class="section-title">Composition</div>
  <div class="grid">
    {#each nums as s (s.key)}
      <div class="scalar">
        <span class="sk" title={s.key}>{s.key}</span>
        <input class="num" type="number" step="0.05" value={liveScalar(s)}
          oninput={(e) => commitScalar(s, (e.target as HTMLInputElement).value)} />
      </div>
    {/each}
  </div>

  <!-- Flags -->
  <div class="section-title">Flags</div>
  <div class="flags">
    {#each flags as s (s.key)}
      <button class="flag" class:on={liveScalar(s) === "yes"} class:absent={!s.present}
        onclick={() => commitScalar(s, liveScalar(s) === "yes" ? "no" : "yes")} title={s.key}>
        <span class="fmark">{liveScalar(s) === "yes" ? "✓" : ""}</span>{s.key}
      </button>
    {/each}
  </div>

  <!-- References -->
  <div class="section-title">References</div>
  <div class="refs">
    {#each strs as s (s.key)}
      <div class="field">
        <span class="lbl">{s.key}</span>
        <input class="txt" placeholder="(none)" value={liveScalar(s)}
          oninput={(e) => commitScalar(s, (e.target as HTMLInputElement).value)} />
      </div>
    {/each}
  </div>

  <!-- Trigger / effect / weight blocks -->
  <div class="section-title">Conditions, effects &amp; weights</div>
  {#each obj.scriptBlocks as sb (sb.name)}
    <EstateScriptBlock
      {installPath}
      {modPath}
      {queue}
      {file}
      objKey={key}
      name={sb.name}
      registry={sb.registry as "triggers" | "effects"}
      present={sb.present}
      known={sb.registry === "triggers" ? triggers : effects}
      {countries}
    />
  {/each}

  <!-- Start-date revolt context -->
  <div class="section-title">Start-date revolts</div>
  <RebelContext {installPath} {modPath} {date} faction={key} {onopenprovince} />

  <!-- Preserve-unknown -->
  {#if obj.rawExtra.length > 0}
    <div class="section-title">Advanced (read-only)</div>
    <p class="dim small">Unmodeled keys, preserved untouched on save.</p>
    <div class="idlist">
      {#each obj.rawExtra as r (r)}<code class="idchip raw">{r}</code>{/each}
    </div>
  {/if}

  <div class="danger-zone">
    <button class="btn danger" onclick={onremove}>Delete faction…</button>
  </div>
</div>

<style>
  .editor { padding: 0.35rem 0.1rem 0.3rem; display: flex; flex-direction: column; gap: 0.35rem; }
  .field { display: flex; align-items: center; gap: 0.5rem; }
  .field label, .field .lbl, .lbl { width: 9rem; flex: none; font-size: 0.78rem; color: var(--text-2); }
  .txt { flex: 1; min-width: 0; background: var(--bg-0); border: 1px solid var(--border-strong); color: var(--text-1); font-family: inherit; font-size: 0.8rem; padding: 0.15rem 0.35rem; }
  .num { width: 5rem; background: var(--bg-0); border: 1px solid var(--border-strong); color: var(--text-1); font-family: inherit; font-size: 0.8rem; padding: 0.15rem 0.35rem; }
  .sel { background: var(--bg-0); border: 1px solid var(--border-strong); color: var(--text-1); font-family: inherit; font-size: 0.8rem; padding: 0.15rem 0.25rem; }
  .colorrow { display: flex; align-items: center; gap: 0.4rem; }
  .swatch { width: 1.1rem; height: 1.1rem; border: 1px solid var(--border); flex: none; }
  .section-title { margin-top: 0.4rem; font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.03em; color: var(--text-2); border-bottom: 1px solid var(--bg-1); padding-bottom: 0.15rem; }
  .grid { display: flex; flex-wrap: wrap; gap: 0.4rem 1rem; }
  .scalar { display: flex; align-items: center; gap: 0.4rem; }
  .sk { font-size: 0.76rem; color: var(--text-1); }
  .flags { display: flex; flex-wrap: wrap; gap: 0.3rem; }
  .flag { display: inline-flex; align-items: center; gap: 0.25rem; border: 1px solid var(--border); background: var(--bg-1); color: var(--text-1); font-family: inherit; font-size: 0.74rem; padding: 0.1rem 0.4rem; cursor: pointer; }
  .flag.on { background: var(--accent); color: var(--text-inverse); border-color: var(--accent); }
  .flag.absent { opacity: 0.75; font-style: italic; }
  .fmark { width: 0.7rem; display: inline-block; }
  .refs { display: flex; flex-direction: column; gap: 0.3rem; }
  .idlist { display: flex; flex-wrap: wrap; gap: 0.25rem; }
  .idchip { color: var(--text-1); background: var(--bg-0); padding: 0.05rem 0.3rem; font-size: 0.72rem; }
  .idchip.raw { color: var(--text-2); font-style: italic; }
  .dim { color: var(--text-2); }
  .small { font-size: 0.74rem; }
  .danger-zone { margin-top: 0.5rem; }
  .btn { border: 1px solid var(--border-strong); background: transparent; color: var(--text-1); font-family: inherit; font-size: 0.78rem; padding: 0.2rem 0.6rem; cursor: pointer; }
  .btn.danger { color: var(--err); border-color: var(--danger-bg); }
  .btn.danger:hover { background: var(--danger-bg); border-color: var(--danger-bg); color: var(--text-inverse); }
</style>
