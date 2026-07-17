<!--
  PromptBanner — the "Click on …" instruction banner shown while a create/paint tool is
  armed and waiting for a map interaction (SPRINT 1.4, 4.1, 5.4, 8.5, 10). Non-blocking;
  optional dismiss button that cancels the armed tool.

  z-index: 30 — the transient map-prompt layer, above anchored popovers (20), below
  modals (100).
-->
<script lang="ts">
  let {
    message,
    oncancel,
    shakeKey = 0,
  }: {
    message: string;
    oncancel?: () => void;
    /**
     * A monotonically-increasing counter: bump it to play a brief shake (e.g.
     * on an invalid map click while a create tool stays armed — SPRINT 4.1).
     */
    shakeKey?: number;
  } = $props();

  let el = $state<HTMLDivElement | null>(null);

  // Restart the shake animation whenever `shakeKey` changes (reflow trick so a
  // repeated invalid click re-triggers it). shakeKey === 0 is the initial /
  // no-shake state and never animates.
  $effect(() => {
    const k = shakeKey;
    if (k > 0 && el) {
      el.classList.remove("shaking");
      void el.offsetWidth;
      el.classList.add("shaking");
    }
  });
</script>

<div bind:this={el} class="prompt-banner" role="status">
  <span class="dot" aria-hidden="true"></span>
  <span class="message">{message}</span>
  {#if oncancel}
    <button class="cancel" onclick={oncancel}>Cancel (Esc)</button>
  {/if}
</div>

<style>
  .prompt-banner {
    position: absolute;
    top: 3.5rem;
    left: 50%;
    transform: translateX(-50%);
    z-index: 30;
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.4rem 0.9rem;
    background: #3f4855;
    border: 1px solid #2b323d;
    color: #cfd4db;
    font-size: 0.9rem;
    box-shadow: 0 3px 10px rgba(0, 0, 0, 0.4);
  }

  /* Keeps the translateX(-50%) centering intact through the shake. The class is
     toggled imperatively (reflow-restart), so :global keeps it from being pruned. */
  .prompt-banner:global(.shaking) {
    animation: banner-shake 0.35s ease;
  }

  @keyframes banner-shake {
    0%,
    100% {
      transform: translateX(-50%);
    }
    20% {
      transform: translateX(calc(-50% - 6px));
    }
    40% {
      transform: translateX(calc(-50% + 6px));
    }
    60% {
      transform: translateX(calc(-50% - 4px));
    }
    80% {
      transform: translateX(calc(-50% + 4px));
    }
  }

  .dot {
    width: 0.55rem;
    height: 0.55rem;
    border-radius: 50%;
    background: #4a6da7;
    animation: pulse 1.2s ease-in-out infinite;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 0.4;
    }
    50% {
      opacity: 1;
    }
  }

  .message {
    font-weight: 600;
  }

  .cancel {
    border: 1px solid #2b323d;
    background: #2b323d;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.2rem 0.5rem;
    cursor: pointer;
  }

  .cancel:hover {
    background: #4a6da7;
    color: #ffffff;
  }
</style>
