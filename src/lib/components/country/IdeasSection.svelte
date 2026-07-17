<!--
  IdeasSection — the national ideas editor (Sprint 1.2). For a country with a
  unique `TAG_ideas` set: shows localized set/tradition/ambition/idea names +
  descriptions, renames them via loc overrides, and edits the traditions, each of
  the 7 ideas, and the ambition through the shared ModifierEditor (whole-block
  setBlock into the ideas file — unmodeled/raw modifier keys are preserved).

  For a country using shared/group ideas (no unique set), it offers "Create unique
  national ideas": a full `TAG_ideas { … }` scaffold appended to a project-owned
  common/ideas file plus loc entries (queued; save & reopen to edit).
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { KnownModifier, ModifierRow } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import IdeaBlockEditor from "./IdeaBlockEditor.svelte";
  import type { CountryDetails } from "./types";

  let {
    installPath,
    modPath,
    tag,
    queue,
    details,
  }: {
    installPath: string;
    modPath: string | null;
    tag: string;
    queue: EditQueue;
    details: CountryDetails;
  } = $props();

  const ideas = $derived(details.ideas);

  let known = $state<KnownModifier[]>([]);
  $effect(() => {
    invoke<KnownModifier[]>("get_known_modifiers").then((k) => (known = k)).catch(() => {});
  });

  function blockValue(rows: ModifierRow[]): string {
    return rows.map((r) => `${r.key} = ${r.value}`).join(" ");
  }
  function commitBlock(path: string[], rows: ModifierRow[], label: string) {
    if (!ideas) return;
    queue.push({
      label,
      edits: [{ kind: "setBlock", file: ideas.source_file, path, value: blockValue(rows) }],
      coalesceKey: `ideas:${tag}:${path.join(".")}`,
    });
  }

  // Loc renames (name + description) via loc overrides.
  function locName(key: string): string | undefined {
    return queue.pendingLocOverride(key);
  }
  function commitLoc(key: string, value: string, label: string) {
    queue.push({ label, edits: [{ kind: "locOverride", key, value }], coalesceKey: `loc:${key}` });
  }

  // --- Create unique ideas scaffold (group-ideas countries) ---
  const SCAFFOLD_FILE = "common/ideas/zz_eutoolkit_ideas.txt";
  let creating = $state(false);
  function createUniqueIdeas() {
    const set = `${tag}_ideas`;
    const ideaKeys = Array.from({ length: 7 }, (_, i) => `${tag.toLowerCase()}_idea_${i + 1}`);
    const body =
      `${set} = {\n` +
      `\tstart = {\n\t\tland_morale = 0.1\n\t}\n` +
      `\tbonus = {\n\t\tdiscipline = 0.05\n\t}\n` +
      `\ttrigger = {\n\t\ttag = ${tag}\n\t}\n` +
      `\tfree = yes\n` +
      ideaKeys.map((k) => `\t${k} = {\n\t\tglobal_tax_modifier = 0.05\n\t}`).join("\n") +
      `\n}\n`;
    const edits: TypedEdit[] = [
      { kind: "appendText", file: SCAFFOLD_FILE, text: body },
      { kind: "locOverride", key: set, value: `${details.localized_name} Ideas` },
      { kind: "locOverride", key: `${set}_start`, value: `${details.localized_name} Traditions` },
      { kind: "locOverride", key: `${set}_bonus`, value: `${details.localized_name} Ambition` },
      ...ideaKeys.map((k, i): TypedEdit => ({ kind: "locOverride", key: k, value: `${details.localized_name} Idea ${i + 1}` })),
    ];
    queue.push({ label: `Create unique national ideas for ${tag}`, edits });
    creating = true;
  }
</script>

<section>
  <h3>National Ideas</h3>

  {#if ideas}
    <!-- Set name (loc override) -->
    <div class="field">
      <span class="lbl">Idea Set Name</span>
      <input
        class="text"
        value={locName(ideas.name) ?? ideas.localized_name}
        onchange={(e) => commitLoc(ideas.name, e.currentTarget.value, `Rename idea set of ${tag}`)}
      />
    </div>

    <!-- Traditions -->
    <div class="idea">
      <div class="field">
        <span class="lbl">Traditions</span>
        <input
          class="text"
          value={locName(`${ideas.name}_start`) ?? ideas.traditions_name}
          onchange={(e) => commitLoc(`${ideas.name}_start`, e.currentTarget.value, `Rename traditions of ${tag}`)}
        />
      </div>
      <IdeaBlockEditor
        base={ideas.traditions}
        {known}
        oncommit={(rows) => commitBlock([ideas.name, "start"], rows, `Edit traditions of ${tag}`)}
      />
    </div>

    <!-- The 7 ideas -->
    {#each ideas.ideas as idea, i (idea.name)}
      <div class="idea">
        <div class="field">
          <span class="lbl">{i + 1}. Idea</span>
          <input
            class="text"
            value={locName(idea.name) ?? idea.localized_name}
            onchange={(e) => commitLoc(idea.name, e.currentTarget.value, `Rename idea ${idea.name}`)}
          />
        </div>
        <div class="field">
          <span class="lbl">Description</span>
          <textarea
            class="text desc"
            value={locName(`${idea.name}_desc`) ?? idea.localized_desc}
            onchange={(e) => commitLoc(`${idea.name}_desc`, e.currentTarget.value, `Edit idea description`)}
          ></textarea>
        </div>
        <IdeaBlockEditor
          base={idea.effects}
          {known}
          oncommit={(rows) => commitBlock([ideas.name, idea.name], rows, `Edit idea ${idea.name}`)}
        />
      </div>
    {/each}

    <!-- Ambition -->
    <div class="idea">
      <div class="field">
        <span class="lbl">Ambition</span>
        <input
          class="text"
          value={locName(`${ideas.name}_bonus`) ?? ideas.ambition_name}
          onchange={(e) => commitLoc(`${ideas.name}_bonus`, e.currentTarget.value, `Rename ambition of ${tag}`)}
        />
      </div>
      <IdeaBlockEditor
        base={ideas.ambition}
        {known}
        oncommit={(rows) => commitBlock([ideas.name, "bonus"], rows, `Edit ambition of ${tag}`)}
      />
    </div>
  {:else if creating}
    <p class="ok">Unique national ideas queued. Save and reopen the country to edit them.</p>
  {:else}
    <p class="dim">This country uses shared / group ideas (no unique set).</p>
    <button class="btn" onclick={createUniqueIdeas}>Create unique national ideas…</button>
  {/if}
</section>

<style>
  section {
    margin-bottom: 1rem;
  }

  h3 {
    margin: 0 0 0.5rem;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #9ca3af;
  }

  .idea {
    margin-bottom: 0.7rem;
    padding-bottom: 0.6rem;
    border-bottom: 1px solid #222831;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .lbl {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: #8a919c;
  }

  .text {
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.25rem 0.4rem;
    outline: none;
  }

  .desc {
    resize: vertical;
    min-height: 2.5rem;
    font-size: 0.78rem;
  }

  .dim {
    color: #8a919c;
    font-size: 0.83rem;
    margin: 0 0 0.4rem;
  }

  .ok {
    color: #86c58a;
    font-size: 0.83rem;
  }

  .btn {
    border: 1px solid #4b5563;
    background: transparent;
    color: inherit;
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.28rem 0.6rem;
    cursor: pointer;
  }

  .btn:hover {
    border-color: #9ca3af;
    background: #4a6da7;
    color: #fff;
  }
</style>
