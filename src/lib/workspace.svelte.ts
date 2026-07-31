import { invoke } from "@tauri-apps/api/core";
import { isView, viewDefinition, type DockPreference, type View } from "$lib/views";

export type OpenTarget = "reuse" | "tab" | "window" | "background-tab";
export interface Rect { x: number; y: number; w: number; h: number }
export interface WorkspaceTab {
  id: string;
  view: View;
  pinned: boolean;
  /** Opened behind the current tab (Ctrl/middle click) and not visited yet. */
  background: boolean;
  /** Browser-style navigation history for this tab, oldest first. */
  history: View[];
  historyIndex: number;
}
export interface WorkspaceWindow {
  id: string;
  rect: Rect;
  z: number;
  tabs: WorkspaceTab[];
  activeTabId: string;
  kind: DockPreference;
}
export interface ClosedTab { tab: WorkspaceTab; windowId: string; index: number; rect: Rect; kind: DockPreference }
export interface WorkspaceSnapshot {
  version: 1;
  windows: WorkspaceWindow[];
  focusedWindowId: string | null;
}

let windows = $state<WorkspaceWindow[]>([]);
let focusedWindowId = $state<string | null>(null);
let closedTabStack = $state<ClosedTab[]>([]);
let serial = 0;
let saveTimer: ReturnType<typeof setTimeout> | null = null;
let persistenceEnabled = false;
let pointerTarget: OpenTarget = "reuse";
let pointerDisarmTimer: ReturnType<typeof setTimeout> | null = null;
let pointerRoutingInstalled = false;
const MAX_HISTORY = 50;
type RecentEntity = Extract<View, { kind: "country" | "province" }>;
let recentEntities = $state<RecentEntity[]>([]);

// Workspace state is deliberately JSON-only (it is persisted verbatim). JSON
// cloning also unwraps Svelte's deep reactive proxies, which structuredClone
// rejects in WebView2.
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
const nextId = (prefix: string) => `${prefix}-${Date.now().toString(36)}-${(++serial).toString(36)}`;
const focused = () => windows.find((w) => w.id === focusedWindowId) ?? null;

function makeTab(view: View, background = false): WorkspaceTab {
  const landed = clone(view);
  return { id: nextId("tab"), view: landed, pinned: false, background, history: [clone(landed)], historyIndex: 0 };
}

/** Repairs a tab restored from a snapshot written before history existed. */
function ensureHistory(tab: WorkspaceTab): View[] {
  if (!Array.isArray(tab.history) || !tab.history.length) {
    tab.history = [clone(tab.view)];
    tab.historyIndex = 0;
  }
  if (typeof tab.historyIndex !== "number" || tab.historyIndex < 0 || tab.historyIndex >= tab.history.length) {
    tab.historyIndex = tab.history.length - 1;
  }
  return tab.history;
}

/**
 * Points a tab at a view and records it the way a browser records a link click:
 * the forward stack is dropped, and the entry is pushed unless it is merely a
 * re-parameterization of what is already showing. A singleton's params are
 * focus tweaks (`{kind:"estates"}` ⇄ `{…, focusKey}`), not navigations, so they
 * amend the current entry instead of stacking up entries Back has to walk.
 */
function navigate(tab: WorkspaceTab, view: View): void {
  const next = clone(view);
  const def = viewDefinition(next);
  const history = ensureHistory(tab);
  const current = history[tab.historyIndex];
  tab.view = next;
  tab.background = false;
  if (current && current.kind === next.kind && (def.singleton || def.paramsEqual(current, next))) {
    history[tab.historyIndex] = clone(next);
    changed();
    return;
  }
  history.splice(tab.historyIndex + 1);
  history.push(clone(next));
  if (history.length > MAX_HISTORY) history.shift();
  tab.historyIndex = history.length - 1;
  changed();
}

export function canGoBack(tab: WorkspaceTab): boolean {
  return (tab.historyIndex ?? 0) > 0;
}
export function canGoForward(tab: WorkspaceTab): boolean {
  return (tab.historyIndex ?? 0) < (tab.history?.length ?? 1) - 1;
}

function stepHistory(tabId: string, delta: number): void {
  for (const win of windows) {
    const tab = win.tabs.find((t) => t.id === tabId);
    if (!tab) continue;
    const history = ensureHistory(tab);
    const at = tab.historyIndex + delta;
    if (at < 0 || at >= history.length) return;
    tab.historyIndex = at;
    tab.view = clone(history[at]);
    win.activeTabId = tab.id;
    focusWindow(win.id);
    changed();
    return;
  }
}

