<!-- «Перевод области экрана»: a transparent overlay over the monitor under
     the cursor (Figma 48:297 picking, 49:192 / 49:244 / 49:300 results;
     «Субтитры», pinning and the options aren't in Figma — composed from
     the same bar, card and tokens).

     Picking (mode bar at the bottom): «Область» — drag a rectangle, then
     move it or pull its handles; «Окно» — the window under the pointer
     lights up, a click takes it; «Весь экран». Enter (or a double click)
     translates, Esc or a right click cancels.

     Result: Rust copies the rectangle and reads its text; the blocks are
     translated in one batch and drawn over the real screen —
     «Поверх» (blocks over the original, backdrop opacity), «Плашка» (one
     card), «Рядом» (numbered markers and a list), «Субтитры» (a line at
     the bottom of the screen). The card, the list, the subtitles and the
     bar itself can be dragged; the list and the subtitles resized.
     Holding Alt shows the original; Esc, × or a click outside closes.

     Pinned (📌): the area stays and is read again whenever its picture
     changes — a game's dialogue box. The overlay then lets every click
     through except on its own controls, and the game keeps focus. The
     frame can be hidden. ⚙ picks the text language, the target, the
     translator (offline models too) and the recogniser for areas.

     Browser previews: ?demo=area | window | over | plate | side | subtitles. -->
