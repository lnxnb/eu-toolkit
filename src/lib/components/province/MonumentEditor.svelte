<script lang="ts">
  // One expanded great project: loc name/desc, entry scalars (build_cost,
  // starting_tier, can_be_moved, type, date, time months, move days), entry
  // trigger/effect blocks (14.2), tier_0..tier_3 editors, preserve-unknown, and
  // delete. Byte-surgical via the typed-edit vocabulary. STATIC (no date ctx).
  import { invoke } from "@tauri-apps/api/core";
  import ScriptBlockField from "./ScriptBlockField.svelte";
  import MonumentTier from "./MonumentTier.svelte";
  import type { KnownModifier, DropdownItem } from "$lib/components/ui";
  import type { KnownKey } from "$lib/components/script";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import type { GreatProject, Scalar } from "$lib/monuments";

  let {
    installPath,
    modPath,
    queue,
    project,
    known,
    triggers,
    effects,
    countries = [],
    onremove,
  }: {
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
    project: GreatProject;
    known: KnownModifier[];
    triggers: KnownKey[];
    effects: KnownKey[];
    countries?: DropdownItem[];
    onremove: () => void;
  } = $props();

  const file = $derived(project.file);
  const key = $derived(project.key);
  const fullPath = (rel: string[]) => [key, ...rel];

  // --- Sprite preview ---
  let spriteUrl = $state<string | null>(null);
  $effect(() => {
    void project.sprite;
    spriteUrl = null;
    const name = `GFX_great_project_${key}`;
    invoke<ArrayBuffer>("get_sprite", { installPath, modPath, name })
      .then((buf) => {
        const blob = new Blob([buf], { type: "image/png" });
        spriteUrl = URL.createObjectURL(blob);
      })
      .catch(() => (spriteUrl = null));
  });

  // --- Loc name / desc ---
  const liveName = $derived(queue.pendingLocOverride(project.nameKey) ?? project.nameLoc ?? "");
  const liveDesc = $derived(queue.pendingLocOverride(project.descKey) ?? project.descLoc ?? "");
  function commitName(v: string) {
    queue.push({ label: `Rename ${key}`, edits: [{ kind: "locOverride", key: project.nameKey, value: v }], coalesceKey: `gpname:${project.nameKey}` });
  }
  function commitDesc(v: string) {
    queue.push({ label: `Edit description of ${key}`, edits: [{ kind: "locOverride", key: project.descKey, value: v }], coalesceKey: `gpdesc:${project.descKey}` });
  }

  // --- Scalars ---
  function liveScalar(s: Scalar): string {
    queue.version;
    const ps = queue.pendingScalar(file, fullPath(s.path));
    if (ps !== undefined) return ps;
    const parent = fullPath(s.path.slice(0, -1));
    const ins = queue.findLast(
      (e) =>
        e.kind === "insertStatement" &&
        e.file === file &&
        e.blockPath.length === parent.length &&
        e.blockPath.every((p, i) => p === parent[i]) &&
        e.statement.split("=")[0].trim() === s.key,
    );
    if (ins?.kind === "insertStatement") return ins.statement.split("=").slice(1).join("=").trim();
    return s.value;
  }
  function commitScalar(s: Scalar, value: string) {
    const parent = fullPath(s.path.slice(0, -1));
    const edit: TypedEdit = s.present
      ? { kind: "setScalar", file, path: fullPath(s.path), value, quoted: false }
      : { kind: "insertStatement", file, blockPath: parent, statement: `${s.key} = ${value}` };
    queue.push({ label: `Edit ${s.key} of ${key}`, edits: [edit], coalesceKey: `gpsc:${file}:${key}:${s.path.join(".")}` });
  }

  const scalarLabel = (s: Scalar) => (s.key === "months" ? "build months" : s.key);
</script>

