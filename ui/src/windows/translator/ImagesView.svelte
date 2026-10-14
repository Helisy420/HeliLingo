<!-- Переводчик → «Перевод изображения» (Figma 48:180): the ways to add a
     picture — paste (with a banner when the clipboard holds one), capture
     an area or a window, drop or choose a file. The picture's text is read
     (OCR) and goes to the text tab, like before. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n.svelte';
  import { clipboardHasImage, clipboardImage, imageFile, inApp, runAction } from '../../lib/ipc';
  import { settings } from '../../lib/settings.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import Keys from '../../lib/ui/Keys.svelte';

  let {
    busy,
    error,
    onimage,
  }: { busy: boolean; error: string; onimage: (image: Blob) => void } = $props();

  const s = $derived(t());
  const tr = $derived(s.translator);

  let hasImage = $state(false);
  let dragging = $state(false);
  let fileInput = $state<HTMLInputElement>();

  const check = () => void clipboardHasImage().then((v) => (hasImage = v));

  async function paste() {
    const png = await clipboardImage();
    if (png) onimage(new Blob([png as BlobPart], { type: 'image/png' }));
  }

  onMount(() => {
    check();
    window.addEventListener('focus', check);
    let off: (() => void) | undefined;
    if (inApp) {
      void import('@tauri-apps/api/webview').then(async ({ getCurrentWebview }) => {
        off = await getCurrentWebview().onDragDropEvent(async (e) => {
          const p = e.payload;
          if (p.type === 'enter' || p.type === 'over') dragging = true;
          else if (p.type === 'drop') {
            dragging = false;
            const path = p.paths.find((x) => /\.(png|jpe?g|webp|bmp|gif)$/i.test(x));
            const bytes = path ? await imageFile(path) : null;
            if (bytes) onimage(new Blob([bytes as BlobPart]));
          } else dragging = false;
        });
      });
    }
    return () => {
      window.removeEventListener('focus', check);
      off?.();
    };
  });

  function onKey(e: KeyboardEvent) {
    if (e.ctrlKey && e.code === 'KeyV' && hasImage) {
      e.preventDefault();
      void paste();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<section class="body">
  {#if hasImage && !busy}
    <div class="banner">
      <Icon name="image" />
      <span class="b-text">{tr.clipboardImage}</span>
      <Button variant="primary" size="sm" onclick={paste}>{tr.paste}</Button>
    </div>
  {/if}
  <div class="zone" class:active={dragging}>
    <span class="big-icon"><Icon name="image-26" size={26} /></span>
    <span class="title">{busy ? tr.recognizing : tr.addImage}</span>
    <span class="sub">{tr.addImageHint}</span>
    <span class="ways">
      <button class="way" onclick={paste} disabled={!hasImage || busy}>
        <span class="w-icon"><Icon name="clip" /></span>
        <span class="w-label">{tr.paste}</span>
        <Keys shortcut="Ctrl+V" />
      </button>
      <button class="way" onclick={() => runAction('screenshot')} disabled={busy}>
        <span class="w-icon"><Icon name="scan" /></span>
        <span class="w-label">{tr.area}</span>
        {#if settings.hotkeys.screenshot}<Keys shortcut={settings.hotkeys.screenshot} />{/if}
      </button>
      <button class="way" onclick={() => runAction('windowCapture')} disabled={busy}>
        <span class="w-icon"><Icon name="appwin-18" size={18} /></span>
        <span class="w-label">{tr.window}</span>
        {#if settings.hotkeys.windowCapture}<Keys shortcut={settings.hotkeys.windowCapture} />{/if}
      </button>
    </span>
    <span class="file-row">
      <span class="sub">{tr.or}</span>
      <Button size="sm" icon="upload" disabled={busy} onclick={() => fileInput?.click()}>{tr.chooseImage}</Button>
    </span>
    <span class="note" class:err={!!error}>{error || tr.formats}</span>
    <input
      bind:this={fileInput}
      type="file"
      accept="image/png,image/jpeg,image/webp,image/bmp,image/gif"
      hidden
      onchange={(e) => {
        const file = e.currentTarget.files?.[0];
        if (file) onimage(file);
        e.currentTarget.value = '';
      }}
    />
  </div>
</section>

<style>
  .body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
    padding: 0 16px 12px;
  }
  .banner {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 44px;
    flex-shrink: 0;
    padding: 0 8px 0 12px;
    border-radius: 10px;
    background: var(--accent-soft);
    color: var(--text-primary);
  }
  .b-text {
    flex: 1;
    font: var(--body-medium);
  }
  .zone {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 22px 0 20px;
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
  .title {
    font: 600 14px/20px var(--font);
    color: var(--text-primary);
  }
  .sub {
    font: var(--caption);
    color: var(--text-secondary);
  }
  .ways {
    display: flex;
    gap: 10px;
    padding: 6px 0 2px;
  }
  .way {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    width: 144px;
    padding: 14px 10px 12px;
    border-radius: 12px;
    border: 1px solid var(--line-strong);
    background: var(--bg-field);
    color: var(--text-primary);
  }
  .way:hover:not(:disabled) {
    background: var(--state-hover);
  }
  .way:disabled {
    opacity: 0.55;
  }
  .w-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: 18px;
    background: var(--accent-soft);
  }
  .w-label {
    font: var(--body-medium);
  }
  .file-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-top: 4px;
  }
  .note {
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .note.err {
    color: var(--danger);
  }
</style>
