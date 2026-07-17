<!--
  SidePanel — the shared right-side panel shell (country/diplomacy, province, thin
  panels). Fixed header, optional tab strip, scrollable body. Matches the existing
  right-side panel geometry (top: 3rem; right/bottom: 0.75rem; width: 20rem) but in
  Windows-classic square-cornered chrome per AGENTS.md.

  z-index: 10 — the docked "surface" layer, same as the top/bottom toolbars.
  Anchored popovers (dropdowns, color/date) float above it at 20; modals sit at 100.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  interface Tab {
    id: string;
    label: string;
  }

  let {
    title = "",
    tabs = [],
    activeTab = $bindable(undefined),
    width = "20rem",
    onclose,
    ontab,
    header,
    children,
  }: {
    title?: string;
    tabs?: Tab[];
    /** Bindable active tab id. Defaults to the first tab when tabs are given. */
    activeTab?: string | undefined;
    width?: string;
    onclose?: () => void;
    ontab?: (id: string) => void;
    /** Extra header content (below the title row) — flags, swatches, etc. */
    header?: Snippet;
    children?: Snippet;
  } = $props();

  // Default the active tab to the first one if the consumer didn't set it.
  $effect(() => {
    if (tabs.length > 0 && !tabs.some((t) => t.id === activeTab)) {
      activeTab = tabs[0].id;
    }
  });

  function selectTab(id: string) {
    activeTab = id;
    ontab?.(id);
  }
</script>

<aside class="side-panel" style="width: {width}">
  <div class="chrome">
    <div class="titlebar">
      <span class="title">{title}</span>
      {#if onclose}
        <button class="close" onclick={onclose} aria-label="Close panel">×</button>
      {/if}
    </div>

    {#if header}
      <div class="header-extra">{@render header()}</div>
    {/if}

    {#if tabs.length > 0}
      <div class="tabs" role="tablist">
        {#each tabs as t}
          <button
            class="tab"
            class:active={t.id === activeTab}
            role="tab"
            aria-selected={t.id === activeTab}
            onclick={() => selectTab(t.id)}
          >
            {t.label}
          </button>
        {/each}
      </div>
    {/if}
  </div>

  <div class="body">
    {@render children?.()}
  </div>
</aside>

<style>
  .side-panel {
    position: absolute;
    top: 3rem;
    right: 0.75rem;
    bottom: 0.75rem;
    z-index: 10;
    display: flex;
    flex-direction: column;
    background: #2b323d;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-size: 0.9rem;
    box-shadow: 2px 3px 10px rgba(0, 0, 0, 0.4);
  }

  .chrome {
    flex: none;
    background: #3f4855;
    border-bottom: 1px solid #1f242c;
  }

  .titlebar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.4rem 0.6rem;
  }

  .title {
    flex: 1;
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .close {
    flex: none;
    border: none;
    background: transparent;
    color: #cfd4db;
    font-size: 1.2rem;
    line-height: 1;
    padding: 0 0.25rem;
    cursor: pointer;
  }

  .close:hover {
    color: #ffffff;
  }

  .header-extra {
    padding: 0 0.6rem 0.5rem;
  }

  .tabs {
    display: flex;
    gap: 1px;
    padding: 0 0.35rem;
  }

  .tab {
    border: none;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.35rem 0.8rem;
    cursor: pointer;
    border-bottom: 2px solid transparent;
  }

  .tab:hover {
    background: #4a6da7;
    color: #ffffff;
  }

  .tab.active {
    border-bottom-color: #4a6da7;
    font-weight: 600;
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0.75rem 0.6rem;
  }
</style>
