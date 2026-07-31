<script lang="ts">
  // Tech groups table (common/technology.txt `groups`): one row per group, two
  // editable numeric columns (start_level / start_cost_modifier). Byte-surgical
  // SetScalar; the registry keeps reading these for pickers unchanged.
  import type { EditQueue } from "$lib/edits.svelte";
  import { liveGroupScalar, type TechGroup } from "$lib/technology";

  let {
    groups,
    queue,
  }: {
    groups: TechGroup[];
    queue: EditQueue;
  } = $props();

  function commit(group: TechGroup, key: "start_level" | "start_cost_modifier", value: string) {
    queue.push({
      label: `Edit ${key} of ${group.key}`,
      edits: [{ kind: "setScalar", file: group.file, path: ["groups", group.key, key], value, quoted: false }],
      coalesceKey: `techgroup:${group.key}:${key}`,
    });
  }
</script>

<table class="groups">
  <thead>
    <tr>
      <th>Group</th>
      <th>Key</th>
      <th>start_level</th>
      <th>start_cost_modifier</th>
    </tr>
  </thead>
  <tbody>
    {#each groups as g (g.key)}
      <tr>
        <td class="name">{g.name}</td>
        <td><code class="key">{g.key}</code></td>
        <td>
          <input class="num" type="number" step="1" value={liveGroupScalar(queue, g, "start_level", g.startLevel ?? "")}
            oninput={(e) => commit(g, "start_level", (e.target as HTMLInputElement).value)} />
        </td>
        <td>
          <input class="num" type="number" step="0.05" value={liveGroupScalar(queue, g, "start_cost_modifier", g.startCostModifier ?? "")}
            oninput={(e) => commit(g, "start_cost_modifier", (e.target as HTMLInputElement).value)} />
        </td>
      </tr>
    {/each}
  </tbody>
</table>

<style>
  .groups { border-collapse: collapse; width: 100%; font-size: 0.83rem; }
  th { text-align: left; color: var(--text-2); font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.03em; padding: 0.3rem 0.5rem; border-bottom: 1px solid var(--bg-1); }
  td { padding: 0.25rem 0.5rem; border-bottom: 1px solid var(--border); color: var(--text-1); }
  .name { font-weight: 600; }
  .key { color: var(--ok); background: var(--bg-0); padding: 0 0.3rem; font-size: 0.76rem; }
  .num { width: 6rem; background: var(--bg-0); border: 1px solid var(--border-strong); color: var(--text-1); font-family: inherit; font-size: 0.8rem; padding: 0.13rem 0.35rem; }
</style>
