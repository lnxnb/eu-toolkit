<!--
  SidePanel — the shared right-side panel shell (country/diplomacy, province, thin
  panels). Fixed header, optional tab strip, scrollable body. Matches the existing
  right-side panel geometry (top: 3rem; right/bottom: 0.75rem; width: 20rem) but in
  legacy dock chrome. Sprint 31's Country/Province proof can render it embedded
  in WorkspaceWindow; Sprint 32 replaces this bridge with the full adapter.

  z-index: 10 — the docked "surface" layer, same as the top/bottom toolbars.
  Anchored popovers (dropdowns, color/date) float above it at 20; modals sit at 100.
-->
<script lang="ts">
  import { getContext, type Snippet } from "svelte";
  import TabStrip from "$lib/components/workspace/TabStrip.svelte";

  const workspaceHosted = getContext<boolean>("eu-toolkit-workspace-window") ?? false;

  interface Tab {
    id: string;
    label: string;
  }

  let {
    title = "",
    tabs = [],
    activeTab = $bindable(undefined),
    width = "20rem",
    embedded = false,
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
    /** Render as content inside a WorkspaceWindow (pilot bridge; Sprint 32 replaces the adapter). */
    embedded?: boolean;
    onclose?: () => void;
    ontab?: (id: string) => void;
    /** Extra header content (below the title row) — flags, swatches, etc. */
    header?: Snippet;
    children?: Snippet;
  } = $props();

  let isEmbedded = $derived(embedded || workspaceHosted);

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

<aside class="side-panel" class:embedded={isEmbedded} style:width={isEmbedded ? undefined : width}>
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
      <TabStrip tier="content" {tabs} activeId={activeTab ?? tabs[0].id} onselect={selectTab} />
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
    z-index: 50;
    display: flex;
    flex-direction: column;
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-size: 0.9rem;
    box-shadow: 2px 3px 10px rgba(0, 0, 0, 0.4);
    resize: horizontal;
    overflow: hidden;
  }
  .side-panel.embedded {
    position: static;
    width: 100%;
    height: 100%;
    border: 0;
    box-shadow: none;
    resize: none;
  }
  .side-panel.embedded .titlebar { display: none; }

  .chrome {
    flex: none;
    background: var(--bg-3);
    border-bottom: 1px solid var(--border);
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
    color: var(--text-1);
    font-size: 1.2rem;
    line-height: 1;
    padding: 0 0.25rem;
    cursor: pointer;
  }

  .close:hover {
    color: var(--text-inverse);
  }

  .header-extra {
    padding: 0 0.6rem 0.5rem;
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0.75rem 0.6rem;
  }
</style>