export function goBack(tabId: string): void { stepHistory(tabId, -1); }
export function goForward(tabId: string): void { stepHistory(tabId, 1); }

/**
 * Reuse/exact-match order: highest window first, and within a window the ACTIVE
 * tab before its siblings, so a plain link click lands on the tab the user is
 * actually looking at rather than an arbitrary same-kind tab behind it.
 */
function navigableTabs(): { w: WorkspaceWindow; tab: WorkspaceTab }[] {
  return [...windows]
    .sort((a, b) => b.z - a.z)
    .flatMap((w) => [
      ...w.tabs.filter((t) => t.id === w.activeTabId),
      ...w.tabs.filter((t) => t.id !== w.activeTabId),
    ].map((tab) => ({ w, tab })));
}

export function workspaceWindows(): WorkspaceWindow[] { return windows; }
export function workspaceFocusedWindowId(): string | null { return focusedWindowId; }
export function workspaceClosedTabs(): ClosedTab[] { return closedTabStack; }
export function workspaceRecentEntities(): RecentEntity[] { return recentEntities; }
export function hasFocusedWorkspaceWindow(): boolean { return focusedWindowId !== null; }

/** Browser-style target routing shared by every clickable view affordance. */
export function openTargetFromEvent(event?: Pick<MouseEvent, "button" | "ctrlKey" | "metaKey" | "shiftKey">): OpenTarget {
  if (!event) return "reuse";
  if (event.shiftKey) return "window";
  if (event.button === 1 || event.ctrlKey || event.metaKey) return "background-tab";
  return "reuse";
}

export function openViewFromEvent(view: View, event?: Pick<MouseEvent, "button" | "ctrlKey" | "metaKey" | "shiftKey">): WorkspaceTab {
  return openView(view, openTargetFromEvent(event));
}

function nextZ(): number {
  const top = Math.max(49, ...windows.map((w) => w.z));
  if (top < 98) return top + 1;
  [...windows].sort((a, b) => a.z - b.z).forEach((w, i) => (w.z = 50 + i));
  return Math.min(99, 50 + windows.length);
}

function viewport(): { w: number; h: number } {
  return typeof window === "undefined" ? { w: 1200, h: 800 } : { w: window.innerWidth, h: window.innerHeight };
}

/**
 * Height of the map's bottom toolbar, which publishes itself as a global CSS
 * variable while mounted. Docked windows must stop above it — see the matching
 * `bottom: calc(...)` in WorkspaceWindow, which is what actually holds the line
 * as the toolbar changes with the map mode; this only keeps the stored rect
 * honest for pop-out and min-size clamping.
 */
function bottomChrome(): number {
  if (typeof document === "undefined") return 0;
  const px = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--bottom-toolbar-h"));
  return Number.isFinite(px) ? px : 0;
}

function defaultRect(view: View, kind: DockPreference): Rect {
  const def = viewDefinition(view);
  const vp = viewport();
  const w = Math.min(def.defaultSize.w, Math.max(def.minSize.w, vp.w - 32));
  const h = Math.min(def.defaultSize.h, Math.max(def.minSize.h, vp.h - 64));
  const docked = Math.max(240, vp.h - 60 - bottomChrome());
  if (kind === "docked-right") return { x: Math.max(12, vp.w - w - 12), y: 48, w, h: docked };
  if (kind === "docked-left") return { x: 12, y: 48, w, h: docked };
  const cascade = windows.filter((x) => x.kind === "floating").length % 8;
  return { x: Math.max(12, Math.round((vp.w - w) / 2) + cascade * 22), y: Math.max(48, Math.round((vp.h - h) / 2) + cascade * 18), w, h };
}

function changed(): void {
  if (!persistenceEnabled || typeof window === "undefined") return;
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => void persistWorkspace(), 250);
}

export function focusWindow(id: string): void {
  const target = windows.find((w) => w.id === id);
  if (!target) return;
  focusedWindowId = id;
  target.z = nextZ();
  changed();
}

export function clearWorkspaceFocus(): void { focusedWindowId = null; }

