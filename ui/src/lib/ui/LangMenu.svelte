<!-- Language list (Figma «Выбор языка»): search, «Определить язык» (source
     only), recent languages, then all languages with their codes. Typing
     filters; ↑/↓ and Enter pick. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../i18n.svelte';
  import { LANGUAGES, langMatches, langName, recentLangs, rememberLang } from '../langs';
  import Icon from './Icon.svelte';
  import Menu from './Menu.svelte';
  import MenuItem from './MenuItem.svelte';

  let {
    value,
    allowAuto = false,
    align = 'left',
    inline = false,
    onpick,
    onclose,
  }: {
    value: string;
    allowAuto?: boolean;
    align?: 'left' | 'right';
    inline?: boolean;
    onpick: (code: string) => void;
    onclose: () => void;
  } = $props();

  const s = $derived(t());
  let query = $state('');
  let active = $state(0);
  let search: HTMLInputElement;

  const recent = recentLangs();
  /** Rows in order, for the keyboard. */
  const rows = $derived.by(() => {
    const q = query.trim();
    const all = LANGUAGES.map(([c]) => c).filter((c) => langMatches(c, q));
    if (q) return all;
    const head = allowAuto ? ['auto'] : [];
    return [...head, ...recent, ...all.filter((c) => !recent.includes(c))];
  });

  function pick(code: string) {
    rememberLang(code);
    onpick(code);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      active = Math.min(rows.length - 1, active + 1);
      e.preventDefault();
    } else if (e.key === 'ArrowUp') {
      active = Math.max(0, active - 1);
      e.preventDefault();
    } else if (e.key === 'Enter') {
      const code = rows[active];
      if (code) pick(code);
      e.preventDefault();
      e.stopPropagation();
    } else if (e.key === 'Escape') {
      onclose();
      e.preventDefault();
      e.stopPropagation();
    }
  }

  onMount(() => search.focus());

  const index = (code: string) => rows.indexOf(code);
</script>

<Menu width={244} {align} {inline} {onclose}>
  <label class="search">
    <Icon name="search" />
    <input
      bind:this={search}
      bind:value={query}
      oninput={() => (active = 0)}
      onkeydown={onKey}
      placeholder={s.findLanguage}
      spellcheck="false"
    />
  </label>
  {#if query.trim()}
    {#each rows as code (code)}
      <MenuItem label={langName(code)} trailing={code} selected={code === value} active={index(code) === active} onpick={() => pick(code)} />
    {/each}
  {:else}
    {#if allowAuto}
      <MenuItem label={s.detectLanguage} icon="sparkles" selected={value === 'auto'} active={active === 0} onpick={() => pick('auto')} />
    {/if}
    {#if recent.length}
      <div class="caption-row">{s.recent}</div>
      {#each recent as code (code)}
        <MenuItem label={langName(code)} selected={code === value} active={index(code) === active} onpick={() => pick(code)} />
      {/each}
    {/if}
    <div class="caption-row">{s.allLanguages}</div>
    {#each rows.filter((c) => c !== 'auto' && !recent.includes(c)) as code (code)}
      <MenuItem label={langName(code)} trailing={code} selected={code === value} active={index(code) === active} onpick={() => pick(code)} />
    {/each}
  {/if}
</Menu>

<style>
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 10px;
    border-radius: var(--radius);
    background: var(--bg-sunken);
    color: var(--text-tertiary);
    flex-shrink: 0;
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
</style>
