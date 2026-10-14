<!-- Быстрый перевод (Figma 16:2547 / 16:818 / 12:24, section 4: 36:167,
     43:172, 43:235, 43:354, 47:180–47:216): type, the translation streams in
     below. Enter copies and closes, Esc closes.

     Layouts (Settings → «Баблы и Вики» → «Быстрый перевод»):
     - window: language bar, input, result; «Другие варианты» and the
       history list are switches; mini-buttons replace the labelled ones;
       with history on, an empty field shows only the history (search,
       filters, ↑↓ to pick, Enter pastes, Tab or Enter on nothing found
       translates the query);
     - line: one launcher-like line, the translation right under it. -->
<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { isWordLike } from '../../lib/bubble/types';
  import { fitWindow, hideOnBlur, hideWindow, onShown } from '../../lib/fit';
  import { engineName, t } from '../../lib/i18n.svelte';
  import {
    cancel,
    copyText,
    historyClear,
    historyList,
    inApp,
    replaceSelection,
    translate,
    type EngineError,
    type HistoryItem,
    type TranslateEvent,
  } from '../../lib/ipc';
  import { langName, sameLang } from '../../lib/langs';
  import { settings } from '../../lib/settings.svelte';
  import Bits from '../../lib/ui/Bits.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import EngineMenu from '../../lib/ui/EngineMenu.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import IconButton from '../../lib/ui/IconButton.svelte';
  import LangMenu from '../../lib/ui/LangMenu.svelte';
  import Masked from '../../lib/ui/Masked.svelte';
  import SelectButton from '../../lib/ui/SelectButton.svelte';

  /** Wait after the last key before translating. */
  const DEBOUNCE_MS = 300;

  // Languages start from Settings; changes here last until the window is
  // reopened.
  let source = $state(settings.sourceLang);
  let target = $state(settings.targetLang);
  let detected = $state<string | null>(null);
  /** Engine that answered last (shown on the engine button). */
  let engine = $state('google');
  /** Engine picked in the menu; goes first in the chain. */
  let preferred = $state<string | undefined>(undefined);
  let pinned = $state(false);
  let menu = $state<'source' | 'target' | 'engine' | null>(null);

  let input = $state('');
  let output = $state('');
  let alternatives = $state<string[]>([]);
  let busy = $state(false);
  let error = $state<EngineError | null>(null);
  let copied = $state(false);
  /** Bumped per request; events of older requests are dropped. */
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let field = $state<HTMLTextAreaElement>();

  // History (list under the result, or the whole window when the field is empty).
  let history = $state<HistoryItem[]>([]);
  let query = $state('');
  let filter = $state<'all' | 'words' | 'phrases' | 'starred'>('all');
  let active = $state(0);
  let searchField = $state<HTMLInputElement>();

  const s = $derived(t());
  const q = $derived(s.quickWin);
  const layout = $derived(settings.quick.layout);
  const historyOnly = $derived(layout === 'window' && settings.quick.history && !input.trim());

  const shownHistory = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    return history.filter((it) => {
      if (filter === 'words' && !isWordLike(it.sourceText)) return false;
      if (filter === 'phrases' && isWordLike(it.sourceText)) return false;
      if (filter === 'starred' && !it.starred) return false;
      return !needle || it.sourceText.toLowerCase().includes(needle) || it.text.toLowerCase().includes(needle);
    });
  });

  async function loadHistory() {
    if (!settings.quick.history) return;
    history = await historyList('', false);
  }

  function onEvent(e: TranslateEvent) {
    switch (e.type) {
      case 'planned':
        if (e.sourceLang) detected = e.sourceLang;
        // Text already in the target language went to the second language.
        if (e.target !== target) target = e.target;
        break;
      case 'started':
        engine = e.engine;
        output = '';
        break;
      case 'delta':
        output += e.text;
        break;
      case 'replace':
        engine = e.engine;
        output = e.text;
        break;
      case 'done':
        if (e.sourceLang) detected = e.sourceLang;
        output = e.text;
        alternatives = e.alternatives.slice(0, 3);
        busy = false;
        void loadHistory();
        break;
      case 'error':
        error = e.kind;
        busy = false;
        break;
    }
  }

  async function run() {
    const text = input.trim();
    if (!text) {
      seq++;
      void cancel('quick');
      output = '';
      alternatives = [];
      busy = false;
      return;
    }
    busy = true;
    error = null;
    alternatives = [];
    const mine = ++seq;
    await translate({ text, source, target, surface: 'quick', engine: preferred }, (e) => {
      if (mine === seq) onEvent(e);
    });
  }

  function onInput() {
    clearTimeout(timer);
    timer = setTimeout(run, DEBOUNCE_MS);
  }

  function swap() {
    const from = source === 'auto' ? (detected ?? 'en') : source;
    source = target;
    target = from;
    if (output) input = output;
    void run();
  }

  async function copy(text = output) {
    if (!text) return;
    await copyText(text);
    copied = true;
    setTimeout(() => (copied = false), 1200);
  }

  async function close() {
    void cancel('quick');
    await hideWindow();
  }

  /** History-only: the query becomes the text to translate. */
  async function translateQuery() {
    input = query;
    query = '';
    await tick();
    if (field) {
      field.focus();
      grow(field);
    }
    void run();
  }

  function onKey(e: KeyboardEvent) {
    if (menu) return; // the open menu handles its keys
    if (e.key === 'Escape') {
      e.preventDefault();
      void close();
      return;
    }
    if (historyOnly) {
      const n = shownHistory.length;
      if (e.key === 'ArrowDown' && n) {
        active = Math.min(n - 1, active + 1);
        e.preventDefault();
      } else if (e.key === 'ArrowUp' && n) {
        active = Math.max(0, active - 1);
        e.preventDefault();
      } else if (e.key === 'Tab' && query.trim()) {
        e.preventDefault();
        void translateQuery();
      } else if (e.key === 'Enter') {
        e.preventDefault();
        const it = shownHistory[active];
        if (it) void replaceSelection(it.text);
        else if (query.trim()) void translateQuery();
      } else if (e.key === 'c' && e.ctrlKey && shownHistory[active] && !window.getSelection()?.toString()) {
        e.preventDefault();
        void copy(shownHistory[active]!.text);
      }
      return;
    }
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void copy().then(() => {
        if (!pinned) void close();
      });
    }
  }

  /** Reads the translation with a Windows voice for the target language. */
  function speak() {
    if (!output) return;
    speechSynthesis.cancel();
    const u = new SpeechSynthesisUtterance(output);
    u.lang = target;
    const voice = speechSynthesis.getVoices().find((v) => sameLang(v.lang, target.split('-')[0]!));
    if (voice) u.voice = voice;
    speechSynthesis.speak(u);
  }

  function pickLang(which: 'source' | 'target', code: string) {
    if (which === 'source') source = code;
    else target = code;
    menu = null;
    void run();
    field?.focus();
  }

  function pickEngine(id: string) {
    preferred = id;
    engine = id;
    menu = null;
    void run();
    field?.focus();
  }

  function toggle(which: 'source' | 'target' | 'engine') {
    menu = menu === which ? null : which;
  }

  function pick(alt: string) {
    alternatives = [output, ...alternatives.filter((a) => a !== alt)].slice(0, 3);
    output = alt;
  }

  function useItem(it: HistoryItem) {
    input = it.sourceText;
    output = it.text;
    detected = it.sourceLang;
    alternatives = [];
    void tick().then(() => field && grow(field));
  }

  /** Auto-grow the input to its text. */
  function grow(el: HTMLTextAreaElement) {
    el.style.height = '0';
    el.style.height = `${el.scrollHeight}px`;
  }

  function code(c: string | null) {
    return (c ?? '?').split('-')[0]!.toUpperCase();
  }

  function day(ts: number): string {
    const d = new Date(ts * 1000);
    if (d.toDateString() === new Date().toDateString()) return s.historyWin.today;
    if (d.toDateString() === new Date(Date.now() - 86_400_000).toDateString()) return s.historyWin.yesterday;
    return d.toLocaleDateString(settings.uiLanguage === 'en' ? 'en-GB' : 'ru-RU', { day: 'numeric', month: 'long' });
  }

  function clock(ts: number): string {
    const d = new Date(ts * 1000);
    if (d.toDateString() !== new Date().toDateString()) return day(ts).toLowerCase();
    return d.toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' });
  }

  const groups = $derived.by(() => {
    const out: { day: string; items: { it: HistoryItem; i: number }[] }[] = [];
    shownHistory.forEach((it, i) => {
      const d = day(it.created);
      const last = out[out.length - 1];
      if (last && last.day === d) last.items.push({ it, i });
      else out.push({ day: d, items: [{ it, i }] });
    });
    return out;
  });

  /** The line layout's language chip: «Авто → Русский» or «EN → RU». */
  const chip = $derived(
    source === 'auto' && !detected ? `${s.autoTitle} → ${langName(target)}` : `${code(source === 'auto' ? detected : source)} → ${code(target)}`,
  );

  let card = $state<HTMLDivElement>();
  let fit = () => {};

  $effect(() => {
    void menu;
    void historyOnly;
    void layout;
    void tick().then(() => fit());
  });

  // History-only: the search field takes the keys.
  $effect(() => {
    if (historyOnly) {
      void loadHistory();
      void tick().then(() => searchField?.focus());
    }
  });
  $effect(() => {
    void query;
    void filter;
    active = 0;
  });

  onMount(async () => {
    const params = new URLSearchParams(location.search);
    const demo = params.get('demo');
    const demoMenu = params.get('menu');
    if (demoMenu === 'source' || demoMenu === 'target' || demoMenu === 'engine') menu = demoMenu;
    if (demo) {
      input = demo;
      void run();
    }
    void loadHistory();
    field?.focus();
    if (field) grow(field);
    if (!inApp || !card) return;
    fit = await fitWindow(() => card);
    const blur = await hideOnBlur(() => pinned);
    await onShown<string | null>('quick:shown', (text) => {
      blur.shown();
      menu = null;
      void loadHistory();
      if (text) {
        input = text;
        void run();
      }
      if (historyOnly) {
        searchField?.focus();
        return;
      }
      field?.focus();
      field?.select();
      requestAnimationFrame(() => {
        if (field) grow(field);
        fit();
      });
    });
  });

  function errorText(e: EngineError): string {
    return s.errors[e.kind];
  }
