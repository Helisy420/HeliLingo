<script lang="ts">
  import Icon from './Icon.svelte';

  let {
    icon,
    label,
    active = false,
    disabled = false,
    size = 'md',
    demo,
    onclick,
  }: {
    icon: string;
    /** Tooltip and accessible name. */
    label: string;
    /** «включена»: a toggle that is on. */
    active?: boolean;
    disabled?: boolean;
    /** md: 32 px (Figma IconButton); sm: 28 px (bubble mini-buttons). */
    size?: 'md' | 'sm';
    /** Gallery only: draw a pseudo-state. */
    demo?: 'hover' | 'pressed';
    onclick?: (e: MouseEvent) => void;
  } = $props();
</script>

<button class="ib {size} {demo ?? ''}" class:active {disabled} {onclick} title={label} aria-label={label} aria-pressed={active}>
  <Icon name={icon} />
</button>

<style>
  /* Figma IconButton/*: 32×32, radius 8, 16 px icon. */
  .ib {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius);
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .ib.sm {
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm);
  }
  .ib:hover,
  .ib.hover {
    background: var(--state-hover);
    color: var(--text-primary);
  }
  .ib:active,
  .ib.pressed {
    background: var(--state-pressed);
    color: var(--text-primary);
  }
  .ib.active {
    background: var(--accent-soft);
    color: var(--text-primary);
  }
  .ib:focus-visible {
    box-shadow: inset 0 0 0 2px var(--text-primary);
  }
  .ib:disabled {
    background: transparent;
    color: var(--text-disabled);
  }
</style>
