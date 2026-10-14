<!-- Настройки (Figma 16:2768 … 16:3774): title bar, 216 px navigation,
     scrolling page. `?page=` opens a page directly (screenshots, links).

     Finding things (not in Figma, asked for by the user):
     - a search field over the navigation (Ctrl+F): every label, note and
       option of every page; picking a hit opens its page, scrolls to the
       row and flashes it;
     - the open page's groups are listed under it in the navigation (when
       it has three or more); the one in view is marked. -->
<script lang="ts">
  import { tick } from 'svelte';
  import { t } from '../../lib/i18n.svelte';
  import { onShown } from '../../lib/fit';
  import { settings } from '../../lib/settings.svelte';
  import Field from '../../lib/ui/Field.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import TitleBar from '../../lib/ui/TitleBar.svelte';
  import About from './pages/About.svelte';
  import Bubbles from './pages/Bubbles.svelte';
  import Context from './pages/Context.svelte';
  import Files from './pages/Files.svelte';
  import General from './pages/General.svelte';
  import Hotkeys from './pages/Hotkeys.svelte';
  import Offline from './pages/Offline.svelte';
  import Providers from './pages/Providers.svelte';
  import Stats from './pages/Stats.svelte';

  type Page = 'general' | 'providers' | 'offline' | 'context' | 'bubbles' | 'files' | 'hotkeys' | 'stats' | 'about';
  const pages: { id: Page; icon: string }[] = [
    { id: 'general', icon: 'sliders-18' },
    { id: 'providers', icon: 'cloud-18' },
    { id: 'offline', icon: 'cpu-18' },
    { id: 'context', icon: 'layers-18' },
    { id: 'bubbles', icon: 'appwin-18' },
    { id: 'files', icon: 'image-18' },
    { id: 'hotkeys', icon: 'keyboard-18' },
    { id: 'stats', icon: 'chart-18' },
    { id: 'about', icon: 'info-18' },
  ];

  const s = $derived(t());
  const initial = new URLSearchParams(location.search).get('page') as Page | null;
  let page = $state<Page>(initial && pages.some((p) => p.id === initial) ? initial : 'general');
  let content: HTMLElement;

  function go(p: Page) {
    page = p;
    content.scrollTop = 0;
  }

  void onShown<Page | null>('window:shown', (p) => {
    if (p) go(p);
  });

  // ── Search ──────────────────────────────────────────────────────────

  type Hit = { page: Page; text: string };

  let query = $state(new URLSearchParams(location.search).get('q') ?? '');
  let picked = $state(0);
  let searchBox = $state<HTMLElement>();

  const norm = (x: string) => x.toLowerCase().replaceAll('ё', 'е').replace(/\s+/g, ' ');
  const escape = (x: string) => x.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

  function strings(v: unknown, out: string[]) {
    if (typeof v === 'string') out.push(v);
    else if (v && typeof v === 'object') for (const x of Object.values(v)) strings(x, out);
  }

  /** Every string a page shows (labels, notes, options), from i18n. */
  const index = $derived.by(() => {
    const out: Hit[] = [];
    const seen = new Set<string>();
    for (const { id } of pages) {
      const list = [s.set.nav[id]];
      strings(s.set[id], list);
      for (const raw of list) {
        const text = raw.trim();
        if (text.length < 3 || text.length > 90 || seen.has(id + text)) continue;
        seen.add(id + text);
        out.push({ page: id, text });
      }
    }
    return out;
  });

  const hits = $derived.by(() => {
    const q = norm(query.trim());
    if (q.length < 2) return [];
    const wordStart = new RegExp(`(^|[^\\p{L}\\p{N}])${escape(q)}`, 'u');
    return index
      .map((h) => ({ h, n: norm(h.text) }))
      // Words that start with the query («ном» finds «номера», not «одном»);
      // texts that start with it first, then short labels.
      .filter(({ n }) => wordStart.test(n))
      .map(({ h, n }) => ({ h, rank: (n.startsWith(q) ? 0 : 1) * 1000 + n.length }))
      .sort((a, b) => a.rank - b.rank)
      .slice(0, 8)
      .map((x) => x.h);
  });

  $effect(() => {
    void query;
    picked = 0;
  });

  /** The row (or group, or any element) that shows `text`. */
  function findShown(text: string): HTMLElement | null {
    const n = norm(text);
    const has = (el: Element) => norm(el.textContent ?? '').includes(n);
    for (const el of content.querySelectorAll<HTMLElement>('[data-setting]')) if (has(el)) return el;
    for (const el of content.querySelectorAll<HTMLElement>('[data-group] > h3')) if (has(el)) return el.parentElement;
    const walk = document.createTreeWalker(content, NodeFilter.SHOW_TEXT);
    for (let node = walk.nextNode(); node; node = walk.nextNode()) {
      if (norm(node.textContent ?? '').includes(n)) {
        const el = node.parentElement;
        return el?.closest<HTMLElement>('[data-setting], p, div') ?? el;
      }
    }
    return null;
  }

  async function jump(h: Hit) {
    query = '';
    if (page !== h.page) go(h.page);
    // Pages load their data on mount: give the row a few frames to appear.
    for (let i = 0; i < 20; i++) {
      await tick();
      const el = findShown(h.text);
      if (el) {
        el.scrollIntoView({ block: 'center', behavior: settings.animations ? 'smooth' : 'auto' });
        el.classList.remove('hl-found');
        void el.offsetWidth;
        el.classList.add('hl-found');
        setTimeout(() => el.classList.remove('hl-found'), 1800);
        return;
      }
      await new Promise((r) => requestAnimationFrame(r));
    }
  }

  function searchKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && query) {
      e.preventDefault();
      e.stopPropagation();
      query = '';
    } else if (e.key === 'ArrowDown' && hits.length) {
      e.preventDefault();
      picked = Math.min(hits.length - 1, picked + 1);
    } else if (e.key === 'ArrowUp' && hits.length) {
      e.preventDefault();
      picked = Math.max(0, picked - 1);
    } else if (e.key === 'Enter' && hits[picked]) {
      e.preventDefault();
      void jump(hits[picked]!);
    }
  }

  function windowKey(e: KeyboardEvent) {
    if (e.code === 'KeyF' && e.ctrlKey && !e.altKey) {
      e.preventDefault();
      searchBox?.querySelector('input')?.focus();
    }
  }

  // ── Sections of the open page ────────────────────────────────────────────────────

  let groups = $state<HTMLElement[]>([]);
  let current = $state(0);

  function collect() {
    const found = [...content.querySelectorAll<HTMLElement>('[data-group]')];
    if (found.length !== groups.length || found.some((g, i) => g !== groups[i])) groups = found;
    onScroll();
  }

  function onScroll() {
    if (!groups.length) return;
    const top = content.scrollTop + 24;
    let i = 0;
    groups.forEach((g, k) => {
      if (g.offsetTop <= top) i = k;
    });
    // Scrolled to the end: the last group, even when it's short.
    if (content.scrollTop + content.clientHeight >= content.scrollHeight - 2) i = groups.length - 1;
    current = i;
  }

  function toSection(i: number) {
    const g = groups[i];
    if (!g) return;
    content.scrollTo({
      top: g.offsetTop - 16,
      behavior: settings.animations ? 'smooth' : 'auto',
    });
  }

  $effect(() => {
    void page;
    let frame = 0;
    const later = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(collect);
    };
    // Groups come and go with the settings that show them.
    const mo = new MutationObserver(later);
    mo.observe(content, { childList: true, subtree: true });
    void tick().then(collect);
    return () => {
      mo.disconnect();
      cancelAnimationFrame(frame);
    };
  });
