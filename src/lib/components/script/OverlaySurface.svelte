<!--
  OverlaySurface — the shared full-screen overlay shell that Sprints 15–17
  (Decisions / Events / Missions) mount their editors inside. Windows-classic
  chrome: a title bar with a close button, a slotted body, and a dark backdrop.

  ── Closing ───────────────────────────────────────────────────────────────────
    • the title-bar × button, or a left-click on the backdrop
    • Esc
    • right-click ON THE BACKDROP (a right-click inside the content is swallowed —
      it must not close the surface, and the browser context menu is suppressed)

  ── Coexisting with MapView's Esc / back-out cascade ──────────────────────────
  MapView listens for Escape on `window` (bubble phase) to run its back-out
  cascade. To avoid the overlay's Esc ALSO stepping that cascade, this component
  listens in the CAPTURE phase and calls `stopPropagation()` — so while the
  overlay is open, Escape closes only the overlay and never reaches MapView. When
  the overlay is closed, the listener no-ops and MapView keeps Escape.

  z-index: 100 (backdrop) / 101 (panel) — the modal layer per AGENTS.md, above the
  toolbar (10) and popover (20) layers.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    open = $bindable(false),
    title,
    onclose,
    toolbar,
    children,
  }: {
    open?: boolean;
    title: string;
    /** Called after the surface closes (via any of the close affordances). */
    onclose?: () => void;
    /** Optional extra controls rendered in the title bar (search, filters, …). */
    toolbar?: Snippet;
    /** The overlay body content. */
    children: Snippet;
  } = $props();

  function close() {
    open = false;
    onclose?.();
  }

  // Capture-phase Esc so it never falls through to MapView's window handler.
  function onWindowKeydown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      e.stopPropagation();
      e.preventDefault();
      close();
    }
  }

  $effect(() => {
    window.addEventListener("keydown", onWindowKeydown, true);
    return () => window.removeEventListener("keydown", onWindowKeydown, true);
  });

  function onBackdropContext(e: MouseEvent) {
    e.preventDefault(); // suppress the browser menu
    close();
  }

  // A right-click inside the content is swallowed: no close, no browser menu.
  function onPanelContext(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
  }
</script>

{#if open}
  <div class="overlay-root" role="dialog" aria-modal="true" aria-label={title}>
    <button
      class="overlay-backdrop"
      aria-label="Close"
      onclick={close}
      oncontextmenu={onBackdropContext}
    ></button>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="overlay-panel" oncontextmenu={onPanelContext}>
      <header class="overlay-head">
        <span class="overlay-title">{title}</span>
        {#if toolbar}
          <div class="overlay-toolbar">{@render toolbar()}</div>
        {/if}
        <span class="spacer"></span>
        <button class="overlay-close" aria-label="Close" onclick={close}>×</button>
      </header>
      <div class="overlay-body">
        {@render children()}
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay-root {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
  }

  .overlay-backdrop {
    position: absolute;
    inset: 0;
    border: none;
    background: rgba(0, 0, 0, 0.6);
    cursor: default;
  }

  .overlay-panel {
    position: relative;
    z-index: 101;
    display: flex;
    flex-direction: column;
    /* Near-full-viewport, with a small margin so the backdrop stays clickable. */
    margin: 1.5rem;
    flex: 1;
    min-width: 0;
    background: #2b323d;
    border: 1px solid #1f242c;
    color: #cfd4db;
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.6);
  }

  .overlay-head {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex: none;
    padding: 0.45rem 0.6rem;
    background: #3f4855;
    border-bottom: 1px solid #1f242c;
  }

  .overlay-title {
    flex: none;
    font-weight: 700;
    font-size: 0.95rem;
  }

  .overlay-toolbar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex: 1;
    min-width: 0;
  }

  .spacer {
    flex: 1;
  }

  .overlay-close {
    flex: none;
    border: none;
    background: transparent;
    color: #cfd4db;
    font-size: 1.4rem;
    line-height: 1;
    cursor: pointer;
    padding: 0 0.3rem;
  }

  .overlay-close:hover {
    color: #fff;
  }

  .overlay-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0.75rem;
  }
</style>
