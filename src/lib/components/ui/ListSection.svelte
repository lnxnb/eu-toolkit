<!--
  ListSection — a fixed-height, scrollable list section with a header row and count
  badge. Consumers: diplomacy sections (3.3), trade-goods list (7.1), terrain list
  (11.2). The body scrolls internally so a panel full of these never grows unbounded.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    count = undefined,
    maxHeight = "12rem",
    actions,
    children,
  }: {
    title: string;
    /** Optional count badge; omit to hide it. */
    count?: number | undefined;
    /** Fixed max height of the scroll area. */
    maxHeight?: string;
    /** Optional header-right slot (add buttons, filters). */
    actions?: Snippet;
    children?: Snippet;
  } = $props();
</script>

<section class="list-section">
  <div class="head">
    <span class="title">{title}</span>
    {#if count !== undefined}
      <span class="badge">{count}</span>
    {/if}
    <span class="spacer"></span>
    {#if actions}
      <div class="actions">{@render actions()}</div>
    {/if}
  </div>
  <div class="scroll" style="max-height: {maxHeight}">
    {@render children?.()}
  </div>
</section>

<style>
  .list-section {
    display: flex;
    flex-direction: column;
    border: 1px solid #1f242c;
    background: #262c35;
    margin-bottom: 0.6rem;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.3rem 0.5rem;
    background: #3f4855;
    border-bottom: 1px solid #1f242c;
  }

  .title {
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #cfd4db;
    font-weight: 600;
  }

  .badge {
    font-size: 0.72rem;
    line-height: 1;
    padding: 0.12rem 0.4rem;
    background: #4a6da7;
    color: #ffffff;
    font-variant-numeric: tabular-nums;
  }

  .spacer {
    flex: 1;
  }

  .actions {
    display: flex;
    gap: 0.25rem;
  }

  .scroll {
    overflow-y: auto;
    padding: 0.25rem;
  }
</style>
