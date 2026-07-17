<script lang="ts">
  // One expanded estate / privilege / agenda: loc name+desc, icon (index or 14.4
  // sprite picker), typed scalars, typed modifier-block rows, trigger/effect trees
  // (14.2), estate id-lists, and — for privileges — availability/context (14.3).
  // Everything unmodeled (rawExtra) is shown read-only. Edits use only the
  // existing typed-edit vocabulary and are byte-surgical.
  import { SpritePicker } from "$lib/components/script";
  import type { KnownKey } from "$lib/components/script";
  import type { DropdownItem, KnownModifier, ModifierRow } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import { modBlockValue, type EstateObject, type ModifierBlock, type Scalar } from "$lib/estates";
  import EstateModifierBlock from "./EstateModifierBlock.svelte";
  import EstateScriptBlock from "./EstateScriptBlock.svelte";
  import PrivilegeAvailability from "./PrivilegeAvailability.svelte";

  let {
    installPath,
    modPath,
    date = null,
    queue,
    obj,
    known,
    triggers,
    effects,
    countries = [],
    onremove,
    onopencountry,
  }: {
    installPath: string;
    modPath: string | null;
    date?: string | null;
    queue: EditQueue;
    obj: EstateObject;
    known: KnownModifier[];
    triggers: KnownKey[];
    effects: KnownKey[];
    countries?: DropdownItem[];
    onremove: () => void;
    onopencountry?: (tag: string) => void;
  } = $props();

  const file = $derived(obj.file);
  const key = $derived(obj.key);

  // --- Loc name / desc (LocOverride) ---
  const liveName = $derived(queue.pendingLocOverride(obj.locKey) ?? obj.name);
  const liveDesc = $derived(queue.pendingLocOverride(obj.descKey) ?? obj.descLoc ?? "");
  function commitName(v: string) {
    queue.push({
      label: `Rename ${key}`,
      edits: [{ kind: "locOverride", key: obj.locKey, value: v }],
      coalesceKey: `estname:${obj.locKey}`,
    });
  }
  function commitDesc(v: string) {
    queue.push({
      label: `Edit description of ${key}`,
      edits: [{ kind: "locOverride", key: obj.descKey, value: v }],
      coalesceKey: `estdesc:${obj.descKey}`,
    });
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
    const edit: TypedEdit = s.present
      ? { kind: "setScalar", file, path: [key, s.key], value, quoted: false }
      : { kind: "insertStatement", file, blockPath: [key], statement: `${s.key} = ${value}` };
    queue.push({
      label: `Edit ${s.key} of ${key}`,
      edits: [edit],
      coalesceKey: `estsc:${file}:${key}:${s.key}`,
    });
  }

  // --- Icon (index number, or sprite via SpritePicker) ---
  const iconScalar: Scalar = $derived({
    key: "icon",
    kind: obj.iconKind === "index" ? "int" : "str",
    present: obj.icon != null,
    value: obj.icon ?? "",
  });
  const liveIcon = $derived(liveScalar(iconScalar));
  let pickIcon = $state(false);
  function setIcon(v: string) {
    commitScalar(iconScalar, v);
  }

  // --- Color (estates; SetBlock on [key,"color"]) ---
  function currentColor(): [number, number, number] {
    queue.version;
    const pend = queue.pendingBlockValue(file, [key, "color"]);
    if (pend !== undefined) {
      const p = pend.trim().split(/\s+/).map(Number);
      if (p.length === 3 && p.every((n) => Number.isFinite(n))) return [p[0], p[1], p[2]];
    }
    return obj.color ?? [128, 128, 128];
  }
  function commitColor(rgb: [number, number, number]) {
    queue.push({
      label: `Edit color of ${key}`,
      edits: [{ kind: "setBlock", file, path: [key, "color"], value: `${rgb[0]} ${rgb[1]} ${rgb[2]}` }],
      coalesceKey: `estcolor:${file}:${key}`,
    });
  }
  function setColorComponent(i: number, v: number) {
    const c = currentColor();
    c[i] = Math.max(0, Math.min(255, v | 0));
    commitColor(c);
  }

  // --- Modifier blocks ---
  function commitModifier(mb: ModifierBlock, rows: ModifierRow[]) {
    const body = modBlockValue(rows);
    const edit: TypedEdit = mb.present
      ? { kind: "setBlock", file, path: [key, mb.name], value: body }
      : { kind: "insertStatement", file, blockPath: [key], statement: `${mb.name} = { ${body} }` };
    queue.push({
      label: `Edit ${mb.name} of ${key}`,
      edits: [edit],
      coalesceKey: `estmod:${file}:${key}:${mb.name}`,
    });
  }

  const isPrivilege = $derived(obj.kind === "privilege");
  const canSelectPresent = $derived(
    obj.scriptBlocks.some((s) => s.name === "can_select" && s.present),
  );
