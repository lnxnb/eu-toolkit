<!--
  EconomySection — development, trade good + latent, centre of trade, extra cost,
  local autonomy, unrest, and province modifiers (Sprint 2.2). Development steppers
  drive the pending queue; the Development map mode repaints from the same edits
  where the machinery exists (panel + save always correct).

  Modifier block shapes (verified against real files):
    add_permanent_province_modifier = { name = X duration = -1 }   (block)
    add_province_modifier           = { name = X duration = 3650 } (block, temporary)
    add_province_triggered_modifier = X                            (scalar name only)
  Registry pickers: event_modifiers (permanent/temporary) + province_triggered_modifiers.
  Removal of a permanent/temporary modifier is first-match (the byte-writer's value
  filter only disambiguates scalars) — fine for the ~always-unique province case.
-->
<script lang="ts">
  import { SearchDropdown } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import FieldRow from "$lib/components/country/FieldRow.svelte";
  import type { EditQueue } from "$lib/edits.svelte";
  import type { ProvinceDetails, ProvinceSnapshot, RawStatement } from "./types";
  import { fieldOps, pushAtDate, type DateCtx } from "./fields";

  let {
    details,
    effective,
    file,
    queue,
    goods,
    eventModifiers,
    triggeredModifiers,
    dateCtx,
    onopenmechanics,
  }: {
    details: ProvinceDetails;
    effective: ProvinceSnapshot;
    file: string;
    queue: EditQueue;
    goods: DropdownItem[];
    eventModifiers: DropdownItem[];
    triggeredModifiers: DropdownItem[];
    /** Sprint 12.3 date context; later dates write into a dated block. */
    dateCtx?: DateCtx;
    /** Open the Mechanics editor (Sprint 27 W4) at the centers_of_trade tier
     *  definitions from the Centre-of-Trade control. */
    onopenmechanics?: (family: string, key?: string) => void;
  } = $props();

  const ops = $derived(fieldOps(queue, file, dateCtx));
  const top = $derived(details.top_level);

  // --- Development ---
  function devVal(key: "base_tax" | "base_production" | "base_manpower", base: number | null): number {
    const p = ops.val(key, base != null ? String(base) : null);
    return p != null ? parseFloat(p) || 0 : 0;
  }
  let tax = $derived(devVal("base_tax", effective.base_tax));
  let prod = $derived(devVal("base_production", effective.base_production));
  let man = $derived(devVal("base_manpower", effective.base_manpower));
  let devSum = $derived(tax + prod + man);
  function setDev(key: "base_tax" | "base_production" | "base_manpower", present: boolean, raw: string) {
    const v = Math.max(0, Math.round(parseFloat(raw) || 0));
    ops.set(key, present, String(v), `Set ${key} of #${details.id}`);
  }

  // --- Trade good + latent ---
  let goodVal = $derived(ops.val("trade_goods", effective.trade_goods));
  let latentVal = $derived.by(() => {
    const p = queue.pendingBlockValue(file, ["latent_trade_goods"]);
    if (p !== undefined) return p;
    const pf = queue.pendingField(file, "latent_trade_goods");
    if (pf !== undefined) return pf.value;
    return effective.latent_trade_goods;
  });
  function setLatent(key: string) {
    const present = top.latent_trade_goods != null;
    const edit = present
      ? { kind: "setBlock" as const, file, path: ["latent_trade_goods"], value: key }
      : { kind: "insertStatement" as const, file, blockPath: [], statement: `latent_trade_goods = { ${key} }` };
    pushAtDate(queue, dateCtx, `Set latent good of #${details.id}`, [edit], [`latent_trade_goods = { ${key} }`]);
  }

  // --- CoT / extra cost / autonomy / unrest ---
  const COT: DropdownItem[] = [
    { key: "0", label: "None (0)" },
    { key: "1", label: "Emporium (1)" },
    { key: "2", label: "Market Town (2)" },
    { key: "3", label: "World Trade Center (3)" },
  ];
  let cotVal = $derived(ops.val("center_of_trade", effective.center_of_trade != null ? String(effective.center_of_trade) : null) ?? "0");
  let extraVal = $derived(ops.val("extra_cost", effective.extra_cost != null ? String(effective.extra_cost) : null) ?? "");
  let autonomyVal = $derived(ops.val("add_local_autonomy", effective.local_autonomy != null ? String(effective.local_autonomy) : null) ?? "");
  let unrestVal = $derived(ops.val("unrest", effective.unrest != null ? String(effective.unrest) : null) ?? "");

  function setNum(key: string, present: boolean, raw: string, human: string) {
    const v = raw.trim();
    if (v === "") { if (present) ops.clear(key, `Clear ${human} of #${details.id}`); return; }
    ops.set(key, present, String(Math.round(parseFloat(v) || 0)), `Set ${human} of #${details.id}`);
  }

  // --- Province modifiers (verified block/scalar shapes) ---
  const MOD_KEYS = new Set([
    "add_permanent_province_modifier",
    "add_province_modifier",
    "add_province_triggered_modifier",
  ]);
  interface Mod { key: string; name: string; duration: string | null; }
  function parseName(text: string): { name: string; duration: string | null } {
    const nm = text.match(/name\s*=\s*"?([\w.]+)"?/);
    const du = text.match(/duration\s*=\s*(-?\d+)/);
    return { name: nm ? nm[1] : text.trim().replace(/[{}]/g, "").trim(), duration: du ? du[1] : null };
  }
  function diskMods(rem: RawStatement[]): Mod[] {
    return rem
      .filter((r) => MOD_KEYS.has(r.key))
      .map((r) => {
        if (!r.is_block) return { key: r.key, name: r.value.trim(), duration: null };
        const p = parseName(r.value);
        return { key: r.key, name: p.name, duration: p.duration };
      });
  }
  let mods = $derived.by<Mod[]>(() => {
    const list = diskMods(details.raw_remainder).map((m) => ({ m, removed: false }));
    for (const e of queue.serialize()) {
      if (e.kind === "insertStatement" && e.file === file && e.blockPath.length === 0) {
        const k = e.statement.slice(0, e.statement.indexOf("=")).trim();
        if (MOD_KEYS.has(k)) {
          const rest = e.statement.slice(e.statement.indexOf("=") + 1);
          if (k === "add_province_triggered_modifier") list.push({ m: { key: k, name: rest.trim(), duration: null }, removed: false });
          else { const p = parseName(rest); list.push({ m: { key: k, name: p.name, duration: p.duration }, removed: false }); }
        }
      } else if (e.kind === "removeStatement" && e.file === file && e.blockPath.length === 0 && MOD_KEYS.has(e.key)) {
        const i = list.findIndex((x) => !x.removed && x.m.key === e.key && (e.value == null || x.m.name === e.value));
        if (i >= 0) list[i].removed = true;
      }
    }
    return list.filter((x) => !x.removed).map((x) => x.m);
  });

  let addOpen = $state(false);
  let addType = $state<"add_permanent_province_modifier" | "add_province_triggered_modifier">("add_permanent_province_modifier");
  let addName = $state<string | null>(null);
  let addDuration = $state("-1");
  function submitMod() {
    if (!addName) return;
    let statement: string;
    if (addType === "add_province_triggered_modifier") statement = `add_province_triggered_modifier = ${addName}`;
    else statement = `add_permanent_province_modifier = {\n\tname = ${addName}\n\tduration = ${addDuration.trim() || "-1"}\n}`;
    pushAtDate(queue, dateCtx, `Add modifier ${addName} to #${details.id}`, [{ kind: "insertStatement", file, blockPath: [], statement }], [statement]);
    addOpen = false; addName = null;
  }
  function removeMod(m: Mod) {
    const value = m.key === "add_province_triggered_modifier" ? m.name : undefined;
    queue.push({ label: `Remove modifier ${m.name} from #${details.id}`, edits: [{ kind: "removeStatement", file, blockPath: [], key: m.key, ...(value ? { value } : {}) }] });
  }
  function modLabel(key: string): string {
    return (key === "add_province_triggered_modifier" ? triggeredModifiers : eventModifiers).find((i) => i.key === key)?.label ?? key;
  }
