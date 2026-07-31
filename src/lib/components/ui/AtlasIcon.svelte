<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  let { installPath, modPath, kind, frame = 0, size = 28, label = "" }: { installPath: string; modPath: string | null; kind: string; frame?: number; size?: number; label?: string } = $props();
  let url = $state(""); let count = $state(1);
  $effect(() => {
    let alive = true; let owned = "";
    invoke<ArrayBuffer>("get_icon_atlas", { installPath, modPath, kind }).then((wire) => {
      const bytes = new Uint8Array(wire); const n = new DataView(bytes.buffer).getUint32(0, true);
      const h = JSON.parse(new TextDecoder().decode(bytes.slice(4, 4 + n)));
      owned = URL.createObjectURL(new Blob([bytes.slice(4 + n)], {type:"image/png"}));
      if (alive) { url = owned; count = h.count; } else URL.revokeObjectURL(owned);
    }).catch(() => {});
    return () => { alive = false; if (owned) URL.revokeObjectURL(owned); };
  });
  let safeFrame = $derived(Math.max(0, Math.min(frame, count - 1)));
  let style = $derived(`width:${size}px;height:${size}px;background-image:url(${url});background-size:${count * size}px ${size}px;background-position:${-safeFrame * size}px 0`);
</script>
<span class="atlas-icon" style={style} role="img" aria-label={label}></span>
<style>.atlas-icon{display:inline-block;flex:none;background-repeat:no-repeat;background-color:var(--bg-3);border-radius:var(--r-1)}</style>
