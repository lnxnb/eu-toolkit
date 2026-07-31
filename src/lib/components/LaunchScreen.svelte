<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import WorkshopModal from "$lib/components/WorkshopModal.svelte";
  import type { Session } from "$lib/session";

  interface Installation {
    path: string;
    source: string;
  }

  interface RecentProject {
    projectPath: string | null;
    installPath: string;
    displayName: string;
    lastOpened: number;
    pinned: boolean;
    missing: boolean;
  }

  let { onopen }: { onopen: (session: Session) => void } = $props();

  let installations = $state<Installation[]>([]);
  let selected = $state<string | null>(null);
  let scanning = $state(true);
  let error = $state("");
  let recents = $state<RecentProject[]>([]);
  // Workshop browser / on-open warn modal (18.2). `warnPath` is the workshop
  // folder the user tried to open directly (kept so "Open anyway" can proceed).
  let workshop = $state<{ mode: "browse" | "warn"; source: string | null } | null>(null);
  let warnPath = $state<string | null>(null);

  onMount(async () => {
    try {
      const [detected, saved] = await Promise.all([
        invoke<Installation[]>("detect_installations"),
        invoke<string | null>("get_saved_installation").catch(() => null),
        loadRecents(),
      ]);
      installations = detected;
      selected = saved ?? detected[0]?.path ?? null;
    } catch (e) {
      error = String(e);
    }
    scanning = false;
  });

  async function loadRecents() {
    try {
      recents = await invoke<RecentProject[]>("list_recent_projects");
    } catch {
      recents = [];
    }
  }

  /// Path with its middle elided (keeps head + tail) so long paths fit one row.
  function truncateMiddle(path: string, max = 52): string {
    if (path.length <= max) return path;
    const keep = max - 1; // room for the ellipsis
    const head = Math.ceil(keep / 2);
    const tail = Math.floor(keep / 2);
    return `${path.slice(0, head)}…${path.slice(path.length - tail)}`;
  }

  /// Last path segment of the install, for the compact badge (full path in title).
  function installBadge(path: string): string {
    const parts = path.replace(/[\\/]+$/, "").split(/[\\/]/);
    return parts[parts.length - 1] || path;
  }

  async function openRecent(rec: RecentProject) {
    if (rec.missing) return;
    error = "";
    try {
      // Remember the install this recent used (it may differ from the current
      // selection), so a bare reopen resolves the same base game.
      await invoke("save_installation", { path: rec.installPath });
    } catch (e) {
      error = String(e);
      return;
    }
    onopen({
      installPath: rec.installPath,
      modPath: rec.projectPath,
      projectName: rec.projectPath ? rec.displayName : null,
    });
  }

  async function removeRecent(rec: RecentProject, event: MouseEvent) {
    event.stopPropagation();
    try {
      await invoke("remove_recent_project", {
        installPath: rec.installPath,
        projectPath: rec.projectPath,
      });
    } catch (e) {
      error = String(e);
    }
    await loadRecents();
  }

  async function togglePin(rec: RecentProject, event: MouseEvent) {
    event.stopPropagation();
    try {
      await invoke("set_recent_project_pinned", {
        installPath: rec.installPath,
        projectPath: rec.projectPath,
        pinned: !rec.pinned,
      });
    } catch (e) {
      error = String(e);
    }
    await loadRecents();
  }

  async function browseInstall() {
    const path = await open({
      directory: true,
      title: "Select your Europa Universalis IV installation folder",
    });
    if (typeof path !== "string") return;
    error = "";
    try {
      await invoke("save_installation", { path });
      if (!installations.some((i) => i.path === path)) {
        installations = [...installations, { path, source: "Selected manually" }];
      }
      selected = path;
    } catch (e) {
      error = String(e);
    }
  }

  async function requireInstall(): Promise<string | null> {
    if (!selected) {
      error =
        "Select your game installation below first — mod projects are viewed on top of the base game.";
      return null;
    }
    try {
      await invoke("save_installation", { path: selected });
      return selected;
    } catch (e) {
      error = String(e);
      return null;
    }
  }

  async function startFromBase() {
    error = "";
    const install = await requireInstall();
    if (!install) return;
    onopen({ installPath: install, modPath: null, projectName: null });
  }

  // Scaffold a blank-world project (SPRINT2 18.3): keeps the base map and
  // definitions but empties the world, then opens it. Recorded into recents via
  // the +page funnel like any other project.
  async function startFromBlank() {
    error = "";
    const install = await requireInstall();
    if (!install) return;
    const target = await open({
      directory: true,
      title: "Select or create an empty folder for your blank-world mod project",
    });
    if (typeof target !== "string") return;
    try {
      const name = await invoke<string>("scaffold_blank_project", {
        installPath: install,
        targetDir: target,
      });
      onopen({ installPath: install, modPath: target, projectName: name });
    } catch (e) {
      error = String(e);
    }
  }

  async function openProject() {
    error = "";
    const install = await requireInstall();
    if (!install) return;
    const path = await open({
      directory: true,
      title: "Select an EU4 mod project folder",
    });
    if (typeof path !== "string") return;
    // Workshop mods live in a Steam-managed folder that gets overwritten on
    // updates — warn (don't block) and offer to fork instead.
    try {
      if (await invoke<boolean>("is_workshop_path", { path })) {
        warnPath = path;
        workshop = { mode: "warn", source: path };
        return;
      }
    } catch {
      /* detection is best-effort; fall through to a normal open */
    }
    try {
      const name = await invoke<string>("validate_project", { path });
      onopen({ installPath: install, modPath: path, projectName: name });
    } catch (e) {
      error = String(e);
    }
  }

  async function browseWorkshop() {
    error = "";
    const install = await requireInstall();
    if (!install) return;
    workshop = { mode: "browse", source: null };
  }

  function onForked(fork: { path: string; name: string }) {
    workshop = null;
    warnPath = null;
    if (!selected) return;
    onopen({ installPath: selected, modPath: fork.path, projectName: fork.name });
  }

  async function openWorkshopAnyway() {
    const path = warnPath;
    workshop = null;
    warnPath = null;
    if (!path || !selected) return;
    try {
      const name = await invoke<string>("validate_project", { path });
      onopen({ installPath: selected, modPath: path, projectName: name });
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="screen">
  <div class="card">
    <h1>EU Toolkit</h1>
    <p class="subtitle">View, edit, and build Europa Universalis IV mods</p>

    {#if recents.length > 0}
      <h2>Recent projects</h2>
      <ul class="recents">
        {#each recents as rec (rec.projectPath ?? "base:" + rec.installPath)}
          <li>
            <div
              class="recent"
              class:missing={rec.missing}
              role="button"
              aria-disabled={rec.missing}
              tabindex={rec.missing ? -1 : 0}
              onclick={() => openRecent(rec)}
              onkeydown={(e) => {
                if (!rec.missing && (e.key === "Enter" || e.key === " ")) {
                  e.preventDefault();
                  openRecent(rec);
                }
              }}
            >
              <span class="recent-text">
                <span class="recent-name">
                  {rec.displayName}
                  {#if rec.missing}<span class="missing-note">missing</span>{/if}
                </span>
                <span class="recent-path" title={rec.projectPath ?? rec.installPath}>
                  {truncateMiddle(rec.projectPath ?? rec.installPath)}
                </span>
              </span>
              <span class="badge" title={rec.installPath}>{installBadge(rec.installPath)}</span>
              <button
                class="recent-btn pin"
                class:pinned={rec.pinned}
                title={rec.pinned ? "Unpin" : "Pin to top"}
                aria-label={rec.pinned ? "Unpin" : "Pin to top"}
                onclick={(e) => togglePin(rec, e)}>{rec.pinned ? "★" : "☆"}</button
              >
              <button
                class="recent-btn remove"
                title="Remove from list"
                aria-label="Remove from list"
                onclick={(e) => removeRecent(rec, e)}>✕</button
              >
            </div>
          </li>
        {/each}
      </ul>
    {/if}

    <div class="actions">
      <button class="action" onclick={openProject}>
        <span class="action-title">Open an Existing Project…</span>
        <span class="action-sub">A mod folder — local, git checkout, or Steam Workshop</span>
      </button>
      <button class="action" onclick={startFromBase}>
        <span class="action-title">Start from Base Game</span>
        <span class="action-sub">Browse vanilla data; saving edits creates a new mod project</span>
      </button>
      <button class="action" onclick={startFromBlank}>
        <span class="action-title">Start from Blank</span>
        <span class="action-sub">A new mod with the base map but an empty world — build it up with the paint tools</span>
      </button>
      <button class="action" onclick={browseWorkshop}>
        <span class="action-title">Browse Workshop Mods…</span>
        <span class="action-sub">Fork a subscribed Steam Workshop mod into an editable project</span>
      </button>
    </div>

    <h2>Game installation</h2>
    {#if scanning}
      <p class="hint">Scanning Steam libraries…</p>
    {:else if installations.length === 0}
      <p class="hint">
        No installation found automatically. Use Browse to locate your game
        folder (under Steam's <code>steamapps\common</code> — not
        Documents\Paradox Interactive).
      </p>
    {:else}
      <ul class="installs">
        {#each installations as inst}
          <li>
            <button
              class="install"
              class:selected={inst.path === selected}
              onclick={() => (selected = inst.path)}
            >
              <span class="radio" aria-hidden="true"></span>
              <span class="install-text">
                <span class="path">{inst.path}</span>
                <span class="source">{inst.source}</span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    <button class="browse" onclick={browseInstall}>Browse…</button>

    {#if error}
      <p class="error">{error}</p>
    {/if}
  </div>
</div>

{#if workshop && selected}
  <WorkshopModal
    installPath={selected}
    mode={workshop.mode}
    source={workshop.source}
    onclose={() => {
      workshop = null;
      warnPath = null;
    }}
    onforked={onForked}
    onopenanyway={openWorkshopAnyway}
  />
{/if}

<style>
  .screen {
    min-height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--sp-4);
    background: var(--bg-0);
  }

  .card {
    width: 100%;
    max-width: 38rem;
    background: var(--bg-2);
    color: var(--text-1);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-2);
    padding: var(--sp-6);
    box-shadow: var(--shadow-window);
    text-align: center;
  }

  h1 {
    margin: 0 0 0.25rem;
  }

  .subtitle {
    margin: 0 0 1.5rem;
    color: var(--text-3);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    margin-bottom: 1.75rem;
  }

  .action {
    flex: 1 1 14rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    padding: 1rem;
    border-radius: var(--r-2);
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: inherit;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
    transition: border-color 0.15s;
  }

  .action:hover {
    border-color: var(--accent-text);
  }

  .action-title {
    font-weight: 600;
    font-size: 1rem;
  }

  .action-sub {
    font-size: 0.8rem;
    color: var(--text-3);
  }

  h2 {
    margin: 0 0 0.5rem;
    font-size: 0.85rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-3);
    text-align: left;
  }

  .hint {
    color: var(--text-3);
    font-size: 0.9rem;
    text-align: left;
  }

  .installs {
    list-style: none;
    margin: 0 0 0.75rem;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .install {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 0.65rem;
    text-align: left;
    padding: 0.6rem 0.85rem;
    border-radius: var(--r-2);
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: inherit;
    font-family: inherit;
    font-size: 0.95rem;
    cursor: pointer;
    transition: border-color 0.15s;
  }

  .install:hover {
    border-color: var(--accent-text);
  }

  .install.selected {
    border-color: var(--accent-text);
    background: rgba(57, 108, 216, 0.08);
  }

  .radio {
    flex-shrink: 0;
    width: 0.9rem;
    height: 0.9rem;
    border-radius: 50%;
    border: 2px solid var(--text-2);
  }

  .install.selected .radio {
    border-color: var(--accent-text);
    background: var(--accent-text);
    box-shadow: inset 0 0 0 2.5px var(--text-inverse);
  }

  .install-text {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
  }

  .install-text .path {
    word-break: break-all;
  }

  .install-text .source {
    font-size: 0.8rem;
    color: var(--text-3);
  }

  .browse {
    border-radius: 8px;
    border: 1px solid transparent;
    padding: 0.55em 1.2em;
    font-size: 0.95em;
    font-weight: 500;
    font-family: inherit;
    background: var(--bg-1);
    color: var(--text-1);
    box-shadow: var(--shadow-popover);
    cursor: pointer;
    transition: border-color 0.25s;
  }

  .browse:hover {
    border-color: var(--accent-text);
  }

  .error {
    color: var(--err);
    font-size: 0.9rem;
  }

  .recents {
    list-style: none;
    margin: 0 0 1.5rem;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .recent {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    text-align: left;
    padding: 0.5rem 0.7rem;
    border-radius: var(--r-2);
    border: 1px solid var(--border);
    background: var(--bg-1);
    cursor: pointer;
    transition: border-color 0.15s;
  }

  .recent:hover:not(.missing) {
    border-color: var(--accent-text);
  }

  .recent.missing {
    cursor: default;
    opacity: 0.55;
  }

  .recent-text {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
    flex: 1;
  }

  .recent-name {
    font-weight: 600;
    font-size: 0.92rem;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .missing-note {
    font-weight: 500;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--err);
    border: 1px solid var(--err);
    border-radius: 4px;
    padding: 0 0.3rem;
  }

  .recent-path {
    font-size: 0.78rem;
    color: var(--text-3);
    white-space: nowrap;
  }

  .badge {
    flex-shrink: 0;
    max-width: 10rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.72rem;
    color: var(--text-2);
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    padding: 0.1rem 0.55rem;
  }

  .recent-btn {
    flex-shrink: 0;
    width: 1.7rem;
    height: 1.7rem;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-3);
    font-family: inherit;
    font-size: 0.95rem;
    line-height: 1;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }

  .recent-btn:hover {
    background: rgba(57, 108, 216, 0.12);
    color: var(--accent-text);
  }

  .recent-btn.pin.pinned {
    color: var(--warn);
  }

  .recent-btn.remove:hover {
    background: rgba(185, 29, 29, 0.12);
    color: var(--err);
  }

  @media (prefers-color-scheme: dark) {
    .card {
      background: var(--bg-1);
      color: var(--text-1);
    }

    .action,
    .install,
    .recent {
      background: var(--bg-0);
      border-color: var(--bg-3);
      color: inherit;
    }

    .recent:hover:not(.missing) {
      border-color: var(--accent-text);
    }

    .badge {
      background: var(--bg-1);
      border-color: var(--bg-3);
      color: var(--text-1);
    }

    .recent-path {
      color: var(--text-2);
    }

    .install.selected {
      border-color: var(--accent-text);
      background: rgba(91, 141, 239, 0.12);
    }

    .install.selected .radio {
      box-shadow: inset 0 0 0 2.5px var(--bg-0);
    }

    .browse {
      background: var(--bg-0);
      color: inherit;
    }

    .subtitle,
    .hint,
    .action-sub,
    .install-text .source {
      color: var(--text-2);
    }
  }
</style>
