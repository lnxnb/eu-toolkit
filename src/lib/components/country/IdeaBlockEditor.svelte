<!--
  IdeaBlockEditor — owns the local ModifierRow state for one idea block
  (traditions / an idea / ambition) so it survives parent re-renders, initialized
  once from the on-disk effects. Commits the whole block on every change; the
  parent turns that into a byte-surgical setBlock into the ideas file.
-->
<script lang="ts">
  import { ModifierEditor } from "$lib/components/ui";
  import type { KnownModifier, ModifierRow } from "$lib/components/ui";
  import type { IdeaEffect } from "./types";

  let {
    base,
    known,
    oncommit,
  }: {
    base: IdeaEffect[];
    known: KnownModifier[];
    oncommit: (rows: ModifierRow[]) => void;
  } = $props();

  // Initialized once from the on-disk effects; not re-synced (base is fixed per
  // mount, and re-syncing would clobber pending edits on every re-render).
  // svelte-ignore state_referenced_locally
  let rows = $state<ModifierRow[]>(base.map((e) => ({ key: e.key, value: e.value })));
</script>

<ModifierEditor bind:modifiers={rows} {known} onchange={(r) => oncommit(r)} />
