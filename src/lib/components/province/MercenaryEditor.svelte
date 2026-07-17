<script lang="ts">
  // One expanded mercenary company: loc name, typed scalars (cost/size/weights/
  // caps/flags), the sprites pack list (space-joined string), the recruitment
  // trigger (14.2 tree), the typed modifier block (flat-gated), preserve-unknown,
  // and delete. Byte-surgical via the typed-edit vocabulary. STATIC (no dates).
  import EstateModifierBlock from "$lib/components/estates/EstateModifierBlock.svelte";
  import ScriptBlockField from "./ScriptBlockField.svelte";
  import type { KnownModifier, ModifierRow, DropdownItem } from "$lib/components/ui";
  import type { KnownKey } from "$lib/components/script";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import { modBlockValue, type MercenaryCompany, type Scalar, type ModifierBlockRef } from "$lib/mercenaries";

  let {
    installPath,
    modPath,
    queue,
    company,
    known,
    triggers,
    countries = [],
    onremove,
  }: {
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
    company: MercenaryCompany;
    known: KnownModifier[];
    triggers: KnownKey[];
    countries?: DropdownItem[];
    onremove: () => void;
  } = $props();

  const file = $derived(company.file);
  const key = $derived(company.key);

  // --- Loc name ---
  const liveName = $derived(queue.pendingLocOverride(company.nameKey) ?? company.nameLoc ?? "");
  function commitName(v: string) {
    queue.push({ label: `Rename ${key}`, edits: [{ kind: "locOverride", key: company.nameKey, value: v }], coalesceKey: `mcname:${company.nameKey}` });
  }

  // --- Scalars ---
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
    const edit: TypedEdit = s.present
      ? { kind: "setScalar", file, path: [key, s.key], value, quoted: false }
      : { kind: "insertStatement", file, blockPath: [key], statement: `${s.key} = ${value}` };
    queue.push({ label: `Edit ${s.key} of ${key}`, edits: [edit], coalesceKey: `mcsc:${file}:${key}:${s.key}` });
  }

  // --- Sprites (space-joined bare-token list edited as a whole block) ---
  function liveSprites(): string {
    queue.version;
    const pb = queue.pendingBlockValue(file, [key, "sprites"]);
    if (pb !== undefined) return pb.trim();
    const ins = queue.findLast(
      (e) => e.kind === "insertStatement" && e.file === file && e.blockPath.length === 1 && e.blockPath[0] === key && e.statement.split("=")[0].trim() === "sprites",
    );
    if (ins?.kind === "insertStatement") return ins.statement.replace(/^[^=]*=\s*\{?/, "").replace(/\}?\s*$/, "").trim();
    return company.sprites;
  }
  function commitSprites(v: string) {
    const body = v.trim().split(/\s+/).filter(Boolean).join(" ");
    const edit: TypedEdit = company.spritesPresent
      ? { kind: "setBlock", file, path: [key, "sprites"], value: body }
      : { kind: "insertStatement", file, blockPath: [key], statement: `sprites = { ${body} }` };
    queue.push({ label: `Edit sprites of ${key}`, edits: [edit], coalesceKey: `mcsprites:${file}:${key}` });
  }

  // --- Modifier block ---
  function commitModifier(mb: ModifierBlockRef, rows: ModifierRow[]) {
    const body = modBlockValue(rows);
    const edit: TypedEdit = mb.present
      ? { kind: "setBlock", file, path: [key, mb.name], value: body }
      : { kind: "insertStatement", file, blockPath: [key], statement: `${mb.name} = { ${body} }` };
    queue.push({ label: `Edit ${mb.name} of ${key}`, edits: [edit], coalesceKey: `mcmod:${file}:${key}:${mb.name}` });
  }

  const scalarLabel = (k: string) => k.replace(/_/g, " ");
</script>

