<!-- История (no Figma frame: built from the settings window parts): search,
     all/starred, entries grouped by day; star, copy, delete, reopen. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { onShown } from '../../lib/fit';
  import { engineName, t } from '../../lib/i18n.svelte';
  import { historyClear, historyDelete, historyList, historyStar, openInTranslator, type HistoryItem, copyText } from '../../lib/ipc';
  import { settings } from '../../lib/settings.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Field from '../../lib/ui/Field.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import IconButton from '../../lib/ui/IconButton.svelte';
  import Masked from '../../lib/ui/Masked.svelte';
  import Segmented from '../../lib/ui/Segmented.svelte';
  import TitleBar from '../../lib/ui/TitleBar.svelte';

  const s = $derived(t());
  const h = $derived(s.historyWin);
  let query = $state('');
  let filter = $state<'all' | 'starred'>('all');
  let items = $state<HistoryItem[]>([]);
  let copiedId = $state<number | null>(null);

  async function load() {
    items = await historyList(query, filter === 'starred');
  }

  $effect(() => {
    void query;
    void filter;
    void load();
  });

  onMount(() => {
    void onShown('window:shown', () => void load());
  });

  /** "Сегодня", "Вчера" or a date, for grouping. */
  function day(ts: number): string {
    const d = new Date(ts * 1000);
    const today = new Date();
    const yesterday = new Date(Date.now() - 86_400_000);
    if (d.toDateString() === today.toDateString()) return h.today;
    if (d.toDateString() === yesterday.toDateString()) return h.yesterday;
    return d.toLocaleDateString(settings.uiLanguage === 'en' ? 'en-GB' : 'ru-RU', { day: 'numeric', month: 'long' });
  }

  function time(ts: number) {
    return new Date(ts * 1000).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' });
  }

  const groups = $derived.by(() => {
    const out: { day: string; items: HistoryItem[] }[] = [];
    for (const it of items) {
      const d = day(it.created);
      const last = out[out.length - 1];
      if (last && last.day === d) last.items.push(it);
      else out.push({ day: d, items: [it] });
    }
    return out;
  });

  async function star(it: HistoryItem) {
    await historyStar(it.id, !it.starred);
    await load();
  }

  async function remove(it: HistoryItem) {
    await historyDelete(it.id);
    await load();
  }

  async function copy(it: HistoryItem) {
    await copyText(it.text);
    copiedId = it.id;
    setTimeout(() => (copiedId = null), 1000);
  }

  async function clearAll() {
    await historyClear();
    await load();
  }

  const code = (c: string | null) => (c ? c.split('-')[0]!.toUpperCase() : '?');
</script>

<div class="win">
  <TitleBar icon="history-18" title={h.title} />

  <div class="tools">
    <span class="search"><Field bind:value={query} icon="search" placeholder={h.search} /></span>
    <Segmented
      size="sm"
      value={filter}
      options={[
        { value: 'all', label: h.all },
        { value: 'starred', label: h.starred },
      ]}
      onchange={(v) => (filter = v as typeof filter)}
    />
  </div>

  <main class="list">
    {#if !settings.saveHistory}
      <p class="empty">{h.off}</p>
    {/if}
    {#each groups as g (g.day)}
      <h3 class="day">{g.day}</h3>
      {#each g.items as it (it.id)}
        <article class="item">
          <button class="body" title={h.reopen} onclick={() => openInTranslator(it.sourceText)}>
            <span class="src"><Masked text={it.sourceText} /></span>
            <span class="tr"><Masked text={it.text} /></span>
            <span class="meta">
              {code(it.sourceLang)} → {code(it.targetLang)} · {engineName(it.engine)} · {time(it.created)}
            </span>
          </button>
          <span class="acts">
            <IconButton icon={copiedId === it.id ? 'check' : 'copy'} label={s.copy} onclick={() => copy(it)} />
            <IconButton icon="trash" label={h.delete} onclick={() => remove(it)} />
          </span>
          <button class="star" class:on={it.starred} title={it.starred ? h.unstar : h.star} onclick={() => star(it)}>
            <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
              <path
                d="M8 1.6l1.98 4.01 4.42.64-3.2 3.12.76 4.4L8 11.69l-3.96 2.08.76-4.4-3.2-3.12 4.42-.64L8 1.6z"
                stroke="currentColor"
                stroke-width="1.17"
                stroke-linejoin="round"
                fill={it.starred ? 'currentColor' : 'none'}
              />
            </svg>
          </button>
        </article>
      {/each}
    {:else}
      <div class="empty-state">
        <span class="empty-icon"><Icon name="history-18" size={18} /></span>
        <p class="empty">{filter === 'starred' ? h.emptyStarred : h.empty}</p>
      </div>
    {/each}
  </main>

  <footer class="foot">
    <span class="hint">{h.clearHint}</span>
    <span class="spacer"></span>
    <Button variant="danger" size="sm" icon="trash" disabled={!items.length} onclick={clearAll}>{h.clearAll}</Button>
  </footer>
</div>

<style>
  .win {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg-surface);
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 20px 10px;
  }
  .search {
    flex: 1;
    display: flex;
  }
  .search :global(.wrap) {
    flex: 1;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 20px 12px;
  }
  .day {
    margin: 12px 0 6px;
    font: var(--caption-medium);
    color: var(--text-tertiary);
  }
  .item {
    position: relative;
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 10px 10px 10px 12px;
    border-radius: var(--radius-lg);
  }
  .item:hover {
    background: var(--bg-sunken);
  }
  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    text-align: left;
  }
  .src {
    font: var(--body);
    color: var(--text-secondary);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .tr {
    font: var(--body-medium);
    color: var(--text-primary);
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .meta {
    margin-top: 2px;
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .acts {
    display: flex;
    opacity: 0;
    transition: opacity 120ms;
  }
  .item:hover .acts,
  .item:focus-within .acts {
    opacity: 1;
  }
  .star {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius);
    color: var(--text-tertiary);
  }
  .star:hover {
    background: var(--state-hover);
    color: var(--text-primary);
  }
  .star.on {
    color: var(--accent);
  }
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding-top: 80px;
  }
  .empty-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    border-radius: 24px;
    background: var(--accent-soft);
    color: var(--text-primary);
  }
  .empty {
    margin: 0;
    font: var(--body);
    color: var(--text-secondary);
    text-align: center;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 52px;
    padding: 0 20px;
    border-top: 1px solid var(--line-soft);
  }
  .hint {
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .spacer {
    flex: 1;
  }
</style>
