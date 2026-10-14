<script lang="ts">
  import Icon from './Icon.svelte';

  type Option = { value: string; label: string; icon?: string; disabled?: boolean };
  let {
    options,
    value = $bindable(),
    size = 'md',
    onchange,
  }: {
    options: Option[];
    value?: string;
    /** sm: the Statistics period switch (4 px × 12 px items, radius 6). */
    size?: 'md' | 'sm';
    onchange?: (v: string) => void;
  } = $props();
</script>

<div class="seg {size}" role="radiogroup">
  {#each options as o (o.value)}
    <button
      class="opt"
      class:sel={o.value === value}
      class:with-icon={!!o.icon}
      role="radio"
      aria-checked={o.value === value}
      disabled={o.disabled}
      onclick={() => {
        value = o.value;
        onchange?.(o.value);
      }}
    >
      {#if o.icon}<Icon name={o.icon} />{/if}
      {o.label}
    </button>
  {/each}
</div>

<style>
  /* Figma Segmented: sunken track, 3 px inset, radius 10; options radius 7,
     5 px × 12 px (10 px left with an icon); selected = ink fill. */
  .seg {
    display: inline-flex;
    gap: 2px;
    padding: 3px;
    border-radius: var(--radius-lg);
    background: var(--bg-sunken);
    border: 1px solid var(--line-soft);
    flex-shrink: 0;
  }
  .opt {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 5px 12px;
    border-radius: 7px;
    font: var(--body);
    color: var(--text-secondary);
    white-space: nowrap;
  }
  .opt.with-icon {
    padding-left: 10px;
  }
  .opt:hover:not(.sel):not(:disabled) {
    color: var(--text-primary);
  }
  .opt:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .sel {
    background: var(--ink-fill);
    color: var(--text-inverse);
    font: var(--body-medium);
  }
  .sm {
    border-radius: 9px;
  }
  .sm .opt {
    padding: 4px 12px;
    border-radius: 6px;
  }
  .opt:focus-visible {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
</style>
