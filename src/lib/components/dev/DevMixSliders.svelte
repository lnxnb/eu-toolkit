<!--
  DevMixSliders — the development paint mix (Sprint 9.2). Three vertical sliders
  (base tax / production / manpower) normalized to sum 1 via the shared SliderGroup
  (locks + proportional redistribution live in sliderMath). Each slider has its dev
  icon underneath (from the `development` atlas; a colored-letter glyph fallback when
  the atlas failed to load). The normalized mix drives how painted dev is split
  across the three components (both raise and lower); it persists per session in
  MapView (sessionStorage), mirroring the brush size.
-->
<script lang="ts">
  import { SliderGroup } from "$lib/components/ui";
  import type { Atlas } from "$lib/overlay";
  import type { DevMix } from "$lib/devpaint";
  import { DEV_KEYS } from "$lib/devpaint";

  let {
    mix = $bindable([1 / 3, 1 / 3, 1 / 3] as DevMix),
    locks = $bindable([false, false, false]),
    atlas = null,
    atlasIndex = new Map<string, number>(),
    onchange,
  }: {
    mix?: DevMix;
    locks?: boolean[];
    atlas?: Atlas | null;
    atlasIndex?: Map<string, number>;
    onchange?: (mix: DevMix) => void;
  } = $props();

  // Glyph fallback (letter + color) when the dev icon atlas is unavailable —
  // matches the province panel's Tax/Prod/Man accents.
  const GLYPH = [
    { letter: "T", color: "#7fbf6f" },
    { letter: "P", color: "#7f9fd0" },
    { letter: "M", color: "#d07f7f" },
  ];

  let icons: (HTMLCanvasElement | null)[] = [null, null, null];

  // Draw each component's atlas frame into its small canvas (device-pixel aware).
  $effect(() => {
    if (!atlas) return;
    for (let k = 0; k < 3; k++) {
      const cv = icons[k];
      if (!cv) continue;
      const dpr = window.devicePixelRatio || 1;
      const css = 22;
      cv.width = Math.round(css * dpr);
      cv.height = Math.round(css * dpr);
      const ctx = cv.getContext("2d");
      if (!ctx) continue;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, css, css);
      const fi = atlasIndex.get(DEV_KEYS[k]) ?? k;
      const f = Math.max(0, Math.min(atlas.count - 1, fi));
      ctx.drawImage(atlas.image, f * atlas.frameW, 0, atlas.frameW, atlas.frameH, 0, 0, css, css);
    }
  });

  function fmt(v: number): string {
    return `${Math.round(v * 100)}%`;
  }
  function handle(next: number[]) {
    onchange?.(next as DevMix);
  }
</script>

<div class="dev-mix">
  <h4>Dev mix</h4>
  <SliderGroup
    bind:values={mix}
    bind:locks
    labels={["", "", ""]}
    orientation="vertical"
    total={1}
    format={fmt}
    onchange={handle}
  />
  <div class="icons">
    {#each [0, 1, 2] as k (k)}
      <div class="icon-cell" title={DEV_KEYS[k]}>
        {#if atlas}
          <canvas bind:this={icons[k]} style="width:22px;height:22px;"></canvas>
        {:else}
          <span class="glyph" style="color:{GLYPH[k].color}">{GLYPH[k].letter}</span>
        {/if}
      </div>
    {/each}
  </div>
</div>

<style>
  .dev-mix {
    position: absolute;
    top: 3rem;
    right: 0.75rem;
    z-index: 9; /* map-anchored chrome: above overlay canvases (5) */
    width: 11rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    padding: 0.6rem 0.7rem 0.8rem;
    border-radius: 10px;
    background: rgba(20, 24, 29, 0.85);
    color: #e5e7eb;
    backdrop-filter: blur(4px);
  }

  h4 {
    margin: 0;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #9ca3af;
  }

  /* Spread the three slider columns evenly so the icon row lines up under them. */
  .dev-mix :global(.slider-group.vertical) {
    justify-content: space-around;
    width: 100%;
  }

  .icons {
    display: flex;
    justify-content: space-around;
    align-items: center;
  }

  .icon-cell {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.8rem;
    height: 1.6rem;
  }

  .glyph {
    font-weight: 800;
    font-size: 1rem;
    line-height: 1;
  }
</style>
