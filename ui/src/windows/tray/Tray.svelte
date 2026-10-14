<!-- Трей-меню (Figma 16:2611 / 16:882 / 12:88): header with the on/off
     switch, actions, direction, pause, settings, quit. «Направление» turns
     the menu into a language list in place. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { fitWindow, hideOnBlur, hideWindow, onShown } from '../../lib/fit';
  import { t } from '../../lib/i18n.svelte';
  import { pause, power, quit, runAction, setEnabled, type Action, type PowerInfo } from '../../lib/ipc';
  import { langName } from '../../lib/langs';
  import { settings, updateSettings } from '../../lib/settings.svelte';
  import Bits from '../../lib/ui/Bits.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import LangMenu from '../../lib/ui/LangMenu.svelte';
  import Logo from '../../lib/ui/Logo.svelte';
  import Switch from '../../lib/ui/Switch.svelte';

  const s = $derived(t());
  let pw = $state<PowerInfo>({ enabled: true, pausedSecs: 0 });
  let picking = $state(false);
  let card: HTMLDivElement;

  const keys = $derived(chordKeys(settings.hotkeys.translateSelection));

  /** "Ctrl×2" → [Ctrl, Ctrl]; "Ctrl+Shift+L" → [Ctrl, ⇧, L]. */
  function chordKeys(h: string): string[] {
    if (!h) return [];
    const double = h.endsWith('×2');
    const parts = h.replace('×2', '').split('+').map((k) => (k === 'Shift' ? '⇧' : k));
    return double ? [...parts, ...parts] : parts;
  }

  async function act(a: Action) {
    await hideWindow();
    await runAction(a);
  }

  async function toggle(on: boolean) {
    pw = await setEnabled(on);
  }

  async function pauseHour() {
    pw = await pause(pw.pausedSecs > 0 ? 0 : 60);
  }

  function pickTarget(code: string) {
    void updateSettings({ targetLang: code });
    picking = false;
  }

  onMount(async () => {
    pw = await power();
    await fitWindow(card);
    const blur = await hideOnBlur();
    await onShown('window:shown', async () => {
      blur.shown();
      picking = false;
      pw = await power();
    });
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && !picking) void hideWindow();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="card" bind:this={card}>
  <header class="head">
    <Logo />
    <span class="brand">HeliLingo</span>
    <span class="spacer"></span>
    <span class="state">{pw.pausedSecs > 0 ? s.tray.paused : pw.enabled ? s.tray.enabled : s.tray.disabled}</span>
    <Switch checked={pw.enabled && pw.pausedSecs === 0} label={s.tray.enabled} onchange={toggle} />
  </header>
  <div class="sep"><span></span></div>

  {#if picking}
    <button class="row" onclick={() => (picking = false)}>
      <Icon name="chev-l" size={16} />
      <span class="label">{s.tray.direction}</span>
    </button>
    <LangMenu value={settings.targetLang} inline onpick={pickTarget} onclose={() => (picking = false)} />
  {:else}
    <button class="row" onclick={() => act('translateSelection')}>
      <Icon name="languages-18" size={18} />
      <span class="label">{s.tray.translateSelection}</span>
      <span class="spacer"></span>
      {#if keys.length}
        <span class="keys">{#each keys as k, i (i)}<Bits kind="key">{k}</Bits>{/each}</span>
      {/if}
    </button>
    <button class="row" onclick={() => (picking = true)}>
      <Icon name="swap-18" size={18} />
      <span class="label">{s.tray.direction}</span>
      <span class="spacer"></span>
      <span class="hint"
        >{settings.sourceLang === 'auto' ? s.autoTitle : langName(settings.sourceLang)} → {langName(settings.targetLang)}</span
      >
      <span class="chev"><Icon name="chev-r-14" size={14} /></span>
    </button>
    <button class="row" onclick={() => act('translator')}>
      <Icon name="appwin-18" size={18} />
      <span class="label">{s.tray.openTranslator}</span>
    </button>
    <button class="row" onclick={() => act('history')}>
      <Icon name="history-18" size={18} />
      <span class="label">{s.history}</span>
    </button>
    <div class="sep"><span></span></div>
    <button class="row" onclick={pauseHour}>
      <Icon name="pause-18" size={18} />
      <span class="label">{pw.pausedSecs > 0 ? s.tray.resume : s.tray.pauseHour}</span>
      {#if pw.pausedSecs > 0}
        <span class="spacer"></span>
        <span class="hint">{s.minutes(Math.ceil(pw.pausedSecs / 60))}</span>
      {/if}
    </button>
    <button class="row" onclick={() => act('settings')}>
      <Icon name="gear-18" size={18} />
      <span class="label">{s.tray.settings}</span>
    </button>
    <div class="sep"><span></span></div>
    <button class="row" onclick={() => quit()}>
      <Icon name="power-18" size={18} />
      <span class="label">{s.tray.quit}</span>
    </button>
  {/if}
</div>

<style>
  :global(html),
  :global(body) {
    background: transparent;
  }
  /* Room for Shadow/Menu around the 324 px card. */
  :global(body) {
    padding: 8px 24px 32px;
  }
  .card {
    width: 324px;
    display: flex;
    flex-direction: column;
    padding: 6px;
    border-radius: 12px;
    background: var(--bg-surface);
    border: 1px solid var(--line-soft);
    box-shadow:
      0 1px 3px rgba(0, 0, 0, 0.12),
      0 8px 24px -4px rgba(0, 0, 0, 0.22);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
  }
  .brand {
    font: 600 14px/20px var(--font);
    color: var(--text-primary);
  }
  .state {
    font: var(--caption);
    color: var(--text-secondary);
  }
  .spacer {
    flex: 1;
  }
  .sep {
    padding: 4px 6px;
  }
  .sep span {
    display: block;
    height: 1px;
    background: var(--line-soft);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 36px;
    padding: 0 10px;
    border-radius: var(--radius);
    color: var(--text-primary);
    text-align: left;
    flex-shrink: 0;
  }
  .row:hover {
    background: var(--state-hover);
  }
  .label {
    font: var(--body);
    white-space: nowrap;
  }
  .keys {
    display: flex;
    gap: 4px;
  }
  .hint {
    font: var(--caption);
    color: var(--text-tertiary);
    white-space: nowrap;
  }
  .chev {
    display: flex;
    color: var(--text-tertiary);
  }
</style>
