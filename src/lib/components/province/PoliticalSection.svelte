<!--
  PoliticalSection — the province panel's 1444 political state (Sprint 2.2):
  owner, controller, cores, claims, is_city, native props (uncolonized), tribal
  owner, HRE, seat in parliament, trade-company assignment. All writes target the
  province history file; changing owner offers a one-composite "add core + set
  controller too?" convenience (default yes).
-->
<script lang="ts">
  import { SearchDropdown } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import FieldRow from "$lib/components/country/FieldRow.svelte";
  import type { EditQueue } from "$lib/edits.svelte";
  import type { ProvinceDetails, ProvinceSnapshot } from "./types";
  import { fieldOps, listAdd, listRemove, toggleFlag, scalarEdit, pushAtDate, type DateCtx } from "./fields";

  let {
    details,
    effective,
    file,
    queue,
    countries,
    dateCtx,
  }: {
    details: ProvinceDetails;
    effective: ProvinceSnapshot;
    file: string;
    queue: EditQueue;
    countries: DropdownItem[];
    /** Sprint 12.3 date context; later dates write into a dated block. */
    dateCtx?: DateCtx;
  } = $props();

  const ops = $derived(fieldOps(queue, file, dateCtx));
  const top = $derived(details.top_level);

  function label(key: string): string {
    return countries.find((c) => c.key === key)?.label ?? key;
  }

  // --- Owner (with add-core + controller convenience composite) ---
  let ownerVal = $derived(ops.val("owner", effective.owner));
  function setOwner(tag: string) {
    if (tag === ownerVal) return;
    const present = top.owner != null;
    const edits = [scalarEdit(file, "owner", tag, present)];
    const statements = [`owner = ${tag}`];
    const also = confirm(
      `Also set controller = ${tag} and add ${tag} as a core?\n(OK = yes, recommended)`,
    );
    if (also) {
      edits.push(scalarEdit(file, "controller", tag, top.controller != null));
      statements.push(`controller = ${tag}`);
      if (!effective.cores.includes(tag)) {
        edits.push(listAdd(file, "add_core", tag));
        statements.push(`add_core = ${tag}`);
      }
    }
    pushAtDate(queue, dateCtx, `Set owner of #${details.id} to ${tag}`, edits, statements);
  }
  function clearOwner() {
    queue.push({ label: `Uncolonize #${details.id}`, edits: [{ kind: "removeStatement", file, blockPath: [], key: "owner" }] });
  }

  // --- Controller ---
  let controllerVal = $derived(ops.val("controller", effective.controller));

  // --- Cores / Claims (repeated top-level keys) ---
  let cores = $derived(queue.pendingList(file, "add_core", effective.cores));
  let claims = $derived(queue.pendingList(file, "add_claim", effective.claims));

  // --- Toggles ---
  let isCity = $derived.by(() => {
    const p = queue.pendingField(file, "is_city");
    return p !== undefined ? p.value === "yes" : effective.is_city === true;
  });
  let hre = $derived.by(() => {
    const p = queue.pendingField(file, "hre");
    return p !== undefined ? p.value === "yes" : effective.hre === true;
  });
  let seat = $derived.by(() => {
    const p = queue.pendingField(file, "seat_in_parliament");
    return p !== undefined ? p.value === "yes" : effective.seat_in_parliament === true;
  });

  // --- Native props (uncolonized) ---
  let uncolonized = $derived(!ownerVal);
  function nativeVal(key: "native_size" | "native_ferocity" | "native_hostileness", base: number | null): string {
    const v = ops.val(key, base != null ? String(base) : null);
    return v ?? "";
  }
  function setNative(key: string, present: boolean, raw: string) {
    const v = raw.trim();
    if (v === "") { if (present) ops.clear(key, `Clear ${key} of #${details.id}`); return; }
    ops.set(key, present, v, `Set ${key} of #${details.id}`);
  }

  // --- Tribal owner + trade company (tag pickers) ---
  let tribalVal = $derived(ops.val("tribal_owner", effective.tribal_owner));
  let companyVal = $derived(ops.val("add_to_trade_company", effective.trade_company));

  function addCore(tag: string) {
    if (!tag || cores.includes(tag)) return;
    pushAtDate(queue, dateCtx, `Add core ${tag} to #${details.id}`, [listAdd(file, "add_core", tag)], [`add_core = ${tag}`]);
  }
  function addClaim(tag: string) {
    if (!tag || claims.includes(tag)) return;
    pushAtDate(queue, dateCtx, `Add claim ${tag} to #${details.id}`, [listAdd(file, "add_claim", tag)], [`add_claim = ${tag}`]);
  }

  /// Boolean flag toggle: turning ON writes `key = yes` into the dated block at a
  /// later date; turning OFF removes the key (top-level — a dated block has no
  /// clean "unset", and flags default off).
  function toggleField(key: string, on: boolean, present: boolean, label: string) {
    if (on) {
      pushAtDate(queue, dateCtx, label, [toggleFlag(file, key, true, present)], [`${key} = yes`]);
    } else {
      queue.push({ label, edits: [toggleFlag(file, key, false, present)] });
    }
  }
</script>

