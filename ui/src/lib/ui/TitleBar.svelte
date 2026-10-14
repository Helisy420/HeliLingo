<!-- Window title bar from the Figma windows: 48 px, 18 px icon, Window/Title
     (13/16 Medium), minimize and close buttons 46×48 with 14 px icons.
     Drag anywhere on it to move the window. -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '../i18n.svelte';
  import { inApp } from '../ipc';
  import Icon from './Icon.svelte';

  let {
    icon,
    title,
    back,
    minimize = true,
    extra,
  }: {
    icon?: string;
    title: string;
    /** Shows a back arrow instead of the icon (sub-pages). */
    back?: () => void;
    minimize?: boolean;
    /** Controls before minimize/close. */
    extra?: Snippet;
  } = $props();

  const s = $derived(t());

  async function win() {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    return getCurrentWindow();
  }
  async function hide() {
    if (inApp) await (await win()).hide();
  }
  async function min() {
    if (inApp) await (await win()).minimize();
  }
</script>

<header class="bar" data-tauri-drag-region>
  {#if back}
    <button class="back" onclick={back} aria-label={s.back}><Icon name="chev-l" /></button>
  {:else if icon}
    <span class="icon"><Icon name={icon} size={18} /></span>
  {/if}
  <span class="title" data-tauri-drag-region>{title}</span>
  <span class="spacer" data-tauri-drag-region></span>
  {@render extra?.()}
  {#if minimize}
    <button class="wbtn" onclick={min} aria-label={s.minimize}><Icon name="minus-14" size={14} /></button>
  {/if}
  <button class="wbtn close" onclick={hide} aria-label={s.closeWindow}><Icon name="x-14" size={14} /></button>
</header>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 48px;
    padding-left: 18px;
    border-bottom: 1px solid var(--line-soft);
    flex-shrink: 0;
  }
  .icon {
    display: flex;
    color: var(--text-primary);
  }
  .back {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    margin-left: -4px;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
  }
  .back:hover {
    background: var(--state-hover);
  }
  .title {
    font: 500 13px/16px var(--font);
    color: var(--text-primary);
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .wbtn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 46px;
    height: 48px;
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
</style>
