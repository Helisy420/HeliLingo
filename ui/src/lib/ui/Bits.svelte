<!-- Small display-only pieces from the Figma sheet "Метки, подсказки, прогресс":
     Tooltip, Progress, KeyCap and Chip, picked with `kind`. -->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    kind,
    value = 0,
    width = 200,
    onclick,
    children,
  }: {
    kind: 'tooltip' | 'progress' | 'key' | 'chip';
    /** progress: 0…1. */
    value?: number;
    /** progress: track width. */
    width?: number;
    onclick?: (e: MouseEvent) => void;
    children?: Snippet;
  } = $props();
</script>

{#if kind === 'tooltip'}
  <span class="tooltip">{@render children?.()}</span>
{:else if kind === 'progress'}
  <span class="track" style:width="{width}px" role="progressbar" aria-valuenow={Math.round(value * 100)}>
    <span class="bar" style:width="{Math.max(0, Math.min(1, value)) * 100}%"></span>
  </span>
{:else if kind === 'key'}
  <kbd class="key">{@render children?.()}</kbd>
{:else}
  <button class="chip" {onclick}>{@render children?.()}</button>
{/if}

<style>
  .tooltip {
    display: inline-flex;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    background: var(--ink-fill);
    color: var(--text-inverse);
    font: var(--caption-medium);
    white-space: nowrap;
  }

  .track {
    display: inline-flex;
    height: 6px;
    border-radius: 3px;
    background: var(--bg-sunken);
    box-shadow: inset 0 0 0 1px var(--line-soft);
    overflow: hidden;
    flex-shrink: 0;
  }
  .bar {
    height: 100%;
    border-radius: 3px;
    background: var(--accent);
    transition: width 200ms ease;
  }

  /* KeyCap: 22 px min height, 24 px min width, 1 px border with a 2 px bottom. */
  .key {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 24px;
    min-height: 22px;
    padding: 2px 7px 1px;
    border: 1px solid var(--line-strong);
    border-bottom-width: 2px;
    border-radius: 5px;
    background: var(--bg-field);
    font: var(--caption-medium);
    color: var(--text-secondary);
    white-space: nowrap;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    padding: 6px 12px;
    border-radius: var(--radius);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    font: var(--body);
    color: var(--text-secondary);
    white-space: nowrap;
  }
  .chip:hover {
    background: var(--state-hover);
    color: var(--text-primary);
  }
</style>
