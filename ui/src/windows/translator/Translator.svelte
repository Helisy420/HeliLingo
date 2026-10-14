<!-- Переводчик (компакт) (Figma 16:3243 text, 48:180 images, 46:178 /
     46:252 / 46:313 files): languages, source with sentence highlights,
     streaming translation, engine, status bar. Sentences pair up between
     the panes: the one under the caret or the mouse is highlighted on both
     sides. The images and files views share the language bar and the
     status bar. -->
<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { onShown } from '../../lib/fit';
  import { engineName, t } from '../../lib/i18n.svelte';
  import { cancel, imageFile, inApp, netStatus, ocrImage, runAction, translate, type EngineError, type NetStatus, type TranslateEvent, copyText } from '../../lib/ipc';
  import { langName, sameLang } from '../../lib/langs';
  import { settings } from '../../lib/settings.svelte';
  import { sentences } from '../../lib/text';
  import Button from '../../lib/ui/Button.svelte';
  import EngineMenu from '../../lib/ui/EngineMenu.svelte';
  import FilesView from './FilesView.svelte';
  import ImagesView from './ImagesView.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import IconButton from '../../lib/ui/IconButton.svelte';
  import LangMenu from '../../lib/ui/LangMenu.svelte';
  import Logo from '../../lib/ui/Logo.svelte';
  import SelectButton from '../../lib/ui/SelectButton.svelte';

  const LIMIT = 5000;
  const DEBOUNCE_MS = 300;
  const s = $derived(t());

  type Tab = 'text' | 'image' | 'files';
  const startTab = new URLSearchParams(location.search).get('tab');
  let tab = $state<Tab>(startTab === 'image' || startTab === 'files' ? startTab : 'text');
  let source = $state(settings.sourceLang);
  let target = $state(settings.targetLang);
  let detected = $state<string | null>(null);
  let engine = $state('google');
  let preferred = $state<string | undefined>(undefined);
  let menu = $state<'source' | 'target' | 'engine' | null>(null);

  let input = $state('');
  let output = $state('');
  let busy = $state(false);
  let error = $state<EngineError | null>(null);
  let copied = $state(false);
  let active = $state(0);
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let field = $state<HTMLTextAreaElement>();
  let mirror = $state<HTMLDivElement>();

  /** Documents dropped on the text tab, for the files view. */
  let droppedFiles = $state<string[]>([]);

  // Images
  let ocrBusy = $state(false);
  let ocrError = $state('');

  const srcParts = $derived(sentences(input));
  const outParts = $derived(sentences(output));
  /** Sentence pairs line up only when both sides split the same way. */
  const paired = $derived(srcParts.length > 1 && srcParts.length === outParts.length);

  function onEvent(e: TranslateEvent) {
    switch (e.type) {
      case 'planned':
        if (e.sourceLang) detected = e.sourceLang;
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
        busy = false;
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
      void cancel('translator');
      output = '';
      busy = false;
      error = null;
      return;
    }
    busy = true;
    error = null;
    const mine = ++seq;
    await translate({ text, source, target, surface: 'translator', engine: preferred }, (e) => {
      if (mine === seq) onEvent(e);
    });
  }

  function onInput() {
    if (input.length > LIMIT) input = input.slice(0, LIMIT);
    clearTimeout(timer);
    timer = setTimeout(run, DEBOUNCE_MS);
    syncCaret();
  }

  /** The sentence under the caret becomes the highlighted pair. */
  function syncCaret() {
    const pos = field?.selectionStart ?? 0;
    let acc = 0;
    for (let i = 0; i < srcParts.length; i++) {
      acc += srcParts[i]!.length;
      if (pos < acc || i === srcParts.length - 1) {
        active = i;
        return;
      }
    }
    active = 0;
  }

  function swap() {
    const from = source === 'auto' ? (detected ?? 'en') : source;
    source = target;
    target = from;
    if (output) input = output;
    void run();
  }

  function pickLang(which: 'source' | 'target', code: string) {
    if (which === 'source') source = code;
    else target = code;
    menu = null;
    void run();
  }

  function pickEngine(id: string) {
    preferred = id;
    engine = id;
    menu = null;
    void run();
  }

  async function copy() {
    if (!output) return;
    await copyText(output);
    copied = true;
    setTimeout(() => (copied = false), 1200);
  }

  async function pasteText() {
    const text = await navigator.clipboard.readText().catch(() => '');
    if (!text) return;
    input = text.slice(0, LIMIT);
    void run();
  }

  function clearAll() {
    input = '';
    output = '';
    error = null;
    void run();
    field?.focus();
  }

  function speak(text: string, lang: string) {
    if (!text) return;
    speechSynthesis.cancel();
    const u = new SpeechSynthesisUtterance(text);
    u.lang = lang;
    const voice = speechSynthesis.getVoices().find((v) => sameLang(v.lang, lang.split('-')[0]!));
    if (voice) u.voice = voice;
    speechSynthesis.speak(u);
  }

  // Images: drop, paste, choose.
  async function useImage(file: Blob) {
    if (file.size > 10 * 1024 * 1024) {
      ocrError = s.translator.tooBig;
      return;
    }
    ocrError = '';
    ocrBusy = true;
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      const r = await ocrImage(bytes, source);
      const text = r.blocks.map((b) => b.text).join('\n');
      if (!text.trim()) {
        ocrError = s.translator.noText;
        return;
      }
      tab = 'text';
      input = text.slice(0, LIMIT);
      await tick();
      void run();
    } catch (e) {
      ocrError = typeof e === 'object' && e && 'kind' in e && e.kind === 'noLanguage' ? s.translator.noText : s.errors.other;
    } finally {
      ocrBusy = false;
    }
  }

  /** The images and files views (Figma 48:180, 46:313) need more height
      than the text view: grow the window for them, never shrink it. */
  const TALL = 566;
  $effect(() => {
    if (tab === 'text' || !inApp || innerHeight >= TALL) return;
    void import('@tauri-apps/api/window').then(({ getCurrentWindow, LogicalSize }) =>
      getCurrentWindow().setSize(new LogicalSize(innerWidth, TALL)),
    );
  });

  async function win() {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    return getCurrentWindow();
  }

  // A picture or a document dropped on the text tab opens its view.
  onMount(() => {
    if (!inApp) return;
    let off: (() => void) | undefined;
    void import('@tauri-apps/api/webview').then(async ({ getCurrentWebview }) => {
      off = await getCurrentWebview().onDragDropEvent(async (e) => {
        if (tab !== 'text' || e.payload.type !== 'drop') return;
        const paths = e.payload.paths;
        const image = paths.find((p) => /\.(png|jpe?g|webp|bmp|gif)$/i.test(p));
        if (image) {
          tab = 'image';
          const bytes = await imageFile(image);
          if (bytes) void useImage(new Blob([bytes as BlobPart]));
        } else if (paths.length) {
          droppedFiles = paths;
          tab = 'files';
        }
      });
    });
    return () => off?.();
  });

  /** Status line: refreshed every 5 s while the window is visible. */
  let net = $state<NetStatus>({ online: true, offlineReady: false });
  onMount(() => {
    const poll = () => {
      if (document.visibilityState === 'visible') void netStatus().then((n) => (net = n));
    };
    poll();
    const id = setInterval(poll, 5000);
    return () => clearInterval(id);
  });

  onMount(() => {
    const demo = new URLSearchParams(location.search).get('demo');
    if (demo) {
      input = demo;
      void run();
    }
    field?.focus();
    void onShown<string | null>('window:shown', (text) => {
      if (text) {
        input = text;
        void run();
      }
      field?.focus();
    });
  });

  function errorText(e: EngineError) {
    return s.errors[e.kind];
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && !menu && inApp) void win().then((w) => w.hide());
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="win">
  <header class="bar" data-tauri-drag-region>
    {#if tab !== 'text'}
      <button class="back" onclick={() => (tab = 'text')} aria-label={s.back}><Icon name="chev-l" /></button>
      <span class="title" data-tauri-drag-region>{tab === 'image' ? s.translator.imageTitle : s.files.title}</span>
    {:else}
      <Logo />
      <span class="brand" data-tauri-drag-region>HeliLingo</span>
    {/if}
    <span class="spacer" data-tauri-drag-region></span>
    <button class="wbtn" onclick={() => inApp && win().then((w) => w.minimize())} aria-label={s.minimize}>
      <Icon name="minus-14" size={14} />
    </button>
    <button class="wbtn close" onclick={() => inApp && win().then((w) => w.hide())} aria-label={s.closeWindow}>
      <Icon name="x-14" size={14} />
    </button>
  </header>

  <div class="langs">
    <span class="anchor" data-menu-anchor>
      <SelectButton open={menu === 'source'} onclick={() => (menu = menu === 'source' ? null : 'source')}>
        {langName(source === 'auto' ? (detected ?? 'en') : source)}
        {#if source === 'auto'}<span class="auto">{s.auto}</span>{/if}
      </SelectButton>
      {#if menu === 'source'}
        <LangMenu value={source} allowAuto onpick={(c) => pickLang('source', c)} onclose={() => (menu = null)} />
      {/if}
    </span>
    <button class="swap" onclick={swap} title={s.swap} aria-label={s.swap}><Icon name="swap" /></button>
    <span class="anchor" data-menu-anchor>
      <SelectButton open={menu === 'target'} onclick={() => (menu = menu === 'target' ? null : 'target')}>
        {langName(target)}
      </SelectButton>
      {#if menu === 'target'}
        <LangMenu value={target} align="right" onpick={(c) => pickLang('target', c)} onclose={() => (menu = null)} />
      {/if}
    </span>
  </div>

  {#if tab === 'text'}
    <section class="pane src">
      <div class="text-wrap">
        <div class="mirror" bind:this={mirror} aria-hidden="true">
          {#each srcParts as part, i (i)}<span class="sent" class:on={i === active && (paired || srcParts.length === 1) && !!output}
              >{part}</span
            >{/each}{' '}
        </div>
        <textarea
          bind:this={field}
          bind:value={input}
          oninput={onInput}
          onclick={syncCaret}
          onkeyup={syncCaret}
          onscroll={() => mirror && field && (mirror.scrollTop = field.scrollTop)}
          placeholder={s.typeHere}
          spellcheck="false"
          maxlength={LIMIT}
        ></textarea>
      </div>
      <div class="actions">
        <IconButton icon="speaker" label={s.speak} disabled={!input} onclick={() => speak(input, detected ?? 'en')} />
        <IconButton icon="clip" label={s.paste} onclick={pasteText} />
        <IconButton icon="image" label={s.translator.image} onclick={() => (tab = 'image')} />
        <IconButton icon="file" label={s.translator.files} onclick={() => (tab = 'files')} />
        <IconButton icon="x" label={s.clear} disabled={!input} onclick={clearAll} />
        <span class="spacer"></span>
        <span class="count">{input.length.toLocaleString('ru-RU')} / 5 000</span>
      </div>
    </section>

    <section class="pane out">
      <div class="text out-text" class:pending={busy && !output}>
        {#if error}
          <p class="error">{errorText(error)}</p>
        {:else}
          {#each outParts as part, i (i)}<span
              class="sent"
              class:on={i === active && (paired || outParts.length === 1)}
              role="presentation"
              onmouseenter={() => paired && (active = i)}>{part}</span
            >{/each}
        {/if}
      </div>
      <div class="actions out-actions">
        <span class="anchor up" data-menu-anchor>
          <SelectButton size="sm" width={116} open={menu === 'engine'} onclick={() => (menu = menu === 'engine' ? null : 'engine')}>
            {engineName(engine)}
          </SelectButton>
          {#if menu === 'engine'}
            <EngineMenu value={preferred ?? engine} align="left" onpick={pickEngine} onclose={() => (menu = null)} />
          {/if}
        </span>
        <span class="spacer"></span>
        <IconButton icon="speaker" label={s.speak} disabled={!output} onclick={() => speak(output, target)} />
        <Button variant="primary" size="sm" icon="copy" disabled={!output} onclick={copy}>{copied ? s.copied : s.copy}</Button>
      </div>
    </section>
  {:else if tab === 'image'}
    <ImagesView busy={ocrBusy} error={ocrError} onimage={(img) => void useImage(img)} />
  {:else}
    <FilesView {source} {target} initial={droppedFiles} />
  {/if}

  <footer class="status">
    <IconButton icon="history" label={s.history} onclick={() => runAction('history')} />
    <IconButton icon="gear" label={s.settings} onclick={() => runAction('settings')} />
    <span class="spacer"></span>
    <span class="dot" class:ok={net.online || net.offlineReady}></span>
    <span class="status-text">
      {net.online ? s.online : s.noNetwork}{net.offlineReady ? ` · ${s.offlineReady}` : ''}
    </span>
  </footer>
</div>

<style>
  .win {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg-surface);
  }
  /* Compact title bar: 40 px, 44 px buttons. */
  .bar {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 40px;
    padding-left: 16px;
    flex-shrink: 0;
  }
  .bar:has(.back) {
    padding-left: 8px;
  }
  .brand {
    font: var(--body-medium);
    color: var(--text-secondary);
  }
  .title {
    font: var(--body-medium);
    color: var(--text-primary);
  }
  .back {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius);
    color: var(--text-primary);
  }
  .back:hover {
    background: var(--state-hover);
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .wbtn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 40px;
    color: var(--text-secondary);
  }
  .wbtn:hover {
    background: var(--state-hover);
    color: var(--text-primary);
  }
  .close:hover {
    background: var(--danger);
    color: var(--text-on-danger);
  }

  .langs {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 56px;
    padding: 0 16px;
    flex-shrink: 0;
  }
  .anchor {
    position: relative;
    display: flex;
    flex: 1;
    min-width: 0;
  }
  .anchor :global(.sel) {
    flex: 1;
    min-width: 0;
  }
  .auto {
    padding: 1px 7px;
    border-radius: 5px;
    background: var(--accent-soft);
    font: var(--caption-medium);
  }
  .swap {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: 18px;
    background: var(--bg-surface);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    color: var(--text-primary);
    flex-shrink: 0;
  }
  .swap:hover {
    background: var(--state-hover);
  }

  .pane {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .out {
    background: var(--bg-sunken);
    border-top: 1px solid var(--line-soft);
  }
  /* Input/Compact: 18/26. The mirror sits under the transparent textarea
     with the same text, so sentence highlights line up. */
  .text-wrap {
    position: relative;
    flex: 1;
    min-height: 0;
    margin: 4px 12px 0;
  }
  .mirror,
  textarea,
  .text {
    font: 400 18px/26px var(--font);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    padding: 3px 8px;
  }
  .mirror {
    position: absolute;
    inset: 0;
    color: transparent;
    overflow: hidden;
    pointer-events: none;
  }
  textarea {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    border: 0;
    margin: 0;
    resize: none;
    background: transparent;
    color: var(--text-primary);
    caret-color: var(--text-primary);
    user-select: text;
  }
  textarea::placeholder {
    color: var(--text-tertiary);
  }
  .sent {
    border-radius: var(--radius-sm);
    box-decoration-break: clone;
    -webkit-box-decoration-break: clone;
  }
  .sent.on {
    background: var(--accent-soft);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .text {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    margin: 8px 12px 0;
    color: var(--text-primary);
    user-select: text;
  }
  .out-text.pending {
    animation: pulse 1s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }
  .error {
    margin: 0;
    font: var(--body);
    color: var(--danger);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 44px;
    padding: 0 12px;
    flex-shrink: 0;
  }
  .out-actions {
    gap: 6px;
    height: 52px;
    padding: 0 12px 0 16px;
  }
  .out-actions .anchor {
    flex: 0 0 auto;
  }
  .anchor.up :global(.menu) {
    top: auto;
    bottom: calc(100% + 6px);
  }
  .count {
    font: var(--caption);
    color: var(--text-tertiary);
  }

  .status {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 40px;
    padding: 0 16px 0 10px;
    border-top: 1px solid var(--line-soft);
    flex-shrink: 0;
  }
  .status :global(.ib) {
    width: 30px;
    height: 30px;
  }
  .dot {
    width: 8px;
    height: 8px;
    margin-right: 6px;
    border-radius: 50%;
    background: var(--text-tertiary);
  }
  .dot.ok {
    background: var(--status-ok);
  }
  .status-text {
    font: var(--caption);
    color: var(--text-secondary);
  }
</style>