<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { onShown } from '../../lib/fit';
  import { engineName, t } from '../../lib/i18n.svelte';
  import {
    copyText,
    engines as listEngines,
    inApp,
    offlineStatus,
    translateBatch,
    type EngineInfo,
    type OcrResult,
  } from '../../lib/ipc';
  import { settings, updateSettings, type ImagesSettings } from '../../lib/settings.svelte';
  import Choice from '../../lib/ui/Choice.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import IconButton from '../../lib/ui/IconButton.svelte';
  import Keys from '../../lib/ui/Keys.svelte';
  import LangChoice from '../../lib/ui/LangChoice.svelte';
  import Slider from '../../lib/ui/Slider.svelte';

  type Rect = { x: number; y: number; w: number; h: number };
  type Mode = 'area' | 'window' | 'screen';
  type Block = { text: string; tr: string; rect: Rect; size: number };
  type View = ImagesSettings['mode'];
  type LiveEvent = { type: 'text'; ocr: OcrResult } | { type: 'error'; message: string };

  const s = $derived(t());
  const a = $derived(s.area);

  let phase = $state<'pick' | 'result'>('pick');
  let mode = $state<Mode>('area');
  let windows = $state<Rect[]>([]);
  let sel = $state<Rect | null>(null);
  let pointer = $state<{ x: number; y: number } | null>(null);

  // Result.
  let area = $state<Rect>({ x: 0, y: 0, w: 0, h: 0 });
  let status = $state<'reading' | 'translating' | 'done' | 'empty' | 'failed'>('reading');
  let blocks = $state<Block[]>([]);
  let engine = $state('');
  let pair = $state('');
  let view = $state<View>('over');
  let alt = $state(false);
  let count = 0;
  let areaNo = $state(1);
  let pinned = $state(false);
  let liveNote = $state('');
  let frameHidden = $state(false);
  let optionsOpen = $state(false);
  let graniteReady = $state(false);
  let engineList = $state<EngineInfo[]>([]);
  /** Moved / resized by the user (null = the default place). */
  let barPos = $state<{ x: number; y: number } | null>(null);
  let sidePos = $state<Rect | null>(null);
  let platePos = $state<{ x: number; y: number } | null>(null);
  let subPos = $state<Rect | null>(null);

  const scale = () => window.devicePixelRatio || 1;
  const code = (c: string | null | undefined) => (c ?? '?').split('-')[0]!.toUpperCase();
  const srcLang = $derived(settings.images.sourceLang || 'auto');
  const dstLang = $derived(settings.images.targetLang || settings.targetLang);

  // ── Picking ───────────────────────────────────────────────────────────

  /** What Enter would translate now. */
  const target = $derived.by<Rect | null>(() => {
    if (mode === 'screen') return { x: 0, y: 0, w: innerWidth, h: innerHeight };
    if (mode === 'window') return hovered;
    return sel && sel.w >= 8 && sel.h >= 8 ? sel : null;
  });

  const hovered = $derived.by<Rect | null>(() => {
    if (mode !== 'window' || !pointer) return null;
    const p = pointer;
    return windows.find((w) => p.x >= w.x && p.x < w.x + w.w && p.y >= w.y && p.y < w.y + w.h) ?? null;
  });

  type Drag =
    | { kind: 'new'; x: number; y: number }
    | { kind: 'move'; dx: number; dy: number }
    | { kind: 'resize'; edges: string; from: Rect; x: number; y: number };
  let drag: Drag | null = null;

  const HANDLES = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'];

  function begin(start: { mode: Mode; windows: Rect[] }) {
    phase = 'pick';
    mode = start.mode;
    windows = start.windows;
    sel = null;
    blocks = [];
    alt = false;
    pinned = false;
    liveNote = '';
    optionsOpen = false;
    view = settings.images.mode;
  }

  async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T | undefined> {
    if (!inApp) return undefined;
    const core = await import('@tauri-apps/api/core');
    return core.invoke<T>(cmd, args);
  }

  function cancel() {
    sel = null;
    pinned = false;
    void invoke('capture_cancel');
  }

  function down(e: PointerEvent) {
    if (e.button === 2) return cancel();
    if (e.button !== 0 || phase !== 'pick') return;
    const p = { x: e.clientX, y: e.clientY };
    if (mode === 'window') {
      if (hovered) void translate(hovered);
      return;
    }
    if (mode === 'screen') return;
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    const handle = (e.target as HTMLElement).dataset.handle;
    if (handle && sel) {
      drag = { kind: 'resize', edges: handle, from: { ...sel }, x: p.x, y: p.y };
    } else if (sel && p.x >= sel.x && p.x <= sel.x + sel.w && p.y >= sel.y && p.y <= sel.y + sel.h) {
      drag = { kind: 'move', dx: p.x - sel.x, dy: p.y - sel.y };
    } else {
      drag = { kind: 'new', x: p.x, y: p.y };
      sel = { x: p.x, y: p.y, w: 0, h: 0 };
    }
  }

  function move(e: PointerEvent) {
    const p = { x: e.clientX, y: e.clientY };
    pointer = p;
    if (!drag || !sel) return;
    if (drag.kind === 'new') {
      sel = { x: Math.min(drag.x, p.x), y: Math.min(drag.y, p.y), w: Math.abs(p.x - drag.x), h: Math.abs(p.y - drag.y) };
    } else if (drag.kind === 'move') {
      sel = {
        ...sel,
        x: Math.min(Math.max(0, p.x - drag.dx), innerWidth - sel.w),
        y: Math.min(Math.max(0, p.y - drag.dy), innerHeight - sel.h),
      };
    } else {
      const f = drag.from;
      let [l, tp, r, b] = [f.x, f.y, f.x + f.w, f.y + f.h];
      const dx = p.x - drag.x;
      const dy = p.y - drag.y;
      if (drag.edges.includes('w')) l += dx;
      if (drag.edges.includes('e')) r += dx;
      if (drag.edges.includes('n')) tp += dy;
      if (drag.edges.includes('s')) b += dy;
      sel = { x: Math.min(l, r), y: Math.min(tp, b), w: Math.abs(r - l), h: Math.abs(b - tp) };
    }
  }

  function up() {
    if (drag?.kind === 'new' && sel && (sel.w < 8 || sel.h < 8)) sel = null;
    drag = null;
  }

  // ── Result ────────────────────────────────────────────────────────────

  async function translate(r: Rect) {
    area = { ...r };
    phase = 'result';
    status = 'reading';
    blocks = [];
    areaNo = ++count;
    sidePos = platePos = subPos = null;
    pair = `${code(srcLang === 'auto' ? null : srcLang)} → ${code(dstLang)}`;
    await tick();
    let ocr: OcrResult;
    try {
      ocr = inApp ? (await invoke<OcrResult>('capture_area', { ...r, lang: srcLang }))! : demoOcr(r);
    } catch {
      status = 'failed';
      return;
    }
    await show(ocr);
  }

  /** Recognised text → translated blocks. The old ones stay on screen
      until the new translation is in (no flicker while pinned). */
  async function show(ocr: OcrResult) {
    const k = inApp ? scale() : 1;
    const found = ocr.blocks.filter((b) => b.text.trim());
    if (!found.length) {
      if (!pinned) status = 'empty';
      return;
    }
    const next: Block[] = found.map((b) => ({
      text: b.text,
      tr: '',
      rect: { x: area.x + b.bounds.x / k, y: area.y + b.bounds.y / k, w: b.bounds.w / k, h: b.bounds.h / k },
      size: Math.max(11, Math.min(40, (b.lineHeight / k) * 0.78)),
    }));
    if (status !== 'done') status = 'translating';
    try {
      const res = await translateBatch(
        found.map((b) => b.text),
        srcLang,
        dstLang,
        settings.images.engine,
      );
      next.forEach((b, i) => (b.tr = res.texts[i] ?? ''));
      blocks = next;
      engine = res.engine;
      const from = srcLang === 'auto' ? (res.sourceLang ?? ocr.language) : srcLang;
      pair = `${code(from)} → ${code(dstLang)}`;
      status = 'done';
    } catch {
      if (!pinned) status = 'failed';
    }
  }

  /** The same text again with the new language / translator. */
  async function retranslate() {
    if (!blocks.length) return;
    try {
      const res = await translateBatch(
        blocks.map((b) => b.text),
        srcLang,
        dstLang,
        settings.images.engine,
      );
      blocks.forEach((b, i) => (b.tr = res.texts[i] ?? b.tr));
      engine = res.engine;
      pair = `${code(srcLang === 'auto' ? res.sourceLang : srcLang)} → ${code(dstLang)}`;
    } catch {
      // Keep what's shown.
    }
  }

  async function togglePin() {
    if (pinned) {
      pinned = false;
      liveNote = '';
      await invoke('capture_unpin');
      return;
    }
    if (!inApp) {
      pinned = true;
      return;
    }
    const { Channel } = await import('@tauri-apps/api/core');
    const channel = new Channel<LiveEvent>();
    channel.onmessage = (e) => {
      if (e.type === 'text') void show(e.ocr);
    };
    try {
      await invoke('capture_live', { area, lang: srcLang, onEvent: channel });
      pinned = true;
      optionsOpen = false;
      liveNote = '';
    } catch {
      liveNote = a.liveUnsupported;
    }
  }

  function setView(v: View) {
    view = v;
    void updateSettings({ images: { mode: v } });
  }

  const setI = (patch: Partial<ImagesSettings>) => updateSettings({ images: patch });

  async function openOptions() {
    optionsOpen = !optionsOpen;
    if (!optionsOpen) return;
    engineList = (await listEngines()).filter((e) => e.enabled && e.implemented);
    const st = await offlineStatus().catch(() => null);
    graniteReady = !!st?.models.some((m) => m.id === 'granite' && m.status.state === 'installed');
  }

  const all = $derived(blocks.map((b) => b.tr).join('\n'));
  const subtitle = $derived(blocks.map((b) => b.tr).join(' '));

  function speak(text: string) {
    if (!text) return;
    speechSynthesis.cancel();
    const u = new SpeechSynthesisUtterance(text);
    u.lang = dstLang;
    speechSynthesis.speak(u);
  }

  /** «Поверх»: each block covers its original, widened to the area's
      right edge; it may grow down to the next block, no further. */
  const covers = $derived(
    blocks.map((b, i) => {
      const x = b.rect.x - 4;
      const y = b.rect.y - 3;
      const below = blocks
        .filter((o, k) => k !== i && o.rect.y > b.rect.y + b.rect.h / 2 && o.rect.x < x + area.w && o.rect.x + o.rect.w > x)
        .map((o) => o.rect.y - 3);
      const limit = Math.min(area.y + area.h + 6, ...below) - y - 2;
      return { x, y, w: b.rect.w + 8, wide: area.x + area.w - x - 4, h: b.rect.h + 6, max: Math.max(b.rect.h + 6, limit) };
    }),
  );

  /** The translation keeps the original's size when it fits; otherwise
      the block widens to the area's edge, then the text shrinks (down to
      70 %), then the block grows to `max`, then the text shrinks more. */
  function fit(el: HTMLElement, o: { size: number; w: number; wide: number; h: number; max: number; text: string }) {
    const run = ({ size, w, wide, h, max }: typeof o) => {
      const box = el.parentElement!;
      const over = () => el.scrollHeight > box.clientHeight - 2 || el.scrollWidth > el.clientWidth + 1;
      box.style.width = `${w}px`;
      box.style.height = `${h}px`;
      let px = size;
      el.style.fontSize = `${px}px`;
      if (over()) box.style.width = `${Math.max(w, wide)}px`;
      while (px > Math.max(10, size * 0.7) && over()) el.style.fontSize = `${--px}px`;
      if (over()) {
        box.style.height = `${Math.min(max, Math.max(h, el.scrollHeight + 4))}px`;
        while (px > 9 && over()) el.style.fontSize = `${--px}px`;
      }
    };
    run(o);
    return { update: run };
  }

  // Default places: the card on the area, the list beside it, subtitles
  // at the bottom of the screen; dragging overrides them.
  const plate = $derived.by(() => {
    const w = Math.min(Math.max(area.w, 360), innerWidth - 24);
    return platePos
      ? { ...platePos, w }
      : { x: Math.min(Math.max(12, area.x), innerWidth - w - 12), y: Math.max(12, area.y), w };
  });
  const side = $derived.by<Rect>(() => {
    if (sidePos) return sidePos;
    const w = 216;
    const right = area.x + area.w + 8;
    const x = right + w <= innerWidth - 8 ? right : Math.max(8, area.x - w - 8);
    return { x, y: Math.max(64, area.y - 32), w, h: 0 };
  });
  const sub = $derived.by<Rect>(() => {
    if (subPos) return subPos;
    const w = Math.min(innerWidth * 0.8, 1100);
    return { x: (innerWidth - w) / 2, y: innerHeight - settings.images.subtitleSize * 3 - 70, w, h: 0 };
  });

  /** Drags something by `grip` (moves `get()` by the pointer's travel). */
  function dragBy(e: PointerEvent, get: () => { x: number; y: number }, set: (x: number, y: number) => void) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const el = e.currentTarget as HTMLElement;
    const from = { px: e.clientX, py: e.clientY, ...get() };
    el.setPointerCapture(e.pointerId);
    const onMove = (ev: PointerEvent) => set(from.x + ev.clientX - from.px, from.y + ev.clientY - from.py);
    const onUp = () => {
      el.removeEventListener('pointermove', onMove);
      el.removeEventListener('pointerup', onUp);
    };
    el.addEventListener('pointermove', onMove);
    el.addEventListener('pointerup', onUp);
  }

  let barEl = $state<HTMLElement>();
  let sideEl = $state<HTMLElement>();
  function moveBar(e: PointerEvent) {
    const r = barEl!.getBoundingClientRect();
    dragBy(e, () => barPos ?? { x: r.left, y: r.top }, (x, y) => (barPos = { x, y }));
  }
  function moveSide(e: PointerEvent) {
    const r = sideEl!.getBoundingClientRect();
    dragBy(e, () => ({ x: side.x, y: side.y }), (x, y) => (sidePos = { x, y, w: side.w, h: sidePos?.h || r.height }));
  }
  function resizeSide(e: PointerEvent) {
    const r = sideEl!.getBoundingClientRect();
    dragBy(
      e,
      () => ({ x: r.width, y: r.height }),
      (w, h) => (sidePos = { x: side.x, y: side.y, w: Math.max(180, w), h: Math.max(140, h) }),
    );
  }
  function movePlate(e: PointerEvent) {
    dragBy(e, () => ({ x: plate.x, y: plate.y }), (x, y) => (platePos = { x, y }));
  }
  function moveSub(e: PointerEvent) {
    dragBy(e, () => ({ x: sub.x, y: sub.y }), (x, y) => (subPos = { ...sub, x, y }));
  }
  function resizeSub(e: PointerEvent) {
    dragBy(e, () => ({ x: sub.w, y: 0 }), (w) => (subPos = { ...sub, w: Math.max(240, w) }));
  }

  // Pinned: tell Rust where the controls are, so clicks there reach us
  // and every other click goes to the game.
  $effect(() => {
    if (!pinned || !inApp) return;
    const send = () => {
      const rects = [...document.querySelectorAll<HTMLElement>('[data-hit]')].map((el) => {
        const r = el.getBoundingClientRect();
        return { x: r.left, y: r.top, w: r.width, h: r.height };
      });
      void invoke('capture_hits', { rects });
    };
    send();
    const id = setInterval(send, 250);
    return () => clearInterval(id);
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Alt') {
      e.preventDefault();
      if (phase === 'result' && settings.images.altOriginal) alt = true;
      return;
    }
    if (e.key === 'Escape') {
      if (optionsOpen) optionsOpen = false;
      else cancel();
      return;
    }
    if (e.key === 'Enter' && phase === 'pick' && target) void translate(target);
  }

  function outside(e: PointerEvent) {
    if (e.target !== e.currentTarget || e.button !== 0) return;
    if (optionsOpen) optionsOpen = false;
    // A click on the bare screen closes a still result.
    else if (!pinned) cancel();
  }

  // ── Demo (browser screenshots) ─────────────────────────────────────────

  const demo = new URLSearchParams(location.search).get('demo');
  const DEMO_AREA: Rect = { x: 72, y: 104, w: 534, h: 190 };

  function demoOcr(r: Rect): OcrResult {
    return {
      language: 'en-US',
      width: r.w,
      height: r.h,
      blocks: [
        { text: 'Free shipping on orders over $50', bounds: { x: 35, y: 31, w: 382, h: 30 }, lines: 1, lineHeight: 30 },
        {
          text: 'Order today and get your package in 2–3 business days. Offer ends Sunday.',
          bounds: { x: 35, y: 71, w: 375, h: 40 },
          lines: 2,
          lineHeight: 18,
        },
        { text: 'Add to cart', bounds: { x: 35, y: 117, w: 112, h: 40 }, lines: 1, lineHeight: 18 },
      ],
    };
  }

  onMount(() => {
    if (!inApp && demo) {
      if (demo === 'area') begin({ mode: 'area', windows: [] });
      if (demo === 'area') sel = { x: 96, y: 88, w: 548, h: 236 };
      if (demo === 'window') {
        begin({ mode: 'window', windows: [{ x: 60, y: 36, w: 760, h: 420 }] });
        pointer = { x: 300, y: 200 };
      }
      if (demo === 'over' || demo === 'plate' || demo === 'side' || demo === 'subtitles') {
        void updateSettings({ images: { mode: demo } });
        view = demo;
        void translate(DEMO_AREA).then(() => {
          if (new URLSearchParams(location.search).has('pinned')) pinned = true;
          if (new URLSearchParams(location.search).has('options')) void openOptions();
        });
      }
    }
    // Listen first, then ask: the first start was sent while this page
    // was still loading.
    void onShown<{ mode: Mode; windows: Rect[] }>('capture:start', begin).then(async () => {
      const last = await invoke<{ mode: Mode; windows: Rect[] } | null>('capture_state');
      if (last) begin(last);
    });
  });
