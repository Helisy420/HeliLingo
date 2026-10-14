<!-- Переводчик → «Перевод файлов» (Figma 46:178 empty, 46:252 dragging
     over, 46:313 queue). Files come by drag and drop, «Выбрать файлы» or
     Ctrl+V (files copied in Explorer) and are translated one at a time;
     a finished one is saved with «Скачать» (save dialog) or «Скачать всё»
     (into «Папка для результатов»). The options below the queue are the
     same settings as Settings → «Изображения и файлы». -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n.svelte';
  import {
    fileCancel,
    fileSave,
    filesChoose,
    filesFromClipboard,
    filesInfo,
    filesOpenDir,
    filesSaveAll,
    fileTranslate,
    inApp,
    type FileEvent,
    type FileInfo,
  } from '../../lib/ipc';
  import { settings, updateSettings, type FilesSettings } from '../../lib/settings.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Choice from '../../lib/ui/Choice.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import IconButton from '../../lib/ui/IconButton.svelte';
  import Switch from '../../lib/ui/Switch.svelte';

  let {
    source,
    target,
    initial = [],
  }: { source: string; target: string; /** Dropped on the text tab. */ initial?: string[] } = $props();

  const s = $derived(t());
  const f = $derived(s.files);

  type Item = {
    id: number;
    info: FileInfo;
    status: 'queued' | 'working' | 'done' | 'error';
    done: number;
    total: number;
    /** «Слайд 14 из 22», «Распознаю страницу 2 из 5»… */
    stage: string;
    pages: number | null;
    sourceLang: string | null;
    /** Languages it was translated with (the pickers may change later). */
    pair: [string, string];
    out?: string;
    outName?: string;
    error?: string;
  };

  let items = $state<Item[]>([]);
  let nextId = 1;
  /** Slides or pages per running file (from its `read` event). */
  const groups = new Map<number, number>();
  /** Files hovering over the window: what they are and where the pointer is. */
  let drag = $state<{ files: FileInfo[]; x: number; y: number } | null>(null);

  const done = $derived(items.filter((i) => i.status === 'done'));

  function add(infos: FileInfo[]) {
    for (const info of infos) {
      if (items.some((i) => i.info.path === info.path && i.status !== 'error')) continue;
      const bad = !info.kind ? 'unsupported' : info.tooBig ? 'tooBig' : undefined;
      items.push({
        id: nextId++,
        info,
        status: bad ? 'error' : 'queued',
        done: 0,
        total: 0,
        stage: '',
        pages: null,
        sourceLang: null,
        pair: [source, target],
        error: bad,
      });
    }
    pump();
  }

  /** Starts the next queued file when nothing is running. */
  function pump() {
    if (items.some((i) => i.status === 'working')) return;
    const next = items.find((i) => i.status === 'queued');
    if (!next) return;
    next.status = 'working';
    next.pair = [source, target];
    const id = next.id;
    const item = () => items.find((i) => i.id === id);
    void fileTranslate(id, next.info.path, source, target, (e: FileEvent) => {
      const it = item();
      if (!it) return;
      switch (e.type) {
        case 'reading':
          it.stage = f.reading(e.page, e.pages);
          it.pages = e.pages;
          break;
        case 'read':
          it.total = e.units;
          it.pages = e.pages ?? (it.info.kind === 'PDF' ? it.pages : null);
          if (e.groups) it.stage = it.info.kind === 'PPTX' ? f.slide(1, e.groups) : f.page(1, e.groups);
          else it.stage = f.translating;
          groups.set(id, e.groups);
          break;
        case 'progress': {
          it.done = e.done;
          it.total = e.total;
          const g = groups.get(id) ?? 0;
          if (g && e.group) it.stage = it.info.kind === 'PPTX' ? f.slide(e.group, g) : f.page(e.group, g);
          break;
        }
        case 'done':
          it.status = 'done';
          it.out = e.out;
          it.outName = e.name;
          it.sourceLang = e.sourceLang;
          pump();
          break;
        case 'error':
          it.status = 'error';
          it.error = e.kind;
          pump();
          break;
      }
    });
  }

  function remove(it: Item) {
    if (it.status === 'working') void fileCancel(it.id);
    items = items.filter((i) => i.id !== it.id);
    pump();
  }

  async function choose() {
    add(await filesChoose());
  }

  function size(bytes: number): string {
    const ru = settings.uiLanguage !== 'en';
    const fmt = (n: number) => n.toLocaleString(ru ? 'ru-RU' : 'en-US', { maximumFractionDigits: 1 });
    return bytes >= 1024 * 1024 ? `${fmt(bytes / 1024 / 1024)} ${f.mb}` : `${fmt(Math.max(1, Math.round(bytes / 1024)))} ${f.kb}`;
  }

  const code = (c: string | null) => (c ?? '?').split('-')[0]!.toUpperCase();

  function pairText(it: Item): string {
    const [src, dst] = it.pair;
    const from = src === 'auto' ? it.sourceLang : src;
    return from ? `${code(from)} → ${code(dst)}` : `→ ${code(dst)}`;
  }

  function meta(it: Item): string {
    if (it.status === 'error') return it.error ? (f.errors[it.error] ?? s.errors[it.error as keyof typeof s.errors] ?? s.errors.other) : '';
    if (it.status === 'working' && it.stage) return `${it.stage} · ${pairText(it)}`;
    const parts = [size(it.info.size)];
    if (it.pages) parts.push(f.pages(it.pages));
    parts.push(pairText(it));
    return parts.join(' · ');
  }

  const pct = (it: Item) => (it.total ? Math.round((it.done / it.total) * 100) : 0);

  const setF = (patch: Partial<FilesSettings>) => updateSettings({ files: patch });

  // Drag and drop through Tauri (the webview never sees the files' paths).
  onMount(() => {
    if (initial.length) void filesInfo(initial).then(add);
    if (!inApp) {
      const demo = new URLSearchParams(location.search).get('files');
      if (demo === 'drag') void filesChoose().then((fs) => (drag = { files: fs, x: 250, y: 345 }));
      if (demo === 'queue') void filesChoose().then(add);
      return;
    }
    let off: (() => void) | undefined;
    void import('@tauri-apps/api/webview').then(async ({ getCurrentWebview }) => {
      off = await getCurrentWebview().onDragDropEvent(async (e) => {
        const p = e.payload;
        const ratio = window.devicePixelRatio || 1;
        if (p.type === 'enter') {
          drag = { files: await filesInfo(p.paths), x: p.position.x / ratio, y: p.position.y / ratio };
        } else if (p.type === 'over' && drag) {
          drag.x = p.position.x / ratio;
          drag.y = p.position.y / ratio;
        } else if (p.type === 'drop') {
          drag = null;
          add(await filesInfo(p.paths));
        } else {
          drag = null;
        }
      });
    });
    return () => off?.();
  });

  function onKey(e: KeyboardEvent) {
    if (e.ctrlKey && e.code === 'KeyV') {
      e.preventDefault();
      void filesFromClipboard().then(add);
    }
  }

  let body = $state<HTMLElement>();
  /** The dragged-file card follows the pointer, inside the drop zone. */
  const cardPos = $derived.by(() => {
    if (!drag || !body) return { left: 0, top: 0 };
    const r = body.getBoundingClientRect();
    return { left: Math.min(Math.max(drag.x - r.left + 12, 8), r.width - 280), top: Math.min(Math.max(drag.y - r.top + 12, 8), r.height - 60) };
  });
  const dragKinds = $derived(drag ? [...new Set(drag.files.map((x) => x.kind ?? '?'))].join(', ') : '');