function addWindow(view: View, kind = viewDefinition(view).dock, rect?: Rect, background = false): WorkspaceTab {
  const tab = makeTab(view, background);
  const win: WorkspaceWindow = {
    id: nextId("window"), rect: rect ? clone(rect) : defaultRect(view, kind), z: nextZ(),
    tabs: [tab], activeTabId: tab.id, kind,
  };
  windows.push(win);
  if (!background) focusedWindowId = win.id;
  changed();
  return tab;
}

export function openView(view: View, target: OpenTarget = "reuse"): WorkspaceTab {
  // A single capture listener makes modifier routing apply to legacy callbacks
  // that only pass entity parameters. Explicit Shift/Ctrl targets still win;
  // plain `reuse` is upgraded for the duration of the originating pointer turn.
  if (target === "reuse" && pointerTarget !== "reuse") target = pointerTarget;
  if (view.kind === "country" || view.kind === "province") {
    recentEntities = [clone(view) as RecentEntity, ...recentEntities.filter((x) => !viewDefinition(view).paramsEqual(x, view))].slice(0, 8);
  }
  const def = viewDefinition(view);
  if (def.singleton) {
    const existing = navigableTabs().find(({ tab }) => tab.view.kind === view.kind);
    if (existing) {
      navigate(existing.tab, view);
      existing.w.activeTabId = existing.tab.id;
      focusWindow(existing.w.id);
      return existing.tab;
    }
  }
  const exact = navigableTabs()
    .find(({ tab }) => tab.view.kind === view.kind && def.paramsEqual(tab.view, view));
  if (exact && target === "reuse") {
    navigate(exact.tab, view);
    exact.w.activeTabId = exact.tab.id;
    focusWindow(exact.w.id);
    return exact.tab;
  }
  if (target === "reuse" && !def.singleton) {
    const candidate = navigableTabs()
      .find(({ tab }) => tab.view.kind === view.kind && !tab.pinned && !tab.background);
    if (candidate) {
      navigate(candidate.tab, view);
      candidate.w.activeTabId = candidate.tab.id;
      focusWindow(candidate.w.id);
      return candidate.tab;
    }
  }
  if (target === "window" || target === "reuse" || !focused()) {
    return addWindow(view, target === "window" ? "floating" : def.dock);
  }
  const host = focused()!;
  const tab = makeTab(view, target === "background-tab");
  host.tabs.push(tab);
  if (target !== "background-tab") {
    host.activeTabId = tab.id;
    focusWindow(host.id);
  }
  changed();
  return tab;
}

/**
 * Navigates an existing tab in place — the browser-style "clicking a link in
 * this tab keeps you in this tab" rule the New-tab page needs. A singleton view
 * that is already open elsewhere wins (there can only be one), and the caller's
 * tab is left alone rather than duplicating it.
 */
export function replaceTabView(tabId: string, view: View): WorkspaceTab {
  const host = windows.find((w) => w.tabs.some((t) => t.id === tabId));
  const tab = host?.tabs.find((t) => t.id === tabId);
  if (!host || !tab) return openView(view, "reuse");
  const def = viewDefinition(view);
  if (def.singleton) {
    const existing = navigableTabs()
      .find(({ tab: t }) => t.id !== tabId && t.view.kind === view.kind);
    if (existing) {
      navigate(existing.tab, view);
      existing.w.activeTabId = existing.tab.id;
      focusWindow(existing.w.id);
      return existing.tab;
    }
  }
  if (view.kind === "country" || view.kind === "province") {
    recentEntities = [clone(view) as RecentEntity, ...recentEntities.filter((x) => !def.paramsEqual(x, view))].slice(0, 8);
  }
  navigate(tab, view);
  host.activeTabId = tab.id;
  focusWindow(host.id);
  return tab;
}

export function activateTab(windowId: string, tabId: string): void {
  const win = windows.find((w) => w.id === windowId);
  const tab = win?.tabs.find((t) => t.id === tabId);
  if (!win || !tab) return;
  tab.background = false;
  win.activeTabId = tabId;
  focusWindow(windowId);
}

export function cycleTabs(windowId = focusedWindowId, backwards = false): void {
  const win = windows.find((w) => w.id === windowId);
  if (!win || win.tabs.length < 2) return;
  const current = Math.max(0, win.tabs.findIndex((t) => t.id === win.activeTabId));
  const next = (current + (backwards ? -1 : 1) + win.tabs.length) % win.tabs.length;
  activateTab(win.id, win.tabs[next].id);
}

