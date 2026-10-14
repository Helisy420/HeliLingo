<script lang="ts">
  let {
    checked = $bindable(false),
    disabled = false,
    label,
    onchange,
  }: { checked?: boolean; disabled?: boolean; label?: string; onchange?: (on: boolean) => void } = $props();
</script>

<button
  class="sw"
  class:on={checked}
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  onclick={() => {
    checked = !checked;
    onchange?.(checked);
  }}
>
  <span class="knob"></span>
</button>

<style>
  /* Figma Switch: 40×22, knob 16. Off: sunken track with a 1.5 px strong
     border, tertiary knob at x 4.5. On: accent track, on-marker knob at x 21. */
  .sw {
    position: relative;
    width: 40px;
    height: 22px;
    border-radius: 11px;
    background: var(--bg-sunken);
    box-shadow: inset 0 0 0 1.5px var(--line-strong);
    flex-shrink: 0;
    transition: background 120ms ease;
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 4.5px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--text-tertiary);
    transition: transform 120ms ease, background 120ms ease;
  }
  .on {
    background: var(--accent);
    box-shadow: none;
  }
  .on .knob {
    background: var(--text-on-marker);
    transform: translateX(16.5px);
  }
  .sw:focus-visible {
    outline: 2px solid var(--text-primary);
    outline-offset: 2px;
  }
  .sw:disabled {
    opacity: 0.5;
  }
</style>
