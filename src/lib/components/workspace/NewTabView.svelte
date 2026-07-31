<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getFlagUrl } from "$lib/flagCache";
  import { entitySource, hasEntityBrowser, matchesEntity, type EntityOption } from "$lib/entityCatalog";
  import { VIEW_GROUPS, VIEW_REGISTRY, type View, type ViewKind } from "$lib/views";
  import { openTargetFromEvent, openView, replaceTabView, workspaceRecentEntities } from "$lib/workspace.svelte";

  let { installPath, modPath, tabId, onopen }: {
    installPath: string;
    modPath: string | null;
    /** This page's own tab — a pick navigates it in place, browser-style. */
    tabId?: string;
    /** Lets the host switch map modes for views that need one (see entityCatalog). */
    onopen?: (view: View, open: () => void) => void;
  } = $props();

  let query = $state("");
  let countryNames = $state<Record<string, string>>({});
  let flags = $state<Record<string, string | null>>({});
  let normalized = $derived(query.trim().toLowerCase());

  // Drill-down state: null = the catalog, otherwise the entity list for a kind.
  let browsing = $state<ViewKind | null>(null);
  let browseQuery = $state("");
  let options = $state<EntityOption[]>([]);
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let source = $derived(browsing ? entitySource(browsing) : null);
  let matches = $derived(options.filter((o) => matchesEntity(o, browseQuery.trim())));

  $effect(() => {
    invoke<{ tag: string; name: string }[]>("list_countries", { installPath, modPath })
      .then((rows) => countryNames = Object.fromEntries(rows.map((r) => [r.tag, r.name])))
      .catch(() => {});
  });
  $effect(() => {
    for (const view of workspaceRecentEntities()) {
      if (view.kind !== "country" || view.tag in flags) continue;
      flags = { ...flags, [view.tag]: null };
      void getFlagUrl(installPath, modPath, view.tag).then((url) => flags = { ...flags, [view.tag]: url });
    }
  });

  /**
   * Clicking a view opens it *here* — this tab becomes that view, the way a
   * link does in a browser. Shift still forks it into its own window.
   */
  function open(view: View, event?: MouseEvent) {
    const target = openTargetFromEvent(event);
    const go = () => {
      if (target !== "reuse") openView(view, target);
      else if (tabId) replaceTabView(tabId, view);
      else openView(view, "reuse");
    };
    if (onopen) onopen(view, go);
    else go();
  }

  function launch(kind: ViewKind, event: MouseEvent) {
    if (hasEntityBrowser(kind)) {
      browsing = kind;
      browseQuery = "";
      void loadOptions(kind);
      return;
    }
    open({ kind } as View, event);
  }

  async function loadOptions(kind: ViewKind) {
    const loader = entitySource(kind);
    if (!loader) return;
    loading = true;
    loadError = null;
    options = [];
    try {
      const rows = await loader.load(installPath, modPath);
      // A slower load for a kind the user has since navigated away from must
      // not overwrite the list they are looking at now.
      if (browsing === kind) options = rows;
    } catch (e) {
      if (browsing === kind) loadError = String(e);
    } finally {
      if (browsing === kind) loading = false;
    }
  }
</script>

<div class="new-tab-page">
  {#if browsing && source}
    <button class="back" onclick={() => (browsing = null)}>← All views</button>
    <h2>{VIEW_REGISTRY[browsing].label}</h2>
    <input type="search" placeholder={source.searchLabel} bind:value={browseQuery} />
    {#if loading}
      <p class="hint">Loading…</p>
    {:else if loadError}
      <p class="hint error">Could not load the list: {loadError}</p>
    {:else if !matches.length}
      <p class="hint">Nothing matches “{browseQuery}”.</p>
    {:else}
      <div class="entity-list">
        {#each matches.slice(0, 400) as option (option.id)}
          <button class="entity" onclick={(event) => open(option.view, event)}>
            <span class="label">{option.label}</span>
            {#if option.hint}<span class="hint-tag">{option.hint}</span>{/if}
          </button>
        {/each}
      </div>
      {#if matches.length > 400}
        <p class="hint">{matches.length - 400} more — keep typing to narrow the list.</p>
      {/if}
    {/if}
  {:else}
    <h2>Open a view</h2>
    <input type="search" placeholder="Search tools and editors…" bind:value={query} />
    {#if workspaceRecentEntities().length && !normalized}
      <section><h3>Recent entities</h3><div class="catalog recent">
        {#each workspaceRecentEntities() as view}
          <button class="recent-item" onclick={(event) => open(view, event)}>
            {#if view.kind === "country"}
              <span class="flag">{#if flags[view.tag]}<img src={flags[view.tag]!} alt="" />{:else}{view.tag.slice(0, 1)}{/if}</span>
              <span>{countryNames[view.tag] ?? view.tag}<small>{view.tag}</small></span>
            {:else}
              <span class="province-mark">◉</span><span>Province #{view.id}</span>
            {/if}
          </button>
        {/each}
      </div></section>
    {/if}
    {#each VIEW_GROUPS as group}
      {@const kinds = group.kinds.filter((kind) => !normalized || VIEW_REGISTRY[kind].label.toLowerCase().includes(normalized))}
      {@const extras = (group.extras ?? []).filter((x) => !normalized || x.label.toLowerCase().includes(normalized))}
      {#if kinds.length || extras.length}
        <section><h3>{group.label}</h3><div class="catalog">
          {#each kinds as kind}
            <button onclick={(event) => launch(kind, event)}>
              {VIEW_REGISTRY[kind].label}{#if hasEntityBrowser(kind)}<span class="chevron">›</span>{/if}
            </button>
          {/each}
          {#each extras as extra}
            <button onclick={(event) => open(extra.view, event)}>{extra.label}</button>
          {/each}
        </div></section>
      {/if}
    {/each}
    <p class="hint">Shift-click to open in a new window. Ctrl-click opens a background tab.</p>
  {/if}
</div>

<style>
  .new-tab-page { max-width: 700px; margin: 0 auto; padding: var(--sp-6); }
  h2 { margin: 0 0 var(--sp-4); font-size: var(--fs-xl); }
  h3 { margin: var(--sp-5) 0 var(--sp-2); color: var(--text-2); font-size: var(--fs-sm); text-transform: uppercase; letter-spacing: .06em; }
  input { width: 100%; padding: var(--sp-3); }
  .catalog { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: var(--sp-2); }
  button { text-align: left; padding: var(--sp-3); }
  .chevron { float: right; color: var(--text-3); }
  .back { padding: var(--sp-1) 0; margin-bottom: var(--sp-2); border: 0; background: none; color: var(--text-2); cursor: pointer; }
  .back:hover { color: var(--text-1); }
  .entity-list { display: flex; flex-direction: column; gap: 2px; margin-top: var(--sp-3); }
  .entity { display: flex; align-items: baseline; justify-content: space-between; gap: var(--sp-3); padding: var(--sp-2) var(--sp-3); }
  .entity .label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hint-tag { flex: none; color: var(--text-3); font-size: var(--fs-xs); }
  .recent-item { display: flex; align-items: center; gap: var(--sp-2); }
  .recent-item small { display: block; color: var(--text-3); font-size: var(--fs-xs); }
  .flag, .province-mark { width: 24px; height: 24px; display: grid; place-items: center; flex: none; overflow: hidden; border-radius: var(--r-1); background: var(--bg-1); color: var(--text-3); }
  .flag img { width: 100%; height: 100%; object-fit: contain; }
  .hint { color: var(--text-3); }
  .hint.error { color: var(--err); }
</style>