<div class="editor">
  <div class="field">
    <label for={`mc-name-${key}`}>Name</label>
    <input id={`mc-name-${key}`} class="txt" value={liveName} placeholder={key} oninput={(e) => commitName((e.target as HTMLInputElement).value)} />
  </div>

  <!-- Scalars -->
  <div class="section-title">Values</div>
  <div class="scalars">
    {#each company.scalars as s (s.key)}
      <div class="scalar">
        <span class="sk" title={s.key}>{scalarLabel(s.key)}</span>
        {#if s.kind === "bool"}
          <button class="toggle" class:on={liveScalar(s) === "yes"} onclick={() => commitScalar(s, liveScalar(s) === "yes" ? "no" : "yes")}>
            {liveScalar(s) === "yes" ? "yes" : "no"}
          </button>
        {:else if s.kind === "str"}
          <input class="txt sm" value={liveScalar(s)} placeholder="(none)" oninput={(e) => commitScalar(s, (e.target as HTMLInputElement).value)} />
        {:else}
          <input class="num" type="number" step={s.kind === "int" ? "1" : "any"} value={liveScalar(s)} oninput={(e) => commitScalar(s, (e.target as HTMLInputElement).value)} />
        {/if}
      </div>
    {/each}
  </div>

  <!-- Sprites -->
  <div class="field">
    <label for={`mc-sprites-${key}`}>Sprites</label>
    <input id={`mc-sprites-${key}`} class="txt" value={liveSprites()} placeholder="sprite_pack …" onchange={(e) => commitSprites((e.target as HTMLInputElement).value)} />
  </div>

  <!-- Trigger -->
  <div class="section-title">Recruitment condition</div>
  <ScriptBlockField
    {installPath}
    {modPath}
    {queue}
    {file}
    path={[key, "trigger"]}
    registry="triggers"
    present={company.triggerPresent}
    known={triggers}
    {countries}
    label="trigger"
  />

  <!-- Modifier -->
  <div class="section-title">Modifier</div>
  <div class="modblock">
    <div class="mb-head">
      <code>modifier</code>
      {#if !company.modifier.present}<span class="tag-abs">absent</span>{/if}
      {#if company.modifier.present && !company.modifier.flat}<span class="tag-raw">nested — read-only</span>{/if}
    </div>
    {#if company.modifier.flat}
      <EstateModifierBlock base={company.modifier.rows} {known} oncommit={(r) => commitModifier(company.modifier, r)} />
    {:else}
      <p class="dim small">Nested content — edit in the raw file to avoid data loss.</p>
    {/if}
  </div>

  <!-- Preserve-unknown -->
  {#if company.rawExtra.length > 0}
    <div class="section-title">Advanced (read-only)</div>
    <div class="idlist">
      {#each company.rawExtra as r (r)}<code class="idchip raw">{r}</code>{/each}
    </div>
  {/if}

  <div class="danger-zone">
    <button class="btn danger" onclick={onremove}>Delete company…</button>
  </div>
</div>

<style>
  .editor { padding: 0.35rem 0.1rem; display: flex; flex-direction: column; gap: 0.35rem; }
  .field { display: flex; align-items: center; gap: 0.5rem; }
  .field label { width: 5rem; flex: none; font-size: 0.76rem; color: #9ca3af; }
  .txt { flex: 1; min-width: 0; background: #14181d; border: 1px solid #4b5563; color: #cfd4db; font-family: inherit; font-size: 0.8rem; padding: 0.15rem 0.35rem; }
  .txt.sm { width: 12rem; flex: none; }
  .num { width: 5rem; background: #14181d; border: 1px solid #4b5563; color: #cfd4db; font-family: inherit; font-size: 0.8rem; padding: 0.15rem 0.35rem; }
  .section-title { margin-top: 0.4rem; font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.03em; color: #9ca3af; border-bottom: 1px solid #232a33; padding-bottom: 0.15rem; }
  .scalars { display: flex; flex-wrap: wrap; gap: 0.4rem 1rem; }
  .scalar { display: flex; align-items: center; gap: 0.4rem; }
  .sk { font-size: 0.74rem; color: #cfd4db; }
  .toggle { width: 3rem; border: 1px solid #1f242c; background: #21262e; color: #cfd4db; font-family: inherit; font-size: 0.78rem; padding: 0.15rem 0; cursor: pointer; }
  .toggle.on { background: #4a6da7; color: #fff; }
  .modblock { border: 1px solid #232a33; padding: 0.3rem; }
  .mb-head { display: flex; align-items: center; gap: 0.4rem; margin-bottom: 0.25rem; }
  .mb-head code { color: #9aecc0; background: #16191f; padding: 0 0.3rem; font-size: 0.76rem; }
  .tag-abs, .tag-raw { font-size: 0.65rem; text-transform: uppercase; color: #8a919c; }
  .tag-raw { color: #d0a24a; }
  .idlist { display: flex; flex-wrap: wrap; gap: 0.25rem; }
  .idchip { color: #b9bec7; background: #16191f; padding: 0.05rem 0.3rem; font-size: 0.72rem; }
  .idchip.raw { color: #9ca3af; font-style: italic; }
  .dim { color: #9ca3af; }
  .small { font-size: 0.74rem; }
  .danger-zone { margin-top: 0.5rem; }
  .btn { border: 1px solid #4b5563; background: transparent; color: #cfd4db; font-family: inherit; font-size: 0.78rem; padding: 0.2rem 0.6rem; cursor: pointer; }
  .btn.danger { color: #fca5a5; border-color: #6b3630; }
  .btn.danger:hover { background: #7a2820; border-color: #9a3226; color: #fff; }
</style>
