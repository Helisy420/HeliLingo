<!-- The double-Ctrl bubble. It never takes focus — the source app keeps its
     selection — so Esc and Ctrl+C reach it through the keyboard hook, not
     the webview. What it looks like is the `Bubble` component and the
     user's choices in Settings → «Баблы и Вики»; this page feeds it live
     data and does the window work:

     - the page reports its size and the bubble's place in it
       (`popup_layout`); Rust places the window by the selection, the
       pointer, the screen centre or the pinned spot (after «Задержка
       появления») and shows it without activating it;
     - auto-close after N seconds, paused while the pointer is over it;
     - for a single word: Wikipedia and example translations load after
       the translation, without holding it up. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import Bubble from '../../lib/bubble/Bubble.svelte';
  import { EMPTY, isWordLike, type BubbleData, type BubbleLook, type WikiState, wantsDict, wantsWiki } from '../../lib/bubble/types';
  import { onShown } from '../../lib/fit';
  import { t } from '../../lib/i18n.svelte';
  import {
    cancel,
    copyText,
    inApp,
    openSettings,
    openUrl,
    popupDrag,
    popupHide,
    popupPin,
    popupPinned,
    replaceSelection,
    translate,
    translateQuiet,
    wikiLookup,
    type TranslateEvent,
  } from '../../lib/ipc';
  import { sameLang } from '../../lib/langs';
  import { settings } from '../../lib/settings.svelte';

  /** What Rust sends with `popup:show`. */
  type Show = { text: string } | { error: 'nothingSelected' };

  const s = $derived(t());
  let card = $state<HTMLElement>();
  let data = $state<BubbleData>({ ...EMPTY });
  let wiki = $state<WikiState>('off');
  let pinned = $state(false);
  let winked = $state(false);
  /** Bumped per show: replays the appear animation. */
  let showId = $state(0);
  let seq = 0;
  /** The first layout of a show waits «Задержка появления». */
  let firstLayout = true;

  const look = $derived<BubbleLook>({
    style: settings.popupStyle === 'compact' ? 'translation' : settings.popupStyle,
    collapseLong: settings.bubble.collapseLong,
    wordMode: settings.bubble.wordMode,
    pos: settings.bubble.showPos,
    variants: settings.bubble.showVariants,
    wiki: settings.showWiki,
    wikiImage: settings.bubble.wikiImage,
    fontSize: settings.bubble.fontSize,
    opacity: settings.bubble.opacity,
    animation: settings.bubble.animation && settings.animations,
  });

  /** Words get Wikipedia by themselves when «Для одиночных слов» says so. */
  const autoWiki = $derived(look.wiki && wantsWiki(look.wordMode));

  function onEvent(e: TranslateEvent, mine: number) {
    if (mine !== seq) return;
    switch (e.type) {
      case 'planned':
        data.target = e.target;
        if (e.sourceLang) data.sourceLang = e.sourceLang;
        break;
      case 'started':
        data.engine = e.engine;
        data.text = '';
        break;
      case 'delta':
        data.text += e.text;
        break;
      case 'replace':
        data.engine = e.engine;
        data.text = e.text;
        break;
      case 'done':
        data.text = e.text;
        data.alternatives = e.alternatives.slice(0, 6);
        data.dict = e.dict;
        data.transcription = e.transcription;
        data.examples = e.examples.map((text) => ({ text, tr: null }));
        if (e.sourceLang) data.sourceLang = e.sourceLang;
        data.busy = false;
        if (settings.readAloud) speak();
        void extras(mine);
        break;
      case 'error':
        data.error = e.kind;
        data.busy = false;
        break;
    }
  }

  /** After the translation: Wikipedia for a word, and the examples'
      translations (one quiet request for all of them). */
  async function extras(mine: number) {
    if (!isWordLike(data.source)) return;
    if (autoWiki) void loadWiki(mine);
    const ex = data.examples;
    if (ex.length && wantsDict(settings.bubble.wordMode) && data.sourceLang) {
      try {
        const joined = await translateQuiet(ex.map((x) => x.text).join('\n'), data.sourceLang, data.target);
        const lines = joined.split('\n');
        if (mine === seq) data.examples = ex.map((x, i) => ({ text: x.text, tr: lines[i]?.trim() || null }));
      } catch {
        // Examples stay untranslated.
      }
    }
  }

  async function loadWiki(mine: number) {
    wiki = 'loading';
    const r = await wikiLookup(data.source, data.sourceLang, data.text || null, data.target);
    if (mine === seq && wiki === 'loading') wiki = r;
  }

  async function show(msg: Show) {
    const mine = ++seq;
    winked = false;
    wiki = 'off';
    firstLayout = true;
    showId++;
    data = { ...EMPTY, target: settings.targetLang };
    pinned = await popupPinned();
    if ('error' in msg) {
      data.notice = s.popup.nothingSelected;
      return;
    }
    data.source = msg.text;
    data.busy = true;
    await translate({ text: msg.text, source: settings.sourceLang, target: data.target, surface: 'popup' }, (e) =>
      onEvent(e, mine),
    );
  }

  async function close() {
    seq++;
    void cancel('popup');
    await popupHide();
  }

  /** Tells Rust the page's size and where the bubble is in it. */
  async function layout() {
    if (!inApp || !card) return;
    if (firstLayout) {
      firstLayout = false;
      const delay = settings.bubble.delayMs;
      if (delay > 0) await new Promise((r) => setTimeout(r, delay));
    }
    const { invoke } = await import('@tauri-apps/api/core');
    const pad = getComputedStyle(document.body);
    const r = card.getBoundingClientRect();
    await invoke('popup_layout', {
      layout: {
        width: Math.ceil(r.right + parseFloat(pad.paddingRight)),
        height: Math.ceil(r.bottom + parseFloat(pad.paddingBottom)),
        card_x: r.left,
        card_y: r.top,
        card_w: r.width,
        card_h: r.height,
      },
    });
    armAutoClose();
  }

  async function copy(text?: string) {
    const t = text ?? data.text;
    if (!t) return;
    await copyText(t);
    winked = false;
    requestAnimationFrame(() => (winked = true));
    setTimeout(() => (winked = false), 800);
  }

  async function replace(text?: string) {
    const t = text ?? data.text;
    if (!t) return;
    void cancel('popup');
    await replaceSelection(t);
  }

  function speak() {
    const text = data.text;
    if (!text) return;
    speechSynthesis.cancel();
    const u = new SpeechSynthesisUtterance(text);
    u.lang = data.target;
    const voice = speechSynthesis.getVoices().find((v) => sameLang(v.lang, data.target.split('-')[0]!));
    if (voice) u.voice = voice;
    speechSynthesis.speak(u);
  }

  async function togglePin() {
    pinned = !pinned;
    await popupPin(pinned);
  }

  function google() {
    const sl = data.sourceLang ?? 'auto';
    void openUrl(`https://translate.google.com/?sl=${sl}&tl=${data.target}&text=${encodeURIComponent(data.source)}&op=translate`);
  }

  // ── Auto-close ──────────────────────────────────────────
  let closeTimer: ReturnType<typeof setTimeout> | undefined;
  let hovering = false;
  function armAutoClose() {
    clearTimeout(closeTimer);
    const secs = settings.bubble.autoCloseSecs;
    if (!secs || pinned || (hovering && settings.bubble.keepOnHover)) return;
    const mine = seq;
    closeTimer = setTimeout(() => mine === seq && void close(), secs * 1000);
  }

  /** The bubble's element changes with the style: re-observe it. */
  $effect(() => {
    if (!card) return;
    const ro = new ResizeObserver(() => void layout());
    ro.observe(card);
    return () => ro.disconnect();
  });

  onMount(async () => {
    const params = new URLSearchParams(location.search);
    const style = params.get('style');
    if (!inApp && (style === 'translation' || style === 'buttons' || style === 'full' || style === 'compact'))
      settings.popupStyle = style;
    const demo = params.get('demo');
    if (demo)
      void show({ text: demo }).then(() => {
        if (params.has('wiki')) setTimeout(() => void loadWiki(seq), 300);
      });
    await onShown<Show>('popup:show', (msg) => {
      void show(msg).then(() => requestAnimationFrame(() => void layout()));
      void layout();
    });
    // The hook forwards the keys the popup reacts to (it has no focus).
    await onShown<string>('popup:key', (key) => {
      if (key === 'escape') void close();
      if (key === 'copy')
        void copy().then(() => {
          // Let the wink play before the bubble goes.
          if (settings.popupCtrlC === 'copyClose') setTimeout(() => void close(), 260);
        });
    });
  });
