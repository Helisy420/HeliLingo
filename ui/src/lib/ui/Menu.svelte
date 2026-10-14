<!-- Dropdown panel (Figma «Выпадающие списки»): surface, 1 px soft border,
     radius 12, 6 px padding, 2 px between rows, Shadow/Menu. Closes on a
     click outside and on Esc. -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { onMount } from 'svelte';

  let {
    width,
    align = 'left',
    inline = false,
    onclose,
    children,
  }: {
    width: number;
    /** Part of the page (no popover frame), e.g. inside the tray menu. */
    inline?: boolean;
    /** Which edge lines up with the button. */
    align?: 'left' | 'right';
    onclose: () => void;
    children: Snippet;
  } = $props();

  let el: HTMLDivElement;

  /** Tells the window its content changed size (windows that fit their
      content, like Quick, listen for it). */
  function layout() {
    window.dispatchEvent(new Event('hl:layout'));
  }

  onMount(() => {
    if (inline) return;
    // Defer, so the click that opened the menu doesn't close it.
    const id = setTimeout(() => {
      window.addEventListener('pointerdown', outside, true);
    });
    // Rows can arrive after the menu opens (the engine list loads).
    const ro = new ResizeObserver(layout);
    ro.observe(el);
    return () => {
      clearTimeout(id);
      ro.disconnect();
      window.removeEventListener('pointerdown', outside, true);
      requestAnimationFrame(layout);
    };
  });

  function outside(e: PointerEvent) {
    if (!el.contains(e.target as Node) && !(e.target as Element).closest('[data-menu-anchor]')) onclose();
  }
</script>

<div class="menu {align}" class:inline style:width={inline ? undefined : `${width}px`} bind:this={el} role="listbox">
  {@render children()}
</div>

<style>
  .menu {
    position: absolute;
    top: calc(100% + 6px);
    z-index: 10;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px;
    border-radius: 12px;
    background: var(--bg-surface);
    border: 1px solid var(--line-soft);
    box-shadow:
      0 1px 3px rgba(0, 0, 0, 0.12),
      0 8px 24px -4px rgba(0, 0, 0, 0.22);
    max-height: 420px;
    overflow-y: auto;
    overscroll-behavior: contain;
    scrollbar-width: thin;
    scrollbar-color: var(--line-strong) transparent;
  }
  .inline {
    position: static;
    padding: 0;
    border: 0;
    box-shadow: none;
    background: none;
    max-height: 300px;
  }
  .left {
    left: 0;
  }
  .right {
    right: 0;
  }
  .menu :global(.caption-row) {
    padding: 10px 10px 4px;
    font: var(--caption-medium);
    color: var(--text-tertiary);
  }
</style>