export function closeFocusedTab(): void {
  const win = focused();
  if (win) closeTab(win.activeTabId);
}

export function openNewTab(): void { openView({ kind: "new-tab" }, "tab"); }

export function setTabPinned(tabId: string, pinned: boolean): void {
  for (const win of windows) {
    const tab = win.tabs.find((t) => t.id === tabId);
    if (tab) { tab.pinned = pinned; changed(); return; }
  }
}

export function closeTab(tabId: string): void {
  const wi = windows.findIndex((w) => w.tabs.some((t) => t.id === tabId));
  if (wi < 0) return;
  const win = windows[wi];
  const ti = win.tabs.findIndex((t) => t.id === tabId);
  closedTabStack.push({ tab: clone(win.tabs[ti]), windowId: win.id, index: ti, rect: clone(win.rect), kind: win.kind });
  if (closedTabStack.length > 20) closedTabStack.shift();
  win.tabs.splice(ti, 1);
  if (!win.tabs.length) {
    windows.splice(wi, 1);
    if (focusedWindowId === win.id) focusedWindowId = windows.at(-1)?.id ?? null;
  } else if (win.activeTabId === tabId) {
    win.activeTabId = win.tabs[Math.min(ti, win.tabs.length - 1)].id;
  }
  changed();
}

export function closeWindow(id: string): void {
  const win = windows.find((w) => w.id === id);
  if (!win) return;
  for (const tab of [...win.tabs]) closeTab(tab.id);
}

export function moveTab(tabId: string, toWindowId: string, index: number): void {
  const from = windows.find((w) => w.tabs.some((t) => t.id === tabId));
  const to = windows.find((w) => w.id === toWindowId);
  const tab = from?.tabs.find((t) => t.id === tabId);
  if (!from || !to || !tab) return;
  from.tabs.splice(from.tabs.indexOf(tab), 1);
  if (!from.tabs.length) windows.splice(windows.indexOf(from), 1);
  else if (from.activeTabId === tabId) from.activeTabId = from.tabs[0].id;
  const at = Math.max(0, Math.min(index, to.tabs.length));
  to.tabs.splice(at, 0, tab);
  to.activeTabId = tab.id;
  focusWindow(to.id);
  changed();
}

export function reorderTab(windowId: string, tabId: string, index: number): void {
  const win = windows.find((w) => w.id === windowId);
  const from = win?.tabs.findIndex((t) => t.id === tabId) ?? -1;
  if (!win || from < 0) return;
  const [tab] = win.tabs.splice(from, 1);
  const at = Math.max(0, Math.min(index > from ? index - 1 : index, win.tabs.length));
  win.tabs.splice(at, 0, tab);
  win.activeTabId = tab.id;
  focusWindow(win.id);
  changed();
}

export function splitTabToWindow(tabId: string, rect: Rect): void {
  const from = windows.find((w) => w.tabs.some((t) => t.id === tabId));
  const tab = from?.tabs.find((t) => t.id === tabId);
  if (!from || !tab) return;
  from.tabs.splice(from.tabs.indexOf(tab), 1);
  if (!from.tabs.length) windows.splice(windows.indexOf(from), 1);
  else if (from.activeTabId === tabId) from.activeTabId = from.tabs[0].id;
  const win: WorkspaceWindow = { id: nextId("window"), rect: clone(rect), z: nextZ(), tabs: [tab], activeTabId: tab.id, kind: "floating" };
  windows.push(win); focusedWindowId = win.id; changed();
}

export function resizeWindow(id: string, rect: Rect): void {
  const win = windows.find((w) => w.id === id);
  if (!win) return;
  const active = win.tabs.find((t) => t.id === win.activeTabId) ?? win.tabs[0];
  const min = active ? viewDefinition(active.view).minSize : { w: 320, h: 240 };
  win.rect = { x: Math.round(rect.x), y: Math.round(rect.y), w: Math.max(min.w, Math.round(rect.w)), h: Math.max(min.h, Math.round(rect.h)) };
  changed();
}

export function setWindowKind(id: string, kind: DockPreference): void {
  const win = windows.find((w) => w.id === id);
  if (!win) return;
  win.kind = kind;
  if (kind !== "floating" && win.tabs[0]) win.rect = defaultRect(win.tabs[0].view, kind);
  changed();
}

