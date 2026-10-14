<!-- Slider (Figma, settings rows): 150 px, thin track with an accent fill,
     16 px knob with a 2 px accent ring; the value is shown by the caller. -->
<script lang="ts">
  let {
    value,
    min,
    max,
    step = 1,
    width = 150,
    label,
    onchange,
  }: {
    value: number;
    min: number;
    max: number;
    step?: number;
    width?: number;
    label?: string;
    onchange: (v: number) => void;
  } = $props();

  const pct = $derived(((value - min) / (max - min)) * 100);
</script>

<input
  class="slider"
  type="range"
  {min}
  {max}
  {step}
  {value}
  aria-label={label}
  style:width="{width}px"
  style:--pct="{pct}%"
  oninput={(e) => onchange(Number(e.currentTarget.value))}
/>

<style>
  .slider {
    appearance: none;
    height: 18px;
    margin: 0;
    background: transparent;
    cursor: default;
  }
  .slider::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(to right, var(--accent) var(--pct), var(--line-strong) var(--pct));
  }
  .slider::-webkit-slider-thumb {
    appearance: none;
    width: 16px;
    height: 16px;
    margin-top: -6px;
    border-radius: 50%;
    background: var(--bg-surface);
    box-shadow: inset 0 0 0 2px var(--text-primary);
  }
  .slider:focus-visible::-webkit-slider-thumb {
    box-shadow:
      inset 0 0 0 2px var(--text-primary),
      0 0 0 3px var(--accent-soft);
  }
</style>
