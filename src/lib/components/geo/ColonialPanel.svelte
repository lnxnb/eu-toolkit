<!--
  ColonialPanel — Sprint 19.2 side panel for a colonial region / trade company.

  Reads the *effective* entry (base + pending) passed by MapView, so name / color
  / member count / weight tables / naming rules reflect queued edits and undo/redo.
  Membership painting happens on the map with the Add/Remove brush; this panel
  handles: the localized name (loc override), color swatch (live client-side
  repaint via the queue fold), the ordered naming-rules editor (14.2 trigger tree
  per row; first match wins), the colonization-outcome weight tables + native/tax
  steppers (colonial regions only), preserve-unknown raw keys, and delete.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { SidePanel, ColorPicker, SearchDropdown } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import type { KnownKey } from "$lib/components/script";
  import FieldRow from "../country/FieldRow.svelte";
  import ValidationStrip, { type ValidationIssue, type JumpTarget } from "../ValidationStrip.svelte";
  import NamingRuleRow from "./NamingRuleRow.svelte";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import { nameLocKey, uniqueKey, type ColonialData, type ColonialEntry } from "$lib/colonial";

  let {
    installPath,
    modPath,
    queue,
    data,
    entry,
    countries = [],
    issues,
    onclose,
    onjump,
    ondeleted,
    onopenmechanics,
  }: {
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
    data: ColonialData;
    entry: ColonialEntry;
    countries?: DropdownItem[];
    issues: ValidationIssue[];
    onclose: () => void;
    onjump: (j: JumpTarget) => void;
    ondeleted: () => void;
    /** Open the Mechanics editor (Sprint 27 W4) at the trade-company
     *  investments family (trade_companies kind only). */
    onopenmechanics?: (family: string, key?: string) => void;
  } = $props();

  // Known-trigger registry for the naming-rule condition trees (fetched once).
  let triggers = $state<KnownKey[]>([]);
  $effect(() => {
    let cancelled = false;
    (async () => {
      try {
        const t = await invoke<KnownKey[]>("get_known_triggers");
        if (!cancelled) triggers = t;
      } catch {
        /* tree editor still renders; leaf key dropdown is just empty */
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  const key = $derived(entry.key);
  const file = $derived(entry.source_file || data.project_file);
  const kindLabel = $derived(data.kind === "colonial_regions" ? "Colonial region" : "Trade company");

  // --- Registries for the weight-table pickers (colonial only) ---
  let goodItems = $state<DropdownItem[]>([]);
  let cultureItems = $state<DropdownItem[]>([]);
  let religionItems = $state<DropdownItem[]>([]);
  $effect(() => {
    if (!data.has_weight_tables) return;
    void installPath;
    void modPath;
    let cancelled = false;
    (async () => {
      try {
        const [goods, cultures, religions] = await Promise.all([
          invoke<{ goods: { key: string; localized_name?: string }[] }>("get_trade_goods", { installPath, modPath }),
          invoke<{ key: string; name: string }[]>("list_cultures", { installPath, modPath }),
          invoke<{ key: string; name: string }[]>("list_religions", { installPath, modPath }),
        ]);
        if (cancelled) return;
        goodItems = goods.goods.map((g) => ({ key: g.key, label: g.localized_name || g.key }));
        cultureItems = cultures.map((c) => ({ key: c.key, label: c.name || c.key }));
        religionItems = religions.map((r) => ({ key: r.key, label: r.name || r.key }));
      } catch {
        /* pickers fall back to whatever loaded */
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  // --- Name (loc override on the primary/first naming rule's loc key) ---
  const primaryRule = $derived(entry.names[0] ?? null);
  const primaryKey = $derived(primaryRule?.name_key ?? "");
  const pendingName = $derived(primaryKey ? queue.pendingLocOverride(primaryKey) : undefined);
  const titleName = $derived(pendingName ?? entry.name ?? key);
  function commitName(v: string) {
    if (!primaryKey) return;
    queue.push({
      label: `Rename ${key}`,
      edits: [{ kind: "locOverride", key: primaryKey, value: v }],
      coalesceKey: `colname:${primaryKey}`,
    });
  }

  // --- Color (live repaint via the queue fold) ---
  const colorRGB = $derived({ r: entry.color[0], g: entry.color[1], b: entry.color[2] });
  const colorEdited = $derived(
    queue.findLast(
      (e) =>
        (e.kind === "setBlock" && e.file === file && e.path.length === 2 && e.path[0] === key && e.path[1] === "color") ||
        (e.kind === "insertStatement" && e.file === file && e.blockPath.length === 1 && e.blockPath[0] === key && /^color\s*=/.test(e.statement)),
    ) !== undefined,
  );
  function commitColor(c: { r: number; g: number; b: number }) {
    const edit: TypedEdit = entry.has_color
      ? { kind: "setBlock", file, path: [key, "color"], value: `${c.r} ${c.g} ${c.b}` }
      : { kind: "insertStatement", file, blockPath: [key], statement: `color = { ${c.r} ${c.g} ${c.b} }` };
    queue.push({ label: `Set color of ${key}`, edits: [edit], coalesceKey: `colcolor:${key}` });
  }
  function css(c: [number, number, number]): string {
    return `rgb(${c[0]}, ${c[1]}, ${c[2]})`;
  }

  // --- Naming rules (ordered; first match wins) ---
  function addNamingRule() {
    const existing = new Set(entry.names.map((n) => n.name_key));
    const nk = uniqueKey(nameLocKey(key, `Name${entry.names.length + 1}`), (k) => existing.has(k));
    queue.push({
      label: `Add naming rule to ${key}`,
      edits: [
        { kind: "insertStatement", file, blockPath: [key], statement: `names = {\n\tname = "${nk}"\n}` },
        { kind: "locOverride", key: nk, value: entry.name },
      ],
    });
  }
  function removeNamingRule(idx: number) {
    if (!confirm(`Delete naming rule #${idx + 1}?`)) return;
    queue.push({
      label: `Remove naming rule from ${key}`,
      edits: [{ kind: "removeStatement", file, blockPath: [key], key: `names#${idx}` }],
    });
  }
  /** Reorder by swapping two adjacent rules' block bodies (byte-surgical). */
  function moveNamingRule(idx: number, delta: number) {
    const j = idx + delta;
    if (j < 0 || j >= entry.names.length) return;
    const innerOf = (raw: string) => {
      const s = raw.indexOf("{");
      const e = raw.lastIndexOf("}");
      return s >= 0 && e > s ? raw.slice(s + 1, e) : raw;
    };
    const a = entry.names[idx];
    const b = entry.names[j];
    queue.push({
      label: `Reorder naming rules of ${key}`,
      edits: [
        { kind: "setBlock", file, path: [key, `names#${idx}`], value: innerOf(b.raw) },
        { kind: "setBlock", file, path: [key, `names#${j}`], value: innerOf(a.raw) },
      ],
    });
  }

  // --- Weight tables (colonial regions only) ---
  type TableName = "trade_goods" | "culture" | "religion";
  const tables: { name: TableName; label: string; items: () => DropdownItem[] }[] = [
    { name: "trade_goods", label: "Trade goods", items: () => goodItems },
    { name: "culture", label: "Culture", items: () => cultureItems },
    { name: "religion", label: "Religion", items: () => religionItems },
  ];
  function rowsOf(name: TableName) {
    return name === "trade_goods" ? entry.trade_goods : name === "culture" ? entry.culture : entry.religion;
  }
  function fmtNum(n: number): string {
    return Number.isInteger(n) ? String(n) : String(n);
  }
  function setWeight(name: TableName, rowKey: string, v: number) {
    queue.push({
      label: `Set ${rowKey} weight`,
      edits: [{ kind: "setScalar", file, path: [key, name, rowKey], value: fmtNum(v), quoted: false }],
      coalesceKey: `colw:${key}:${name}:${rowKey}`,
    });
  }
  function addWeight(name: TableName, rowKey: string) {
    if (!rowKey || rowsOf(name).some((r) => r.key === rowKey)) return;
    queue.push({
      label: `Add ${rowKey} to ${name}`,
      edits: [{ kind: "insertStatement", file, blockPath: [key, name], statement: `${rowKey} = 1` }],
    });
  }
  function removeWeight(name: TableName, rowKey: string) {
    queue.push({
      label: `Remove ${rowKey} from ${name}`,
      edits: [{ kind: "removeStatement", file, blockPath: [key, name], key: rowKey }],
    });
  }

  // --- Native / tax steppers (colonial regions only) ---
  const scalarFields: { field: "tax_income" | "native_size" | "native_ferocity" | "native_hostileness"; label: string }[] = [
    { field: "tax_income", label: "Tax income" },
    { field: "native_size", label: "Native size" },
    { field: "native_ferocity", label: "Native ferocity" },
    { field: "native_hostileness", label: "Native hostileness" },
  ];
  function scalarValue(field: string): number {
    const v = (entry as unknown as Record<string, number | null>)[field];
    return v ?? 0;
  }
  function scalarPresent(field: string): boolean {
    return (entry as unknown as Record<string, number | null>)[field] != null;
  }
  function setScalar(field: string, v: number) {
    const value = String(Math.max(0, Math.round(v)));
    const edit: TypedEdit = scalarPresent(field)
      ? { kind: "setScalar", file, path: [key, field], value, quoted: false }
      : { kind: "insertStatement", file, blockPath: [key], statement: `${field} = ${value}` };
    queue.push({ label: `Set ${field} of ${key}`, edits: [edit], coalesceKey: `cols:${key}:${field}` });
  }

  // --- Delete entry ---
  function deleteEntry() {
    if (!confirm(`Delete ${kindLabel.toLowerCase()} "${titleName}"?\n\nIts ${entry.provinces.length} province(s) become unassigned.`)) return;
    queue.push({
      label: `Delete ${key}`,
      edits: [{ kind: "removeStatement", file, blockPath: [], key }],
    });
    ondeleted();
  }
</script>

<SidePanel title={titleName} {onclose}>
  {#snippet header()}
    <div class="head">
      <span class="swatch" style="background: {css(entry.color)}"></span>
      <span class="key-chip">{key}</span>
    </div>
  {/snippet}

  <div class="strip-wrap">
    <ValidationStrip {issues} {onjump} title={kindLabel} />
  </div>

  <section>
    <h3>{kindLabel}</h3>
    <FieldRow label="Name" edited={pendingName !== undefined}>
      {#if primaryKey}
        <input class="text" value={titleName} oninput={(e) => commitName((e.target as HTMLInputElement).value)} />
      {:else}
        <span class="dim small">Add a naming rule below to give this {kindLabel.toLowerCase()} a name.</span>
      {/if}
    </FieldRow>
    <FieldRow label="Key"><span class="mono">{key}</span></FieldRow>
    <FieldRow label="Color" edited={colorEdited}>
      <ColorPicker value={colorRGB} onchange={commitColor} />
    </FieldRow>
    <FieldRow label="Provinces"><span>{entry.provinces.length}</span></FieldRow>
    {#if onopenmechanics && data.kind === "trade_companies"}
      <FieldRow label="Investments">
        <button class="link" title="Edit trade-company investment definitions" onclick={() => onopenmechanics?.("tradecompany_investments")}>investments…</button>
      </FieldRow>
    {/if}
  </section>

  <section>
    <h3>Naming rules ({entry.names.length})</h3>
    <p class="dim small">First matching rule wins (top → bottom). The condition is usually the colonizing overlord's primary culture / culture group / tag.</p>
    {#each entry.names as rule (rule.index)}
      <NamingRuleRow
        {installPath}
        {modPath}
        {queue}
        {file}
        entryKey={key}
        {rule}
        ruleCount={entry.names.length}
        {triggers}
        {countries}
        onmove={(d) => moveNamingRule(rule.index, d)}
        onremove={() => removeNamingRule(rule.index)}
      />
    {/each}
    <button class="btn wide" onclick={addNamingRule}>＋ Add naming rule</button>
  </section>

  {#if data.has_weight_tables}
    <section>
      <h3>Colonization outcome weights</h3>
      <p class="dim small">Weighted picks for a Random New World colony formed here.</p>
      {#each tables as t (t.name)}
        <div class="wtable">
          <div class="wt-head">{t.label}</div>
          {#each rowsOf(t.name) as row (row.key)}
            <div class="wrow">
              <span class="wkey mono">{row.key}</span>
              <input
                class="wnum"
                type="number"
                min="0"
                step="1"
                value={row.weight}
                oninput={(e) => setWeight(t.name, row.key, parseFloat((e.target as HTMLInputElement).value) || 0)}
              />
              <button class="wdel" aria-label="Remove" title="Remove" onclick={() => removeWeight(t.name, row.key)}>×</button>
            </div>
          {/each}
          <div class="wadd">
            <SearchDropdown
              items={t.items()}
              placeholder={`Add ${t.label.toLowerCase()}…`}
              value={null}
              onselect={(k) => addWeight(t.name, k)}
            />
          </div>
        </div>
      {/each}
    </section>

    <section>
      <h3>Random New World</h3>
      {#each scalarFields as s (s.field)}
        <FieldRow label={s.label}>
          <input
            class="wnum"
            type="number"
            min="0"
            step="1"
            value={scalarValue(s.field)}
            oninput={(e) => setScalar(s.field, parseFloat((e.target as HTMLInputElement).value) || 0)}
          />
        </FieldRow>
      {/each}
    </section>
  {/if}

  {#if entry.raw_extra.length > 0}
    <section>
      <h3>Advanced (read-only)</h3>
      <p class="dim small">Unmodeled keys, preserved untouched on save.</p>
      <ul class="raw">
        {#each entry.raw_extra as r (r)}<li><span class="mono">{r}</span></li>{/each}
      </ul>
    </section>
  {/if}

  <section class="hint">
    <p class="dim small">
      Paint provinces into this {kindLabel.toLowerCase()} with the Add / Remove brush below. A province belongs to one — painting steals it from its previous one.
    </p>
  </section>

  <section>
    <button class="btn danger wide" onclick={deleteEntry}>Delete {kindLabel.toLowerCase()}…</button>
  </section>
</SidePanel>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .swatch {
    width: 1rem;
    height: 1rem;
    display: inline-block;
    border: 1px solid #1f242c;
  }
  .key-chip {
    font-size: 0.8rem;
    color: #9ca3af;
  }
  .strip-wrap {
    margin: -0.2rem 0 0.4rem;
  }
  section {
    padding: 0.4rem 0 0.6rem;
    border-bottom: 1px solid #232a33;
  }
  section.hint {
    border-bottom: none;
  }
  h3 {
    margin: 0 0 0.4rem;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #9ca3af;
  }
  .text {
    width: 100%;
    background: #14181d;
    border: 1px solid #4b5563;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.2rem 0.4rem;
  }
  .mono {
    font-family: ui-monospace, monospace;
    color: #9ca3af;
    font-size: 0.82rem;
  }
  .link {
    border: 1px solid #4b5563;
    background: #2b323d;
    color: #9cc7ea;
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.1rem 0.4rem;
    cursor: pointer;
  }
  .link:hover {
    border-color: #4a6da7;
    color: #fff;
  }
  .wtable {
    margin-bottom: 0.5rem;
  }
  .wt-head {
    font-size: 0.74rem;
    color: #cfd4db;
    margin-bottom: 0.2rem;
  }
  .wrow {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.1rem 0;
  }
  .wkey {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .wnum {
    width: 4rem;
    background: #14181d;
    border: 1px solid #4b5563;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.15rem 0.3rem;
    text-align: right;
  }
  .wdel {
    border: none;
    background: transparent;
    color: #8a919c;
    font-size: 1.1rem;
    line-height: 1;
    cursor: pointer;
    padding: 0 0.2rem;
  }
  .wdel:hover {
    color: #fca5a5;
  }
  .wadd {
    margin-top: 0.2rem;
  }
  .raw {
    list-style: none;
    margin: 0;
    padding: 0;
    font-size: 0.8rem;
    color: #cfd4db;
  }
  .btn {
    border: 1px solid #4b5563;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.2rem 0.6rem;
    cursor: pointer;
  }
  .btn.wide {
    width: 100%;
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
  .dim {
    color: #9ca3af;
  }
  .small {
    font-size: 0.76rem;
  }
</style>
