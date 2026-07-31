<!--
  InlineNamePrompt — a small floating name input anchored at a map position, the last
  step of the create-entity flow (SPRINT 4.1, 5.4, 6.4, 8.5, 10). Enter accepts, Esc
  cancels; supports a prefill (e.g. "New Country") that starts selected for quick retype.

  z-index: 30 — the transient map-prompt layer (above popovers at 20, below modals).
-->
<script lang="ts">
  let {
    x,
    y,
    value = $bindable(""),
    label = "Name",
    placeholder = "",
    onaccept,
    oncancel,
  }: {
    /** Screen position (px) to anchor the prompt near. */
    x: number;
    y: number;
    value?: string;
    label?: string;
    placeholder?: string;
    onaccept: (name: string) => void;
    oncancel: () => void;
  } = $props();

  let inputEl = $state<HTMLInputElement | null>(null);

  $effect(() => {
    // Focus and pre-select the prefill so typing replaces it immediately.
    inputEl?.focus();
    inputEl?.select();
  });

  function accept() {
    const name = value.trim();
    if (name) onaccept(name);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      accept();
      e.preventDefault();
    } else if (e.key === "Escape") {
      oncancel();
      e.preventDefault();
    }
  }
</script>

<div class="inline-name-prompt" style="left: {x}px; top: {y}px">
  <span class="label">{label}</span>
  <input
    bind:this={inputEl}
    type="text"
    bind:value
    {placeholder}
    onkeydown={onKeydown}
  />
  <button class="ok" onclick={accept} aria-label="Accept">✓</button>
  <button class="cancel" onclick={oncancel} aria-label="Cancel">×</button>
</div>

<style>
  .inline-name-prompt {
    position: absolute;
    z-index: 30;
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.3rem 0.4rem;
    background: var(--bg-3);
    border: 1px solid var(--bg-2);
    box-shadow: 0 3px 10px rgba(0, 0, 0, 0.45);
    transform: translate(-50%, 0.5rem);
  }

  .label {
    font-size: 0.78rem;
    color: var(--text-2);
  }

  input {
    width: 9rem;
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.25rem 0.4rem;
    outline: none;
  }

  .ok,
  .cancel {
    border: 1px solid var(--bg-2);
    background: var(--bg-2);
    color: var(--text-1);
    font-size: 0.9rem;
    line-height: 1;
    padding: 0.25rem 0.4rem;
    cursor: pointer;
  }

  .ok:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .cancel:hover {
    background: var(--danger-bg);
    color: var(--text-inverse);
  }
</style>
