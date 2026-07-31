<script lang="ts">
  export interface TabStripItem { id: string; label: string; pinned?: boolean; count?: number }

  let {
    tabs,
    activeId,
    tier = "window",
    closable = tier === "window",
    onselect,
    onclose,
    ontabpointerdown,
  }: {
    tabs: TabStripItem[];
    activeId: string;
    tier?: "window" | "content";
    closable?: boolean;
    onselect?: (id: string) => void;
    onclose?: (id: string) => void;
    ontabpointerdown?: (event: PointerEvent, id: string) => void;
  } = $props();

  function pointerDown(event: PointerEvent, id: string) {
    if (event.button === 1) {
      event.preventDefault();
      onclose?.(id);
      return;
    }
    ontabpointerdown?.(event, id);
  }
</script>

<div class="tab-strip {tier}" role="tablist" aria-label={tier === "window" ? "Window tabs" : "Content tabs"}>
  {#each tabs as tab (tab.id)}
    <button
      class="tab"
      class:active={tab.id === activeId}
      class:pinned={tab.pinned}
      role="tab"
      aria-selected={tab.id === activeId}
      title={tab.label}
      data-tab-id={tab.id}
      onpointerdown={(event) => pointerDown(event, tab.id)}
      onclick={() => onselect?.(tab.id)}
    >
      {#if tab.pinned && tier === "window"}<span class="pin" aria-label="Pinned">◆</span>{/if}
      <span class="label">{tab.label}</span>
      {#if tab.count != null}<span class="count">{tab.count}</span>{/if}
      {#if closable && !tab.pinned}
        <span
          class="close"
          role="button"
          tabindex="0"
          aria-label={`Close ${tab.label}`}
          onclick={(event) => { event.stopPropagation(); onclose?.(tab.id); }}
          onkeydown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault(); event.stopPropagation(); onclose?.(tab.id);
            }
          }}
        >×</span>
      {/if}
    </button>
  {/each}
</div>

<style>
  .tab-strip { min-width: 0; }
  .tab-strip.window {
    display: flex;
    align-self: stretch;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }
  .tab-strip.window::-webkit-scrollbar { display: none; }
  .tab {
    border: 0;
    font: inherit;
    cursor: pointer;
    min-width: 0;
  }
  .window .tab {
    position: relative;
    flex: 0 1 180px;
    min-width: 72px;
    max-width: 180px;
    height: 30px;
    display: flex;
    align-items: center;
    gap: var(--sp-1);
    padding: 0 var(--sp-2);
    border-radius: var(--r-1) var(--r-1) 0 0;
    background: transparent;
    color: var(--text-2);
  }
  .window .tab:hover { background: var(--bg-hover); }
  .window .tab.active { background: var(--bg-2); color: var(--text-1); }
  .window .tab:focus-visible, .content .tab:focus-visible { outline-offset: -2px; }
  .label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .window .label { flex: 1; text-align: left; }
  .close {
    width: 20px;
    height: 20px;
    display: grid;
    place-items: center;
    border-radius: var(--r-1);
    opacity: 0;
    font-size: var(--fs-lg);
    line-height: 1;
  }
  .tab:hover .close, .tab.active .close { opacity: 1; }
  .close:hover { background: var(--bg-hover); }
  .pin { color: var(--accent-text); font-size: 8px; }

  .tab-strip.content {
    display: flex;
    align-items: end;
    gap: var(--sp-4);
    min-height: 28px;
    overflow-x: auto;
    /* `overflow-x: auto` alone promotes overflow-y from visible to auto, so the
       active tab's 1px underline overhang raised a vertical scrollbar. */
    overflow-y: hidden;
    scrollbar-width: none;
    border-bottom: 1px solid var(--border);
  }
  .tab-strip.content::-webkit-scrollbar { display: none; }
  .content .tab {
    position: relative;
    flex: none;
    height: 28px;
    padding: 0;
    background: transparent;
    color: var(--text-2);
    font-size: var(--fs-md);
    font-weight: 500;
  }
  .content .tab.active { color: var(--text-1); }
  .content .tab.active::after {
    content: "";
    position: absolute;
    left: 0; right: 0; bottom: 0;
    height: 2px;
    background: var(--accent);
  }
  .count { color: var(--text-3); font-size: var(--fs-xs); }
</style>