<section>
  <h3>Political</h3>

  <FieldRow label="Owner" edited={ops.edited("owner", effective.owner)}>
    <SearchDropdown items={countries} value={ownerVal} placeholder="(uncolonized)" onselect={setOwner} />
    {#if ownerVal}<button class="mini" title="Uncolonize" onclick={clearOwner}>×</button>{/if}
  </FieldRow>

  <FieldRow label="Controller" edited={ops.edited("controller", effective.controller)}>
    <SearchDropdown
      items={countries}
      value={controllerVal}
      placeholder="(defaults to owner)"
      onselect={(k) => ops.set("controller", top.controller != null, k, `Set controller of #${details.id}`)}
    />
  </FieldRow>

  <div class="list-field">
    <div class="list-label">Cores</div>
    {#each cores as t (t)}
      <span class="chip">{label(t)}<button class="x" onclick={() => queue.push({ label: `Remove core ${t}`, edits: [listRemove(file, "add_core", t)] })}>×</button></span>
    {/each}
    <SearchDropdown items={countries} value={null} placeholder="Add core…" onselect={addCore} />
  </div>

  <div class="list-field">
    <div class="list-label">Claims</div>
    {#each claims as t (t)}
      <span class="chip">{label(t)}<button class="x" onclick={() => queue.push({ label: `Remove claim ${t}`, edits: [listRemove(file, "add_claim", t)] })}>×</button></span>
    {/each}
    <SearchDropdown items={countries} value={null} placeholder="Add claim…" onselect={addClaim} />
  </div>

  <FieldRow label="Colonized (is_city)" edited={queue.pendingField(file, "is_city") !== undefined}>
    <label class="check"><input type="checkbox" checked={isCity} onchange={() => toggleField("is_city", !isCity, top.is_city != null, `Toggle is_city of #${details.id}`)} /><span>{isCity ? "City" : "Not a city"}</span></label>
  </FieldRow>

  <FieldRow label="HRE" edited={queue.pendingField(file, "hre") !== undefined}>
    <label class="check"><input type="checkbox" checked={hre} onchange={() => toggleField("hre", !hre, top.hre != null, `Toggle HRE of #${details.id}`)} /><span>{hre ? "In the Empire" : "Outside"}</span></label>
  </FieldRow>

  <FieldRow label="Seat in Parliament" edited={queue.pendingField(file, "seat_in_parliament") !== undefined}>
    <label class="check"><input type="checkbox" checked={seat} onchange={() => toggleField("seat_in_parliament", !seat, top.seat_in_parliament != null, `Toggle seat_in_parliament of #${details.id}`)} /><span>{seat ? "Yes" : "No"}</span></label>
  </FieldRow>

  <FieldRow label="Trade Company" edited={ops.edited("add_to_trade_company", effective.trade_company)}>
    <SearchDropdown
      items={countries}
      value={companyVal}
      placeholder="(none)"
      onselect={(k) => ops.set("add_to_trade_company", top.trade_company != null, k, `Assign #${details.id} to ${k} trade company`)}
    />
    {#if companyVal}<button class="mini" title="Clear" onclick={() => ops.clear("add_to_trade_company", `Clear trade company of #${details.id}`)}>×</button>{/if}
  </FieldRow>

  {#if uncolonized || effective.native_size != null}
    <div class="native">
      <div class="list-label">Natives (uncolonized)</div>
      <FieldRow label="Tribal Owner" edited={ops.edited("tribal_owner", effective.tribal_owner)}>
        <SearchDropdown items={countries} value={tribalVal} placeholder="(none)" onselect={(k) => ops.set("tribal_owner", top.tribal_owner != null, k, `Set tribal owner of #${details.id}`)} />
      </FieldRow>
      <FieldRow label="Native Size">
        <input class="num" type="number" min="0" value={nativeVal("native_size", effective.native_size)} onchange={(e) => setNative("native_size", top.native_size != null, e.currentTarget.value)} />
      </FieldRow>
      <FieldRow label="Native Ferocity">
        <input class="num" type="number" min="0" step="0.1" value={nativeVal("native_ferocity", effective.native_ferocity)} onchange={(e) => setNative("native_ferocity", top.native_ferocity != null, e.currentTarget.value)} />
      </FieldRow>
      <FieldRow label="Native Hostileness">
        <input class="num" type="number" min="0" value={nativeVal("native_hostileness", effective.native_hostileness)} onchange={(e) => setNative("native_hostileness", top.native_hostileness != null, e.currentTarget.value)} />
      </FieldRow>
    </div>
  {/if}
</section>

<style>
  section { margin-bottom: 1rem; }
  h3 { margin: 0 0 0.5rem; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-2); }
  .list-field { display: flex; flex-direction: column; gap: 0.3rem; margin-bottom: 0.7rem; }
  .list-label { font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.03em; color: var(--text-2); }
  .chip { display: inline-flex; align-items: center; gap: 0.3rem; align-self: flex-start; background: var(--bg-1); border: 1px solid var(--border); color: var(--text-1); font-size: 0.8rem; padding: 0.12rem 0.2rem 0.12rem 0.45rem; }
  .x { border: none; background: transparent; color: var(--text-2); cursor: pointer; font-size: 0.95rem; line-height: 1; padding: 0 0.2rem; }
  .x:hover { color: var(--err); }
  .mini { border: 1px solid var(--border); background: var(--bg-2); color: var(--text-1); cursor: pointer; font-size: 0.85rem; line-height: 1; padding: 0.15rem 0.35rem; }
  .mini:hover { background: var(--danger-bg); color: var(--text-inverse); }
  .check { display: flex; align-items: center; gap: 0.4rem; font-size: 0.85rem; cursor: pointer; }
  .num { width: 5rem; background: var(--bg-1); border: 1px solid var(--border); color: var(--text-1); font-family: inherit; font-size: 0.85rem; padding: 0.25rem 0.4rem; outline: none; }
  .native { border-top: 1px solid var(--border); padding-top: 0.5rem; margin-top: 0.3rem; }
</style>