</script>

<svelte:window onkeydown={onKey} />

{#if drag || !items.length}
  <section class="body" bind:this={body}>
    <div class="zone" class:active={!!drag}>
      {#if drag}
        <span class="big-icon on"><Icon name="download-28" size={28} /></span>
        <span class="title">{f.release}</span>
        <span class="sub">{f.dragged(drag.files.length, dragKinds)}</span>
        {#if drag.files[0]}
          <div class="dragged" style:left="{cardPos.left}px" style:top="{cardPos.top}px">
            <span class="kind sm">{drag.files[0].kind ?? '?'}</span>
            <span class="d-text">
              <span class="name">{drag.files[0].name}</span>
              <span class="meta">{size(drag.files[0].size)}</span>
            </span>
            {#if drag.files.length > 1}<span class="more">+{drag.files.length - 1}</span>{/if}
          </div>
        {/if}
      {:else}
        <span class="big-icon"><Icon name="file-26" size={26} /></span>
        <span class="title">{f.dropHere}</span>
        <span class="sub">{f.orPaste}</span>
        <span class="buttons"><Button variant="primary" icon="upload" onclick={choose}>{f.choose}</Button></span>
        <span class="chips">
          {#each ['DOCX', 'PDF', 'PPTX', 'XLSX', 'TXT', 'SRT'] as k (k)}<span class="chip">{k}</span>{/each}
        </span>
        <span class="sub">{f.limit}</span>
      {/if}
    </div>
  </section>
{:else}
  <section class="queue">
    <div class="list">
      {#each items as it (it.id)}
        <div class="file">
          <span class="kind">{it.info.kind ?? '?'}</span>
          <span class="f-text">
            <span class="name">{it.info.name}</span>
            <span class="meta" class:err={it.status === 'error'}>{meta(it)}</span>
            {#if it.status === 'working'}
              <span class="bar"><span style:width="{pct(it)}%"></span></span>
            {/if}
          </span>
          <span class="state">
            {#if it.status === 'done'}
              <span class="ok"><Icon name="check" />{f.ready}</span>
              <Button size="sm" icon="download" onclick={() => it.out && fileSave(it.out, it.outName ?? it.info.name)}>{f.download}</Button>
            {:else if it.status === 'working'}
              <span class="pct">{pct(it)} %</span>
              <IconButton size="sm" icon="x" label={f.cancel} onclick={() => remove(it)} />
            {:else if it.status === 'queued'}
              <span class="badge">{f.queued}</span>
              <IconButton size="sm" icon="x" label={f.remove} onclick={() => remove(it)} />
            {:else}
              <IconButton size="sm" icon="x" label={f.remove} onclick={() => remove(it)} />
            {/if}
          </span>
        </div>
      {/each}
    </div>

    <div class="options">
      <div class="opt">
        <span>{s.set.files.format}</span>
        <Choice
          value={settings.files.format}
          width={170}
          options={(['same', 'docx', 'txt'] as const).map((v) => ({ value: v, label: s.set.files.formats[v]! }))}
          onchange={(v) => setF({ format: v })}
        />
      </div>
      <div class="opt">
        <span>{s.set.files.keepFormatting}</span>
        <Switch checked={settings.files.keepFormatting} label={s.set.files.keepFormatting} onchange={(v) => setF({ keepFormatting: v })} />
      </div>
      <div class="opt">
        <span>{s.set.files.bilingual}</span>
        <Switch checked={settings.files.bilingual} label={s.set.files.bilingual} onchange={(v) => setF({ bilingual: v })} />
      </div>
    </div>

    <div class="actions">
      <Button variant="ghost" size="sm" icon="plus" onclick={choose}>{f.addMore}</Button>
      <span class="spacer"></span>
      <Button size="sm" icon="folder" onclick={() => filesOpenDir()}>{f.openFolder}</Button>
      <Button
        variant="primary"
        size="sm"
        icon="download"
        disabled={!done.length}
        onclick={() => filesSaveAll(done.map((i) => [i.out!, i.outName ?? i.info.name]))}>{f.downloadAll}</Button
      >
    </div>
  </section>
{/if}

<style>
  /* Empty / dragging (46:178, 46:252): the dashed zone fills the body. */
  .body {
    position: relative;
    flex: 1;
    display: flex;
    min-height: 0;
    padding: 0 16px 12px;
  }
  .zone {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    border: 1.5px dashed var(--line-strong);
    border-radius: 12px;
    background: var(--bg-sunken);
  }
  .zone.active {
    border: 2px dashed var(--accent);
    background: var(--accent-soft);
  }
  .big-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 56px;
    height: 56px;
    border-radius: 28px;
    background: var(--accent-soft);
    color: var(--text-primary);
  }
  .big-icon.on {
    width: 64px;
    height: 64px;
    border-radius: 32px;
    background: var(--accent);
    color: var(--text-on-marker);
  }
  .title {
    font: 600 14px/20px var(--font);
    color: var(--text-primary);
  }
  .sub {
    font: var(--caption);
    color: var(--text-secondary);
  }
  .buttons {
    padding-top: 8px;
  }
  .chips {
    display: flex;
    gap: 6px;
    padding-top: 4px;
  }
  .chip {
    padding: 2px 8px;
    border-radius: 6px;
    border: 1px solid var(--line-strong);
    background: var(--bg-field);
    font: var(--caption-medium);
    color: var(--text-secondary);
  }
  .dragged {
    position: absolute;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 14px 8px 10px;
    border-radius: 10px;
    border: 1px solid var(--line-strong);
    background: var(--bg-surface);
    filter: drop-shadow(0 2px 3px rgba(0, 0, 0, 0.12)) drop-shadow(0 16px 20px rgba(0, 0, 0, 0.2));
    pointer-events: none;
  }
  .d-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    white-space: nowrap;
  }
  .more {
    padding: 1px 7px;
    border-radius: 5px;
    background: var(--accent-soft);
    font: var(--caption-medium);
    color: var(--text-primary);
  }

  /* Queue (46:313). */
  .queue {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
    padding: 2px 16px 14px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
    overflow-y: auto;
  }
  .file {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid var(--line-soft);
    background: var(--bg-sunken);
  }
  .kind {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 40px;
    height: 40px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--bg-field);
    font: var(--caption-medium);
    color: var(--text-secondary);
  }
  .kind.sm {
    width: 32px;
    height: 32px;
    border-radius: 6px;
  }
  .f-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .name {
    font: var(--body-medium);
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    font: var(--caption);
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta.err {
    color: var(--danger);
  }
  .bar {
    display: flex;
    width: 250px;
    max-width: 100%;
    height: 6px;
    border-radius: 3px;
    border: 1px solid var(--line-soft);
    background: var(--bg-sunken);
    overflow: hidden;
  }
  .bar span {
    height: 100%;
    border-radius: 3px;
    background: var(--accent);
    transition: width 0.3s;
  }
  .state {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }
  .ok {
    display: flex;
    align-items: center;
    gap: 6px;
    font: var(--caption-medium);
    color: var(--text-primary);
  }
  .ok :global(.icon) {
    color: var(--status-ok);
  }
  .pct {
    font: var(--caption-medium);
    color: var(--text-primary);
    font-variant-numeric: tabular-nums;
  }
  .badge {
    padding: 1px 7px;
    border-radius: 5px;
    border: 1px solid var(--line-strong);
    background: var(--bg-field);
    font: var(--caption-medium);
    color: var(--text-secondary);
  }
  .options {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    border-radius: 10px;
    border: 1px solid var(--line-soft);
    background: var(--bg-surface);
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 44px;
    padding: 0 12px 0 14px;
    font: var(--body-medium);
    color: var(--text-primary);
  }
  .opt span {
    flex: 1;
  }
  .opt + .opt {
    border-top: 1px solid var(--line-soft);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
  .spacer {
    flex: 1;
  }
</style>
