<!-- A small dropdown for a fixed list (settings rows): 30 px button with
     Body/Default text and a chevron (Figma Dropdown in settings), opening a
     Menu with the options. -->
<script lang="ts" generics="T extends string | number">
  import Icon from './Icon.svelte';
  import Menu from './Menu.svelte';
  import MenuItem from './MenuItem.svelte';

  let {
    value,
    options,
    width = 156,
    onchange,
  }: {
    value: T;
    options: { value: T; label: string }[];
    width?: number;
    onchange: (v: T) => void;
  } = $props();

  let open = $state(false);
  const current = $derived(options.find((o) => o.value === value)?.label ?? String(value));

  function pick(v: T) {
    open = false;
    if (v !== value) onchange(v);
  }

  function onKey(e: KeyboardEvent) {
    if (open && e.key === 'Escape') {
      e.stopPropagation();
      open = false;
    }
  }
</script>

<svelte:window onkeydowncapture={onKey} />

<span class="anchor" data-menu-anchor>
  <button class="choice" class:open style:width="{width}px" onclick={() => (open = !open)} aria-expanded={open}>
    <span class="text">{current}</span>
    <Icon name={open ? 'chev-u' : 'chev-d'} />
  </button>
  {#if open}
    <Menu width={Math.max(width, 180)} align="right" onclose={() => (open = false)}>
      {#each options as o (o.value)}
        <MenuItem label={o.label} selected={o.value === value} onpick={() => pick(o.value)} />
      {/each}
    </Menu>
  {/if}
</span>

<style>
  .anchor {
    position: relative;
    display: inline-flex;
  }
  .choice {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 10px 0 12px;
    border-radius: var(--radius);
    background: var(--bg-field);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    font: var(--body);
    color: var(--text-primary);
  }
  .choice :global(.icon) {
    color: var(--text-secondary);
  }
  .choice:hover {
    background: var(--state-hover);
  }
  .choice.open,
  .choice:focus-visible {
    background: var(--bg-field);
    box-shadow: inset 0 0 0 1.5px var(--text-primary);
  }
  .text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }
</style>
