<!--
  ModifierIcon — the game's own icon for a modifier key (land_morale, discipline …).

  EU4 has no script-visible modifier-key → sprite table: `GFX_modifier_icons` is an
  18-frame positional strip whose frame choice is compiled into the engine, and only
  a partial, inconsistently-named subset of modifiers is registered as a `GFX_icon_*`
  spriteType. The reliable mapping is the filename convention in
  `gfx/interface/ideas_EU4/<modifier_key>.dds`, which the backend resolves through
  the Vfs (so a mod's own art shadows the base file, and mod-only modifiers work).

  A key with no art renders as nothing — the caller shows its label. That is the
  honest outcome: better no icon than a wrong one.

  Icons are fetched once per (session, key) and shared through a module-level
  cache, so a table with the same modifier on twenty levels makes one IPC call.
-->
<script lang="ts" module>
  import { invoke } from "@tauri-apps/api/core";

  // key: `${modPath ?? ""}|${modifierKey}` → object URL, or null for "no art".
  const cache = new Map<string, Promise<string | null>>();

  function load(installPath: string, modPath: string | null, key: string): Promise<string | null> {
    const id = `${modPath ?? ""}|${key}`;
    let hit = cache.get(id);
    if (!hit) {
      hit = invoke<ArrayBuffer>("get_modifier_icon", { installPath, modPath, key })
        .then((buf) => URL.createObjectURL(new Blob([buf], { type: "image/png" })))
        .catch(() => null);
      cache.set(id, hit);
    }
    return hit;
  }
</script>

<script lang="ts">
  let {
    installPath,
    modPath = null,
    key,
    size = "1.15rem",
  }: {
    installPath: string;
    modPath?: string | null;
    /** The modifier key, e.g. `land_morale`. */
    key: string;
    size?: string;
  } = $props();

  let url = $state<string | null>(null);

  $effect(() => {
    const want = key;
    let live = true;
    // Cached URLs are shared and intentionally not revoked: they live as long as
    // the session, and revoking here would blank every other row using the key.
    load(installPath, modPath, want).then((u) => {
      if (live) url = u;
    });
    return () => {
      live = false;
    };
  });
</script>

{#if url}
  <img class="micon" src={url} alt="" style="width: {size}; height: {size};" />
{/if}

<style>
  .micon {
    object-fit: contain;
    flex: none;
    vertical-align: middle;
  }
</style>
