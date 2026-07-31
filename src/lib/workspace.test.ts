import { beforeEach, describe, expect, it } from "vitest";
import {
  activateTab,
  canGoBack,
  canGoForward,
  closeTab,
  cycleTabs,
  goBack,
  goForward,
  moveTab,
  openTargetFromEvent,
  openView,
  reopenClosedTab,
  reorderTab,
  replaceTabView,
  resetWorkspaceForTests,
  restoreWorkspace,
  setTabPinned,
  workspaceSnapshot,
  workspaceWindows,
} from "./workspace.svelte";

describe("workspace", () => {
  beforeEach(resetWorkspaceForTests);

  it("opens singleton views once and updates their parameters", () => {
    openView({ kind: "estates", focusKey: "estate_nobles" }, "window");
    openView({ kind: "estates", focusKey: "estate_church" }, "window");
    expect(workspaceWindows()).toHaveLength(1);
    expect(workspaceWindows()[0].tabs).toHaveLength(1);
    expect(workspaceWindows()[0].tabs[0].view).toEqual({ kind: "estates", focusKey: "estate_church" });
  });

  it("retargets the most recent reusable entity tab but preserves pinned tabs", () => {
    const first = openView({ kind: "province", id: 1 }, "reuse");
    openView({ kind: "province", id: 2 }, "reuse");
    expect(workspaceWindows()[0].tabs[0].view).toEqual({ kind: "province", id: 2 });

    setTabPinned(first.id, true);
    openView({ kind: "province", id: 3 }, "reuse");
    expect(workspaceWindows()).toHaveLength(2);
    expect(workspaceWindows().flatMap((w) => w.tabs).map((t) => t.view)).toContainEqual({ kind: "province", id: 3 });
  });

  it("navigates a tab in place instead of spawning a window", () => {
    const blank = openView({ kind: "new-tab" }, "window");
    replaceTabView(blank.id, { kind: "country", tag: "SWE" });
    expect(workspaceWindows()).toHaveLength(1);
    expect(workspaceWindows()[0].tabs).toHaveLength(1);
    expect(workspaceWindows()[0].tabs[0].id).toBe(blank.id);
    expect(workspaceWindows()[0].tabs[0].view).toEqual({ kind: "country", tag: "SWE" });
  });

  it("defers to an already-open singleton rather than duplicating it in place", () => {
    const estates = openView({ kind: "estates" }, "window");
    const blank = openView({ kind: "new-tab" }, "window");
    const landed = replaceTabView(blank.id, { kind: "estates", focusKey: "estate_nobles" });
    expect(landed.id).toBe(estates.id);
    // The picker's own tab is left as it was — nothing was duplicated.
    expect(workspaceWindows().flatMap((w) => w.tabs).filter((t) => t.view.kind === "estates")).toHaveLength(1);
    expect(workspaceWindows().flatMap((w) => w.tabs).find((t) => t.id === blank.id)?.view.kind).toBe("new-tab");
  });

  it("opens foreground and background tabs in the focused window", () => {
    openView({ kind: "country", tag: "SWE" }, "window");
    const background = openView({ kind: "province", id: 1 }, "background-tab");
    const win = workspaceWindows()[0];
    expect(win.tabs).toHaveLength(2);
    // Background is its own flag: a Ctrl-clicked tab is closable and unpinned.
    expect(background.background).toBe(true);
    expect(background.pinned).toBe(false);
    expect(win.activeTabId).not.toBe(background.id);
    activateTab(win.id, background.id);
    expect(win.activeTabId).toBe(background.id);
    expect(background.background).toBe(false);
  });

  it("keeps an unvisited background tab out of plain-click reuse", () => {
    openView({ kind: "country", tag: "SWE" }, "window");
    const parked = openView({ kind: "country", tag: "FRA" }, "background-tab");
    openView({ kind: "country", tag: "ENG" }, "reuse");
    expect(parked.view).toEqual({ kind: "country", tag: "FRA" });
    expect(workspaceWindows()[0].tabs[0].view).toEqual({ kind: "country", tag: "ENG" });
  });

  it("walks a tab's navigation history back and forward", () => {
    const tab = openView({ kind: "country", tag: "SWE" }, "window");
    openView({ kind: "country", tag: "FRA" }, "reuse");
    openView({ kind: "country", tag: "ENG" }, "reuse");
    expect(canGoBack(tab)).toBe(true);
    expect(canGoForward(tab)).toBe(false);

    goBack(tab.id);
    expect(tab.view).toEqual({ kind: "country", tag: "FRA" });
    goBack(tab.id);
    expect(tab.view).toEqual({ kind: "country", tag: "SWE" });
    expect(canGoBack(tab)).toBe(false);

    goForward(tab.id);
    expect(tab.view).toEqual({ kind: "country", tag: "FRA" });

    // Navigating after Back drops the forward stack, exactly like a browser.
    openView({ kind: "country", tag: "CAS" }, "reuse");
    expect(canGoForward(tab)).toBe(false);
    goBack(tab.id);
    expect(tab.view).toEqual({ kind: "country", tag: "FRA" });
  });

  it("treats singleton re-parameterization as a focus tweak, not a navigation", () => {
    const tab = openView({ kind: "estates", focusKey: "estate_nobles" }, "window");
    openView({ kind: "estates" }, "reuse");
    openView({ kind: "estates", focusKey: "estate_church" }, "reuse");
    expect(tab.view).toEqual({ kind: "estates", focusKey: "estate_church" });
    expect(canGoBack(tab)).toBe(false);
  });

  it("re-anchors restored history when a view is no longer allowed", () => {
    const tab = openView({ kind: "country", tag: "SWE" }, "window");
    openView({ kind: "estates" }, "reuse");
    replaceTabView(tab.id, { kind: "country", tag: "FRA" });
    const snapshot = workspaceSnapshot();
    resetWorkspaceForTests();
    restoreWorkspace(snapshot, (view) => view.kind === "country");
    const restored = workspaceWindows().flatMap((w) => w.tabs).find((t) => t.view.kind === "country")!;
    expect(restored.history.every((v) => v.kind === "country")).toBe(true);
    expect(restored.history[restored.historyIndex]).toEqual(restored.view);
  });

  it("moves tabs between windows and removes an empty donor", () => {
    const country = openView({ kind: "country", tag: "FRA" }, "window");
    openView({ kind: "province", id: 12 }, "window");
    const target = workspaceWindows()[1];
    moveTab(country.id, target.id, 0);
    expect(workspaceWindows()).toHaveLength(1);
    expect(workspaceWindows()[0].tabs.map((t) => t.id)).toEqual([country.id, expect.any(String)]);
  });

  it("closes and reopens a tab in its original window", () => {
    const country = openView({ kind: "country", tag: "ENG" }, "window");
    const winId = workspaceWindows()[0].id;
    closeTab(country.id);
    expect(workspaceWindows()).toHaveLength(0);
    reopenClosedTab();
    expect(workspaceWindows()[0].id).toBe(winId);
    expect(workspaceWindows()[0].tabs[0].view).toEqual({ kind: "country", tag: "ENG" });
  });

  it("round-trips persistence and drops disallowed views", () => {
    openView({ kind: "country", tag: "SWE" }, "window");
    openView({ kind: "estates" }, "window");
    const snapshot = workspaceSnapshot();
    resetWorkspaceForTests();
    restoreWorkspace(snapshot, (view) => view.kind === "estates");
    expect(workspaceWindows()).toHaveLength(1);
    expect(workspaceWindows()[0].tabs[0].view.kind).toBe("estates");
  });

  it("routes browser-style pointer modifiers at one boundary", () => {
    expect(openTargetFromEvent()).toBe("reuse");
    expect(openTargetFromEvent({ button: 0, ctrlKey: true, metaKey: false, shiftKey: false })).toBe("background-tab");
    expect(openTargetFromEvent({ button: 1, ctrlKey: false, metaKey: false, shiftKey: false })).toBe("background-tab");
    expect(openTargetFromEvent({ button: 0, ctrlKey: false, metaKey: false, shiftKey: true })).toBe("window");
  });

  it("cycles and reorders tabs without changing their views", () => {
    openView({ kind: "country", tag: "FRA" }, "window");
    const second = openView({ kind: "province", id: 12 }, "tab");
    const third = openView({ kind: "province", id: 13 }, "tab");
    const win = workspaceWindows()[0];
    cycleTabs(win.id);
    expect(win.activeTabId).not.toBe(third.id);
    reorderTab(win.id, third.id, 0);
    expect(win.tabs[0].id).toBe(third.id);
    expect(win.tabs.some((t) => t.id === second.id)).toBe(true);
  });

  it("persists deep-linked content tabs", () => {
    openView({ kind: "country", tag: "SWE", tab: "rulers" }, "window");
    const snapshot = workspaceSnapshot();
    resetWorkspaceForTests();
    restoreWorkspace(snapshot);
    expect(workspaceWindows()[0].tabs[0].view).toEqual({ kind: "country", tag: "SWE", tab: "rulers" });
  });
});
