<script lang="ts">
  // One expanded mechanic object (config-driven — same typed surface as
  // estates/rebels): loc name/desc, optional color + sprite icon, typed scalars
  // (enum/bool/number/string), self-modifier rows (families whose block IS the
  // modifier, e.g. deities/cults/schools), `modifier`-style flat blocks,
  // trigger/effect/weight trees (14.2), ordered reform steps (reorderable),
  // event cross-refs (disasters/incidents), country-shaped availability (14.3),
  // and preserve-unknown raw. Edits use only the existing typed-edit vocabulary
  // and are byte-surgical. Group-nested families thread the [group,
  // "religious_schools", key] path prefix through every edit.
  import { SpritePicker } from "$lib/components/script";
  import type { KnownKey } from "$lib/components/script";
  import { SearchDropdown, AtlasIcon, SpriteIcon } from "$lib/components/ui";
  import type { DropdownItem, KnownModifier, ModifierRow } from "$lib/components/ui";
  import EstateModifierBlock from "$lib/components/estates/EstateModifierBlock.svelte";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import {
    modBlockValue,
    isValidKey,
    type FamilyMeta,
    type ListField,
    type MechanicObject,
    type ModifierBlock,
    type ReformStep,
    type Scalar,
    type SubGroupData,
  } from "$lib/mechanics";
  import MechanicScriptBlock from "./MechanicScriptBlock.svelte";
  import MechanicAvailability from "./MechanicAvailability.svelte";
  import MechanicEventLinks from "./MechanicEventLinks.svelte";
  import MechanicSubEntry from "./MechanicSubEntry.svelte";

  let {
    installPath,
    modPath,
    date = null,
    queue,
    obj,
    meta,
    known,
    triggers,
    effects,
    countries = [],
    pickerItems = {},
    onremove,
    onopenevents,
    onopennaming,
  }: {
    installPath: string;
    modPath: string | null;
    date?: string | null;
    queue: EditQueue;
    obj: MechanicObject;
    meta: FamilyMeta;
    known: KnownModifier[];
    triggers: KnownKey[];
    effects: KnownKey[];
    countries?: DropdownItem[];
    /** Registry option lists keyed by picker kind ("building", "trade_good"). */
    pickerItems?: Record<string, DropdownItem[]>;
    onremove: () => void;
    onopenevents?: (id: string) => void;
    /** Government-ranks → government-names cross-link (ranks index naming tables). */
    onopennaming?: () => void;
  } = $props();

  const file = $derived(obj.file);
  const key = $derived(obj.key);
  // Edit path prefix: group-nested schools live two levels deep. Uses editKey
  // (occurrence-qualified for de-duped families like subject types) so byte-
  // surgical edits resolve the entity's real definition, not a forward decl.
  const editKey = $derived(obj.editKey || key);
  const basePath = $derived<string[]>(
    obj.group ? [obj.group, "religious_schools", editKey] : [editKey],
  );
  function previewFrame(value: string): number { let h = 2166136261; for (let i=0;i<value.length;i++) h=Math.imul(h^value.charCodeAt(i),16777619); return (h>>>0)%18; }

  // --- Loc name / desc ---
  const liveName = $derived(queue.pendingLocOverride(obj.nameKey) ?? obj.name);
  const liveDesc = $derived(queue.pendingLocOverride(obj.descKey) ?? obj.descLoc ?? "");
  function commitName(v: string) {
    queue.push({ label: `Rename ${key}`, edits: [{ kind: "locOverride", key: obj.nameKey, value: v }], coalesceKey: `mecname:${obj.nameKey}` });
  }
  function commitDesc(v: string) {
    queue.push({ label: `Edit description of ${key}`, edits: [{ kind: "locOverride", key: obj.descKey, value: v }], coalesceKey: `mecdesc:${obj.descKey}` });
  }

  // --- Scalar helpers (present → SetScalar; absent → InsertStatement) ---
  function liveField(fieldKey: string, present: boolean, fallback: string): string {
    queue.version;
    const ps = queue.pendingScalar(file, [...basePath, fieldKey]);
    if (ps !== undefined) return ps;
    const ins = queue.findLast(
      (e) =>
        e.kind === "insertStatement" &&
        e.file === file &&
        e.blockPath.length === basePath.length &&
        e.blockPath.every((p, i) => p === basePath[i]) &&
        e.statement.split("=")[0].trim() === fieldKey,
    );
    if (ins?.kind === "insertStatement") return ins.statement.split("=").slice(1).join("=").trim();
    void present;
    return fallback;
  }
  function commitField(fieldKey: string, present: boolean, value: string, quoted = false) {
    const written = quoted ? `"${value}"` : value;
    const edit: TypedEdit = present
      ? { kind: "setScalar", file, path: [...basePath, fieldKey], value, quoted }
      : { kind: "insertStatement", file, blockPath: basePath, statement: `${fieldKey} = ${written}` };
    queue.push({ label: `Edit ${fieldKey} of ${key}`, edits: [edit], coalesceKey: `mecsc:${file}:${key}:${fieldKey}` });
  }
  const liveScalar = (s: Scalar) => liveField(s.key, s.present, s.value);
  // Only plain string scalars round-trip quoted; token/pickered refs write bare.
  const commitScalar = (s: Scalar, v: string) =>
    commitField(s.key, s.present, v, s.kind === "str" && !s.picker);

  // --- Bare-token list fields (buildings' manufactory) ---
  function liveTokens(lf: ListField): string[] {
    queue.version;
    const pend = queue.pendingBlockValue(file, [...basePath, lf.name]);
    if (pend !== undefined) return pend.trim().split(/\s+/).filter(Boolean);
    // A pending insertStatement creating the block (was absent).
    const ins = queue.findLast(
      (e) =>
        e.kind === "insertStatement" &&
        e.file === file &&
        e.blockPath.length === basePath.length &&
        e.blockPath.every((p, i) => p === basePath[i]) &&
        e.statement.split("=")[0].trim() === lf.name,
    );
    if (ins?.kind === "insertStatement") {
      const inner = ins.statement.slice(ins.statement.indexOf("{") + 1, ins.statement.lastIndexOf("}"));
      return inner.trim().split(/\s+/).filter(Boolean);
    }
    return lf.tokens;
  }
  function commitTokens(lf: ListField, tokens: string[]) {
    const present = lf.present || liveTokens(lf).length > 0 || tokens.length > 0;
    const body = tokens.join(" ");
    const edit: TypedEdit = lf.present
      ? { kind: "setBlock", file, path: [...basePath, lf.name], value: body }
      : { kind: "insertStatement", file, blockPath: basePath, statement: `${lf.name} = { ${body} }` };
    void present;
    queue.push({ label: `Edit ${lf.name} of ${key}`, edits: [edit], coalesceKey: `meclist:${file}:${key}:${lf.name}` });
  }
  function addToken(lf: ListField, tok: string) {
    const t = tok.trim();
    if (!t) return;
    const cur = liveTokens(lf);
    if (cur.includes(t)) return;
    commitTokens(lf, [...cur, t]);
  }
  function removeToken(lf: ListField, tok: string) {
    commitTokens(lf, liveTokens(lf).filter((x) => x !== tok));
  }

  // --- Icon (sprite) ---
  const liveIcon = $derived(liveField("icon", obj.icon != null, obj.icon ?? ""));
  let pickIcon = $state(false);

  // --- Color ---
  function currentColor(): [number, number, number] {
    queue.version;
    const pend = queue.pendingBlockValue(file, [...basePath, "color"]);
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
      edits: [{ kind: "setBlock", file, path: [...basePath, "color"], value: `${c[0]} ${c[1]} ${c[2]}` }],
      coalesceKey: `meccolor:${file}:${key}`,
    });
  }

  // --- Modifier blocks ---
  function commitModifier(mb: ModifierBlock, rows: ModifierRow[]) {
    const body = modBlockValue(rows);
    const edit: TypedEdit = mb.present
      ? { kind: "setBlock", file, path: [...basePath, mb.name], value: body }
      : { kind: "insertStatement", file, blockPath: basePath, statement: `${mb.name} = { ${body} }` };
    queue.push({ label: `Edit ${mb.name} of ${key}`, edits: [edit], coalesceKey: `mecmod:${file}:${key}:${mb.name}` });
  }

  // --- Self-modifier rows (per-row scalar edits so structural blocks survive) ---
  let newRowKey = $state("");
  let newRowVal = $state("");
  function removeSelfRow(rowKey: string) {
    queue.push({
      label: `Remove ${rowKey} from ${key}`,
      edits: [{ kind: "removeStatement", file, blockPath: basePath, key: rowKey }],
    });
  }
  function addSelfRow() {
    const rk = newRowKey.trim();
    if (!rk) return;
    commitField(rk, false, newRowVal.trim() || "0");
    newRowKey = "";
    newRowVal = "";
  }

  // --- Ordered reform steps (reorder = byte-surgical body swap, like 19.3) ---
  function innerOf(rows: { key: string; value: string }[]): string {
    return rows.map((r) => `${r.key} = ${r.value}`).join(" ");
  }
  function commitReformStep(step: ReformStep, rows: ModifierRow[]) {
    queue.push({
      label: `Edit reform ${step.key}`,
      edits: [{ kind: "setBlock", file, path: [...basePath, step.key], value: modBlockValue(rows) }],
      coalesceKey: `mecreform:${file}:${key}:${step.key}`,
    });
  }
  function moveStep(i: number, delta: number) {
    const j = i + delta;
    const steps = obj.orderedChildren;
    if (j < 0 || j >= steps.length) return;
    const a = steps[i];
    const b = steps[j];
    queue.push({
      label: `Reorder reforms in ${key}`,
      edits: [
        { kind: "setBlock", file, path: [...basePath, a.key], value: innerOf(b.rows) },
        { kind: "setBlock", file, path: [...basePath, b.key], value: innerOf(a.rows) },
      ],
    });
  }

  // --- Sub-groups (ages' objectives / abilities) ---
  let newSubKey = $state<Record<string, string>>({});
  function containerPathOf(sg: SubGroupData): string[] {
    return [...basePath, sg.container];
  }
  function addSubEntry(sg: SubGroupData) {
    const raw = (newSubKey[sg.container] ?? "").trim();
    if (!isValidKey(raw)) return;
    if (sg.entries.some((e) => e.key === raw)) return;
    queue.push({
      label: `Add ${sg.label} entry ${raw}`,
      edits: [
        {
          kind: "insertStatement",
          file,
          blockPath: containerPathOf(sg),
          statement: `${raw} = ${sg.childScaffold}`,
        },
      ],
    });
    newSubKey[sg.container] = "";
  }
  function removeSubEntry(sg: SubGroupData, entryKey: string) {
    if (!confirm(`Delete ${sg.label.toLowerCase()} entry "${entryKey}"?`)) return;
    queue.push({
      label: `Remove ${sg.label} entry ${entryKey}`,
      edits: [{ kind: "removeStatement", file, blockPath: containerPathOf(sg), key: entryKey }],
    });
  }

  // Free-text token add for list fields whose picker has no registry (provinces,
  // legacy governments, disabled peace options).
  let newListTok = $state<Record<string, string>>({});
  function addFreeToken(lf: ListField) {
    const t = (newListTok[lf.name] ?? "").trim();
    if (!t) return;
    addToken(lf, t);
    newListTok[lf.name] = "";
  }

  // Group the scalars for a tidier layout. Pickered scalars (province/registry
  // refs) render separately; token scalars are bare-text refs.
  const pickers = $derived(obj.scalars.filter((s) => s.picker));
  const enums = $derived(obj.scalars.filter((s) => s.kind === "enum" && !s.picker));
  const bools = $derived(obj.scalars.filter((s) => s.kind === "bool" && !s.picker));
  const nums = $derived(obj.scalars.filter((s) => (s.kind === "num" || s.kind === "int") && !s.picker));
  const strs = $derived(
    obj.scalars.filter((s) => (s.kind === "str" || s.kind === "token") && s.key !== "icon" && !s.picker),
  );
  function pickerLabel(kind: string, key: string): string {
    return (pickerItems[kind] ?? []).find((i) => i.key === key)?.label ?? key;
  }

  const showEvents = $derived(obj.eventRefs.length > 0 || meta.id === "disasters" || meta.id === "incidents");