</script>

<div class="editor">
  <!-- Loc name + description -->
  <div class="field">
    <label for={`est-name-${key}`}>Name</label>
    <input
      id={`est-name-${key}`}
      class="txt"
      value={liveName}
      oninput={(e) => commitName((e.target as HTMLInputElement).value)}
    />
  </div>
  <div class="field">
    <label for={`est-desc-${key}`}>Description</label>
    <input
      id={`est-desc-${key}`}
      class="txt"
      placeholder="(loc description)"
      value={liveDesc}
      oninput={(e) => commitDesc((e.target as HTMLInputElement).value)}
    />
  </div>

  <!-- Icon -->
  {#if obj.iconKind === "index"}
    <div class="field">
      <label for={`est-icon-${key}`}>Icon (strip index)</label>
      <input
        id={`est-icon-${key}`}
        class="num"
        type="number"
        value={liveIcon}
        oninput={(e) => setIcon((e.target as HTMLInputElement).value)}
      />
    </div>
  {:else if obj.iconKind === "sprite"}
    <div class="field">
      <span class="lbl">Icon (sprite)</span>
      <div class="iconrow">
        <code class="iconval">{liveIcon || "(none)"}</code>
        <button class="mini" onclick={() => (pickIcon = !pickIcon)}>{pickIcon ? "close" : "change…"}</button>
      </div>
    </div>
    {#if pickIcon}
      <div class="picker">
        <SpritePicker
          {installPath}
          {modPath}
          prefix="privilege_"
          value={liveIcon}
          onselect={(name) => {
            setIcon(name);
            pickIcon = false;
          }}
        />
      </div>
    {/if}
  {/if}

  <!-- Color (estates) -->
  {#if obj.kind === "estate"}
    {@const c = currentColor()}
    <div class="field">
      <span class="lbl">Color</span>
      <div class="colorrow">
        <span class="swatch" style={`background: rgb(${c[0]}, ${c[1]}, ${c[2]})`}></span>
        {#each [0, 1, 2] as i (i)}
          <input
            class="num"
            type="number"
            min="0"
            max="255"
            value={c[i]}
            oninput={(e) => setColorComponent(i, Number((e.target as HTMLInputElement).value))}
          />
        {/each}
      </div>
    </div>
  {/if}

  <!-- Scalars -->
  {#if obj.scalars.length > 0}
    <div class="section-title">Values</div>
    <div class="scalars">
      {#each obj.scalars as s (s.key)}
        <div class="scalar">
          <span class="sk" title={s.key}>{s.key}</span>
          {#if s.kind === "bool"}
            <button
              class="toggle"
              class:on={liveScalar(s) === "yes"}
              onclick={() => commitScalar(s, liveScalar(s) === "yes" ? "no" : "yes")}
            >
              {liveScalar(s) === "yes" ? "yes" : "no"}
            </button>
          {:else}
            <input
              class="num"
              type="number"
              step={s.kind === "int" ? "1" : "any"}
              value={liveScalar(s)}
              oninput={(e) => commitScalar(s, (e.target as HTMLInputElement).value)}
            />
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <!-- Modifier blocks -->
  {#if obj.modifierBlocks.length > 0}
    <div class="section-title">Modifiers</div>
    {#each obj.modifierBlocks as mb (mb.name)}
      <div class="modblock">
        <div class="mb-head">
          <code>{mb.name}</code>
          {#if !mb.present}<span class="tag-abs">absent</span>{/if}
          {#if mb.present && !mb.flat}<span class="tag-raw">nested — read-only</span>{/if}
        </div>
        {#if mb.flat}
          <EstateModifierBlock base={mb.rows} {known} oncommit={(r) => commitModifier(mb, r)} />
        {:else}
          <p class="dim small">This block contains nested content; edit it in the raw file to avoid data loss.</p>
        {/if}
      </div>
    {/each}
  {/if}

  <!-- Trigger / effect blocks -->
  {#if obj.scriptBlocks.length > 0}
    <div class="section-title">Conditions & effects</div>
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
  {/if}

  <!-- Estate id-lists (read-only reference) -->
  {#if obj.kind === "estate"}
    <div class="section-title">Privileges ({obj.privileges.length})</div>
    <p class="dim small">
      Offered privileges. Create new ones from the Privileges tab (they register here automatically).
    </p>
    {#if obj.privileges.length > 0}
      <div class="idlist">
        {#each obj.privileges as p (p)}<code class="idchip">{p}</code>{/each}
      </div>
    {/if}
    <div class="section-title">Agendas ({obj.agendas.length})</div>
    {#if obj.agendas.length > 0}
      <div class="idlist">
        {#each obj.agendas as a (a)}<code class="idchip">{a}</code>{/each}
      </div>
    {/if}
  {/if}

  <!-- Availability (privileges) -->
  {#if isPrivilege}
    <div class="section-title">Availability</div>
    <PrivilegeAvailability
      {installPath}
      {modPath}
      {date}
      {file}
      objKey={key}
      hasCanSelect={canSelectPresent}
      {countries}
      {onopencountry}
    />
  {/if}

  <!-- Preserve-unknown -->
  {#if obj.rawExtra.length > 0}
    <div class="section-title">Advanced (read-only)</div>
    <p class="dim small">Unmodeled keys, preserved untouched on save.</p>
    <div class="idlist">
      {#each obj.rawExtra as r (r)}<code class="idchip raw">{r}</code>{/each}
    </div>
  {/if}

  <div class="danger-zone">
    <button class="btn danger" onclick={onremove}>Delete {obj.kind}…</button>
  </div>
</div>

<style>
  .editor {
    padding: 0.35rem 0.1rem 0.3rem;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .field label,
  .field .lbl {
    width: 9rem;
    flex: none;
    font-size: 0.78rem;
    color: #9ca3af;
  }
  .txt {
    flex: 1;
    min-width: 0;
    background: #14181d;
    border: 1px solid #4b5563;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.15rem 0.35rem;
  }
  .num {
    width: 5rem;
    background: #14181d;
    border: 1px solid #4b5563;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.15rem 0.35rem;
  }
  .iconrow,
  .colorrow {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .iconval {
    color: #9aecc0;
    background: #16191f;
    padding: 0.05rem 0.3rem;
    font-size: 0.76rem;
  }
  .swatch {
    width: 1.1rem;
    height: 1.1rem;
    border: 1px solid #1f242c;
    flex: none;
  }
  .picker {
    height: 22rem;
    margin: 0.2rem 0;
  }
  .section-title {
    margin-top: 0.4rem;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: #9ca3af;
    border-bottom: 1px solid #232a33;
    padding-bottom: 0.15rem;
  }
  .scalars {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem 1rem;
  }
  .scalar {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .sk {
    font-size: 0.76rem;
    color: #cfd4db;
  }
  .toggle {
    width: 3rem;
    border: 1px solid #1f242c;
    background: #21262e;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.15rem 0;
    cursor: pointer;
  }
  .toggle.on {
    background: #4a6da7;
    color: #fff;
  }
  .modblock {
    border: 1px solid #232a33;
    padding: 0.3rem;
  }
  .mb-head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin-bottom: 0.25rem;
  }
  .mb-head code {
    color: #9aecc0;
    background: #16191f;
    padding: 0 0.3rem;
    font-size: 0.76rem;
  }
  .tag-abs,
  .tag-raw {
    font-size: 0.65rem;
    text-transform: uppercase;
    color: #8a919c;
  }
  .tag-raw {
    color: #d0a24a;
  }
  .idlist {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
  }
  .idchip {
    color: #b9bec7;
    background: #16191f;
    padding: 0.05rem 0.3rem;
    font-size: 0.72rem;
  }
  .idchip.raw {
    color: #9ca3af;
    font-style: italic;
  }
  .mini {
    border: 1px solid #4b5563;
    background: #2b323d;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.72rem;
    padding: 0.05rem 0.4rem;
    cursor: pointer;
  }
  .mini:hover {
    border-color: #4a6da7;
    background: #4a6da7;
    color: #fff;
  }
  .dim {
    color: #9ca3af;
  }
  .small {
    font-size: 0.74rem;
  }
  .danger-zone {
    margin-top: 0.5rem;
  }
  .btn {
    border: 1px solid #4b5563;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.2rem 0.6rem;
    cursor: pointer;
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
</style>
