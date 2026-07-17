<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import LaunchScreen from "$lib/components/LaunchScreen.svelte";
  import MapView from "$lib/components/MapView.svelte";
  import type { Session } from "$lib/session";

  let session = $state<Session | null>(null);
  // Bumped when the map view must fully reload (different project/base);
  // not bumped when a base session becomes a project after its first save.
  let sessionKey = $state(0);

  // Key under which the live session is mirrored to sessionStorage.
  const SESSION_KEY = "euSession";

  // --- Session survival across unexpected reloads -------------------------
  //
  // The in-memory `session` $state is the source of truth, but a full-page
  // reload wipes it back to null → the launch screen. Reloads can happen
  // WITHOUT any user action: vite's dep optimizer re-bundles mid-session the
  // first time a new bare dependency is imported ("optimized dependencies
  // changed. reloading"), and in run.bat stable mode an undeliverable reload
  // leaves a stale graph. Either way the user's just-opened project would
  // silently vanish. To prevent that we mirror the session to sessionStorage
  // and restore it on startup.
  //
  // Why sessionStorage (not localStorage): it is scoped to the top-level
  // browsing context and, in the Tauri WebView2, is cleared when the WebView
  // is torn down on app close — so it does NOT leak a stale session across
  // separate app launches. Even in the edge case where a platform DID persist
  // it across a restart, reopening the last session on relaunch is harmless-
  // to-good UX (recents already record it), so the restore rule is simply:
  // "if sessionStorage holds a session, restore it." No per-process nonce is
  // needed (a pid isn't obtainable frontend-side, and the fallback is benign).
  //
  // Pending unsaved edits live in MapView's own $state and are LOST on a
  // reload regardless — that is accepted; this only preserves which project
  // is open, not its in-flight edit queue.
  function persistSession(s: Session | null) {
    try {
      if (s) sessionStorage.setItem(SESSION_KEY, JSON.stringify(s));
      else sessionStorage.removeItem(SESSION_KEY);
    } catch {
      // Storage can throw (private mode / quota); session survival is a
      // best-effort nicety, never block the actual session on it.
    }
  }

  // Record every successfully-opened session in the recent-projects list.
  // Both entry points below funnel through here — the launch screen's opens
  // (openSession) and in-app switches / save-created projects (updateSession) —
  // so a new project made by saving a base session gets recorded too. Fire and
  // forget: recents are best-effort UI state and must never block a session.
  function recordRecent(s: Session) {
    invoke("record_recent_project", {
      installPath: s.installPath,
      projectPath: s.modPath,
    }).catch(() => {});
  }

  function openSession(next: Session) {
    session = next;
    sessionKey += 1;
    persistSession(next);
    recordRecent(next);
  }

  function updateSession(next: Session, remount: boolean) {
    session = next;
    if (remount) sessionKey += 1;
    persistSession(next);
    recordRecent(next);
  }

  function goHome() {
    session = null;
    persistSession(null);
  }

  // --- Silent-failure banner ---------------------------------------------
  //
  // Any uncaught error or unhandled promise rejection (e.g. a mount/flush
  // failure that would otherwise leave the app doing "nothing") is surfaced as
  // a dismissible banner the user can screenshot, instead of failing silently.
  let errorMessage = $state<string | null>(null);

  onMount(() => {
    // Restore a session persisted before an unexpected reload (see above).
    try {
      const raw = sessionStorage.getItem(SESSION_KEY);
      if (raw) {
        const restored = JSON.parse(raw) as Session;
        if (restored && restored.installPath) session = restored;
      }
    } catch {
      // Corrupt/blocked storage: just start at the launch screen.
    }

    const onError = (e: ErrorEvent) => {
      errorMessage = e.message || String(e.error ?? "unknown error");
    };
    const onRejection = (e: PromiseRejectionEvent) => {
      const r = e.reason;
      errorMessage =
        (r && (r.message ?? String(r))) || "unhandled promise rejection";
    };
    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);
    return () => {
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
    };
  });
</script>

{#if errorMessage !== null}
  <div class="error-banner" role="alert">
    <span class="error-text">Unexpected error: {errorMessage}</span>
    <button
      class="error-dismiss"
      onclick={() => (errorMessage = null)}
      aria-label="Dismiss error">×</button
    >
  </div>
{/if}

{#if session === null}
  <LaunchScreen onopen={openSession} />
{:else}
  {#key sessionKey}
    <MapView
      installPath={session.installPath}
      modPath={session.modPath}
      projectName={session.projectName}
      onsession={updateSession}
      onhome={goHome}
    />
  {/key}
{/if}

<style>
  /* Windows-classic error banner, above everything (modals top out ~100+). */
  .error-banner {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    z-index: 1000;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 10px;
    background: #7a1f1f;
    border-bottom: 1px solid #2b323d;
    color: #f2d6d6;
    font-size: 13px;
  }
  .error-text {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .error-dismiss {
    flex: none;
    width: 22px;
    height: 22px;
    line-height: 20px;
    padding: 0;
    background: #3f4855;
    border: 1px solid #2b323d;
    color: #cfd4db;
    cursor: pointer;
    font-size: 16px;
  }
  .error-dismiss:hover {
    background: #4a6da7;
  }
</style>
