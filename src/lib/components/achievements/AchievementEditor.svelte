<script lang="ts">
  // One expanded achievement: the game's icon (resolved by filename through the
  // Vfs), loc name/description (LocOverride on the `localization` stem keys),
  // id/localization scalars, and the four trigger blocks (possible / happened /
  // visible / provinces_to_highlight) via the 14.2 tree editor. Edits use only
  // the existing typed-edit vocabulary and are byte-surgical.
  import EstateScriptBlock from "$lib/components/estates/EstateScriptBlock.svelte";
  import type { KnownKey } from "$lib/components/script";
  import { ModifierIcon, type DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import type { Achievement } from "$lib/achievements";

  let {
    installPath,
    modPath,
    queue,
    obj,
    triggers,
    countries = [],
    onremove,
  }: {
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
    obj: Achievement;
    triggers: KnownKey[];
    countries?: DropdownItem[];
    onremove: () => void;
  } = $props();

  const file = $derived(obj.file);
  const key = $derived(obj.key);
  const iconPath = $derived(`gfx/interface/achievements/${obj.key}.dds`);

  // --- Loc name / description (LocOverride on the stem keys) ---
  function liveLoc(locKey: string, base: string | null): string {
    return queue.pendingLocOverride(locKey) ?? base ?? "";
  }
  function commitLoc(locKey: string, label: string, v: string) {
    queue.push({ label, edits: [{ kind: "locOverride", key: locKey, value: v }], coalesceKey: `achloc:${locKey}` });
  }

  // --- id / localization scalars (SetScalar present, InsertStatement absent) ---
  function liveScalar(name: string, base: string): string {
    queue.version;
    const ps = queue.pendingScalar(file, [key, name]);
    if (ps !== undefined) return ps;
    const ins = queue.findLast(
      (e) =>
        e.kind === "insertStatement" &&
        e.file === file &&
        e.blockPath.length === 1 &&
        e.blockPath[0] === key &&
        e.statement.split("=")[0].trim() === name,
    );
    if (ins?.kind === "insertStatement") return ins.statement.split("=").slice(1).join("=").trim();
    return base;
  }
  function commitScalar(name: string, present: boolean, value: string) {
    if (!value.trim()) return;
    const edit: TypedEdit = present
      ? { kind: "setScalar", file, path: [key, name], value, quoted: false }
      : { kind: "insertStatement", file, blockPath: [key], statement: `${name} = ${value}` };
    queue.push({ label: `Edit ${name} of ${key}`, edits: [edit], coalesceKey: `achsc:${file}:${key}:${name}` });
  }

  const liveId = $derived(liveScalar("id", obj.id != null ? String(obj.id) : ""));
  const liveLocalization = $derived(liveScalar("localization", obj.localization ?? ""));
</script>

<div class="editor">
  <!-- Icon + loc -->
  <div class="head">
    <div class="iconbox" title={iconPath}>
      {#if obj.hasIcon}
        <ModifierIcon {installPath} {modPath} key={obj.key} size="3rem" command="get_achievement_icon" />
      {:else}
        <span class="noicon">?</span>
      {/if}
    </div>
    <div class="headfields">
      <div class="field">
        <label for={`ach-name-${key}`}>Name</label>
        <input id={`ach-name-${key}`} class="txt" value={liveLoc(obj.nameKey, obj.nameLoc)}
          oninput={(e) => commitLoc(obj.nameKey, `Rename ${key}`, (e.target as HTMLInputElement).value)} />
        <code class="lockey" title="Localisation key">{obj.nameKey}</code>
      </div>
      <div class="field">
        <label for={`ach-desc-${key}`}>Description</label>
        <input id={`ach-desc-${key}`} class="txt" placeholder="(loc description)" value={liveLoc(obj.descKey, obj.descLoc)}
          oninput={(e) => commitLoc(obj.descKey, `Edit description of ${key}`, (e.target as HTMLInputElement).value)} />
        <code class="lockey" title="Localisation key">{obj.descKey}</code>
      </div>
    </div>
  </div>
  {#if !obj.hasIcon}
    <p class="dim small">No icon file — the game looks for <code>{iconPath}</code> (uncompressed 32-bit DDS, 64×64). Add one to the project to give this achievement art.</p>
  {/if}

  <!-- Scalars -->
  <div class="grid">
    <div class="scalar">
      <span class="sk" title="Steam/console mapping index — mods cannot register new Steam achievements">id</span>
      <input class="num" type="number" step="1" value={liveId}
        oninput={(e) => commitScalar("id", obj.id != null, (e.target as HTMLInputElement).value)} />
    </div>
    <div class="scalar">
      <span class="sk" title="Loc-key stem: the game reads <stem>_NAME and <stem>_DESC">localization</span>
      <input class="txt stem" value={liveLocalization}
        oninput={(e) => commitScalar("localization", obj.localization != null, (e.target as HTMLInputElement).value)} />
    </div>
  </div>

  <!-- Trigger blocks -->
  <div class="section-title">Conditions</div>
  <p class="dim small">
    <code>possible</code> = preconditions at game start · <code>happened</code> = completion ·
    <code>visible</code> = listing gate · <code>provinces_to_highlight</code> = map highlight.
  </p>
  {#each obj.scriptBlocks as sb (sb.name)}
    <EstateScriptBlock
      {installPath}
      {modPath}
      {queue}
      {file}
      objKey={key}
      name={sb.name}
      registry="triggers"
      present={sb.present}
      known={triggers}
      {countries}
    />
  {/each}

  <!-- Preserve-unknown -->
  {#if obj.rawExtra.length > 0}
    <div class="section-title">Advanced (read-only)</div>
    <p class="dim small">Unmodeled keys, preserved untouched on save.</p>
    <div class="idlist">
      {#each obj.rawExtra as r (r)}<code class="idchip raw">{r}</code>{/each}
    </div>
  {/if}

  <div class="danger-zone">
    <button class="btn danger" onclick={onremove}>Delete achievement…</button>
  </div>
</div>

<style>
  .editor { padding: 0.35rem 0.1rem 0.3rem; display: flex; flex-direction: column; gap: 0.35rem; }
  .head { display: flex; align-items: flex-start; gap: 0.6rem; }
  .iconbox { width: 3.4rem; height: 3.4rem; flex: none; display: flex; align-items: center; justify-content: center; background: var(--bg-0); border: 1px solid var(--border); }
  .noicon { color: var(--text-3); font-size: 1.4rem; }
  .headfields { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 0.3rem; }
  .field { display: flex; align-items: center; gap: 0.5rem; }
  .field label { width: 6.5rem; flex: none; font-size: 0.78rem; color: var(--text-2); }
  .txt { flex: 1; min-width: 0; background: var(--bg-0); border: 1px solid var(--border-strong); color: var(--text-1); font-family: inherit; font-size: 0.8rem; padding: 0.15rem 0.35rem; }
  .lockey { flex: none; color: var(--text-3); font-size: 0.7rem; max-width: 14rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .num { width: 5rem; background: var(--bg-0); border: 1px solid var(--border-strong); color: var(--text-1); font-family: inherit; font-size: 0.8rem; padding: 0.15rem 0.35rem; }
  .stem { flex: none; width: 16rem; }
  .grid { display: flex; flex-wrap: wrap; gap: 0.4rem 1rem; }
  .scalar { display: flex; align-items: center; gap: 0.4rem; }
  .sk { font-size: 0.76rem; color: var(--text-1); }
  .section-title { margin-top: 0.4rem; font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.03em; color: var(--text-2); border-bottom: 1px solid var(--bg-1); padding-bottom: 0.15rem; }
  .idlist { display: flex; flex-wrap: wrap; gap: 0.25rem; }
  .idchip { color: var(--text-1); background: var(--bg-0); padding: 0.05rem 0.3rem; font-size: 0.72rem; }
  .idchip.raw { color: var(--text-2); font-style: italic; }
  .dim { color: var(--text-2); margin: 0; }
  .small { font-size: 0.74rem; }
  .danger-zone { margin-top: 0.5rem; }
  .btn { border: 1px solid var(--border-strong); background: transparent; color: var(--text-1); font-family: inherit; font-size: 0.78rem; padding: 0.2rem 0.6rem; cursor: pointer; }
  .btn.danger { color: var(--err); border-color: var(--danger-bg); }
  .btn.danger:hover { background: var(--danger-bg); border-color: var(--danger-bg); color: var(--text-inverse); }
</style>
