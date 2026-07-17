<script lang="ts">
  // Local-state wrapper around ModifierEditor (mirrors country/IdeaBlockEditor):
  // owns the row array, initialized ONCE from the on-disk block, so pending edits
  // aren't clobbered; commits the whole block on every change.
  import { ModifierEditor } from "$lib/components/ui";
  import type { KnownModifier, ModifierRow } from "$lib/components/ui";
  import type { ModRow } from "./types";

  let {
    base,
    known,
    oncommit,
  }: {
    base: ModRow[];
    known: KnownModifier[];
    oncommit: (rows: ModifierRow[]) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let rows = $state<ModifierRow[]>(base.map((e) => ({ key: e.key, value: e.value })));
</script>

<ModifierEditor bind:modifiers={rows} {known} onchange={(r) => oncommit(r)} />
