<!--
  FieldRow — the shared label + control row used by every editable country field
  (Identity + Government sections, Sprint 1.2). A label on the left, the control
  (slot) on the right, and an optional "edited" badge when a pending edit differs
  from the on-disk value. Windows-classic chrome to match the rest of the panel.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    edited = false,
    children,
  }: {
    label: string;
    /** Show the amber "edited" badge (a pending edit differs from disk). */
    edited?: boolean;
    children?: Snippet;
  } = $props();
</script>

<div class="field-row">
  <div class="label">
    <span>{label}</span>
    {#if edited}<span class="edited" title="Unsaved change">edited</span>{/if}
  </div>
  <div class="control">{@render children?.()}</div>
</div>

<style>
  .field-row {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    margin-bottom: 0.55rem;
  }

  .label {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: #8a919c;
  }

  .edited {
    text-transform: none;
    letter-spacing: 0;
    background: rgba(234, 179, 8, 0.22);
    color: #fde68a;
    font-size: 0.68rem;
    padding: 0.02rem 0.4rem;
    border: 1px solid rgba(234, 179, 8, 0.35);
  }

  .control {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
</style>