export function reopenClosedTab(): void {
  const closed = closedTabStack.pop();
  if (!closed) return;
  const win = windows.find((w) => w.id === closed.windowId);
  if (win) {
    win.tabs.splice(Math.min(closed.index, win.tabs.length), 0, closed.tab);
    win.activeTabId = closed.tab.id;
    focusWindow(win.id);
  } else {
    const restored: WorkspaceWindow = { id: closed.windowId, rect: closed.rect, z: nextZ(), tabs: [closed.tab], activeTabId: closed.tab.id, kind: closed.kind };
    windows.push(restored); focusedWindowId = restored.id;
  }
  changed();
}

export function workspaceSnapshot(): WorkspaceSnapshot {
  return clone({ version: 1, windows, focusedWindowId });
}

export function restoreWorkspace(value: unknown, allow: (view: View) => boolean = () => true): void {
  if (!value || typeof value !== "object" || (value as WorkspaceSnapshot).version !== 1) return;
  const input = value as WorkspaceSnapshot;
  const restored: WorkspaceWindow[] = [];
  for (const candidate of input.windows ?? []) {
    const tabs = (candidate.tabs ?? [])
      .filter((t) => isView(t.view) && allow(t.view))
      .map((t) => {
        const tab = clone(t);
        tab.pinned = !!tab.pinned;
        tab.background = false;
        // A history entry may name an entity this session no longer has; the
        // same `allow` gate that drops whole tabs prunes the stack too. Pruning
        // shifts indices, so re-anchor on the view actually being shown.
        tab.history = (Array.isArray(tab.history) ? tab.history : []).filter((v) => isView(v) && allow(v));
        const at = tab.history.findIndex((v) => JSON.stringify(v) === JSON.stringify(tab.view));
        if (at >= 0) tab.historyIndex = at;
        else {
          tab.history.push(clone(tab.view));
          tab.historyIndex = tab.history.length - 1;
        }
        return tab;
      });
    if (!tabs.length) continue;
    const activeTabId = tabs.some((t) => t.id === candidate.activeTabId) ? candidate.activeTabId : tabs[0].id;
    restored.push({ ...clone(candidate), tabs, activeTabId, z: Math.max(50, Math.min(99, candidate.z)) });
  }
  windows = restored;
  focusedWindowId = restored.some((w) => w.id === input.focusedWindowId) ? input.focusedWindowId : restored.at(-1)?.id ?? null;
}

export async function persistWorkspace(): Promise<void> {
  await invoke("set_ui_layout", { value: JSON.stringify(workspaceSnapshot()) }).catch(() => {});
}

export async function initializeWorkspace(allow: (view: View) => boolean = () => true): Promise<void> {
  if (!pointerRoutingInstalled && typeof window !== "undefined") {
    pointerRoutingInstalled = true;
    // Arming on `pointerdown` alone was the bug behind "Ctrl/Shift do nothing":
    // its own 0ms disarm timer fired during the (human-length) gap before
    // mouseup, so every `onclick` link handler read a disarmed "reuse". Re-arm
    // in the CAPTURE phase of click/auxclick — that runs before any bubbling
    // handler, so the modifier is live for exactly the click that carried it.
    const arm = (event: MouseEvent) => {
      pointerTarget = openTargetFromEvent(event);
      if (pointerDisarmTimer) clearTimeout(pointerDisarmTimer);
      pointerDisarmTimer = setTimeout(() => { pointerTarget = "reuse"; }, 0);
    };
    window.addEventListener("pointerdown", arm, true);
    window.addEventListener("click", arm, true);
    window.addEventListener("auxclick", arm, true);
  }
  persistenceEnabled = false;
  try {
    const raw = await invoke<string | null>("get_ui_layout");
    if (raw) restoreWorkspace(JSON.parse(raw), allow);
  } catch { /* A corrupt or unavailable layout is equivalent to no layout. */ }
  persistenceEnabled = true;
}

export function resetWorkspaceForTests(): void {
  windows = []; focusedWindowId = null; closedTabStack = []; recentEntities = []; serial = 0;
  persistenceEnabled = false;
  pointerTarget = "reuse";
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = null;
  if (pointerDisarmTimer) clearTimeout(pointerDisarmTimer);
  pointerDisarmTimer = null;
}