</script>

<section>
  <h3>Population &amp; Economy</h3>

  <div class="dev">
    <div class="dev-stepper"><span class="dev-lbl adm">Tax</span><input class="num" type="number" min="0" value={tax} onchange={(e) => setDev("base_tax", top.base_tax != null, e.currentTarget.value)} /></div>
    <div class="dev-stepper"><span class="dev-lbl dip">Prod</span><input class="num" type="number" min="0" value={prod} onchange={(e) => setDev("base_production", top.base_production != null, e.currentTarget.value)} /></div>
    <div class="dev-stepper"><span class="dev-lbl mil">Man</span><input class="num" type="number" min="0" value={man} onchange={(e) => setDev("base_manpower", top.base_manpower != null, e.currentTarget.value)} /></div>
    <div class="dev-total">Σ {devSum}</div>
  </div>

  <FieldRow label="Trade Good" edited={ops.edited("trade_goods", effective.trade_goods)}>
    <SearchDropdown items={goods} value={goodVal} placeholder="Trade good…" onselect={(k) => ops.set("trade_goods", top.trade_goods != null, k, `Set trade good of #${details.id}`)} />
  </FieldRow>

  <FieldRow label="Latent Trade Good" edited={queue.pendingBlockValue(file, ["latent_trade_goods"]) !== undefined || queue.pendingField(file, "latent_trade_goods") !== undefined}>
    <SearchDropdown items={goods} value={latentVal} placeholder="(none)" onselect={setLatent} />
    {#if latentVal}<button class="mini" title="Clear" onclick={() => ops.clear("latent_trade_goods", `Clear latent good of #${details.id}`)}>×</button>{/if}
  </FieldRow>

  <FieldRow label="Centre of Trade" edited={ops.edited("center_of_trade", effective.center_of_trade != null ? String(effective.center_of_trade) : null)}>
    <SearchDropdown items={COT} value={cotVal} onselect={(k) => ops.set("center_of_trade", top.center_of_trade != null, k, `Set CoT of #${details.id}`)} />
    {#if onopenmechanics}<button class="mini" title="Edit centre-of-trade tier definitions (level, cost, modifiers)" onclick={() => onopenmechanics?.("centers_of_trade")}>tiers…</button>{/if}
  </FieldRow>

  <FieldRow label="Extra Cost" edited={ops.edited("extra_cost", effective.extra_cost != null ? String(effective.extra_cost) : null)}>
    <input class="num" type="number" value={extraVal} placeholder="(none)" onchange={(e) => setNum("extra_cost", top.extra_cost != null, e.currentTarget.value, "extra cost")} />
  </FieldRow>

  <FieldRow label="Local Autonomy" edited={ops.edited("add_local_autonomy", effective.local_autonomy != null ? String(effective.local_autonomy) : null)}>
    <input class="num" type="number" min="0" max="100" value={autonomyVal} placeholder="(none)" onchange={(e) => setNum("add_local_autonomy", top.local_autonomy != null, e.currentTarget.value, "autonomy")} />
  </FieldRow>

  <FieldRow label="Unrest" edited={ops.edited("unrest", effective.unrest != null ? String(effective.unrest) : null)}>
    <input class="num" type="number" value={unrestVal} placeholder="(none)" onchange={(e) => setNum("unrest", top.unrest != null, e.currentTarget.value, "unrest")} />
  </FieldRow>

  <div class="list-field">
    <div class="list-label">Province Modifiers</div>
    {#each mods as m}
      <span class="chip">
        <span class="mtype" class:trig={m.key === "add_province_triggered_modifier"}>{m.key === "add_province_triggered_modifier" ? "T" : "P"}</span>
        {modLabel(m.name)}{#if m.duration && m.duration !== "-1"}<span class="dim"> · {m.duration}d</span>{/if}
        <button class="x" onclick={() => removeMod(m)}>×</button>
      </span>
    {/each}
    {#if addOpen}
      <div class="add-mod">
        <select bind:value={addType} class="sel">
          <option value="add_permanent_province_modifier">Permanent</option>
          <option value="add_province_triggered_modifier">Triggered</option>
        </select>
        <SearchDropdown items={addType === "add_province_triggered_modifier" ? triggeredModifiers : eventModifiers} bind:value={addName} placeholder="Modifier…" />
        {#if addType === "add_permanent_province_modifier"}
          <input class="num" type="number" bind:value={addDuration} title="Duration (days; -1 = permanent)" />
        {/if}
        <button class="mini ok" disabled={!addName} onclick={submitMod}>Add</button>
        <button class="mini" onclick={() => (addOpen = false)}>Cancel</button>
      </div>
    {:else}
      <button class="add-btn" onclick={() => (addOpen = true)}>+ Add modifier</button>
    {/if}
  </div>
</section>

<style>
  section { margin-bottom: 1rem; }
  h3 { margin: 0 0 0.5rem; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; color: #9ca3af; }
  .dev { display: flex; align-items: flex-end; gap: 0.5rem; margin-bottom: 0.6rem; }
  .dev-stepper { display: flex; flex-direction: column; gap: 0.15rem; }
  .dev-lbl { font-size: 0.68rem; text-transform: uppercase; color: #8a919c; }
  .dev-lbl.adm { color: #7fbf6f; }
  .dev-lbl.dip { color: #7f9fd0; }
  .dev-lbl.mil { color: #d07f7f; }
  .dev-total { margin-left: auto; font-weight: 700; font-variant-numeric: tabular-nums; }
  .num { width: 4rem; background: #21262e; border: 1px solid #1f242c; color: #cfd4db; font-family: inherit; font-size: 0.85rem; padding: 0.25rem 0.4rem; outline: none; }
  .sel { background: #21262e; border: 1px solid #1f242c; color: #cfd4db; font-family: inherit; font-size: 0.82rem; padding: 0.2rem; }
  .list-field { display: flex; flex-direction: column; gap: 0.3rem; margin-bottom: 0.7rem; }
  .list-label { font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.03em; color: #8a919c; }
  .chip { display: inline-flex; align-items: center; gap: 0.3rem; align-self: flex-start; background: #21262e; border: 1px solid #1f242c; color: #cfd4db; font-size: 0.8rem; padding: 0.12rem 0.2rem 0.12rem 0.35rem; }
  .mtype { background: #3a5a86; color: #fff; font-size: 0.62rem; padding: 0.05rem 0.28rem; }
  .mtype.trig { background: #6b46c1; }
  .dim { color: #8a919c; }
  .x { border: none; background: transparent; color: #9ca3af; cursor: pointer; font-size: 0.95rem; line-height: 1; padding: 0 0.2rem; }
  .x:hover { color: #fca5a5; }
  .add-mod { display: flex; align-items: center; gap: 0.3rem; flex-wrap: wrap; }
  .add-btn { align-self: flex-start; border: 1px solid #1f242c; background: #3f4855; color: #cfd4db; font-family: inherit; font-size: 0.8rem; padding: 0.2rem 0.5rem; cursor: pointer; }
  .add-btn:hover { background: #4a6da7; color: #fff; }
  .mini { border: 1px solid #1f242c; background: #2b323d; color: #cfd4db; cursor: pointer; font-size: 0.8rem; padding: 0.2rem 0.4rem; }
  .mini:hover { background: #4a6da7; color: #fff; }
  .mini.ok { background: #3a5a86; }
  .mini:disabled { opacity: 0.5; cursor: default; }
</style>
