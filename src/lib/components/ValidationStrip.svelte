<!--
  ValidationStrip — Phase 0.9 persistent, non-blocking validation strip.

  A horizontal, collapsible strip (Windows-classic chrome) listing validation
  issues with severity icons. Each issue is clickable and, when it carries a
  jump target, invokes `onjump(jump)` so a host (MapView, in a later sprint)
  can select the province / open the country / focus the area it points at.

  Backend shape mirror: `validation::ValidationIssue` / `JumpTarget` in
  src-tauri/src/validation.rs. JumpTarget is adjacently tagged
  (`{ kind, id }`) so `kind` can be switched on directly.
-->
<script module lang="ts">
  export type Severity = "error" | "warning" | "info";

  /** Typed pointer to the thing an issue is about (mirrors the Rust enum). */
  export type JumpTarget =
    | { kind: "province"; id: number }
    | { kind: "country"; id: string }
    | { kind: "area"; id: string }
    | { kind: "node"; id: string }
    | { kind: "file"; id: string }
    | { kind: "colonial_region"; id: string }
    | { kind: "trade_company"; id: string };

  export interface ValidationIssue {
    severity: Severity;
    message: string;
    jump: JumpTarget | null;
  }

  const SEV_ORDER: Record<Severity, number> = { error: 0, warning: 1, info: 2 };
  const SEV_GLYPH: Record<Severity, string> = { error: "✕", warning: "!", info: "i" };
</script>

<script lang="ts">
  let {
    issues = [],
    onjump,
    collapsed = $bindable(false),
    title = "Validation",
    emptyLabel = "No issues found",
  }: {
    issues?: ValidationIssue[];
    /** Called when an issue with a jump target is clicked. */
    onjump?: (jump: JumpTarget) => void;
    /** Bindable collapse state. */
    collapsed?: boolean;
    title?: string;
    emptyLabel?: string;
  } = $props();

  const counts = $derived({
    error: issues.filter((i) => i.severity === "error").length,
    warning: issues.filter((i) => i.severity === "warning").length,
    info: issues.filter((i) => i.severity === "info").length,
  });

  // Most-severe-first so errors surface at the head of the strip.
  const sorted = $derived(
    [...issues].sort((a, b) => SEV_ORDER[a.severity] - SEV_ORDER[b.severity]),
  );

  function clickIssue(issue: ValidationIssue) {
    if (issue.jump) onjump?.(issue.jump);
  }
</script>

<div class="strip" class:collapsed>
  <button
    class="head"
    onclick={() => (collapsed = !collapsed)}
    aria-expanded={!collapsed}
    title={collapsed ? "Expand" : "Collapse"}
  >
    <span class="chevron">{collapsed ? "▸" : "▾"}</span>
    <span class="label">{title}</span>
    <span class="tally">
      {#each ["error", "warning", "info"] as const as sev}
        {#if counts[sev] > 0}
          <span class="pill sev-{sev}" title="{counts[sev]} {sev}">
            <span class="glyph">{SEV_GLYPH[sev]}</span>{counts[sev]}
          </span>
        {/if}
      {/each}
      {#if issues.length === 0}
        <span class="ok">{"✓"} clean</span>
      {/if}
    </span>
  </button>

  {#if !collapsed}
    <div class="items">
      {#if sorted.length === 0}
        <span class="empty">{emptyLabel}</span>
      {:else}
        {#each sorted as issue}
          <button
            class="item sev-{issue.severity}"
            class:jumpable={!!issue.jump}
            disabled={!issue.jump}
            onclick={() => clickIssue(issue)}
            title={issue.jump ? "Jump to problem" : issue.message}
          >
            <span class="glyph sev-{issue.severity}">{SEV_GLYPH[issue.severity]}</span>
            <span class="msg">{issue.message}</span>
          </button>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  /* Windows-classic chrome: square corners, bluish-gray on dark borders. */
  .strip {
    display: flex;
    flex-direction: column;
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.8rem;
    user-select: none;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    padding: 0.28rem 0.5rem;
    background: var(--bg-3);
    border: none;
    border-bottom: 1px solid var(--border);
    color: var(--text-1);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .strip.collapsed .head {
    border-bottom: none;
  }

  .head:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .chevron {
    width: 0.9rem;
    color: var(--text-2);
  }

  .head:hover .chevron {
    color: var(--text-inverse);
  }

  .label {
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-size: 0.72rem;
    font-weight: 600;
  }

  .tally {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    margin-left: auto;
  }

  .pill {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.05rem 0.35rem;
    font-size: 0.72rem;
    font-variant-numeric: tabular-nums;
    color: var(--text-inverse);
    border: 1px solid rgba(0, 0, 0, 0.35);
  }

  .ok {
    color: var(--ok);
    font-size: 0.74rem;
  }

  /* The horizontal strip of clickable issues. */
  .items {
    display: flex;
    flex-direction: row;
    gap: 0.3rem;
    overflow-x: auto;
    padding: 0.3rem 0.4rem;
    align-items: stretch;
  }

  .empty {
    color: var(--text-2);
    font-size: 0.78rem;
    padding: 0.15rem 0.1rem;
  }

  .item {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    flex: 0 0 auto;
    max-width: 26rem;
    padding: 0.22rem 0.5rem;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-left-width: 3px;
    color: var(--text-1);
    font: inherit;
    text-align: left;
    cursor: default;
  }

  .item.jumpable {
    cursor: pointer;
  }

  .item.jumpable:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .item:disabled {
    opacity: 0.85;
  }

  .msg {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Severity glyph badge. */
  .glyph {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1rem;
    height: 1rem;
    flex: 0 0 auto;
    font-size: 0.68rem;
    font-weight: 700;
    line-height: 1;
    color: var(--text-inverse);
    border-radius: 1px;
  }

  /* Severity colors: red / amber / blue. */
  .sev-error > .glyph,
  .glyph.sev-error,
  .pill.sev-error {
    background: var(--err);
  }

  .sev-warning > .glyph,
  .glyph.sev-warning,
  .pill.sev-warning {
    background: var(--warn);
    color: var(--bg-0);
  }

  .sev-info > .glyph,
  .glyph.sev-info,
  .pill.sev-info {
    background: var(--accent);
  }

  .item.sev-error {
    border-left-color: var(--err);
  }

  .item.sev-warning {
    border-left-color: var(--warn);
  }

  .item.sev-info {
    border-left-color: var(--accent);
  }
</style>
