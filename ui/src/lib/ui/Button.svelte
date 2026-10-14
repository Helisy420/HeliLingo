<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    variant = 'secondary',
    size = 'md',
    icon,
    iconRight,
    disabled = false,
    type = 'button',
    demo,
    onclick,
    children,
  }: {
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
    size?: 'md' | 'sm';
    icon?: string;
    iconRight?: string;
    disabled?: boolean;
    type?: 'button' | 'submit';
    /** Gallery only: draw a pseudo-state. */
    demo?: 'hover' | 'pressed' | 'focus';
    onclick?: (e: MouseEvent) => void;
    children?: Snippet;
  } = $props();
</script>

<button class="btn {variant} {size} {demo ?? ''}" {type} {disabled} {onclick}>
  {#if icon}<Icon name={icon} size={size === 'sm' ? 14 : 16} />{/if}
  {#if children}<span>{@render children()}</span>{/if}
  {#if iconRight}<Icon name={iconRight} size={size === 'sm' ? 14 : 16} />{/if}
</button>

<style>
  /* Figma Button/*: 36 px tall, 16 px sides, radius 8, Body/Medium;
     small: 28 px, 12 px sides, radius 6, Caption/Medium. */
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 36px;
    padding: 0 16px;
    border-radius: var(--radius);
    font: var(--body-medium);
    white-space: nowrap;
    color: var(--text-primary);
    flex-shrink: 0;
  }
  .sm {
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    border-radius: var(--radius-sm);
    font: var(--caption-medium);
  }
  .btn:focus-visible,
  .btn.focus {
    box-shadow: inset 0 0 0 2px var(--text-primary);
  }

  .primary {
    background: var(--accent);
    color: var(--text-on-marker);
  }
  .primary:hover,
  .primary.hover {
    background: var(--accent-hover);
  }
  .primary:active,
  .primary.pressed {
    background: var(--accent-pressed);
  }

  .secondary {
    background: var(--bg-field);
    border: 1px solid var(--line-strong);
  }
  .secondary:hover,
  .secondary.hover {
    background: var(--state-hover);
  }
  .secondary:active,
  .secondary.pressed {
    background: var(--state-pressed);
  }

  .ghost:hover,
  .ghost.hover {
    background: var(--state-hover);
  }
  .ghost:active,
  .ghost.pressed {
    background: var(--state-pressed);
  }

  .danger {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .danger:hover,
  .danger.hover,
  .danger:active,
  .danger.pressed {
    background: var(--danger);
    color: var(--text-on-danger);
  }
  .danger:active,
  .danger.pressed {
    opacity: 0.85;
  }

  .btn:disabled,
  .btn:disabled:hover,
  .btn:disabled:active {
    background: transparent;
    color: var(--text-disabled);
    opacity: 1;
  }
  .primary:disabled,
  .primary:disabled:hover {
    background: var(--bg-disabled);
  }
  .secondary:disabled,
  .danger:disabled {
    border: 1px solid var(--line-soft);
  }
</style>
