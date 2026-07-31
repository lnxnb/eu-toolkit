<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let { installPath, modPath, name, size = 24, label = "" }: {
    installPath: string;
    modPath: string | null;
    name: string;
    size?: number;
    label?: string;
  } = $props();

  let url = $state<string | null>(null);
  $effect(() => {
    let alive = true;
    let owned: string | null = null;
    url = null;
    invoke<ArrayBuffer>("get_sprite", { installPath, modPath, name })
      .then((buf) => {
        owned = URL.createObjectURL(new Blob([buf], { type: "image/png" }));
        if (alive) url = owned;
        else URL.revokeObjectURL(owned);
      })
      .catch(() => {});
    return () => {
      alive = false;
      if (owned) URL.revokeObjectURL(owned);
    };
  });
</script>

<span class="sprite" style:width={`${size}px`} style:height={`${size}px`} role="img" aria-label={label}>
  {#if url}<img src={url} alt="" />{:else}<span aria-hidden="true">{label.trim().charAt(0).toUpperCase() || "?"}</span>{/if}
</span>

<style>
  .sprite { display: inline-flex; flex: none; align-items: center; justify-content: center; overflow: hidden; border-radius: var(--r-1); background: var(--bg-1); color: var(--text-3); font-size: var(--fs-xs); }
  img { width: 100%; height: 100%; object-fit: contain; }
</style>