</script>

<svelte:window onkeydown={onKey} />

{#snippet historyRows(rows: { it: HistoryItem; i: number }[], selectable: boolean)}
  {#each rows as { it, i } (it.id)}
    <div class="hrow" class:active={selectable && i === active} role="presentation" onmouseenter={() => selectable && (active = i)}>
      <button class="htext" onclick={() => (selectable ? void replaceSelection(it.text) : useItem(it))}>
        <span class="htr"><Masked text={it.text} /></span>
        <span class="hsrc"><Masked text={it.sourceText} /></span>
      </button>
      <span class="hmeta">{code(it.sourceLang)} → {code(it.targetLang)} · {clock(it.created)}</span>
      <IconButton size="sm" icon="copy" label={q.copyItem} onclick={() => void copy(it.text)} />
    </div>
  {/each}
{/snippet}

{#if layout === 'line'}
  <!-- Line (47:180 / 47:191 / 47:216). -->
  <div class="card line" bind:this={card}>
    <div class="lrow" data-tauri-drag-region>
      <span class="lead"><Icon name="languages" /></span>
      <textarea
        bind:this={field}
        bind:value={input}
        oninput={(e) => {
          grow(e.currentTarget);
          onInput();
        }}
        placeholder={q.lineHint}
        rows="1"
        spellcheck="false"
      ></textarea>
      <span class="anchor" data-menu-anchor>
        <button class="lchip" onclick={() => toggle('target')}>
          {chip}
          {#if source === 'auto' && detected}<span class="auto">{s.auto}</span>{/if}
        </button>
        {#if menu === 'target'}
          <LangMenu value={target} align="right" onpick={(c) => pickLang('target', c)} onclose={() => (menu = null)} />
        {/if}
      </span>
    </div>
    {#if output || busy || error}
      <div class="lres">
        {#if error}
          <p class="error">{errorText(error)}</p>
        {:else}
          <p class="ltr" class:pending={busy && !output}>{#if output}{output}{:else}&nbsp;{/if}</p>
          <span class="ltools">
            <IconButton size="sm" icon="speaker" label={s.speak} disabled={!output} onclick={speak} />
            <IconButton size="sm" icon="copy" label={copied ? s.copied : s.copy} disabled={!output} onclick={() => void copy()} />
          </span>
        {/if}
      </div>
      {#if output.length > 120}
        <footer class="lfoot">{q.by(engineName(engine))}</footer>
      {/if}
    {/if}
  </div>
{:else if historyOnly}
  <!-- History only (43:354): the field is empty. -->
  <div class="card hist-only" bind:this={card}>
    <header class="hbar" data-tauri-drag-region>
      <Icon name="history" />
      <span class="htitle" data-tauri-drag-region>{q.history}</span>
      <span class="count">{history.length}</span>
      <span class="spacer" data-tauri-drag-region></span>
      <label class="search">
        <Icon name="search" />
        <input bind:this={searchField} bind:value={query} placeholder={q.search} spellcheck="false" />
      </label>
      <IconButton icon="pin" label={s.pin} active={pinned} onclick={() => (pinned = !pinned)} />
      <IconButton icon="x" label={s.closeWindow} onclick={close} />
    </header>
    <div class="filters">
      <span class="seg" role="radiogroup">
        {#each ['all', 'words', 'phrases', 'starred'] as const as f (f)}
          <button class="opt" class:sel={filter === f} role="radio" aria-checked={filter === f} onclick={() => (filter = f)}
            >{q.filters[f]}</button
          >
        {/each}
      </span>
    </div>
    <div class="hlist">
      {#if !history.length}
        <p class="empty">{q.empty}</p>
      {:else if !shownHistory.length}
        <p class="empty">{q.nothingFound}</p>
      {:else}
        {#each groups as g (g.day)}
          <p class="hday">{g.day}</p>
          {@render historyRows(g.items, true)}
        {/each}
      {/if}
    </div>
    <footer class="foot">
      <Bits kind="key">Esc</Bits>
      <span class="hint">{s.close}</span>
      <span class="spacer"></span>
      <Bits kind="key">↑</Bits><Bits kind="key">↓</Bits>
      <span class="hint">{q.choose}</span>
      <Bits kind="key">Enter</Bits>
      <span class="hint">{shownHistory.length ? q.paste : q.translate}</span>
    </footer>
  </div>
{:else}
  <div class="card" bind:this={card}>
    <header class="bar" data-tauri-drag-region>
      <span class="anchor" data-menu-anchor>
        <SelectButton size="sm" open={menu === 'source'} onclick={() => toggle('source')}>
          {langName(source === 'auto' ? (detected ?? 'en') : source)}
          {#if source === 'auto'}<span class="auto">{s.auto}</span>{/if}
        </SelectButton>
        {#if menu === 'source'}
          <LangMenu value={source} allowAuto onpick={(c) => pickLang('source', c)} onclose={() => (menu = null)} />
        {/if}
      </span>
      <button class="swap" onclick={swap} title={s.swap} aria-label={s.swap}><Icon name="swap-14" size={14} /></button>
      <span class="anchor" data-menu-anchor>
        <SelectButton size="sm" open={menu === 'target'} onclick={() => toggle('target')}>{langName(target)}</SelectButton>
        {#if menu === 'target'}
          <LangMenu value={target} onpick={(c) => pickLang('target', c)} onclose={() => (menu = null)} />
        {/if}
      </span>
      <span class="spacer" data-tauri-drag-region></span>
      <span class="anchor" data-menu-anchor>
        <SelectButton size="sm" open={menu === 'engine'} onclick={() => toggle('engine')}>
          <Icon name="globe" />{engineName(engine)}
        </SelectButton>
        {#if menu === 'engine'}
          <EngineMenu value={preferred ?? engine} onpick={pickEngine} onclose={() => (menu = null)} />
        {/if}
      </span>
      <IconButton icon="pin" label={s.pin} active={pinned} onclick={() => (pinned = !pinned)} />
      <IconButton icon="x" label={s.closeWindow} onclick={close} />
    </header>

    <div class="input">
      <textarea
        bind:this={field}
        bind:value={input}
        oninput={(e) => {
          grow(e.currentTarget);
          onInput();
        }}
        placeholder={s.typeHere}
        rows="1"
        spellcheck="false"
      ></textarea>
    </div>

    {#if output || busy || error}
      <section class="result">
        {#if error}
          <p class="error">{errorText(error)}</p>
        {:else}
          <p class="translation" class:pending={busy && !output}>{#if output}{output}{:else}&nbsp;{/if}</p>
        {/if}
        {#if settings.quick.suggestions && alternatives.length}
          <div class="variants">
            <span class="variants-title">{s.otherVariants}</span>
            <div class="chips">
              {#each alternatives as alt (alt)}
                <button class="chip" onclick={() => pick(alt)}>{alt}</button>
              {/each}
            </div>
          </div>
        {/if}
      </section>
      {#if settings.quick.history && history.length}
        <!-- Suggestions + history (43:235). -->
        <section class="hist">
          <header class="hhead">
            <Icon name="history" />
            <span>{q.history}</span>
            <span class="spacer"></span>
            <IconButton
              size="sm"
              icon="search"
              label={q.search}
              onclick={() => {
                input = '';
                void run();
              }}
            />
            <IconButton
              size="sm"
              icon="trash"
              label={q.clear}
              onclick={() => void historyClear().then(loadHistory)}
            />
          </header>
          <div class="hlist short">
            {@render historyRows(
              history.slice(0, 20).map((it, i) => ({ it, i })),
              false,
            )}
          </div>
        </section>
      {/if}
    {/if}

    <footer class="foot">
      <Bits kind="key">Esc</Bits>
      <span class="hint">{s.close}</span>
      <span class="spacer"></span>
      <IconButton icon="speaker" label={s.speak} disabled={!output} onclick={speak} />
      {#if settings.quick.miniButtons}
        <!-- Mini-buttons (36:167): icons, the label in the tooltip. -->
        <IconButton icon="replace" label={s.replaceSelection} disabled={!output || busy} onclick={() => replaceSelection(output)} />
        <button class="mini-primary" title={copied ? s.copied : s.copy} aria-label={s.copy} disabled={!output} onclick={() => void copy()}>
          <Icon name={copied ? 'check' : 'copy'} />
        </button>
      {:else}
        <Button icon="replace" disabled={!output || busy} onclick={() => replaceSelection(output)}>{s.replaceSelection}</Button>
        <Button variant="primary" icon="copy" disabled={!output} onclick={() => void copy()}>
          {copied ? s.copied : s.copy}
          <span class="enter"><Icon name="enter" size={12} /></span>
        </Button>
      {/if}
    </footer>
  </div>
{/if}

<style>
  :global(html),
  :global(body) {
    background: transparent;
  }
  /* Window = card + room for its shadow (Figma Shadow/Window). */
  :global(body) {
    padding: 8px 24px 40px;
  }

  .card {
    width: 680px;
    display: flex;
    flex-direction: column;
    background: var(--bg-surface);
    border: 1px solid var(--line-soft);
    border-radius: var(--radius-window);
    box-shadow:
      0 2px 6px rgba(0, 0, 0, 0.12),
      0 16px 40px -6px rgba(0, 0, 0, 0.2);
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 12px 12px 0 14px;
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .anchor {
    position: relative;
    display: inline-flex;
  }
  .auto {
    padding: 1px 7px;
    border-radius: 5px;
    background: var(--accent-soft);
    font: var(--caption-medium);
    color: var(--text-primary);
  }
  .swap {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 16px;
    background: var(--bg-surface);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    color: var(--text-primary);
    flex-shrink: 0;
  }
  .swap:hover {
    background: var(--state-hover);
  }

  /* Input/Large: 22/32 Regular. */
  .input {
    padding: 14px 22px 18px;
  }
  textarea {
    display: block;
    width: 100%;
    max-height: 160px;
    border: 0;
    padding: 0;
    resize: none;
    background: none;
    font: 400 22px/32px var(--font);
    color: var(--text-primary);
    caret-color: var(--text-primary);
    user-select: text;
  }
  textarea::placeholder {
    color: var(--text-tertiary);
  }

  .result {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px 14px 12px;
    background: var(--bg-sunken);
    border-top: 1px solid var(--line-soft);
  }
  .translation {
    margin: 0;
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    font: 400 22px/32px var(--font);
    color: var(--text-primary);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 288px;
    overflow-y: auto;
    user-select: text;
  }
  .pending {
    animation: pulse 1s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }
  .error {
    margin: 0;
    padding: 4px 8px;
    font: var(--body);
    color: var(--danger);
  }

  .variants {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 0 8px;
  }
  .variants-title {
    font: var(--caption-medium);
    color: var(--text-tertiary);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .chip {
    padding: 6px 12px;
    border-radius: var(--radius);
    background: var(--bg-field);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    font: var(--body);
    color: var(--text-secondary);
  }
  .chip:hover {
    background: var(--state-hover);
    color: var(--text-primary);
  }

  .foot {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 14px 14px;
    background: var(--bg-sunken);
    /* The card doesn't clip (menus hang out of it): round the last row. */
    border-radius: 0 0 15px 15px;
  }
  /* Without a result the footer sits on the surface, under a divider. */
  .input + .foot,
  .hist + .foot,
  .hlist + .foot {
    padding-top: 12px;
    border-top: 1px solid var(--line-soft);
  }
  .hint {
    font: var(--caption);
    color: var(--text-secondary);
  }
  /* kbd/Enter inside the primary button. */
  .enter {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 20px;
    margin-left: 8px;
    padding: 0 5px;
    border: 1px solid currentColor;
    border-bottom-width: 2px;
    border-radius: 4px;
  }
  .mini-primary {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius);
    background: var(--accent);
    color: var(--text-on-marker);
  }
  .mini-primary:hover {
    background: var(--accent-hover);
  }
  .mini-primary:disabled {
    background: var(--bg-disabled);
    color: var(--text-disabled);
  }

  /* ── History list ─────────────────────────────────────── */
  .hist {
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--line-soft);
  }
  .hhead {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px 4px 16px;
    font: var(--caption-medium);
    color: var(--text-secondary);
  }
  .hlist {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 4px 8px 8px;
    max-height: 340px;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--line-strong) transparent;
  }
  .hlist.short {
    max-height: 176px;
  }
  .hday {
    margin: 8px 12px 2px;
    font: var(--caption-medium);
    color: var(--text-tertiary);
  }
  .hrow {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 6px 4px 4px;
    border-radius: var(--radius);
  }
  .hrow:hover,
  .hrow.active {
    background: var(--state-hover);
  }
  .htext {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 4px 8px;
    text-align: left;
  }
  .htr {
    font: var(--body-medium);
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hsrc {
    font: var(--caption);
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hmeta {
    font: var(--caption);
    color: var(--text-secondary);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .empty {
    margin: 0;
    padding: 28px 16px;
    text-align: center;
    font: var(--body);
    color: var(--text-tertiary);
  }

  /* History only. */
  .hbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 12px 8px 16px;
    color: var(--text-secondary);
  }
  .htitle {
    font: var(--body-strong, 600 14px/20px var(--font));
    color: var(--text-primary);
  }
  .count {
    padding: 1px 7px;
    border-radius: 6px;
    background: var(--accent-soft);
    font: var(--caption-medium);
    color: var(--text-primary);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 230px;
    height: 32px;
    padding: 0 10px;
    border-radius: var(--radius);
    background: var(--bg-field);
    box-shadow: inset 0 0 0 1px var(--accent);
    color: var(--text-tertiary);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    padding: 0;
    background: none;
    font: var(--body);
    color: var(--text-primary);
    user-select: text;
  }
  .search input::placeholder {
    color: var(--text-tertiary);
  }
  .filters {
    display: flex;
    padding: 0 12px 8px 16px;
    border-bottom: 1px solid var(--line-soft);
  }
  .seg {
    display: inline-flex;
    gap: 2px;
    padding: 3px;
    border-radius: 9px;
    background: var(--bg-sunken);
    box-shadow: inset 0 0 0 1px var(--line-soft);
  }
  .opt {
    padding: 3px 11px;
    border-radius: 6px;
    font: var(--caption-medium);
    color: var(--text-secondary);
  }
  .opt:hover:not(.sel) {
    color: var(--text-primary);
  }
  .opt.sel {
    background: var(--ink-fill);
    color: var(--text-inverse);
  }

  /* ── Line ─────────────────────────────────────────────── */
  .line {
    width: 640px;
  }
  .lrow {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 14px 12px 14px 18px;
  }
  .lead {
    display: flex;
    padding-top: 5px;
    color: var(--text-secondary);
  }
  .line textarea {
    flex: 1;
    font: 400 17px/26px var(--font);
    max-height: 104px;
  }
  .lchip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 8px 0 10px;
    border-radius: var(--radius);
    background: var(--bg-field);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    font: var(--caption-medium);
    color: var(--text-secondary);
    white-space: nowrap;
  }
  .lchip:hover {
    color: var(--text-primary);
  }
  .lres {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 10px 10px 10px 12px;
    border-top: 1px solid var(--line-soft);
  }
  .ltr {
    flex: 1;
    margin: 0;
    padding: 5px 10px;
    border-radius: var(--radius);
    background: var(--accent-soft);
    font: 400 15px/24px var(--font);
    color: var(--text-primary);
    white-space: pre-wrap;
    overflow-wrap: break-word;
    max-height: 168px;
    overflow-y: auto;
    user-select: text;
  }
  .ltools {
    display: inline-flex;
    gap: 2px;
    padding-top: 2px;
  }
  .lfoot {
    padding: 0 16px 10px;
    font: var(--caption);
    color: var(--text-tertiary);
  }
</style>
