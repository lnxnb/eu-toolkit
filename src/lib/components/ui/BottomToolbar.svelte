<!--
  BottomToolbar — a bottom-docked Windows-classic toolbar strip (same chrome as the top
  menu bar). Tool buttons carry an icon + tooltip; exactly one tool is armable at a
  time. Click arms; Esc or re-click disarms. Armed state is exposed via the bindable
  `armed` prop and the `onarm` callback. Consumers: every map-mode toolbar (1.4, 5.3,
  6.3, 7.2, 8.3, 9.1, …). The `extra` snippet holds tool-specific chrome that appears
  when armed (e.g. the brush size slider from 1.4b).

  z-index: 10 — same docked-chrome layer as the top toolbar.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { ToolButton } from "./types";

  let {
    tools,
    armed = $bindable(null),
    onarm,
    extra,
    children,
  }: {
    tools: ToolButton[];
    /** Bindable id of the currently armed tool, or null. */
    armed?: string | null;
    onarm?: (id: string | null) => void;
    /** Extra chrome shown to the right of the tools (e.g. brush size slider). */
    extra?: Snippet;
    /** Optional leading content (labels, mode hints). */
    children?: Snippet;
  } = $props();

  function arm(id: string) {
    const next = armed === id ? null : id;
    armed = next;
    onarm?.(next);
  }

  function disarm() {
    if (armed === null) return;
    armed = null;
    onarm?.(null);
  }

  // Esc disarms the active tool, globally, while this toolbar is mounted.
  $effect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") disarm();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
</script>

<div class="bottom-toolbar">
  {#if children}
    <div class="lead">{@render children()}</div>
  {/if}

  <div class="tools">
    {#each tools as tool (tool.id)}
      <button
        class="tool"
        class:armed={armed === tool.id}
        title={tool.tooltip ?? tool.label}
        aria-pressed={armed === tool.id}
        onclick={() => arm(tool.id)}
      >
        {#if tool.icon}
          {#if tool.icon.includes("/") || tool.icon.startsWith("data:")}
            <img class="tool-icon" src={tool.icon} alt="" />
          {:else}
            <span class="tool-glyph">{tool.icon}</span>
          {/if}
        {/if}
        <span class="tool-label">{tool.label}</span>
      </button>
    {/each}
  </div>

  {#if extra}
    <div class="extra">{@render extra()}</div>
  {/if}
</div>

<style>
  .bottom-toolbar {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.25rem 0.5rem;
    background: #3f4855;
    border-top: 1px solid #2b323d;
    color: #cfd4db;
    font-size: 0.9rem;
  }

  .lead {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    color: #cfd4db;
  }

  .tools {
    display: flex;
    gap: 0.2rem;
  }

  .tool {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: none;
    border-radius: 0;
    background: transparent;
    color: inherit;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.3rem 0.7rem;
    cursor: pointer;
  }

  .tool:hover {
    background: #4a6da7;
    color: #ffffff;
  }

  .tool.armed {
    background: #4a6da7;
    color: #ffffff;
    outline: 1px solid #cfd4db;
    outline-offset: -2px;
  }

  .tool-glyph {
    font-size: 1rem;
    line-height: 1;
  }

  .tool-icon {
    width: 1rem;
    height: 1rem;
    object-fit: contain;
  }

  .extra {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-left: auto;
  }
</style>
