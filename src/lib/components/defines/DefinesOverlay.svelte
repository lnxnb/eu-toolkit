<!--
  DefinesOverlay (View ▸ Defines…) — Sprint 28.

  A searchable, namespace-grouped tree of every scalar `NDefines.<NS>.<KEY>`
  define (base + mod, additive last-wins) with typed values. Editing queues a
  SetDefine override (generalizes Sprint 12's START_DATE writer to any namespace);
  overridden defines are highlighted against their base value (diff view).
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { OverlaySurface } from "$lib/components/script";
  import type { EditQueue } from "$lib/edits.svelte";

  interface DefineEntry {
    namespace: string;
    key: string;
    dotted: string;
    value: string;
    valueType: "number" | "string" | "bool";
    baseValue: string | null;
    overridden: boolean;
  }

  let {
    open = $bindable(false),
    installPath,
    modPath,
    queue,
  }: {
    open?: boolean;
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
  } = $props();

  let all = $state<DefineEntry[]>([]);
  let query = $state("");
  let onlyOverridden = $state(false);
  let collapsed = $state<Set<string>>(new Set());
  // Session edits (dotted -> value), for optimistic diff + input value.
  let edits = $state<Map<string, string>>(new Map());

  $effect(() => {
    void open;
    void installPath;
    void modPath;
    if (open) void invoke<DefineEntry[]>("get_defines", { installPath, modPath }).then((d) => (all = d));
  });

  const q = $derived(query.trim().toLowerCase());
  const filtered = $derived(
    all.filter((d) => {
      if (onlyOverridden && !(d.overridden || edits.has(d.dotted))) return false;
      if (!q) return true;
      return d.key.toLowerCase().includes(q) || d.namespace.toLowerCase().includes(q) || d.dotted.toLowerCase().includes(q);
    }),
  );

  // Group by namespace, preserving first-seen order.
  const groups = $derived.by(() => {
    const out: { ns: string; rows: DefineEntry[] }[] = [];
    for (const d of filtered) {
      let g = out.find((x) => x.ns === d.namespace);
      if (!g) { g = { ns: d.namespace, rows: [] }; out.push(g); }
      g.rows.push(d);
    }
    return out;
  });

  function currentValue(d: DefineEntry): string {
    return edits.get(d.dotted) ?? d.value;
  }
  function isOverridden(d: DefineEntry): boolean {
    return edits.has(d.dotted) || d.overridden;
  }

  function setValue(d: DefineEntry, value: string) {
    if (value === currentValue(d)) return;
    edits.set(d.dotted, value);
    edits = new Map(edits);
    queue.push({
      label: `Define ${d.namespace}.${d.key}`,
      edits: [{ kind: "setDefine", key: d.key, value, namespace: d.namespace }],
    });
  }

  function resetToBase(d: DefineEntry) {
    if (d.baseValue === null) return;
    setValue(d, d.baseValue);
  }

  function toggleGroup(ns: string) {
    if (collapsed.has(ns)) collapsed.delete(ns); else collapsed.add(ns);
    collapsed = new Set(collapsed);
  }

  const overriddenCount = $derived(all.filter((d) => d.overridden || edits.has(d.dotted)).length);
</script>

<OverlaySurface bind:open title="Defines (NDefines)">
  <div class="bar">
    <input class="q" placeholder="Search defines…" bind:value={query} />
    <button class="chip" class:on={onlyOverridden} onclick={() => (onlyOverridden = !onlyOverridden)}>
      Overridden only{#if overriddenCount} ({overriddenCount}){/if}
    </button>
    <span class="count">{filtered.length} of {all.length}</span>
  </div>

  <div class="tree">
    {#each groups as g (g.ns)}
      <div class="ns">
        <button class="ns-head" onclick={() => toggleGroup(g.ns)}>
          <span class="caret">{collapsed.has(g.ns) ? "▸" : "▾"}</span>
          <span class="ns-name">{g.ns}</span>
          <span class="ns-count">{g.rows.length}</span>
        </button>
        {#if !collapsed.has(g.ns)}
          <table class="rows">
            <tbody>
              {#each g.rows as d (d.dotted)}
                <tr class:overridden={isOverridden(d)}>
                  <td class="k"><code title={d.dotted}>{d.key}</code></td>
                  <td class="v">
                    {#if d.valueType === "bool"}
                      <button class="toggle" class:on={currentValue(d) === "true"} onclick={() => setValue(d, currentValue(d) === "true" ? "false" : "true")}>
                        {currentValue(d)}
                      </button>
                    {:else if d.valueType === "number"}
                      <input type="number" step="any" value={currentValue(d)} onchange={(e) => setValue(d, e.currentTarget.value)} />
                    {:else}
                      <input type="text" value={currentValue(d)} onchange={(e) => setValue(d, e.currentTarget.value)} />
                    {/if}
                  </td>
                  <td class="t">{d.valueType}</td>
                  <td class="base">
                    {#if isOverridden(d) && d.baseValue !== null}
                      <span class="base-val" title="Base value">base: {d.baseValue}</span>
                      <button class="reset" title="Reset to base" onclick={() => resetToBase(d)}>↺</button>
                    {:else if isOverridden(d)}
                      <span class="added">mod-added</span>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>
    {/each}
    {#if groups.length === 0}
      <p class="dim">No matching defines.</p>
    {/if}
  </div>
</OverlaySurface>

<style>
  .bar { display: flex; align-items: center; gap: 0.6rem; margin-bottom: 0.5rem; }
  .q { flex: 1; background: #16191f; border: 1px solid #1f242c; color: #cfd4db; padding: 0.3rem 0.5rem; font-family: inherit; font-size: 0.85rem; }
  .chip { border: 1px solid #3a434f; background: #2b323d; color: #cfd4db; font-family: inherit; font-size: 0.74rem; padding: 0.2rem 0.6rem; cursor: pointer; white-space: nowrap; }
  .chip.on { background: #4a6da7; border-color: #4a6da7; color: #fff; }
  .count { font-size: 0.72rem; color: #8a919c; white-space: nowrap; }
  .ns { margin-bottom: 0.25rem; }
  .ns-head { display: flex; align-items: center; gap: 0.4rem; width: 100%; text-align: left; border: none; background: #21262e; color: #cfd4db; font-family: inherit; font-size: 0.82rem; padding: 0.25rem 0.4rem; cursor: pointer; }
  .caret { color: #8a919c; width: 0.8rem; }
  .ns-name { font-weight: 700; color: #9aecc0; }
  .ns-count { color: #8a919c; font-size: 0.72rem; }
  .rows { width: 100%; border-collapse: collapse; font-size: 0.78rem; }
  .rows td { padding: 0.1rem 0.4rem; border-bottom: 1px solid #21262e; vertical-align: middle; }
  tr.overridden td.k code { color: #ffd479; }
  tr.overridden { background: #262218; }
  td.k { width: 40%; }
  td.k code { color: #cfd4db; background: #16191f; padding: 0 0.3rem; }
  td.v { width: 30%; }
  td.v input { width: 100%; background: #16191f; border: 1px solid #1f242c; color: #cfd4db; padding: 0.15rem 0.35rem; font-family: inherit; font-size: 0.78rem; }
  .toggle { border: 1px solid #1f242c; background: #21262e; color: #cfd4db; font-family: inherit; font-size: 0.76rem; padding: 0.12rem 0.6rem; cursor: pointer; }
  .toggle.on { background: #4a6da7; color: #fff; }
  td.t { color: #8a919c; font-size: 0.7rem; width: 5rem; }
  td.base { color: #8a919c; font-size: 0.72rem; white-space: nowrap; }
  .base-val { color: #d8b45a; }
  .added { color: #9aecc0; }
  .reset { border: none; background: transparent; color: #8a919c; cursor: pointer; font-size: 0.9rem; padding: 0 0.2rem; }
  .reset:hover { color: #fff; }
  .dim { color: #8a919c; font-size: 0.8rem; }
</style>
