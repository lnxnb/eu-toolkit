<script lang="ts">
  // A bulk-editable name pool: one name per line in a paste-friendly textarea
  // (male/female/dynasty names). Local state is seeded once from `base`; commits
  // the whole parsed list (blank lines dropped, each trimmed) on every change.
  // The parent turns the list into a byte-clean `{ ... }` block via setBlock.
  let {
    base,
    placeholder = "One name per line…",
    oncommit,
  }: {
    base: string[];
    placeholder?: string;
    oncommit: (names: string[]) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let text = $state(base.join("\n"));

  function parse(v: string): string[] {
    return v
      .split(/\r?\n/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
  }

  let count = $derived(parse(text).length);

  function commit() {
    oncommit(parse(text));
  }
</script>

<div class="pool">
  <textarea
    bind:value={text}
    {placeholder}
    oninput={commit}
    spellcheck="false"
    rows="6"
  ></textarea>
  <span class="count">{count} name{count === 1 ? "" : "s"}</span>
</div>

<style>
  .pool {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }
  textarea {
    width: 100%;
    resize: vertical;
    min-height: 4rem;
    background: var(--bg-0);
    border: 1px solid var(--border-strong);
    color: var(--text-1);
    font-family: ui-monospace, monospace;
    font-size: 0.8rem;
    padding: 0.3rem 0.4rem;
    line-height: 1.35;
  }
  .count {
    font-size: 0.72rem;
    color: var(--text-2);
    align-self: flex-end;
  }
</style>