</script>

<svelte:window onkeydown={windowKey} />

<div class="win">
  <TitleBar icon="gear-18" title={s.set.title} />
  <div class="body">
    <nav class="nav">
      <div class="search" bind:this={searchBox} role="search">
        <Field bind:value={query} icon="search" placeholder={s.set.search} onkeydown={searchKey} />
      </div>
      {#if query.trim().length >= 2}
        {#each hits as h, i (h.page + h.text)}
          <button class="hit" class:active={i === picked} onmouseenter={() => (picked = i)} onclick={() => jump(h)}>
            <span class="hit-text">{h.text}</span>
            <span class="hit-page">{s.set.nav[h.page]}</span>
          </button>
        {:else}
          <p class="none">{s.set.nothingFound}</p>
        {/each}
      {:else}
        {#each pages as p (p.id)}
          <button class="item" class:active={page === p.id} onclick={() => go(p.id)}>
            <Icon name={p.icon} size={18} />
            {s.set.nav[p.id]}
          </button>
          {#if page === p.id && groups.length >= 3}
            <div class="subs" role="group" aria-label={s.set.sections}>
              {#each groups as g, i (g)}
                <button class="sub" class:on={i === current} aria-current={i === current} onclick={() => toSection(i)}
                  >{g.dataset.group}</button
                >
              {/each}
            </div>
          {/if}
        {/each}
      {/if}
    </nav>
    <main class="content" bind:this={content} onscroll={onScroll}>
      {#if page === 'general'}
        <General />
      {:else if page === 'providers'}
        <Providers />
      {:else if page === 'offline'}
        <Offline />
      {:else if page === 'context'}
        <Context />
      {:else if page === 'bubbles'}
        <Bubbles />
      {:else if page === 'files'}
        <Files />
      {:else if page === 'hotkeys'}
        <Hotkeys />
      {:else if page === 'stats'}
        <Stats />
      {:else}
        <About />
      {/if}
    </main>
  </div>
</div>

<style>
  .win {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg-surface);
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .nav {
    width: 216px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 16px 12px;
    background: var(--bg-sunken);
    border-right: 1px solid var(--line-soft);
  }
  .search {
    display: flex;
    flex-direction: column;
    margin-bottom: 10px;
  }
  .hit {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 12px;
    border-radius: var(--radius);
    text-align: left;
  }
  .hit.active {
    background: var(--state-hover);
  }
  .hit-text {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    font: var(--body);
    color: var(--text-primary);
  }
  .hit-page {
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .none {
    margin: 4px 12px;
    font: var(--caption);
    color: var(--text-secondary);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 38px;
    padding: 0 12px;
    border-radius: var(--radius);
    font: var(--body);
    color: var(--text-secondary);
    text-align: left;
    white-space: nowrap;
  }
  .item:hover {
    background: var(--state-hover);
    color: var(--text-primary);
  }
  .item.active {
    background: var(--accent-soft);
    color: var(--text-primary);
    font: var(--body-medium);
  }
  /* The open page's groups: plain text in the menu's own type, indented to
     the labels, a thin line on the left. */
  .subs {
    display: flex;
    flex-direction: column;
    margin: 2px 0 6px 21px;
    padding-left: 7px;
    border-left: 1px solid var(--line-soft);
  }
  .sub {
    padding: 4px 10px;
    border-radius: 6px;
    font: var(--caption);
    color: var(--text-tertiary);
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub:hover {
    color: var(--text-primary);
  }
  .sub.on {
    color: var(--text-primary);
    font: var(--caption-medium);
  }
  .content {
    position: relative;
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: 28px 40px;
  }
  /* A search hit: the row flashes in the accent. */
  .content :global(.hl-found) {
    border-radius: var(--radius);
    animation: hl-found 1.8s ease-out;
  }
  @keyframes hl-found {
    0%,
    40% {
      background: var(--accent-soft);
      box-shadow: 0 0 0 8px var(--accent-soft);
    }
    100% {
      background: transparent;
      box-shadow: 0 0 0 8px transparent;
    }
  }
  .content :global(.page) {
    display: flex;
    flex-direction: column;
    gap: 26px;
  }
  .content :global(.page-head) {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .content :global(.lead) {
    margin: 0;
    font: var(--body);
    color: var(--text-secondary);
  }
</style>
