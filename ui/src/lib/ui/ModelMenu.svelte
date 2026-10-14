<!-- OpenRouter model list: search by name or id, price per million tokens
     (input / output) on the right, free models marked. Built from the
     Menu + MenuItem components like the language list. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../i18n.svelte';
  import { openrouterModels, type ModelInfo } from '../ipc';
  import Icon from './Icon.svelte';
  import Menu from './Menu.svelte';
  import MenuItem from './MenuItem.svelte';

  let {
    value,
    align = 'right',
    onpick,
    onclose,
  }: {
    value: string | null;
    align?: 'left' | 'right';
    onpick: (id: string) => void;
    onclose: () => void;
  } = $props();

  const s = $derived(t());
  const p = $derived(s.set.providers);
  let all = $state<ModelInfo[]>([]);
  let failed = $state(false);
  let loading = $state(true);
  let query = $state('');
  let active = $state(0);
  let search: HTMLInputElement;

  /** At most 60 rows: the list has hundreds of models. */
  const rows = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const hit = q ? all.filter((m) => m.name.toLowerCase().includes(q) || m.id.toLowerCase().includes(q)) : all;
    return hit.slice(0, 60);
  });

  async function load(refresh = false) {
    loading = true;
    try {
      all = await openrouterModels(refresh);
      failed = false;
    } catch {
      failed = true;
    }
    loading = false;
  }

  function price(m: ModelInfo): string {
    if (m.priceIn === 0 && m.priceOut === 0) return p.free;
    const f = (n: number) => (n < 1 ? n.toFixed(2) : n < 10 ? n.toFixed(1) : n.toFixed(0)).replace(/\.0+$/, '');
    return `$${f(m.priceIn)} / ${f(m.priceOut)}`;
  }

  function note(m: ModelInfo): string {
    const k = m.context >= 1000 ? `${Math.round(m.context / 1000)}K` : `${m.context}`;
    return `${m.id} · ${p.context(k)}`;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      active = Math.min(rows.length - 1, active + 1);
      e.preventDefault();
    } else if (e.key === 'ArrowUp') {
      active = Math.max(0, active - 1);
      e.preventDefault();
    } else if (e.key === 'Enter') {
      const m = rows[active];
      if (m) onpick(m.id);
      e.preventDefault();
    } else if (e.key === 'Escape') {
      onclose();
      e.preventDefault();
      e.stopPropagation();
    }
  }

  onMount(() => {
    search.focus();
    void load();
  });
</script>

<Menu width={380} {align} {onclose}>
  <div class="top">
    <label class="search">
      <Icon name="search" />
      <input
        bind:this={search}
        bind:value={query}
        oninput={() => (active = 0)}
        onkeydown={onKey}
        placeholder={p.findModel}
        spellcheck="false"
      />
    </label>
    <button class="refresh" title={p.refreshModels} aria-label={p.refreshModels} onclick={() => load(true)} disabled={loading}>
      <Icon name="refresh" />
    </button>
  </div>
  {#if failed && !all.length}
    <div class="caption-row">{p.modelsFailed}</div>
  {:else if loading && !all.length}
    <div class="caption-row">…</div>
  {/if}
  {#each rows as m, i (m.id)}
    <MenuItem label={m.name || m.id} note={note(m)} trailing={price(m)} selected={m.id === value} active={i === active} onpick={() => onpick(m.id)} />
  {/each}
</Menu>

<style>
  .top {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }
  .search {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 10px;
    border-radius: var(--radius);
    background: var(--bg-sunken);
    color: var(--text-tertiary);
  }
  input {
    flex: 1;
    min-width: 0;
    border: 0;
    padding: 0;
    background: none;
    font: var(--body);
    color: var(--text-primary);
    user-select: text;
  }
  input::placeholder {
    color: var(--text-tertiary);
  }
  .refresh {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius);
    color: var(--text-secondary);
  }
  .refresh:hover {
    background: var(--state-hover);
    color: var(--text-primary);
  }
</style>
