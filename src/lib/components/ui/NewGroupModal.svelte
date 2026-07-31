<!--
  NewGroupModal — collects the inputs for a brand-new religion or culture group
  (S2.3 / S2.4), shared by the create-religion/culture flow and the panels'
  move-to-group dropdowns.

  Religion groups copy their defaults (defender_of_faith, flags, …) from a chosen
  sibling group. Culture groups additionally require a graphical_culture (the game
  crashes renderers without one) and copy the sibling's name pools. Both need a
  sibling to copy from, so the sibling picker is always shown.

  z-index: 100/101 — the modal layer, above the map-prompt/popover layers.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import SearchDropdown from "./SearchDropdown.svelte";
  import type { DropdownItem, NewGroupResult } from "./types";

  interface GroupOption {
    key: string;
    name: string;
  }
  interface RegistryEntry {
    key: string;
    name: string;
  }

  let {
    open = $bindable(false),
    kind,
    installPath,
    modPath,
    groups,
    defaultSibling = "",
    entityLabel = "",
    onconfirm,
    oncancel,
  }: {
    open?: boolean;
    kind: "religion" | "culture";
    installPath: string;
    modPath: string | null;
    /** Existing groups to copy defaults/pools from. */
    groups: GroupOption[];
    /** Sibling preselected when the modal opens (e.g. the current group). */
    defaultSibling?: string;
    /** Optional context line, e.g. the religion/culture being created/moved. */
    entityLabel?: string;
    onconfirm: (result: NewGroupResult) => void;
    oncancel: () => void;
  } = $props();

  let name = $state("");
  let sibling = $state("");
  let graphicalCulture = $state<string | null>(null);
  let graphicalCultures = $state<RegistryEntry[]>([]);
  let seededOpen = false;

  // Seed the form each time the modal opens (defaultSibling may load late).
  $effect(() => {
    if (open && !seededOpen) {
      seededOpen = true;
      name = "";
      sibling = defaultSibling || groups[0]?.key || "";
      graphicalCulture = null;
    } else if (!open) {
      seededOpen = false;
    }
  });

  // Culture groups need a graphical_culture; load the registry once opened.
  $effect(() => {
    if (open && kind === "culture" && graphicalCultures.length === 0) {
      invoke<RegistryEntry[]>("get_registry", {
        name: "graphical_cultures",
        installPath,
        modPath,
      })
        .then((v) => {
          graphicalCultures = v;
          // Default the gfx to the sibling group's own where known.
        })
        .catch(() => {});
    }
  });

  let groupItems = $derived<DropdownItem[]>(groups.map((g) => ({ key: g.key, label: g.name })));
  let gfxItems = $derived<DropdownItem[]>(
    graphicalCultures.map((g) => ({ key: g.key, label: g.name })),
  );

  let canConfirm = $derived(
    name.trim().length > 0 &&
      sibling.length > 0 &&
      (kind === "religion" || (graphicalCulture != null && graphicalCulture.length > 0)),
  );

  function confirm() {
    if (!canConfirm) return;
    onconfirm({
      name: name.trim(),
      sibling,
      graphicalCulture: kind === "culture" ? (graphicalCulture ?? undefined) : undefined,
    });
    open = false;
  }
  function cancel() {
    oncancel();
    open = false;
  }
  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      cancel();
      e.preventDefault();
    } else if (e.key === "Enter" && canConfirm) {
      confirm();
      e.preventDefault();
    }
  }

  const title = $derived(kind === "religion" ? "New religion group" : "New culture group");
</script>

{#if open}
  <div class="modal-root" role="dialog" aria-modal="true" aria-label={title}>
    <button class="backdrop" aria-label="Cancel" onclick={cancel}></button>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="dialog" onkeydown={onKeydown} role="document">
      <header class="dialog-head">
        <span class="dialog-title">{title}</span>
        <button class="close" aria-label="Close" onclick={cancel}>×</button>
      </header>

      <div class="body">
        {#if entityLabel}
          <p class="ctx">For {entityLabel}</p>
        {/if}

        <label class="field">
          <span class="lbl">Group name</span>
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="text"
            type="text"
            bind:value={name}
            placeholder={kind === "religion" ? "e.g. Solar Faiths" : "e.g. Sky People"}
            autofocus
          />
        </label>

        <label class="field">
          <span class="lbl">Copy {kind === "religion" ? "defaults" : "name pools"} from</span>
          <SearchDropdown
            items={groupItems}
            value={sibling}
            placeholder="Pick a group…"
            onselect={(k) => (sibling = k)}
          />
        </label>

        {#if kind === "culture"}
          <label class="field">
            <span class="lbl">Graphical culture <span class="req">required</span></span>
            <SearchDropdown
              items={gfxItems}
              value={graphicalCulture}
              placeholder="Pick graphical culture…"
              onselect={(k) => (graphicalCulture = k)}
            />
          </label>
        {/if}

        <p class="hint">
          {#if kind === "religion"}
            Group-level defaults (defender of faith, flags, crusade name…) are
            copied from the chosen group. The new religion goes inside it.
          {:else}
            Name pools are copied from the chosen group so generated rulers have
            names. The new culture goes inside it.
          {/if}
        </p>
      </div>

      <footer class="dialog-foot">
        <span class="spacer"></span>
        <button class="btn" onclick={cancel}>Cancel</button>
        <button class="btn primary" disabled={!canConfirm} onclick={confirm}>Create group</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .modal-root {
    position: fixed;
    inset: 0;
    z-index: var(--z-modal);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .backdrop {
    position: absolute;
    inset: 0;
    border: none;
    background: rgba(0, 0, 0, 0.55);
    cursor: default;
  }
  .dialog {
    position: relative;
    z-index: var(--z-modal-content);
    display: flex;
    flex-direction: column;
    width: 24rem;
    max-width: calc(100vw - 2rem);
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text-1);
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.5);
  }
  .dialog-head {
    display: flex;
    align-items: center;
    padding: 0.45rem 0.6rem;
    background: var(--bg-3);
    border-bottom: 1px solid var(--border);
  }
  .dialog-title {
    flex: 1;
    font-weight: 700;
  }
  .close {
    border: none;
    background: transparent;
    color: var(--text-1);
    font-size: 1.2rem;
    line-height: 1;
    cursor: pointer;
    padding: 0 0.25rem;
  }
  .close:hover {
    color: var(--text-inverse);
  }
  .body {
    padding: 0.6rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .ctx {
    margin: 0;
    font-size: 0.78rem;
    color: var(--text-2);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  .lbl {
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }
  .req {
    text-transform: none;
    letter-spacing: 0;
    color: var(--warn);
    font-size: 0.7rem;
  }
  .text {
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.3rem 0.45rem;
    outline: none;
  }
  .text:focus {
    border-color: var(--accent);
  }
  .hint {
    margin: 0;
    font-size: 0.75rem;
    color: var(--text-2);
    line-height: 1.35;
  }
  .dialog-foot {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.5rem 0.6rem;
    background: var(--bg-2);
    border-top: 1px solid var(--border);
  }
  .spacer {
    flex: 1;
  }
  .btn {
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.3rem 0.8rem;
    cursor: pointer;
  }
  .btn:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }
  .btn.primary {
    background: var(--accent);
    color: var(--text-inverse);
    font-weight: 600;
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
    background: var(--bg-3);
    color: var(--text-2);
  }
</style>
