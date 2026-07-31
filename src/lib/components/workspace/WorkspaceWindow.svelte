<script lang="ts">
  import { setContext, type Snippet } from "svelte";
  import { viewDefinition } from "$lib/views";
  import {
    activateTab, canGoBack, canGoForward, closeFocusedTab, closeTab, closeWindow, cycleTabs,
    focusWindow, goBack, goForward, moveTab, openNewTab, reopenClosedTab, reorderTab,
    resizeWindow, setTabPinned, splitTabToWindow, setWindowKind, workspaceFocusedWindowId,
    type Rect, type WorkspaceTab, type WorkspaceWindow as WindowModel,
  } from "$lib/workspace.svelte";
  import TabStrip from "./TabStrip.svelte";

  /** Shared legacy shells use this while their public APIs are phased out. */
  setContext("eu-toolkit-workspace-window", true);

  let { window: model, toolbar, children }: {
    window: WindowModel;
    toolbar?: Snippet<[WorkspaceTab]>;
    children: Snippet<[WorkspaceTab]>;
  } = $props();

  let active = $derived(model.tabs.find((t) => t.id === model.activeTabId) ?? model.tabs[0]);
  let focused = $derived(workspaceFocusedWindowId() === model.id);
  let tabDrag = $state<{ id: string; x: number; y: number } | null>(null);
  let labels = $derived(model.tabs.map((t) => ({
    id: t.id, label: viewDefinition(t.view).title(t.view), pinned: t.pinned,
  })));
  let back = $derived(!!active && canGoBack(active));
  let forward = $derived(!!active && canGoForward(active));

  function dragStart(event: PointerEvent) {
    if (event.button !== 0 || model.kind !== "floating") return;
    const origin = { x: event.clientX, y: event.clientY, rect: { ...model.rect } };
    event.preventDefault();
    focusWindow(model.id);
    const move = (e: PointerEvent) => resizeWindow(model.id, {
      ...origin.rect,
      x: Math.max(0, origin.rect.x + e.clientX - origin.x),
      y: Math.max(0, origin.rect.y + e.clientY - origin.y),
    });
    const up = () => { window.removeEventListener("pointermove", move); window.removeEventListener("pointerup", up); };
    window.addEventListener("pointermove", move); window.addEventListener("pointerup", up);
  }

  type Edge = "n" | "ne" | "e" | "se" | "s" | "sw" | "w" | "nw";
  function resizeStart(event: PointerEvent, edge: Edge) {
    if (event.button !== 0) return;
    event.preventDefault(); event.stopPropagation(); focusWindow(model.id);
    const origin = { x: event.clientX, y: event.clientY, rect: { ...model.rect } };
    const min = active ? viewDefinition(active.view).minSize : { w: 320, h: 240 };
    const move = (e: PointerEvent) => {
      const dx = e.clientX - origin.x, dy = e.clientY - origin.y;
      let { x, y, w, h } = origin.rect;
      if (edge.includes("e")) w = Math.max(min.w, w + dx);
      if (edge.includes("s")) h = Math.max(min.h, h + dy);
      if (edge.includes("w")) { const nw = Math.max(min.w, w - dx); x += w - nw; w = nw; }
      if (edge.includes("n")) { const nh = Math.max(min.h, h - dy); y += h - nh; h = nh; }
      resizeWindow(model.id, { x, y, w, h });
    };
    const up = () => { window.removeEventListener("pointermove", move); window.removeEventListener("pointerup", up); };
    window.addEventListener("pointermove", move); window.addEventListener("pointerup", up);
  }

  function popOrDock() {
    setWindowKind(model.id, model.kind === "floating" ? "docked-right" : "floating");
  }

  function keydown(event: KeyboardEvent) {
    if (!focused) return;
    const mod = event.ctrlKey || event.metaKey;
    if (mod && event.key.toLowerCase() === "w") { event.preventDefault(); event.stopPropagation(); closeFocusedTab(); }
    else if (mod && event.key.toLowerCase() === "t" && event.shiftKey) { event.preventDefault(); reopenClosedTab(); }
    else if (mod && event.key.toLowerCase() === "t") { event.preventDefault(); openNewTab(); }
    else if (mod && event.key === "Tab") { event.preventDefault(); cycleTabs(model.id, event.shiftKey); }
    else if (event.altKey && event.key === "ArrowLeft") { event.preventDefault(); event.stopPropagation(); goBack(model.activeTabId); }
    else if (event.altKey && event.key === "ArrowRight") { event.preventDefault(); event.stopPropagation(); goForward(model.activeTabId); }
    else if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); closeTab(model.activeTabId); }
  }

  function tabPointerDown(event: PointerEvent, id: string) {
    if (event.button !== 0) return;
    const startX = event.clientX, startY = event.clientY;
    let dragging = false;
    const move = (e: PointerEvent) => {
      if (!dragging && Math.hypot(e.clientX - startX, e.clientY - startY) < 5) return;
      dragging = true; tabDrag = { id, x: e.clientX, y: e.clientY };
    };
    const up = (e: PointerEvent) => {
      window.removeEventListener("pointermove", move); window.removeEventListener("pointerup", up);
      if (!dragging) return;
      const host = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-workspace-window]");
      const targetId = host?.dataset.workspaceWindow;
      if (targetId) {
        const buttons = [...host.querySelectorAll<HTMLElement>("[data-tab-id]")];
        const index = buttons.findIndex((b) => e.clientX < b.getBoundingClientRect().left + b.offsetWidth / 2);
        if (targetId === model.id) reorderTab(model.id, id, index < 0 ? buttons.length : index);
        else moveTab(id, targetId, index < 0 ? buttons.length : index);
      } else {
        splitTabToWindow(id, { x: Math.max(8, e.clientX - 120), y: Math.max(42, e.clientY - 16), w: model.rect.w, h: model.rect.h });
      }
      tabDrag = null;
    };
    window.addEventListener("pointermove", move); window.addEventListener("pointerup", up);
  }

  $effect(() => {
    window.addEventListener("keydown", keydown, true);
    return () => window.removeEventListener("keydown", keydown, true);
  });

  /**
   * A docked window spans from below the menu bar to just above the map's
   * bottom toolbar, and derives that height from CSS rather than its stored
   * rect: `--bottom-toolbar-h` changes as tools come and go with the map mode,
   * so a height frozen at open time would sit on top of a toolbar that appeared
   * later. Only floating windows own their height (docked ones resize width).
   */
  let style = $derived(
    model.kind === "floating"
      ? `left:${model.rect.x}px;top:${model.rect.y}px;width:${model.rect.w}px;height:${model.rect.h}px;z-index:${model.z}`
      : `left:${model.rect.x}px;top:${model.rect.y}px;width:${model.rect.w}px;bottom:calc(var(--bottom-toolbar-h, 0px) + 12px);z-index:${model.z}`,
  );
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<section
  class="workspace-window {model.kind}"
  class:focused
  style={style}
  data-workspace-window={model.id}
  onpointerdown={() => focusWindow(model.id)}
