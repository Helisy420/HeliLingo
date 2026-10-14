<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    tone = 'plain',
    dot = false,
    children,
  }: {
    /** accent: «авто»; plain: «новое», «офлайн». */
    tone?: 'accent' | 'plain';
    /** Green status dot before the text. */
    dot?: boolean;
    children: Snippet;
  } = $props();
</script>

<span class="badge {tone}">
  {#if dot}<span class="dot"></span>{/if}
  {@render children()}
</span>

<style>
  /* Figma Badge: 2 × 8 px, radius 6, Caption/Medium. */
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    font: var(--caption-medium);
    color: var(--text-primary);
    white-space: nowrap;
    flex-shrink: 0;
  }
  .plain {
    background: var(--bg-sunken);
    box-shadow: inset 0 0 0 1px var(--line-soft);
  }
  .accent {
    background: var(--accent-soft);
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--status-ok);
  }
</style>