</script>

<div class="editor">
  {#if meta.id === "buildings"}
    <div class="entity-art"><SpriteIcon {installPath} {modPath} name={`GFX_${obj.key}`} size={32} label={`${liveName} building icon`} /><strong>{liveName}</strong></div>
  {/if}
  <!-- Loc name + description -->
  <div class="field">
    <label for={`mec-name-${key}`}>Name</label>
    <input id={`mec-name-${key}`} class="txt" value={liveName} oninput={(e) => commitName((e.target as HTMLInputElement).value)} />
  </div>
  <div class="field">
    <label for={`mec-desc-${key}`}>Description</label>
    <input id={`mec-desc-${key}`} class="txt" placeholder="(loc description)" value={liveDesc} oninput={(e) => commitDesc((e.target as HTMLInputElement).value)} />
  </div>

  <!-- Icon (sprite) -->
  {#if meta.iconKind === "sprite"}
    <div class="field">
      <span class="lbl">Icon (sprite)</span>
      <div class="iconrow">
        {#if liveIcon}<SpriteIcon {installPath} {modPath} name={liveIcon} size={32} label={`${liveName} icon`} />{/if}
        <code class="iconval">{liveIcon || "(none)"}</code>
        <button class="mini" onclick={() => (pickIcon = !pickIcon)}>{pickIcon ? "close" : "change…"}</button>
      </div>
    </div>
    {#if pickIcon}
      <div class="picker">
        <SpritePicker {installPath} {modPath} prefix="GFX_" value={liveIcon} onselect={(name) => { commitField("icon", obj.icon != null, name); pickIcon = false; }} />
      </div>
    {/if}
  {:else if meta.iconKind === "named"}
    <!-- Named-icon reference (e.g. reform `icon = "crown"`), written quoted. -->
    <div class="field">
      <span class="lbl">Icon (named)</span>
      {#if liveIcon}<SpriteIcon {installPath} {modPath} name={`GFX_${liveIcon}`} size={32} label={`${liveName} icon`} />{/if}
      <input
        class="txt"
        placeholder="(e.g. crown)"
        value={liveIcon}
        oninput={(e) => commitField("icon", obj.icon != null, (e.target as HTMLInputElement).value, true)}
      />
    </div>
  {/if}

  <!-- Color -->
  {#if meta.hasColor}
    {@const c = currentColor()}
    <div class="field">
      <span class="lbl">Color</span>
      <div class="colorrow">
        <span class="swatch" style={`background: rgb(${c[0]}, ${c[1]}, ${c[2]})`}></span>
        {#each [0, 1, 2] as i (i)}
          <input class="num" type="number" min="0" max="255" value={c[i]} oninput={(e) => setColorComponent(i, Number((e.target as HTMLInputElement).value))} />
        {/each}
      </div>
    </div>
  {/if}

  <!-- Typed scalars -->
  {#if enums.length || nums.length}
    <div class="section-title">Values</div>
    <div class="grid">
      {#each enums as s (s.key)}
        <div class="scalar">
          <span class="sk" title={s.key}>{s.key}</span>
          <select class="sel" value={liveScalar(s)} onchange={(e) => commitScalar(s, (e.target as HTMLSelectElement).value)}>
            {#if !s.present && !s.options.includes(liveScalar(s))}<option value="">(unset)</option>{/if}
            {#each s.options as o (o)}<option value={o}>{o}</option>{/each}
          </select>
        </div>
      {/each}
      {#each nums as s (s.key)}
        <div class="scalar">
          <span class="sk" title={s.key}>{s.key}</span>
          <input class="num" type="number" step={s.kind === "int" ? "1" : "any"} value={liveScalar(s)} oninput={(e) => commitScalar(s, (e.target as HTMLInputElement).value)} />
        </div>
      {/each}
    </div>
  {/if}

  {#if bools.length}
    <div class="section-title">Flags</div>
    <div class="flags">
      {#each bools as s (s.key)}
        <button class="flag" class:on={liveScalar(s) === "yes"} class:absent={!s.present} onclick={() => commitScalar(s, liveScalar(s) === "yes" ? "no" : "yes")} title={s.key}>
          <span class="fmark">{liveScalar(s) === "yes" ? "✓" : ""}</span>{s.key}
        </button>
      {/each}
    </div>
  {/if}

  {#if strs.length}
    <div class="section-title">References</div>
    <div class="refs">
      {#each strs as s (s.key)}
        <div class="field">
          <span class="lbl">{s.key}</span>
          <input class="txt" placeholder="(none)" value={liveScalar(s)} oninput={(e) => commitScalar(s, (e.target as HTMLInputElement).value)} />
        </div>
      {/each}
    </div>
  {/if}

  <!-- Pickered scalars (province on-map / registry refs) -->
  {#if pickers.length}
    <div class="section-title">Links</div>
    <div class="refs">
      {#each pickers as s (s.key)}
        <div class="field">
          <span class="lbl">{s.key}</span>
          {#if s.picker === "province"}
            {@const cur = liveScalar(s)}
            <input
              class="num"
              type="number"
              min="1"
              step="1"
              placeholder="province id"
              value={cur}
              oninput={(e) => commitScalar(s, (e.target as HTMLInputElement).value)}
            />
            <span class="dim small">province #{cur || "?"}</span>
          {:else}
            <div class="pickerslot">
              <SearchDropdown
                items={pickerItems[s.picker] ?? []}
                value={liveScalar(s) || null}
                placeholder={`${s.picker.replace("_", " ")}…`}
                onselect={(k) => commitScalar(s, k)}
              />
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <!-- Bare-token list fields (manufactory trade goods) -->
  {#if obj.listFields.length}
    {#each obj.listFields as lf (lf.name)}
      {@const toks = liveTokens(lf)}
      <div class="section-title">{lf.name}</div>
      <div class="tokens">
        {#each toks as t (t)}
          <span class="chip">
            {pickerLabel(lf.picker, t)}
            <button class="x" title="Remove" onclick={() => removeToken(lf, t)}>×</button>
          </span>
        {/each}
        {#if (pickerItems[lf.picker] ?? []).length > 0}
          <div class="pickerslot">
            <SearchDropdown
              items={(pickerItems[lf.picker] ?? []).filter((i) => !toks.includes(i.key))}
              value={null}
              placeholder={`Add ${lf.picker.replace("_", " ")}…`}
              onselect={(k) => addToken(lf, k)}
            />
          </div>
        {:else}
          <!-- No registry for this token kind (province ids, legacy govs): free text. -->
          <input
            class="rk-in"
            placeholder={lf.picker === "province" ? "province id…" : "add token…"}
            bind:value={newListTok[lf.name]}
            onkeydown={(e) => e.key === "Enter" && addFreeToken(lf)}
          />
          <button class="mini" onclick={() => addFreeToken(lf)}>＋ add</button>
        {/if}
      </div>
    {/each}
  {/if}

  <!-- Self-modifier rows -->
  {#if obj.selfModifier}
    <div class="section-title">Modifiers</div>
    <p class="dim small">Flat modifiers on this block. Each row is an individual key = value; structural blocks below are untouched.</p>
    <div class="selfrows">
      {#each obj.selfRows as r (r.key)}
        <div class="selfrow">
          <code class="rk">{r.key}</code>
          <input class="num wide" value={liveField(r.key, true, r.value)} oninput={(e) => commitField(r.key, true, (e.target as HTMLInputElement).value)} />
          <button class="mini danger" onclick={() => removeSelfRow(r.key)}>✕</button>
        </div>
      {/each}
      <div class="selfrow addrow">
        <input class="rk-in" placeholder="modifier key" bind:value={newRowKey} />
        <input class="num wide" placeholder="value" bind:value={newRowVal} onkeydown={(e) => e.key === "Enter" && addSelfRow()} />
        <button class="mini" onclick={addSelfRow}>＋ add</button>
      </div>
    </div>
  {/if}

  <!-- Modifier blocks -->
  {#if obj.modifierBlocks.length > 0}
    {#if !obj.selfModifier}<div class="section-title">Modifiers</div>{/if}
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

  <!-- Ordered reform steps -->
  {#if obj.ordered}
    <div class="section-title">Reform steps ({obj.orderedChildren.length})</div>
    <p class="dim small">File order = progression order. Reorder swaps the step bodies byte-surgically.</p>
    {#each obj.orderedChildren as step, i (step.key)}
      <div class="modblock">
        <div class="mb-head">
          {#if meta.id === "idea_groups"}<AtlasIcon {installPath} {modPath} kind="idea_modifiers" frame={previewFrame(step.key)} size={28} label={`${step.key} idea icon`} />{/if}
          <span class="step-n">{i + 1}</span>
          <code>{step.key}</code>
          {#if !step.flat}<span class="tag-raw">nested — read-only</span>{/if}
          <span class="spacer"></span>
          <button class="mini" disabled={i === 0} onclick={() => moveStep(i, -1)}>▲</button>
          <button class="mini" disabled={i === obj.orderedChildren.length - 1} onclick={() => moveStep(i, 1)}>▼</button>
        </div>
        {#if step.flat}
          <EstateModifierBlock base={step.rows} {known} oncommit={(r) => commitReformStep(step, r)} />
        {:else}
          <p class="dim small">This step contains nested content; edit it in the raw file.</p>
        {/if}
      </div>
    {/each}
  {/if}

  <!-- Sub-groups (ages' objectives / abilities) -->
  {#each obj.subGroups as sg (sg.container)}
    <div class="section-title">{sg.label} ({sg.entries.length})</div>
    <div class="subgroup">
      {#each sg.entries as entry (entry.key)}
        <MechanicSubEntry
          {installPath}
          {modPath}
          {queue}
          {file}
          containerPath={[...basePath, sg.container]}
          {entry}
          childIsTrigger={sg.childIsTrigger}
          {known}
          {triggers}
          {effects}
          {countries}
          onremove={() => removeSubEntry(sg, entry.key)}
        />
      {/each}
      <div class="selfrow addrow">
        <input class="rk-in" placeholder={`new ${sg.label.toLowerCase()} key`} bind:value={newSubKey[sg.container]} onkeydown={(e) => e.key === "Enter" && addSubEntry(sg)} />
        <button class="mini" onclick={() => addSubEntry(sg)}>＋ add</button>
      </div>
    </div>
  {/each}

  <!-- Government ranks → naming-tables cross-link (ranks index the naming cells) -->
  {#if meta.id === "government_ranks" && onopennaming}
    <div class="section-title">Naming</div>
    <button class="btn" onclick={() => onopennaming?.()}>Naming schemes for this rank →</button>
    <p class="dim small">Government-name schemes hold rank-indexed titles ({key}=…). Opens the Government Names editor.</p>
  {/if}

  <!-- Trigger / effect / weight blocks -->
  {#if obj.scriptBlocks.length > 0}
    <div class="section-title">Conditions, effects &amp; weights</div>
    {#each obj.scriptBlocks as sb (sb.name)}
      <MechanicScriptBlock
        {installPath}
        {modPath}
        {queue}
        {file}
        {basePath}
        name={sb.name}
        registry={sb.registry as "triggers" | "effects"}
        present={sb.present}
        known={sb.registry === "triggers" ? triggers : effects}
        {countries}
      />
    {/each}
  {/if}

  <!-- Linked events (disasters/incidents) -->
  {#if showEvents}
    <div class="section-title">Linked events</div>
    <MechanicEventLinks {installPath} {modPath} objKey={key} eventRefs={obj.eventRefs} {onopenevents} />
  {/if}

  <!-- Availability (country-shaped trigger) -->
  {#if meta.availTrigger}
    <div class="section-title">Availability</div>
    <MechanicAvailability
      {installPath}
      {modPath}
      {date}
      {file}
      {basePath}
      trigger={meta.availTrigger}
      present={obj.scriptBlocks.find((s) => s.name === meta.availTrigger)?.present ?? false}
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
    <button class="btn danger" onclick={onremove}>Delete…</button>
  </div>
</div>

<style>
  .entity-art { display: flex; align-items: center; gap: var(--sp-2); }
  .editor { padding: 0.35rem 0.1rem 0.3rem; display: flex; flex-direction: column; gap: 0.35rem; }
  .field { display: flex; align-items: center; gap: 0.5rem; }
  .field label, .field .lbl, .lbl { width: 9rem; flex: none; font-size: 0.78rem; color: var(--text-2); }
  .txt { flex: 1; min-width: 0; background: var(--bg-0); border: 1px solid var(--border-strong); color: var(--text-1); font-family: inherit; font-size: 0.8rem; padding: 0.15rem 0.35rem; }
  .num { width: 5rem; background: var(--bg-0); border: 1px solid var(--border-strong); color: var(--text-1); font-family: inherit; font-size: 0.8rem; padding: 0.15rem 0.35rem; }
  .num.wide { width: 8rem; }
  .sel { background: var(--bg-0); border: 1px solid var(--border-strong); color: var(--text-1); font-family: inherit; font-size: 0.8rem; padding: 0.15rem 0.25rem; }
  .iconrow, .colorrow { display: flex; align-items: center; gap: 0.4rem; }
  .iconval { color: var(--ok); background: var(--bg-0); padding: 0.05rem 0.3rem; font-size: 0.76rem; }
  .swatch { width: 1.1rem; height: 1.1rem; border: 1px solid var(--border); flex: none; }
  .picker { height: 22rem; margin: 0.2rem 0; }
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
  .pickerslot { flex: 1; min-width: 0; }
  .tokens { display: flex; flex-wrap: wrap; align-items: center; gap: 0.3rem; }
  .chip { display: inline-flex; align-items: center; gap: 0.25rem; background: var(--bg-1); border: 1px solid var(--border); color: var(--text-1); font-size: 0.78rem; padding: 0.08rem 0.2rem 0.08rem 0.4rem; }
  .chip .x { border: none; background: transparent; color: var(--text-2); cursor: pointer; font-size: 0.9rem; line-height: 1; padding: 0 0.15rem; }
  .chip .x:hover { color: var(--err); }
  .selfrows { display: flex; flex-direction: column; gap: 0.2rem; }
  .selfrow { display: flex; align-items: center; gap: 0.4rem; }
  .selfrow .rk { flex: 1; color: var(--text-1); background: var(--bg-0); padding: 0.1rem 0.35rem; font-size: 0.76rem; }
  .rk-in { flex: 1; background: var(--bg-0); border: 1px solid var(--border-strong); color: var(--text-1); font-family: inherit; font-size: 0.78rem; padding: 0.15rem 0.35rem; }
  .addrow { margin-top: 0.15rem; }
  .subgroup { display: flex; flex-direction: column; gap: 0.3rem; }
  .modblock { border: 1px solid var(--bg-1); padding: 0.3rem; }
  .mb-head { display: flex; align-items: center; gap: 0.4rem; margin-bottom: 0.25rem; }
  .mb-head code { color: var(--ok); background: var(--bg-0); padding: 0 0.3rem; font-size: 0.76rem; }
  .step-n { font-size: 0.7rem; color: var(--text-2); width: 1rem; }
  .spacer { flex: 1; }
  .tag-abs, .tag-raw { font-size: 0.65rem; text-transform: uppercase; color: var(--text-2); }
  .tag-raw { color: var(--warn); }
  .idlist { display: flex; flex-wrap: wrap; gap: 0.25rem; }
  .idchip { color: var(--text-1); background: var(--bg-0); padding: 0.05rem 0.3rem; font-size: 0.72rem; }
  .idchip.raw { color: var(--text-2); font-style: italic; }
  .mini { border: 1px solid var(--border-strong); background: var(--bg-2); color: var(--text-1); font-family: inherit; font-size: 0.72rem; padding: 0.05rem 0.4rem; cursor: pointer; }
  .mini:hover:not(:disabled) { border-color: var(--accent); background: var(--accent); color: var(--text-inverse); }
  .mini:disabled { opacity: 0.4; cursor: default; }
  .mini.danger { color: var(--err); border-color: var(--danger-bg); }
  .dim { color: var(--text-2); }
  .small { font-size: 0.74rem; }
  .danger-zone { margin-top: 0.5rem; }
  .btn { border: 1px solid var(--border-strong); background: transparent; color: var(--text-1); font-family: inherit; font-size: 0.78rem; padding: 0.2rem 0.6rem; cursor: pointer; }
  .btn.danger { color: var(--err); border-color: var(--danger-bg); }
  .btn.danger:hover { background: var(--danger-bg); border-color: var(--danger-bg); color: var(--text-inverse); }
</style>
