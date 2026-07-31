<!--
  IncidentOptions — the numbered AI-weight option blocks of an imperial incident
  (`0 = { factor … }`, `1 = { … }`), Sprint 29. **Verified reality**: an imperial
  incident's "options" are not full event options (name/tooltip/effect) but the
  per-index AI selection weights (`factor` + trigger modifiers) that steer which
  option of the driving `event` the AI picks; `default_option` is the fallback.
  Each is edited via the shared 14.2 tree editor (MechanicScriptBlock) at path
  [incidentKey, "<index>"]. Indices are read from the object's raw block.
-->
<script lang="ts">
  import type { KnownKey } from "$lib/components/script";
  import type { EditQueue } from "$lib/edits.svelte";
  import MechanicScriptBlock from "$lib/components/mechanics/MechanicScriptBlock.svelte";
  import type { MechanicObject } from "$lib/mechanics";

  let {
    installPath,
    modPath,
    queue,
    obj,
    known,
  }: {
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
    obj: MechanicObject;
    known: KnownKey[];
  } = $props();

  // Top-level numeric-keyed option blocks in the raw incident body.
  const indices = $derived.by(() => {
    const out: string[] = [];
    const re = /(^|\n)\s*(\d+)\s*=\s*\{/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(obj.raw)) !== null) if (!out.includes(m[2])) out.push(m[2]);
    return out.sort((a, b) => Number(a) - Number(b));
  });
</script>

<div class="opts">
  <h4>AI option weights</h4>
  {#if indices.length === 0}
    <p class="msg">No numbered option blocks. (Options are defined on the driving event.)</p>
  {:else}
    {#each indices as idx (idx)}
      <div class="opt">
        <span class="idx">option {idx}</span>
        <MechanicScriptBlock
          {installPath}
          {modPath}
          {queue}
          file={obj.file}
          basePath={[obj.editKey || obj.key]}
          name={idx}
          registry="triggers"
          present={true}
          {known}
        />
      </div>
    {/each}
  {/if}
</div>

<style>
  .opts { border-top: 1px solid var(--border); margin-top: 0.4rem; padding-top: 0.4rem; }
  h4 { margin: 0 0 0.3rem; font-size: 0.8rem; color: var(--warn); font-weight: 600; }
  .opt { display: flex; flex-direction: column; gap: 0.15rem; margin-bottom: 0.35rem; }
  .idx { font-size: 0.74rem; color: var(--text-2); font-family: monospace; }
  .msg { color: var(--text-2); font-size: 0.82rem; margin: 0.1rem 0; }
</style>
