<script lang="ts">
  // Windows-classic submenu (SPRINT2 18.3): a menu item that reveals a child
  // dropdown to its right on hover or keyboard. The existing menus are flat;
  // this is the one nested piece. Self-contained styling (matches .dropdown in
  // MapView) so it drops into any dropdown without piercing scoped styles.
  interface FlyoutItem {
    label: string;
    action: () => void;
    disabled?: boolean;
    title?: string;
  }

  let { label, items }: { label: string; items: FlyoutItem[] } = $props();

  let open = $state(false);
  let root = $state<HTMLDivElement>();
  let panel = $state<HTMLDivElement>();

  function focusFirst() {
    // Wait for the panel to render, then focus its first enabled button.
    queueMicrotask(() => {
      const btn = panel?.querySelector<HTMLButtonElement>("button:not(:disabled)");
      btn?.focus();
    });
  }

  function openFlyout() {
    open = true;
    focusFirst();
  }

  function closeFlyout(refocus = true) {
    open = false;
    if (refocus) root?.querySelector<HTMLButtonElement>(".flyout-parent")?.focus();
  }

  function onParentKey(e: KeyboardEvent) {
    if (e.key === "ArrowRight" || e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      openFlyout();
    } else if (e.key === "ArrowDown" && open) {
      e.preventDefault();
      focusFirst();
    }
  }

  function onPanelKey(e: KeyboardEvent) {
    const btns = panel
      ? Array.from(panel.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"))
      : [];
    const idx = btns.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape" || e.key === "ArrowLeft") {
      // Close the flyout only — the parent menu stays open.
      e.preventDefault();
      e.stopPropagation();
      closeFlyout();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      btns[(idx + 1) % btns.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      btns[(idx - 1 + btns.length) % btns.length]?.focus();
    }
  }

  function choose(item: FlyoutItem) {
    if (item.disabled) return;
    open = false;
    item.action();
  }
</script>

<!-- Hover anywhere in the item (parent + panel) keeps the flyout open. -->
<div
  class="flyout"
  bind:this={root}
  role="none"
  onmouseenter={() => (open = true)}
  onmouseleave={() => (open = false)}
>
  <button
    class="flyout-parent"
    class:open
    aria-haspopup="true"
    aria-expanded={open}
    onclick={() => (open ? closeFlyout(false) : openFlyout())}
    onkeydown={onParentKey}
  >
    <span>{label}</span>
    <span class="arrow" aria-hidden="true">▸</span>
  </button>
  {#if open}
    <div
      class="flyout-panel"
      bind:this={panel}
      role="menu"
      tabindex="-1"
      onkeydown={onPanelKey}
    >
      {#each items as item (item.label)}
        <button
          role="menuitem"
          disabled={item.disabled}
          title={item.title}
          onclick={() => choose(item)}
        >
          {item.label}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .flyout {
    position: relative;
  }

  /* Mirrors MapView's `.dropdown button`, plus a trailing arrow. */
  .flyout-parent {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    width: 100%;
    border: none;
    border-radius: 0;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.88rem;
    text-align: left;
    padding: 0.35rem 0.7rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .flyout-parent:hover,
  .flyout-parent.open {
    background: #4a6da7;
    color: #ffffff;
  }

  .arrow {
    font-size: 0.7rem;
  }

  /* Opens to the right, top-aligned with the parent item. Above the parent
     dropdown; menus live on layer 10, so 11 keeps the flyout over its host. */
  .flyout-panel {
    position: absolute;
    top: -3px;
    left: 100%;
    min-width: 12rem;
    display: flex;
    flex-direction: column;
    padding: 2px;
    background: #3f4855;
    border: 1px solid #2b323d;
    box-shadow: 2px 3px 8px rgba(0, 0, 0, 0.35);
    z-index: 11;
  }

  .flyout-panel button {
    display: flex;
    justify-content: flex-start;
    border: none;
    border-radius: 0;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.88rem;
    text-align: left;
    padding: 0.35rem 0.7rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .flyout-panel button:hover:not(:disabled) {
    background: #4a6da7;
    color: #ffffff;
  }

  .flyout-panel button:disabled {
    color: #8a919c;
    cursor: default;
  }
</style>
