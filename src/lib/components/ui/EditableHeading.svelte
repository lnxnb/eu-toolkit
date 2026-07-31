<!--
  EditableHeading — a panel header's headline that doubles as its edit
  affordance: the country name, the province's capital city. Click the text to
  edit it in place; Enter or blur commits, Escape reverts. Shared so every
  headline behaves the same way rather than each panel rolling its own pencil
  button.

  `oncommit` only fires for a non-empty value that actually changed — callers
  push an edit unconditionally.
-->
<script lang="ts">
  let {
    value,
    placeholder = "—",
    label,
    edited = false,
    readonly = false,
    size = "lg",
    oncommit,
  }: {
    value: string;
    /** Shown when `value` is empty; never committed. */
    placeholder?: string;
    /** Accessible name for the edit control, e.g. "Country name". */
    label: string;
    /** Marks an unsaved pending edit. */
    edited?: boolean;
    readonly?: boolean;
    size?: "lg" | "md";
    oncommit: (next: string) => void;
  } = $props();

  let editing = $state(false);
  let draft = $state("");
  let input = $state<HTMLInputElement | null>(null);

  function start() {
    if (readonly) return;
    draft = value;
    editing = true;
  }

  function commit() {
    if (!editing) return;
    editing = false;
    const next = draft.trim();
    if (next && next !== value) oncommit(next);
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      commit();
    } else if (event.key === "Escape") {
      // Stop the workspace window from reading this as "close the tab".
      event.preventDefault();
      event.stopPropagation();
      editing = false;
    }
  }

  // Focus + select on entering edit mode so typing replaces the old name.
  $effect(() => {
    if (editing && input) input.select();
  });
</script>

<span class="heading {size}">
  {#if editing}
    <input
      bind:this={input}
      bind:value={draft}
      aria-label={label}
      onkeydown={keydown}
      onblur={commit}
    />
  {:else}
    <button
      class="text"
      class:empty={!value}
      class:readonly
      disabled={readonly}
      title={readonly ? value : `${label} — click to edit`}
      onclick={start}
    >{value || placeholder}</button>
  {/if}
  {#if edited}<span class="edited">edited</span>{/if}
</span>

<style>
  .heading {
    display: flex;
    align-items: baseline;
    gap: var(--sp-2);
    min-width: 0;
  }
  .text,
  input {
    min-width: 0;
    font: inherit;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--text-1);
  }
  .lg .text,
  .lg input { font-size: var(--fs-xl); }
  .md .text,
  .md input { font-size: var(--fs-lg); }
  .text {
    flex: 0 1 auto;
    padding: 1px var(--sp-1);
    margin-left: calc(var(--sp-1) * -1);
    border: 1px solid transparent;
    border-radius: var(--r-1);
    background: transparent;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: text;
  }
  .text:hover:not(.readonly) { background: var(--bg-hover); border-color: var(--border); }
  .text.readonly { cursor: default; }
  .text.empty { color: var(--text-3); font-weight: 500; }
  input {
    flex: 1 1 auto;
    padding: 1px var(--sp-1);
    border: 1px solid var(--accent);
    border-radius: var(--r-1);
    background: var(--bg-1);
  }
  .edited {
    flex: none;
    font-size: var(--fs-xs);
    font-weight: 500;
    color: var(--accent-text);
  }
</style>
