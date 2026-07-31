<script lang="ts">
  const groups = [
    ["Files & edits", [["Ctrl+S", "Save changes"], ["Ctrl+Z / Ctrl+Y", "Undo / redo"], ["Ctrl+Shift+F", "Find in project"]]],
    ["Windows & tabs", [["Ctrl+T", "New tab catalog"], ["Ctrl+W", "Close active tab"], ["Ctrl+Shift+T", "Reopen closed tab"], ["Ctrl+Tab", "Next tab"], ["Ctrl+Shift+Tab", "Previous tab"], ["Esc", "Close focused view or cancel map tool"]]],
    ["Map editing", [["[ / ]", "Decrease / increase brush size"], ["Ctrl+click or middle-click", "Open pinned background tab"], ["Shift+click", "Open in a new floating window"]]],
  ] as const;
</script>
<div class="shortcuts">
  <h2>Keyboard shortcuts</h2>
  {#each groups as group}<section><h3>{group[0]}</h3><dl>{#each group[1] as row}<dt><kbd>{row[0]}</kbd></dt><dd>{row[1]}</dd>{/each}</dl></section>{/each}
</div>
<style>
  .shortcuts { max-width: 620px; padding: var(--sp-4); }
  h2 { margin-top: 0; } h3 { margin: var(--sp-5) 0 var(--sp-2); color: var(--text-2); }
  dl { display: grid; grid-template-columns: minmax(180px, auto) 1fr; gap: var(--sp-2) var(--sp-5); margin: 0; }
  dt, dd { margin: 0; } dd { color: var(--text-2); }
  kbd { padding: 2px 6px; border: 1px solid var(--border); border-bottom-color: var(--border-strong); border-radius: var(--r-1); background: var(--bg-3); }
</style>
