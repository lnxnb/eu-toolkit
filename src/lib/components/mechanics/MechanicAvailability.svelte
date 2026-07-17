<script lang="ts">
  // Availability/context (14.3): a country-shaped trigger evaluated across every
  // country at the selected date — "potentially available to N countries at
  // start". Honest with Unknown (conditions the evaluator can't resolve are
  // reported as approximate). Mirrors estates/PrivilegeAvailability.
  import { invoke } from "@tauri-apps/api/core";

  interface TriggerEvaluation {
    verdicts: { tag: string; verdict: string }[];
    unevaluated: string[];
  }

  let {
    installPath,
    modPath,
    date = null,
    file,
    basePath,
    trigger,
    present,
  }: {
    installPath: string;
    modPath: string | null;
    date?: string | null;
    file: string;
    basePath: string[];
    trigger: string;
    present: boolean;
  } = $props();

  let result = $state<TriggerEvaluation | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  const yesCount = $derived(result?.verdicts.filter((v) => v.verdict === "yes").length ?? 0);
  const noCount = $derived(result?.verdicts.filter((v) => v.verdict === "no").length ?? 0);
  const unkCount = $derived(result?.verdicts.filter((v) => v.verdict === "unknown").length ?? 0);

  async function load() {
    loading = true;
    error = null;
    try {
      result = await invoke<TriggerEvaluation>("evaluate_trigger", {
        installPath,
        modPath,
        date,
        file,
        path: [...basePath, trigger],
      });
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
</script>

<div class="avail">
  {#if !present}
    <p class="dim">No <code>{trigger}</code> block — availability can't be evaluated.</p>
  {:else if !result && !loading}
    <button class="load" onclick={load}>Evaluate <code>{trigger}</code> across all countries…</button>
  {:else if loading}
    <p class="dim">Evaluating…</p>
  {:else if error}
    <p class="err">{error}</p>
  {:else if result}
    <div class="res">
      <strong>Potentially available to {yesCount}</strong>
      {yesCount === 1 ? "country" : "countries"} at start
      <span class="dim">· {noCount} no · {unkCount} unknown</span>
      {#if result.unevaluated.length > 0}
        <div class="approx">Approximate — {result.unevaluated.length} conditions not evaluated.</div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .avail { margin-top: 0.3rem; font-size: 0.8rem; color: #cfd4db; }
  .load { border: 1px solid #4b5563; background: #2b323d; color: #cfd4db; font-family: inherit; font-size: 0.76rem; padding: 0.2rem 0.5rem; cursor: pointer; }
  .load:hover { border-color: #4a6da7; background: #4a6da7; color: #fff; }
  code { color: #9aecc0; background: #16191f; padding: 0 0.25rem; font-size: 0.74rem; }
  .res { color: #b9bec7; }
  .approx { color: #d0a24a; font-size: 0.76rem; margin-top: 0.15rem; }
  .dim { color: #9ca3af; }
  .err { color: #fca5a5; }
</style>