>
  <header class="titlebar">
    <div class="nav">
      <button
        class="chrome-button"
        title="Back (Alt+Left)"
        aria-label="Back"
        disabled={!back}
        onclick={() => goBack(model.activeTabId)}
      >◀</button>
      <button
        class="chrome-button"
        title="Forward (Alt+Right)"
        aria-label="Forward"
        disabled={!forward}
        onclick={() => goForward(model.activeTabId)}
      >▶</button>
    </div>
    <TabStrip tabs={labels} activeId={model.activeTabId} onselect={(id) => activateTab(model.id, id)} onclose={closeTab} ontabpointerdown={tabPointerDown} />
    <button class="new-tab" title="New tab (Ctrl+T)" aria-label="New tab" onclick={openNewTab}>+</button>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="drag-space" onpointerdown={dragStart}></div>
    {#if active && toolbar}<div class="toolbar">{@render toolbar(active)}</div>{/if}
    {#if active}
      <button class="chrome-button" title={active.pinned ? "Unpin tab" : "Pin tab"} aria-label={active.pinned ? "Unpin tab" : "Pin tab"} onclick={() => setTabPinned(active.id, !active.pinned)}>◆</button>
    {/if}
    <button class="chrome-button" title={model.kind === "floating" ? "Dock right" : "Pop out"} aria-label={model.kind === "floating" ? "Dock right" : "Pop out"} onclick={popOrDock}>{model.kind === "floating" ? "▐" : "↗"}</button>
    <button class="chrome-button close" aria-label="Close window" onclick={() => closeWindow(model.id)}>×</button>
  </header>
  <div class="window-body">
    {#if active}{@render children(active)}{/if}
  </div>

  {#if model.kind === "floating"}
    {#each ["n", "ne", "e", "se", "s", "sw", "w", "nw"] as edge}
      <div class="resize {edge}" onpointerdown={(e) => resizeStart(e, edge as Edge)}></div>
    {/each}
  {:else}
    <div class="resize w" onpointerdown={(e) => resizeStart(e, "w")}></div>
  {/if}
</section>
{#if tabDrag}<div class="tab-ghost" style={`left:${tabDrag.x + 12}px;top:${tabDrag.y + 12}px`}>{labels.find((x) => x.id === tabDrag?.id)?.label}</div>{/if}

<style>
  .workspace-window {
    position: absolute;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: visible;
    color: var(--text-1);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-2);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.28);
  }
  .workspace-window.focused { border-color: var(--border-strong); box-shadow: var(--shadow-window); }
  .titlebar {
    height: 34px;
    flex: none;
    display: flex;
    align-items: end;
    padding: 4px 4px 0;
    gap: var(--sp-1);
    background: var(--bg-3);
    border-radius: var(--r-2) var(--r-2) 0 0;
    user-select: none;
  }
  .titlebar :global(.tab-strip) { max-width: min(65%, 720px); }
  .new-tab {
    flex: none;
    align-self: center;
    width: 22px; height: 22px;
    display: grid; place-items: center;
    padding: 0;
    border: 1px solid transparent; border-radius: var(--r-1);
    background: transparent; color: var(--text-2);
    font-size: var(--fs-lg); line-height: 1; cursor: pointer;
  }
  .new-tab:hover { color: var(--text-1); background: var(--bg-hover); border-color: var(--border); }
  .drag-space { flex: 1; align-self: stretch; cursor: move; }
  .toolbar { align-self: center; display: flex; align-items: center; gap: var(--sp-2); min-width: 0; }
  .chrome-button {
    align-self: center;
    width: 26px; height: 26px;
    display: grid; place-items: center;
    padding: 0; border: 0; border-radius: var(--r-1);
    background: transparent; color: var(--text-2); cursor: pointer;
  }
  .chrome-button:hover:not(:disabled) { color: var(--text-1); background: var(--bg-hover); }
  .chrome-button:disabled { color: var(--text-3); cursor: default; opacity: 0.45; }
  .nav { flex: none; align-self: center; display: flex; gap: 2px; }
  .nav .chrome-button { width: 22px; height: 22px; font-size: var(--fs-xs); }
  .chrome-button.close { font-size: var(--fs-xl); }
  .window-body { flex: 1; min-height: 0; overflow: auto; padding: var(--sp-4); background: var(--bg-2); border-radius: 0 0 var(--r-2) var(--r-2); }
  .resize { position: absolute; z-index: 2; }
  .resize.n, .resize.s { left: 6px; right: 6px; height: 6px; }
  .resize.n { top: -3px; cursor: ns-resize; } .resize.s { bottom: -3px; cursor: ns-resize; }
  .resize.e, .resize.w { top: 6px; bottom: 6px; width: 6px; }
  .resize.e { right: -3px; cursor: ew-resize; } .resize.w { left: -3px; cursor: ew-resize; }
  .resize.ne, .resize.se, .resize.sw, .resize.nw { width: 10px; height: 10px; }
  .resize.ne { right: -4px; top: -4px; cursor: nesw-resize; }
  .resize.se { right: -4px; bottom: -4px; cursor: nwse-resize; }
  .resize.sw { left: -4px; bottom: -4px; cursor: nesw-resize; }
  .resize.nw { left: -4px; top: -4px; cursor: nwse-resize; }
  .tab-ghost { position: fixed; z-index: var(--z-workspace-max); pointer-events: none; padding: var(--sp-2) var(--sp-3); border: 1px solid var(--accent); border-radius: var(--r-1); background: var(--bg-3); box-shadow: var(--shadow-window); color: var(--text-1); }
</style>
