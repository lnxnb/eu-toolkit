<script lang="ts">
  // Privilege availability/context: which countries grant it at start (scan), and
  // a 14.3 evaluation of its can_select where evaluable.
  import { invoke } from "@tauri-apps/api/core";
  import type { DropdownItem } from "$lib/components/ui";
  import type { PrivilegeHolder } from "$lib/estates";

  interface TriggerEvaluation {
    verdicts: { tag: string; verdict: string }[];
    unevaluated: string[];
  }

  let {
    installPath,
    modPath,
    date = null,
    file,
    objKey,
    hasCanSelect,
    countries = [],
    onopencountry,
  }: {
    installPath: string;
    modPath: string | null;
    date?: string | null;
    file: string;
    objKey: string;
    hasCanSelect: boolean;
    countries?: DropdownItem[];
    onopencountry?: (tag: string) => void;
  } = $props();

  let holders = $state<PrivilegeHolder[] | null>(null);
  let evalResult = $state<TriggerEvaluation | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  const nameOf = (tag: string) => countries.find((c) => c.key === tag)?.label ?? tag;

  async function load() {
    loading = true;
    error = null;
    try {
      holders = await invoke<PrivilegeHolder[]>("get_privilege_holders", {
        installPath,
        modPath,
        privilege: objKey,
      });
      if (hasCanSelect) {
        evalResult = await invoke<TriggerEvaluation>("evaluate_trigger", {
          installPath,
          modPath,
          date,
          file,
          path: [objKey, "can_select"],
        });
      } else {
        evalResult = null;
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  const yesCount = $derived(evalResult?.verdicts.filter((v) => v.verdict === "yes").length ?? 0);
</script>

<div class="avail">
  {#if !holders && !loading}
    <button class="load" onclick={load}>Show availability & who starts with it…</button>
  {:else if loading}
    <p class="dim">Scanning…</p>
  {:else if error}
    <p class="err">{error}</p>
  {:else if holders}
    <div class="held">
      <strong>Held at start by {holders.length}</strong>
      {holders.length === 1 ? "country" : "countries"}
      {#if holders.length > 0}
        <div class="tags">
          {#each holders as h (h.tag)}
            <button
              class="tag"
              title={h.date ? `granted ${h.date}` : "granted at start"}
              onclick={() => onopencountry?.(h.tag)}
            >
              {nameOf(h.tag)}{#if h.date}<span class="dt">·{h.date}</span>{/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
    {#if hasCanSelect && evalResult}
      <div class="ev">
        <strong>can_select</strong>: {yesCount} yes,
        {evalResult.verdicts.filter((v) => v.verdict === "no").length} no,
        {evalResult.verdicts.filter((v) => v.verdict === "unknown").length} unknown
        {#if evalResult.unevaluated.length > 0}
          <span class="approx">(approximate — {evalResult.unevaluated.length} conditions not evaluated)</span>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .avail {
    margin-top: 0.4rem;
    font-size: 0.8rem;
    color: #cfd4db;
  }
  .load {
    border: 1px solid #4b5563;
    background: #2b323d;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.76rem;
    padding: 0.2rem 0.5rem;
    cursor: pointer;
  }
  .load:hover {
    border-color: #4a6da7;
    background: #4a6da7;
    color: #fff;
  }
  .held {
    margin-bottom: 0.3rem;
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
    margin-top: 0.25rem;
  }
  .tag {
    border: 1px solid #2f3946;
    background: #1b2027;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.72rem;
    padding: 0.1rem 0.35rem;
    cursor: pointer;
  }
  .tag:hover {
    border-color: #4a6da7;
    background: #303844;
  }
  .dt {
    color: #8a919c;
    margin-left: 0.15rem;
  }
  .ev {
    color: #b9bec7;
  }
  .approx {
    color: #d0a24a;
  }
  .dim {
    color: #9ca3af;
  }
  .err {
    color: #fca5a5;
  }
</style>
