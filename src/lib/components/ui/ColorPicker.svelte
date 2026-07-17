<!--
  ColorPicker — a compact, dependency-free RGB picker. A swatch button opens a
  popover with R/G/B sliders + a hex field; returns {r,g,b} 0-255. Consumers: country
  color and revolutionary colors (1.5), religion/trade-good/node colors (5.2/7.3/8.2).

  Two modes:
    • single  — pass `value` (bindable RGB); `onchange(rgb)` fires on edit.
    • 3-color — pass `values` (bindable RGB[]); the swatch row shows every color and
                one shared popover edits the clicked index; `onchangeall(rgb[])` fires.
  Revolutionary colors use the 3-color mode.

  z-index: popover at 20 (above the panel surface it lives in, below modals).
-->
<script lang="ts">
  import type { RGB } from "./types";

  let {
    value = $bindable({ r: 128, g: 128, b: 128 }),
    values = $bindable(undefined),
    label = "",
    onchange,
    onchangeall,
  }: {
    value?: RGB;
    /** When provided, switches to N-swatch mode (revolutionary = 3). */
    values?: RGB[] | undefined;
    label?: string;
    onchange?: (c: RGB) => void;
    onchangeall?: (cs: RGB[]) => void;
  } = $props();

  const multi = $derived(values !== undefined);

  let open = $state(false);
  let activeIndex = $state(0);

  // The color currently under edit, whichever mode we're in.
  let current = $derived<RGB>(
    multi && values ? (values[activeIndex] ?? { r: 0, g: 0, b: 0 }) : value,
  );

  let hex = $state("");
  // Keep the hex field in sync when the active color changes externally.
  $effect(() => {
    hex = toHex(current);
  });

  function clamp(n: number): number {
    return Math.max(0, Math.min(255, Math.round(n)));
  }

  function toHex(c: RGB): string {
    const h = (n: number) => clamp(n).toString(16).padStart(2, "0");
    return `#${h(c.r)}${h(c.g)}${h(c.b)}`;
  }

  function css(c: RGB): string {
    return `rgb(${clamp(c.r)}, ${clamp(c.g)}, ${clamp(c.b)})`;
  }

  function commit(next: RGB) {
    const c = { r: clamp(next.r), g: clamp(next.g), b: clamp(next.b) };
    if (multi && values) {
      const arr = values.slice();
      arr[activeIndex] = c;
      values = arr;
      onchangeall?.(arr);
    } else {
      value = c;
      onchange?.(c);
    }
  }

  function setChannel(ch: "r" | "g" | "b", raw: string) {
    commit({ ...current, [ch]: Number(raw) });
  }

  function applyHex(raw: string) {
    const m = /^#?([0-9a-fA-F]{6})$/.exec(raw.trim());
    if (!m) return;
    const n = parseInt(m[1], 16);
    commit({ r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 });
  }

  function openAt(i: number) {
    activeIndex = i;
    open = true;
  }

  const channels: Array<{ key: "r" | "g" | "b"; name: string }> = [
    { key: "r", name: "R" },
    { key: "g", name: "G" },
    { key: "b", name: "B" },
  ];
</script>

<div class="color-picker">
  {#if label}<span class="field-label">{label}</span>{/if}

  <div class="swatch-row">
    {#if multi && values}
      {#each values as c, i (i)}
        <button
          class="swatch-btn"
          class:active={open && i === activeIndex}
          style="background: {css(c)}"
          aria-label="Edit color {i + 1}"
          onclick={() => openAt(i)}
        ></button>
      {/each}
    {:else}
      <button
        class="swatch-btn"
        class:active={open}
        style="background: {css(value)}"
        aria-label="Edit color"
        onclick={() => openAt(0)}
      ></button>
    {/if}
  </div>

  {#if open}
    <button class="popover-backdrop" aria-label="Close" onclick={() => (open = false)}
    ></button>
    <div class="popover">
      <div class="preview" style="background: {css(current)}"></div>
      {#each channels as ch}
        <label class="slider-row">
          <span class="ch-name">{ch.name}</span>
          <input
            type="range"
            min="0"
            max="255"
            value={current[ch.key]}
            oninput={(e) => setChannel(ch.key, e.currentTarget.value)}
          />
          <input
            class="ch-num"
            type="number"
            min="0"
            max="255"
            value={current[ch.key]}
            oninput={(e) => setChannel(ch.key, e.currentTarget.value)}
          />
        </label>
      {/each}
      <label class="hex-row">
        <span class="ch-name">Hex</span>
        <input
          class="hex-input"
          type="text"
          bind:value={hex}
          onchange={() => applyHex(hex)}
        />
      </label>
    </div>
  {/if}
</div>

<style>
  .color-picker {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
  }

  .field-label {
    font-size: 0.8rem;
    color: #8a919c;
  }

  .swatch-row {
    display: inline-flex;
    gap: 0.25rem;
  }

  .swatch-btn {
    width: 1.6rem;
    height: 1.6rem;
    border: 1px solid #1f242c;
    cursor: pointer;
    padding: 0;
  }

  .swatch-btn.active {
    outline: 2px solid #4a6da7;
    outline-offset: 1px;
  }

  .popover-backdrop {
    position: fixed;
    inset: 0;
    z-index: 19;
    border: none;
    background: transparent;
    cursor: default;
  }

  .popover {
    position: absolute;
    z-index: 20;
    top: calc(100% + 4px);
    left: 0;
    width: 15rem;
    padding: 0.6rem;
    background: #3f4855;
    border: 1px solid #1f242c;
    box-shadow: 2px 3px 10px rgba(0, 0, 0, 0.45);
  }

  .preview {
    height: 1.75rem;
    border: 1px solid #1f242c;
    margin-bottom: 0.5rem;
  }

  .slider-row,
  .hex-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin-bottom: 0.35rem;
  }

  .ch-name {
    width: 1.6rem;
    font-size: 0.78rem;
    color: #cfd4db;
  }

  .slider-row input[type="range"] {
    flex: 1;
    min-width: 0;
    accent-color: #4a6da7;
  }

  .ch-num {
    width: 3rem;
  }

  .hex-input {
    flex: 1;
    min-width: 0;
  }

  .ch-num,
  .hex-input {
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.2rem 0.35rem;
    outline: none;
  }
</style>