</script>

<svelte:window
  onkeydown={onKey}
  onkeyup={(e) => e.key === 'Alt' && (alt = false)}
  onblur={() => (alt = false)}
  oncontextmenu={(e) => e.preventDefault()}
/>

{#if demo}
  <!-- A stand-in "app" under the overlay, like the Figma frames. -->
  <div class="fake">
    <div class="f-top"></div>
    <div class="f-side"><i></i><i></i><i></i><i></i></div>
    <div class="f-page" style:left="{phase === 'pick' ? 109 : 79}px" style:top="{phase === 'pick' ? 103 : 111}px">
      <b>Free shipping on orders over $50</b>
      <span>Order today and get your package in 2–3 business days. Offer ends Sunday.</span>
      <em>Add to cart</em>
    </div>
  </div>
{/if}

{#if phase === 'pick'}
  <div
    class="overlay"
    class:crosshair={mode === 'area'}
    class:dim={mode === 'area' ? !sel : mode === 'window' ? !hovered : false}
    role="application"
    aria-label={s.capture.hint}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    ondblclick={() => target && translate(target)}
  >
    {#if mode === 'area' && sel}
      <div class="sel" style:left="{sel.x}px" style:top="{sel.y}px" style:width="{sel.w}px" style:height="{sel.h}px">
        <span class="size">{Math.round(sel.w * scale())} × {Math.round(sel.h * scale())}</span>
        {#if sel.w >= 8 && sel.h >= 8}
          {#each HANDLES as h (h)}<span class="handle {h}" data-handle={h}></span>{/each}
        {/if}
      </div>
    {:else if mode === 'window' && hovered}
      <div class="sel win" style:left="{hovered.x}px" style:top="{hovered.y}px" style:width="{hovered.w}px" style:height="{hovered.h}px">
        <span class="size">{Math.round(hovered.w * scale())} × {Math.round(hovered.h * scale())}</span>
      </div>
    {:else if mode === 'screen'}
      <div class="sel screen"></div>
    {/if}

    <div class="bar pick-bar" role="toolbar" tabindex="-1" onpointerdown={(e) => e.stopPropagation()}>
      <span class="seg">
        {#each [['area', 'scan', a.area], ['window', 'appwin-18', a.window], ['screen', 'monitor', a.screen]] as [m, icon, label] (m)}
          <button class="opt" class:on={mode === m} onclick={() => ((mode = m as Mode), (sel = null))}>
            <Icon name={icon!} size={icon === 'appwin-18' ? 18 : 16} />{label}
          </button>
        {/each}
      </span>
      <span class="hints">
        {#if mode === 'window' && !hovered}<span class="hint-text">{a.pickWindow}</span>{/if}
        <button class="hint" onclick={cancel}><Keys shortcut="Esc" />{a.cancel}</button>
        <button class="hint" disabled={!target} onclick={() => target && translate(target)}><Keys shortcut="Enter" />{a.translate}</button>
      </span>
    </div>
  </div>
{:else}
  <div class="overlay result" role="presentation" onpointerdown={outside}>
    {#if !frameHidden}
      <div
        class="frame"
        class:solid={view === 'over'}
        style:left="{area.x - 8}px"
        style:top="{area.y - 8}px"
        style:width="{area.w + 16}px"
        style:height="{area.h + 16}px"
      ></div>
      <span class="label" style:left="{area.x - 8}px" style:top="{area.y - 36}px">
        {#if pinned}<span class="live-dot"></span>{/if}
        {status === 'reading' ? a.recognizing : status === 'translating' ? a.translatingNow : a.label(areaNo, pair)}
      </span>
    {/if}
    {#if (status === 'empty' || status === 'failed') && !pinned}
      <span class="label err" style:left="{area.x - 8}px" style:top="{area.y + area.h + 14}px">
        {status === 'empty' ? a.noText : a.failed}
      </span>
    {/if}

    {#if status === 'done' && !alt}
      {#if view === 'over'}
        {#each blocks as b, i (i)}
          {@const c = covers[i]!}
          <div
            class="cover"
            style:left="{c.x}px"
            style:top="{c.y}px"
            style:background="color-mix(in srgb, var(--bg-surface) {settings.images.backdropOpacity}%, transparent)"
          >
            <span
              class="cover-text"
              style:font-weight={b.size >= 18 ? 600 : 400}
              use:fit={{ size: b.size, w: c.w, wide: c.wide, h: c.h, max: c.max, text: b.tr }}>{b.tr}</span
            >
          </div>
        {/each}
      {:else if view === 'plate'}
        <div class="card plate" data-hit style:left="{plate.x}px" style:top="{plate.y}px" style:width="{plate.w}px">
          <header class="p-head grip" role="presentation" title={a.drag} onpointerdown={movePlate}>
            <span class="p-title">{a.translation} · {engineName(engine)}</span>
            <span class="spacer"></span>
            <span class="no-drag" role="presentation" onpointerdown={(e) => e.stopPropagation()}>
              <IconButton size="sm" icon="speaker" label={s.speak} onclick={() => speak(all)} />
              <IconButton size="sm" icon="copy" label={s.copy} onclick={() => copyText(all)} />
            </span>
          </header>
          {#each blocks as b, i (i)}
            {@const size = Math.round(Math.min(22, Math.max(14, 14 * (b.size / (blocks.map((x) => x.size).sort((p, q) => p - q)[Math.floor(blocks.length / 2)] ?? 14)))))}
            <p class="p-text" style:font-size="{size}px" class:strong={size >= 18}>{b.tr}</p>
          {/each}
        </div>
      {:else if view === 'side'}
        {#each blocks as b, i (i)}
          <span class="marker" style:left="{Math.max(area.x + 2, b.rect.x - 28)}px" style:top="{b.rect.y + Math.max(0, (b.rect.h - 22) / 2)}px"
            >{i + 1}</span
          >
        {/each}
        <div
          class="card side"
          data-hit
          bind:this={sideEl}
          style:left="{side.x}px"
          style:top="{side.y}px"
          style:width="{side.w}px"
          style:height={side.h ? `${side.h}px` : undefined}
        >
          <header class="s-head grip" role="presentation" title={a.drag} onpointerdown={moveSide}>
            <span class="s-title">{a.translation}</span>
            <span class="spacer"></span>
            <span class="s-count">{a.blocks(blocks.length)}</span>
          </header>
          <div class="s-list">
            {#each blocks as b, i (i)}
              <div class="s-row">
                <span class="num">{i + 1}</span>
                <span class="s-text">{b.tr}</span>
                <IconButton size="sm" icon="copy" label={s.copy} onclick={() => copyText(b.tr)} />
              </div>
            {/each}
          </div>
          <footer class="s-foot">
            <button class="primary" onclick={() => copyText(all)}><Icon name="copy" />{a.copyAll}</button>
            <span class="spacer"></span>
            <IconButton size="sm" icon="speaker" label={s.speak} onclick={() => speak(all)} />
          </footer>
          <span class="corner" role="presentation" title={a.resize} onpointerdown={resizeSide}></span>
        </div>
      {:else}
        <!-- «Субтитры»: one line at the bottom of the screen. -->
        <div
          class="subs"
          data-hit
          role="presentation"
          title={a.drag}
          style:left="{sub.x}px"
          style:top="{sub.y}px"
          style:width="{sub.w}px"
          style:font-size="{settings.images.subtitleSize}px"
          onpointerdown={moveSub}
        >
          {subtitle}
          <span class="sub-edge" role="presentation" title={a.resize} onpointerdown={resizeSub}></span>
        </div>
      {/if}
    {/if}

    <div
      class="bar top-bar"
      class:moved={!!barPos}
      data-hit
      bind:this={barEl}
      role="toolbar"
      tabindex="-1"
      style:left={barPos ? `${barPos.x}px` : undefined}
      style:top={barPos ? `${barPos.y}px` : undefined}
      onpointerdown={(e) => e.stopPropagation()}
    >
      <span class="grip-dots" role="presentation" title={a.drag} onpointerdown={moveBar}><i></i><i></i><i></i></span>
      <span class="seg">
        {#each [['over', 'layers', a.over], ['plate', 'plate', a.plate], ['side', 'columns', a.side], ['subtitles', 'minus-14', a.subtitles]] as [v, icon, label] (v)}
          <button class="opt" class:on={view === v} onclick={() => setView(v as View)}>
            <Icon name={icon!} />{label}
          </button>
        {/each}
      </span>
      {#if view === 'over'}
        <span class="opacity">
          <span class="hint-text">{a.opacity}</span>
          <Slider
            value={settings.images.backdropOpacity}
            min={30}
            max={100}
            step={5}
            width={70}
            label={a.opacity}
            onchange={(v) => setI({ backdropOpacity: v })}
          />
        </span>
      {/if}
      {#if settings.images.altOriginal && !pinned}
        <span class="hints"><Keys shortcut="Alt" /><span class="hint-text">{a.original}</span></span>
      {/if}
      <span class="tools">
        <IconButton size="sm" icon="refresh" label={a.translate} onclick={() => translate(area)} />
        <IconButton size="sm" icon="pin" active={pinned} label={pinned ? a.unpin : a.pin} onclick={togglePin} />
        <IconButton size="sm" icon="eye" active={frameHidden} label={frameHidden ? a.showFrame : a.hideFrame} onclick={() => (frameHidden = !frameHidden)} />
        <IconButton size="sm" icon="gear" active={optionsOpen} label={a.options} onclick={openOptions} />
        <IconButton size="sm" icon="x" label={s.closeWindow} onclick={cancel} />
      </span>
      {#if liveNote}<span class="note">{liveNote}</span>{/if}

      {#if optionsOpen}
        <div class="options" data-hit role="dialog" aria-label={a.options}>
          <div class="o-row">
            <span>{a.textLang}</span>
            <LangChoice value={srcLang} allowAuto width={230} onchange={(c) => setI({ sourceLang: c }).then(retranslate)} />
          </div>
          <div class="o-row">
            <span>{a.toLang}</span>
            <LangChoice value={dstLang} width={230} onchange={(c) => setI({ targetLang: c }).then(retranslate)} />
          </div>
          <div class="o-row">
            <span>{a.engine}</span>
            <Choice
              value={settings.images.engine ?? ''}
              width={230}
              options={[{ value: '', label: a.engineDefault }, ...engineList.map((e) => ({ value: e.id, label: e.name }))]}
              onchange={(v) => setI({ engine: v || null }).then(retranslate)}
            />
          </div>
          <div class="o-row">
            <span>{a.ocr}</span>
            <Choice
              value={graniteReady ? settings.offline.screenOcr : 'windows'}
              width={230}
              options={[
                { value: 'windows', label: a.ocrWindows },
                ...(graniteReady ? [{ value: 'granite', label: a.ocrGranite }] : []),
              ]}
              onchange={(v) => updateSettings({ offline: { screenOcr: v as 'windows' | 'granite' } })}
            />
          </div>
          {#if !graniteReady}<p class="o-note">{a.graniteMissing}</p>{/if}
          <div class="o-row">
            <span>{a.subtitleSize}</span>
            <Slider
              value={settings.images.subtitleSize}
              min={16}
              max={48}
              step={2}
              width={160}
              label={a.subtitleSize}
              onchange={(v) => setI({ subtitleSize: v })}
            />
            <span class="o-val">{settings.images.subtitleSize} px</span>
          </div>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  :global(html),
  :global(body) {
    background: transparent;
    margin: 0;
    overflow: hidden;
  }
  .overlay {
    position: fixed;
    inset: 0;
    user-select: none;
  }
  .crosshair {
    cursor: crosshair;
  }
  .dim {
    background: rgba(0, 0, 0, 0.55);
  }
  /* Clear inside, the rest dimmed by a huge shadow (Figma: 55 % black). */
  .sel {
    position: absolute;
    box-shadow: 0 0 0 100vmax rgba(0, 0, 0, 0.55);
    outline: 2px solid var(--accent);
    outline-offset: -1px;
    cursor: move;
  }
  .sel.win {
    cursor: pointer;
    pointer-events: none;
  }
  .sel.screen {
    inset: 0;
    box-shadow: none;
    outline-offset: -2px;
    pointer-events: none;
  }
  .size {
    position: absolute;
    left: 0;
    bottom: calc(100% + 6px);
    padding: 4px 8px;
    border-radius: 6px;
    background: var(--ink-fill);
    color: var(--text-inverse);
    font: var(--caption-medium);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    pointer-events: none;
  }
  .handle {
    position: absolute;
    width: 9px;
    height: 9px;
    margin: -4.5px 0 0 -4.5px;
    border: 2px solid var(--accent);
    border-radius: 2px;
    background: #fff;
    box-sizing: border-box;
  }
  .handle.nw { left: 0; top: 0; cursor: nwse-resize; }
  .handle.n { left: 50%; top: 0; cursor: ns-resize; }
  .handle.ne { left: 100%; top: 0; cursor: nesw-resize; }
  .handle.e { left: 100%; top: 50%; cursor: ew-resize; }
  .handle.se { left: 100%; top: 100%; cursor: nwse-resize; }
  .handle.s { left: 50%; top: 100%; cursor: ns-resize; }
  .handle.sw { left: 0; top: 100%; cursor: nesw-resize; }
  .handle.w { left: 0; top: 50%; cursor: ew-resize; }

  /* Bars (48:325, 49:361): surface, soft line, radius 12, Figma shadow. */
  .bar {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 12px 6px 8px;
    border-radius: 12px;
    border: 1px solid var(--line-soft);
    background: var(--bg-surface);
    filter: drop-shadow(0 2px 3px rgba(0, 0, 0, 0.12)) drop-shadow(0 16px 20px rgba(0, 0, 0, 0.2));
    cursor: default;
    white-space: nowrap;
  }
  .bar.moved {
    transform: none;
  }
  .pick-bar {
    bottom: 40px;
  }
  .top-bar {
    top: 13px;
    padding: 6px 6px 6px 4px;
  }
  .grip-dots {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 6px 4px;
    cursor: grab;
  }
  .grip-dots i {
    display: block;
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--text-tertiary);
  }
  .seg {
    display: flex;
    gap: 2px;
    padding: 2px;
    border-radius: 8px;
    border: 1px solid var(--line-soft);
    background: var(--bg-sunken);
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px 0 10px;
    border-radius: 6px;
    font: var(--caption-medium);
    color: var(--text-secondary);
  }
  .opt:hover:not(.on) {
    color: var(--text-primary);
  }
  .opt.on {
    background: var(--ink-fill);
    color: var(--text-inverse);
  }
  .hints,
  .opacity,
  .tools {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .tools {
    gap: 2px;
  }
  .hint {
    display: flex;
    align-items: center;
    gap: 6px;
    font: var(--caption);
    color: var(--text-secondary);
  }
  .hint:disabled {
    opacity: 0.5;
  }
  .hint-text {
    font: var(--caption);
    color: var(--text-secondary);
  }
  .note {
    position: absolute;
    top: calc(100% + 8px);
    left: 50%;
    transform: translateX(-50%);
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--bg-surface);
    border: 1px solid var(--line-soft);
    font: var(--caption);
    color: var(--danger);
  }

  /* ⚙ Area settings: a card hanging under the bar. */
  .options {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    display: flex;
    flex-direction: column;
    width: 410px;
    padding: 6px 0;
    border-radius: 12px;
    border: 1px solid var(--line-soft);
    background: var(--bg-surface);
    white-space: normal;
  }
  .o-row {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 0 12px 0 14px;
    font: var(--body-medium);
    color: var(--text-primary);
  }
  .o-row > span:first-child {
    flex: 1;
  }
  .o-row + .o-row {
    border-top: 1px solid var(--line-soft);
  }
  .o-note {
    margin: -6px 14px 6px;
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .o-val {
    min-width: 40px;
    text-align: right;
    font: var(--caption-medium);
    color: var(--text-secondary);
  }

  /* Result. */
  .result {
    background: transparent;
  }
  .frame {
    position: absolute;
    border: 1.5px dashed var(--accent);
    border-radius: 14px;
    pointer-events: none;
  }
  .label {
    position: absolute;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px;
    border-radius: 6px;
    border: 1px solid var(--line-soft);
    background: var(--bg-surface);
    font: var(--caption-medium);
    color: var(--text-secondary);
    white-space: nowrap;
    pointer-events: none;
  }
  .label.err {
    color: var(--danger);
  }
  .live-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--status-ok);
  }
  .cover {
    position: absolute;
    display: flex;
    align-items: center;
    box-sizing: border-box;
    padding: 2px 8px;
    border-radius: 6px;
    color: var(--text-primary);
    overflow: hidden;
    pointer-events: none;
  }
  .cover-text {
    display: block;
    width: 100%;
    line-height: 1.25;
    font-family: var(--font);
    overflow: hidden;
  }
  .card {
    position: absolute;
    display: flex;
    flex-direction: column;
    border-radius: 12px;
    border: 1px solid var(--line-soft);
    background: var(--bg-surface);
    filter: drop-shadow(0 2px 3px rgba(0, 0, 0, 0.12)) drop-shadow(0 16px 20px rgba(0, 0, 0, 0.2));
    color: var(--text-primary);
  }
  .grip {
    cursor: grab;
  }
  .no-drag {
    display: flex;
    gap: 2px;
    cursor: default;
  }
  .plate {
    gap: 10px;
    padding: 14px 20px 18px;
  }
  .p-head,
  .s-head,
  .s-foot {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .p-title {
    font: var(--caption);
    color: var(--text-secondary);
  }
  .p-text {
    margin: 0;
    font-family: var(--font);
    line-height: 1.45;
  }
  .p-text.strong {
    font-weight: 600;
    line-height: 1.3;
  }
  .spacer {
    flex: 1;
  }
  .marker,
  .num {
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 11px;
    background: var(--accent);
    font: var(--caption-medium);
    color: var(--text-on-marker);
  }
  .marker {
    position: absolute;
    width: 22px;
    height: 22px;
    pointer-events: none;
  }
  .num {
    flex-shrink: 0;
    width: 20px;
    height: 20px;
    border-radius: 10px;
  }
  .side {
    min-height: 140px;
  }
  .s-head {
    padding: 10px 10px 6px 14px;
  }
  .s-title {
    font: 600 14px/20px var(--font);
  }
  .s-count {
    font: var(--caption);
    color: var(--text-secondary);
  }
  .s-list {
    flex: 1;
    min-height: 0;
    max-height: calc(100vh - 220px);
    overflow-y: auto;
  }
  .s-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 6px 8px 14px;
    border-top: 1px solid var(--line-soft);
  }
  .s-text {
    flex: 1;
    min-width: 0;
    font: var(--body);
  }
  .s-foot {
    padding: 10px 10px 12px 14px;
    border-top: 1px solid var(--line-soft);
  }
  .corner {
    position: absolute;
    right: 2px;
    bottom: 2px;
    width: 12px;
    height: 12px;
    border-right: 2px solid var(--text-tertiary);
    border-bottom: 2px solid var(--text-tertiary);
    border-radius: 0 0 4px 0;
    cursor: nwse-resize;
  }
  .primary {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    border-radius: 6px;
    background: var(--accent);
    font: var(--caption-medium);
    color: var(--text-on-marker);
  }
  /* Subtitles: big, centred, a dark plate so they read over any game. */
  .subs {
    position: absolute;
    box-sizing: border-box;
    padding: 10px 22px 12px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--bg-surface) 82%, transparent);
    color: var(--text-primary);
    font-family: var(--font);
    font-weight: 500;
    line-height: 1.3;
    text-align: center;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.35);
    cursor: grab;
  }
  .sub-edge {
    position: absolute;
    top: 8px;
    bottom: 8px;
    right: 2px;
    width: 6px;
    border-radius: 3px;
    background: var(--line-strong);
    opacity: 0;
    cursor: ew-resize;
  }
  .subs:hover .sub-edge {
    opacity: 1;
  }

  /* Demo only: the Figma sample "app". */
  .fake {
    position: fixed;
    inset: 0;
    background: #dfe3ea;
  }
  .f-top {
    height: 34px;
    background: #cfd5de;
  }
  .f-side {
    position: absolute;
    top: 34px;
    bottom: 0;
    width: 64px;
    padding-top: 20px;
    background: #d7dde5;
  }
  .f-side i {
    display: block;
    width: 40px;
    height: 8px;
    margin: 0 0 26px 12px;
    border-radius: 4px;
    background: #c2c9d4;
  }
  .f-page {
    position: absolute;
    display: flex;
    flex-direction: column;
    gap: 12px;
    align-items: flex-start;
    width: 464px;
    padding: 26px 28px;
    border-radius: 10px;
    background: #f6f7f9;
    font-family: var(--font);
  }
  .f-page b {
    font-size: 24px;
    font-weight: 600;
    color: #14181f;
  }
  .f-page span {
    font-size: 14px;
    color: #4a5463;
  }
  .f-page em {
    padding: 10px 18px;
    border-radius: 20px;
    background: #14181f;
    font-style: normal;
    font-size: 14px;
    font-weight: 500;
    color: #fff;
  }
</style>