</script>

<div
  class="stage"
  role="presentation"
  onmouseenter={() => {
    hovering = true;
    if (settings.bubble.keepOnHover) clearTimeout(closeTimer);
  }}
  onmouseleave={() => {
    hovering = false;
    armAutoClose();
  }}
>
  {#key showId}
    <div class="appear" class:animated={look.animation}>
      <Bubble
        {data}
        {look}
        {wiki}
        {pinned}
        {winked}
        bind:card
        actions={{
          copy: (text) => void copy(text),
          replace: (text) => void replace(text),
          speak,
          close: () => void close(),
          pin: () => void togglePin(),
          wiki: () => (wiki === 'off' ? void loadWiki(seq) : (wiki = 'off')),
          openUrl: (url) => void openUrl(url),
          settings: () => void openSettings('bubbles'),
          google,
          drag: () => void popupDrag(),
        }}
      />
    </div>
  {/key}
</div>

<style>
  :global(html),
  :global(body) {
    background: transparent;
  }
  :global(body) {
    padding: 4px 20px 32px;
  }
  .stage {
    width: max-content;
  }
  .appear.animated {
    animation: appear 160ms cubic-bezier(0.2, 0.8, 0.3, 1);
    transform-origin: top left;
  }
  @keyframes appear {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .appear.animated {
      animation: none;
    }
  }
</style>
