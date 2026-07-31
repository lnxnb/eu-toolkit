<!--
  ProblemsOverlay (View ▸ Problems) — Sprint 30.2.

  A modal dashboard over the aggregate `validate_all` backend command: every
  validation domain run in one pass, grouped by domain, with severity filter
  chips and a re-run button. Every row keeps its typed JumpTarget (same wiring as
  the per-mode ValidationStrips) — the host routes it via `onjump`, closing the
  modal so the map is visible. Count badge + auto-rerun-on-save live in MapView.

  Reports/loading are owned by MapView (so the View-menu badge stays live); this
  is a pure view over them.
-->
<script module lang="ts">
  import type { ValidationIssue, JumpTarget } from "./ValidationStrip.svelte";

  /** One domain's slice of an aggregate run (mirrors backend `DomainReport`). */
  export interface DomainReport {
    domain: string;
    issues: ValidationIssue[];
  }

  export type SevFilter = "error" | "warning" | "info";

  const SEV_GLYPH: Record<SevFilter, string> = { error: "✕", warning: "!", info: "i" };
  const SEV_ORDER: Record<SevFilter, number> = { error: 0, warning: 1, info: 2 };

  /** Prettify a domain key (`trade_nodes` → `Trade Nodes`). */
  export function prettyDomain(key: string): string {
    return key
      .split("_")
      .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
      .join(" ");
  }
</script>

<script lang="ts">
  import { OverlaySurface } from "$lib/components/script";

  let {
    open = $bindable(false),
    reports = [],
    running = false,
    hasRun = false,
    onrerun,
    onjump,
  }: {
    open?: boolean;
    reports?: DomainReport[];
    running?: boolean;
    /** True once at least one run has completed (drives the empty vs clean text). */
    hasRun?: boolean;
    onrerun?: () => void;
    onjump?: (jump: JumpTarget) => void;
  } = $props();

  // Which severities are shown (chips toggle these).
  let show = $state<Record<SevFilter, boolean>>({ error: true, warning: true, info: true });

  // Totals across every domain, per severity (for the header chips).
  const totals = $derived.by(() => {
    const t: Record<SevFilter, number> = { error: 0, warning: 0, info: 0 };
    for (const r of reports) for (const i of r.issues) t[i.severity] += 1;
    return t;
  });

  // Domains with at least one issue passing the active severity filter, each
  // with its filtered+sorted issue list.
  const groups = $derived.by(() => {
    const out: { domain: string; issues: ValidationIssue[] }[] = [];
    for (const r of reports) {
      const kept = r.issues
        .filter((i) => show[i.severity])
        .sort((a, b) => SEV_ORDER[a.severity] - SEV_ORDER[b.severity]);
      if (kept.length > 0) out.push({ domain: r.domain, issues: kept });
    }
    return out;
  });

  const shownCount = $derived(groups.reduce((n, g) => n + g.issues.length, 0));

  function toggle(sev: SevFilter) {
    show[sev] = !show[sev];
    show = { ...show };
  }

  function clickIssue(issue: ValidationIssue) {
    if (issue.jump) onjump?.(issue.jump);
  }
</script>

<OverlaySurface bind:open title="Problems">
  {#snippet toolbar()}
    <div class="chips">
      {#each ["error", "warning", "info"] as const as sev}
        <button
          class="chip sev-{sev}"
          class:on={show[sev]}
          onclick={() => toggle(sev)}
          title="Toggle {sev}s"
        >
          <span class="glyph sev-{sev}">{SEV_GLYPH[sev]}</span>
          {totals[sev]}
        </button>
      {/each}
    </div>
    <button class="rerun" disabled={running} onclick={() => onrerun?.()}>
      {running ? "Running…" : "Re-run"}
    </button>
  {/snippet}

  <div class="report">
    {#if groups.length === 0}
      <p class="empty">
        {#if running}
          Running validation…
        {:else if !hasRun}
          Press “Re-run” to validate the project across every domain.
        {:else if shownCount === 0 && (totals.error || totals.warning || totals.info)}
          No issues match the active filters.
        {:else}
          ✓ No problems found across all domains.
        {/if}
      </p>
    {:else}
      {#each groups as g (g.domain)}
        <section class="domain">
          <header class="domain-head">
            <span class="domain-name">{prettyDomain(g.domain)}</span>
            <span class="domain-count">{g.issues.length}</span>
          </header>
          <ul class="issues">
            {#each g.issues as issue}
              <li>
                <button
                  class="issue sev-{issue.severity}"
                  class:jumpable={!!issue.jump}
                  disabled={!issue.jump}
                  title={issue.jump ? "Jump to problem" : issue.message}
                  onclick={() => clickIssue(issue)}
                >
                  <span class="glyph sev-{issue.severity}">{SEV_GLYPH[issue.severity]}</span>
                  <span class="msg">{issue.message}</span>
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    {/if}
  </div>
</OverlaySurface>

<style>
  .chips {
    display: flex;
    gap: 0.35rem;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    border: 1px solid var(--bg-3);
    background: var(--bg-2);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.74rem;
    padding: 0.15rem 0.5rem;
    cursor: pointer;
    font-variant-numeric: tabular-nums;
  }
  .chip:not(.on) {
    opacity: 0.45;
  }
  .chip.on.sev-error {
    border-color: var(--err);
  }
  .chip.on.sev-warning {
    border-color: var(--warn);
  }
  .chip.on.sev-info {
    border-color: var(--accent);
  }

  .rerun {
    border: 1px solid var(--accent);
    background: var(--accent);
    color: var(--text-inverse);
    font-family: inherit;
    font-size: 0.76rem;
    padding: 0.18rem 0.7rem;
    cursor: pointer;
  }
  .rerun:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .report {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .empty {
    color: var(--text-2);
    font-size: 0.85rem;
    padding: 1rem 0.3rem;
  }

  .domain {
    border: 1px solid var(--border);
    background: var(--bg-2);
  }

  .domain-head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.3rem 0.5rem;
    background: var(--bg-1);
    border-bottom: 1px solid var(--border);
  }

  .domain-name {
    flex: 1;
    font-weight: 700;
    color: var(--ok);
    font-size: 0.85rem;
  }

  .domain-count {
    font-size: 0.72rem;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }

  .issues {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .issue {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    width: 100%;
    text-align: left;
    border: none;
    border-left: 3px solid transparent;
    border-bottom: 1px solid var(--bg-1);
    background: transparent;
    color: var(--text-1);
    font: inherit;
    font-size: 0.8rem;
    padding: 0.28rem 0.55rem;
    cursor: default;
  }
  .issue.jumpable {
    cursor: pointer;
  }
  .issue.jumpable:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .msg {
    flex: 1;
    min-width: 0;
  }

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
  .glyph.sev-error {
    background: var(--err);
  }
  .glyph.sev-warning {
    background: var(--warn);
    color: var(--bg-0);
  }
  .glyph.sev-info {
    background: var(--accent);
  }
  .issue.sev-error {
    border-left-color: var(--err);
  }
  .issue.sev-warning {
    border-left-color: var(--warn);
  }
  .issue.sev-info {
    border-left-color: var(--accent);
  }
</style>
