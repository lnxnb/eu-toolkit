<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, untrack } from "svelte";

  interface WorkshopMod {
    id: string;
    path: string;
    name: string;
    sizeBytes: number;
  }

  interface ForkPlan {
    name: string;
    slug: string;
    sizeSkip: number;
    sizeFull: number;
    freeBytes: number;
  }

  interface ForkProgress {
    copiedBytes: number;
    totalBytes: number;
    currentFile: string;
  }

  interface ForkFinished {
    error: string | null;
    canceled: boolean;
    path: string | null;
    name: string | null;
  }

  let {
    installPath,
    mode,
    source = null,
    onclose,
    onforked,
    onopenanyway,
  }: {
    installPath: string;
    /** "browse" opens the subscribed-mods list; "warn" is the on-open advisory. */
    mode: "browse" | "warn";
    /** The workshop folder for "warn" mode (and its direct fork). */
    source?: string | null;
    onclose: () => void;
    onforked: (fork: { path: string; name: string }) => void;
    onopenanyway?: () => void;
  } = $props();

  // Props are fixed for this modal's lifetime (the parent remounts it per open),
  // so capture the initial values once for state initialization.
  const initialMode = untrack(() => mode);
  const initialSource = untrack(() => source);

  type View = "warn" | "list" | "dialog" | "progress";
  let view = $state<View>(initialMode === "warn" ? "warn" : "list");

  let mods = $state<WorkshopMod[]>([]);
  let listLoading = $state(initialMode === "browse");
  let error = $state("");

  // Fork dialog state.
  let forkSource = $state<string>(initialSource ?? "");
  let plan = $state<ForkPlan | null>(null);
  let forkName = $state("");
  let forkSlug = $state("");
  let fullCopy = $state(false);
  let preparing = $state(false);

  // Progress state.
  let progress = $state<ForkProgress>({ copiedBytes: 0, totalBytes: 0, currentFile: "" });
  let canceling = $state(false);
  let unlisten: UnlistenFn[] = [];

  onDestroy(() => teardownListeners());

  function teardownListeners() {
    for (const u of unlisten) u();
    unlisten = [];
  }

  if (initialMode === "browse") loadList();

  async function loadList() {
    listLoading = true;
    error = "";
    try {
      mods = await invoke<WorkshopMod[]>("list_workshop_mods", { installPath });
    } catch (e) {
      error = String(e);
    }
    listLoading = false;
  }

  function fmtBytes(n: number): string {
    if (n <= 0) return "0 MB";
    const mb = n / (1024 * 1024);
    if (mb >= 1024) return `${(mb / 1024).toFixed(2)} GB`;
    if (mb >= 1) return `${mb.toFixed(0)} MB`;
    return `${(n / 1024).toFixed(0)} KB`;
  }

  async function openForkDialog(src: string) {
    forkSource = src;
    error = "";
    preparing = true;
    view = "dialog";
    try {
      plan = await invoke<ForkPlan>("prepare_fork", { sourcePath: src });
      forkName = plan.name;
      forkSlug = plan.slug;
      fullCopy = false;
    } catch (e) {
      error = String(e);
      plan = null;
    }
    preparing = false;
  }

  let payloadSize = $derived(plan ? (fullCopy ? plan.sizeFull : plan.sizeSkip) : 0);
  // Mirror the backend cushion (5% + 64MiB) for an advisory client-side check.
  let enoughSpace = $derived(
    !plan || plan.freeBytes === 0
      ? true
      : plan.freeBytes >= payloadSize + Math.floor(payloadSize / 20) + 64 * 1024 * 1024,
  );

  async function startFork() {
    if (!forkSource || !forkSlug.trim()) return;
    error = "";
    teardownListeners();
    progress = { copiedBytes: 0, totalBytes: payloadSize, currentFile: "" };
    canceling = false;

    // Subscribe BEFORE starting so no early event is missed.
    unlisten.push(
      await listen<ForkProgress>("fork-progress", (e) => {
        progress = e.payload;
      }),
    );
    unlisten.push(
      await listen<ForkFinished>("fork-finished", (e) => {
        teardownListeners();
        const f = e.payload;
        if (f.error) {
          error = f.error;
          view = "dialog";
        } else if (f.canceled) {
          view = "dialog";
        } else if (f.path && f.name) {
          onforked({ path: f.path, name: f.name });
        }
      }),
    );

    try {
      await invoke("start_fork", {
        installPath,
        sourcePath: forkSource,
        name: forkName,
        slug: forkSlug.trim(),
        fullCopy,
      });
      view = "progress";
    } catch (e) {
      // Synchronous preflight failure (collision / free space): no events coming.
      teardownListeners();
      error = String(e);
      view = "dialog";
    }
  }

  async function cancelFork() {
    canceling = true;
    try {
      await invoke("cancel_fork");
    } catch {
      /* best effort */
    }
  }

  function backFromDialog() {
    error = "";
    if (mode === "browse") {
      view = "list";
    } else {
      onclose();
    }
  }

  let pct = $derived(
    progress.totalBytes > 0
      ? Math.min(100, Math.round((progress.copiedBytes / progress.totalBytes) * 100))
      : 0,
  );
