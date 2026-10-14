<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    open = false,
    size = 'md',
    width,
    demo,
    onclick,
    children,
  }: {
    /** The list is open: primary border, chevron up. */
    open?: boolean;
    /** md: 36 px (settings); sm: 32 px (window toolbars). */
    size?: 'md' | 'sm';
    width?: number;
    /** Gallery only: draw the hover state. */
    demo?: 'hover';
    onclick?: (e: MouseEvent) => void;
    children: Snippet;
  } = $props();
</script>

<button class="sel {size} {demo ?? ''}" class:open style:width={width ? `${width}px` : undefined} {onclick} aria-expanded={open}>
  <span class="text">{@render children()}</span>
  <Icon name={open ? 'chev-u' : 'chev-d'} />
</button>

<style>
  /* Figma Dropdown: 36 px, 12 px left / 10 px right, radius 8, Body/Medium. */
  .sel {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 10px 0 12px;
    border-radius: var(--radius);
    background: var(--bg-field);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    font: var(--body-medium);
    color: var(--text-primary);
    flex-shrink: 0;
  }
  .sel.sm {
    height: 32px;
  }
  .sel :global(.icon) {
    color: var(--text-secondary);
  }
  .sel:hover,
  .sel.hover {
    background: var(--state-hover);
  }
  .sel.open,
  .sel:focus-visible {
    background: var(--bg-field);
    box-shadow: inset 0 0 0 1.5px var(--text-primary);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }
</style>