<div class="editor">
  <div class="top">
    {#if spriteUrl}
      <img class="sprite" src={spriteUrl} alt={`${key} sprite`} />
    {:else}
      <div class="sprite ph">no gfx</div>
    {/if}
    <div class="meta">
      <div class="field">
        <label for={`gp-name-${key}`}>Name</label>
        <input id={`gp-name-${key}`} class="txt" value={liveName} placeholder={key} oninput={(e) => commitName((e.target as HTMLInputElement).value)} />
      </div>
      <div class="field">
        <label for={`gp-desc-${key}`}>Description</label>
        <input id={`gp-desc-${key}`} class="txt" value={liveDesc} placeholder="(loc description)" oninput={(e) => commitDesc((e.target as HTMLInputElement).value)} />
      </div>
    </div>
  </div>

  <!-- Scalars -->
  <div class="section-title">Values</div>
  <div class="scalars">
    {#each project.scalars as s (s.key)}
      <div class="scalar">
        <span class="sk" title={s.path.join(".")}>{scalarLabel(s)}</span>
        {#if s.kind === "bool"}
          <button class="toggle" class:on={liveScalar(s) === "yes"} onclick={() => commitScalar(s, liveScalar(s) === "yes" ? "no" : "yes")}>
            {liveScalar(s) === "yes" ? "yes" : "no"}
          </button>
        {:else if s.kind === "enum"}
          <select class="sel" value={liveScalar(s)} onchange={(e) => commitScalar(s, (e.target as HTMLSelectElement).value)}>
            {#each s.options as o (o)}<option value={o}>{o}</option>{/each}
          </select>
        {:else if s.kind === "str"}
          <input class="txt sm" value={liveScalar(s)} oninput={(e) => commitScalar(s, (e.target as HTMLInputElement).value)} />
        {:else}
          <input class="num" type="number" step={s.kind === "int" ? "1" : "any"} value={liveScalar(s)} oninput={(e) => commitScalar(s, (e.target as HTMLInputElement).value)} />
        {/if}
      </div>
    {/each}
  </div>

  <!-- Entry trigger / effect blocks -->
  <div class="section-title">Conditions & effects</div>
  {#each project.scriptBlocks as sb (sb.name)}
    <ScriptBlockField
      {installPath}
      {modPath}
      {queue}
      {file}
      path={fullPath(sb.path)}
      registry={sb.registry as "triggers" | "effects"}
      present={sb.present}
      known={sb.registry === "triggers" ? triggers : effects}
      {countries}
    />
  {/each}

  <!-- Tiers -->
  <div class="section-title">Tiers</div>
  {#each project.tiers as tier (tier.index)}
    <MonumentTier {installPath} {modPath} {queue} {file} entryKey={key} {tier} {known} {effects} {countries} />
  {/each}

  <!-- Preserve-unknown -->
  {#if project.rawExtra.length > 0}
    <div class="section-title">Advanced (read-only)</div>
    <div class="idlist">
      {#each project.rawExtra as r (r)}<code class="idchip raw">{r}</code>{/each}
    </div>
  {/if}

  <div class="danger-zone">
    <button class="btn danger" onclick={onremove}>Delete great project…</button>
  </div>
</div>

<style>
  .editor { padding: 0.35rem 0.1rem; display: flex; flex-direction: column; gap: 0.35rem; }
  .top { display: flex; gap: 0.5rem; align-items: flex-start; }
  .sprite { width: 3rem; height: 3rem; object-fit: contain; border: 1px solid #232a33; background: #14181d; flex: none; }
  .sprite.ph { display: flex; align-items: center; justify-content: center; font-size: 0.6rem; color: #8a919c; }
  .meta { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 0.3rem; }
  .field { display: flex; align-items: center; gap: 0.5rem; }
  .field label { width: 5rem; flex: none; font-size: 0.76rem; color: #9ca3af; }
  .txt { flex: 1; min-width: 0; background: #14181d; border: 1px solid #4b5563; color: #cfd4db; font-family: inherit; font-size: 0.8rem; padding: 0.15rem 0.35rem; }
  .txt.sm { width: 12rem; flex: none; }
  .num { width: 5rem; background: #14181d; border: 1px solid #4b5563; color: #cfd4db; font-family: inherit; font-size: 0.8rem; padding: 0.15rem 0.35rem; }
  .sel { background: #14181d; border: 1px solid #4b5563; color: #cfd4db; font-family: inherit; font-size: 0.8rem; padding: 0.15rem 0.25rem; }
  .section-title { margin-top: 0.4rem; font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.03em; color: #9ca3af; border-bottom: 1px solid #232a33; padding-bottom: 0.15rem; }
  .scalars { display: flex; flex-wrap: wrap; gap: 0.4rem 1rem; }
  .scalar { display: flex; align-items: center; gap: 0.4rem; }
  .sk { font-size: 0.74rem; color: #cfd4db; }
  .toggle { width: 3rem; border: 1px solid #1f242c; background: #21262e; color: #cfd4db; font-family: inherit; font-size: 0.78rem; padding: 0.15rem 0; cursor: pointer; }
  .toggle.on { background: #4a6da7; color: #fff; }
  .idlist { display: flex; flex-wrap: wrap; gap: 0.25rem; }
  .idchip { color: #b9bec7; background: #16191f; padding: 0.05rem 0.3rem; font-size: 0.72rem; }
  .idchip.raw { color: #9ca3af; font-style: italic; }
  .danger-zone { margin-top: 0.5rem; }
  .btn { border: 1px solid #4b5563; background: transparent; color: #cfd4db; font-family: inherit; font-size: 0.78rem; padding: 0.2rem 0.6rem; cursor: pointer; }
  .btn.danger { color: #fca5a5; border-color: #6b3630; }
  .btn.danger:hover { background: #7a2820; border-color: #9a3226; color: #fff; }
</style>