</script>

<div class="overlay" role="dialog" aria-modal="true">
  <div class="modal">
    {#if view === "warn"}
      <h2>This is a Steam Workshop mod</h2>
      <p class="body">
        Steam <strong>overwrites this folder whenever the mod updates</strong>, so any edits
        you make here can be lost without warning. Fork it into your own projects folder to
        edit safely.
      </p>
      <p class="path" title={source ?? ""}>{source}</p>
      <p class="note">Forks don't track upstream updates from the Workshop.</p>
      {#if error}<p class="error">{error}</p>{/if}
      <div class="buttons">
        <button class="primary" onclick={() => source && openForkDialog(source)}>
          Fork to my projects <span class="rec">(recommended)</span>
        </button>
        <button
          onclick={() => {
            onopenanyway?.();
          }}>Open anyway</button
        >
        <button class="ghost" onclick={onclose}>Cancel</button>
      </div>
    {:else if view === "list"}
      <div class="head">
        <h2>Fork from Steam Workshop</h2>
        <button class="close" aria-label="Close" onclick={onclose}>✕</button>
      </div>
      <p class="body">
        Copy a subscribed Workshop mod into your own projects folder so you can edit it
        safely. Forks don't track upstream updates.
      </p>
      {#if listLoading}
        <p class="hint">Scanning your Workshop subscriptions…</p>
      {:else if error}
        <p class="error">{error}</p>
      {:else if mods.length === 0}
        <p class="hint">
          No subscribed EU4 Workshop mods were found for this installation.
        </p>
      {:else}
        <ul class="mods">
          {#each mods as m (m.id)}
            <li>
              <span class="mod-text">
                <span class="mod-name">{m.name}</span>
                <span class="mod-meta">#{m.id} · {fmtBytes(m.sizeBytes)}</span>
              </span>
              <button class="primary small" onclick={() => openForkDialog(m.path)}>Fork</button>
            </li>
          {/each}
        </ul>
      {/if}
      <div class="buttons end">
        <button class="ghost" onclick={onclose}>Close</button>
      </div>
    {:else if view === "dialog"}
      <div class="head">
        <h2>Fork mod</h2>
        <button class="close" aria-label="Close" onclick={backFromDialog}>✕</button>
      </div>
      {#if preparing}
        <p class="hint">Measuring…</p>
      {:else if plan}
        <label class="field">
          <span>Name</span>
          <input type="text" bind:value={forkName} />
        </label>
        <label class="field">
          <span>Folder</span>
          <input type="text" bind:value={forkSlug} spellcheck="false" />
        </label>
        <p class="dest" title={forkSlug}>
          → Documents\Paradox Interactive\Europa Universalis IV\mod\{forkSlug}
        </p>
        <label class="checkbox">
          <input type="checkbox" bind:checked={fullCopy} />
          <span>Full copy (include <code>.git</code>, <code>.github</code>, <code>*.psd</code>)</span>
        </label>
        <p class="sizes">
          Copies about <strong>{fmtBytes(payloadSize)}</strong>
          {#if plan.freeBytes > 0}· {fmtBytes(plan.freeBytes)} free{/if}
        </p>
        {#if !enoughSpace}
          <p class="error">Not enough free space on the destination drive.</p>
        {/if}
        <p class="note">Forks don't track upstream updates from the Workshop.</p>
        {#if error}<p class="error">{error}</p>{/if}
        <div class="buttons end">
          <button class="ghost" onclick={backFromDialog}>Cancel</button>
          <button class="primary" disabled={!forkSlug.trim() || !enoughSpace} onclick={startFork}>
            Fork
          </button>
        </div>
      {:else}
        {#if error}<p class="error">{error}</p>{/if}
        <div class="buttons end">
          <button class="ghost" onclick={backFromDialog}>Back</button>
        </div>
      {/if}
    {:else if view === "progress"}
      <h2>Forking…</h2>
      <div class="bar"><div class="fill" style="width:{pct}%"></div></div>
      <p class="prog-line">
        {pct}% · {fmtBytes(progress.copiedBytes)} / {fmtBytes(progress.totalBytes)}
      </p>
      <p class="cur" title={progress.currentFile}>{progress.currentFile || "…"}</p>
      <div class="buttons end">
        <button class="ghost" disabled={canceling} onclick={cancelFork}>
          {canceling ? "Cancelling…" : "Cancel"}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 200;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.55);
    padding: 1rem;
  }

  .modal {
    width: 100%;
    max-width: 30rem;
    background: #2b323d;
    border: 1px solid #1c2129;
    color: #cfd4db;
    padding: 1.1rem 1.25rem 1.25rem;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.5);
    max-height: 85vh;
    overflow-y: auto;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }

  h2 {
    margin: 0 0 0.6rem;
    font-size: 1.05rem;
  }

  .head h2 {
    margin-bottom: 0.4rem;
  }

  .body {
    font-size: 0.9rem;
    line-height: 1.4;
    margin: 0 0 0.75rem;
  }

  .path,
  .dest {
    font-size: 0.78rem;
    color: #9aa4b2;
    background: #222831;
    border: 1px solid #1c2129;
    padding: 0.35rem 0.5rem;
    word-break: break-all;
    margin: 0 0 0.75rem;
  }

  .note {
    font-size: 0.8rem;
    color: #c7a95b;
    margin: 0 0 0.75rem;
  }

  .hint {
    font-size: 0.88rem;
    color: #9aa4b2;
  }

  .error {
    font-size: 0.85rem;
    color: #e08585;
    margin: 0 0 0.6rem;
  }

  .buttons {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    margin-top: 0.5rem;
  }

  .buttons.end {
    flex-direction: row;
    justify-content: flex-end;
  }

  button {
    font-family: inherit;
    font-size: 0.9rem;
    padding: 0.5rem 0.9rem;
    border-radius: 0;
    border: 1px solid #1c2129;
    background: #3f4855;
    color: #cfd4db;
    cursor: pointer;
  }

  button:hover:not(:disabled) {
    background: #4a6da7;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  button.primary {
    background: #4a6da7;
    border-color: #33507d;
  }

  button.primary:hover:not(:disabled) {
    background: #5a7fbd;
  }

  button.ghost {
    background: transparent;
  }

  button.small {
    padding: 0.35rem 0.75rem;
  }

  .rec {
    opacity: 0.85;
    font-size: 0.82em;
  }

  .close {
    border: none;
    background: transparent;
    padding: 0.1rem 0.4rem;
    font-size: 0.95rem;
  }

  .mods {
    list-style: none;
    margin: 0 0 0.5rem;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    max-height: 44vh;
    overflow-y: auto;
  }

  .mods li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.45rem 0.55rem;
    background: #222831;
    border: 1px solid #1c2129;
  }

  .mod-text {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
    flex: 1;
  }

  .mod-name {
    font-size: 0.9rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mod-meta {
    font-size: 0.74rem;
    color: #9aa4b2;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    margin-bottom: 0.6rem;
    font-size: 0.82rem;
    color: #9aa4b2;
  }

  .field input {
    font-family: inherit;
    font-size: 0.9rem;
    padding: 0.4rem 0.5rem;
    background: #222831;
    border: 1px solid #1c2129;
    color: #e6e9ee;
  }

  .checkbox {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    font-size: 0.82rem;
    margin-bottom: 0.6rem;
  }

  .checkbox code {
    font-size: 0.9em;
    color: #c7a95b;
  }

  .sizes {
    font-size: 0.82rem;
    color: #9aa4b2;
    margin: 0 0 0.6rem;
  }

  .bar {
    height: 0.75rem;
    background: #222831;
    border: 1px solid #1c2129;
    margin: 0.4rem 0;
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: #4a6da7;
    transition: width 0.1s linear;
  }

  .prog-line {
    font-size: 0.85rem;
    margin: 0.2rem 0;
  }

  .cur {
    font-size: 0.76rem;
    color: #9aa4b2;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    margin: 0 0 0.4rem;
  }
</style>
