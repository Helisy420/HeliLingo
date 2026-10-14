<!-- Welcome / how it works (no Figma frame: built from the About page and
     KeyCaps). Shown once, on the first start. -->
<script lang="ts">
  import { t } from '../../lib/i18n.svelte';
  import { inApp, runAction } from '../../lib/ipc';
  import { settings, updateSettings } from '../../lib/settings.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Keys from '../../lib/ui/Keys.svelte';
  import Logo from '../../lib/ui/Logo.svelte';
  import TitleBar from '../../lib/ui/TitleBar.svelte';

  const s = $derived(t());
  const w = $derived(s.welcome);

  async function finish(open?: 'settings') {
    await updateSettings({ welcomed: true });
    if (open) await runAction(open);
    if (inApp) {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().hide();
    }
  }
</script>

<div class="win">
  <TitleBar title="HeliLingo" minimize={false} />
  <main class="body">
    <div class="hero">
      <Logo size={48} />
      <div>
        <h1 class="h-page">{w.title}</h1>
        <p class="lead">{w.lead}</p>
      </div>
    </div>

    <ol class="steps">
      <li>
        <span class="num">1</span>
        <span class="txt"><b>{w.step1}</b><small>{w.step1Note}</small></span>
      </li>
      <li>
        <span class="num">2</span>
        <span class="txt"><b>{w.step2}</b><small>{w.step2Note}</small></span>
        <Keys shortcut={settings.hotkeys.translateSelection} />
      </li>
      <li>
        <span class="num">3</span>
        <span class="txt"><b>{s.set.hotkeys.quick}</b><small>{w.step3Note}</small></span>
        <Keys shortcut={settings.hotkeys.quick} />
      </li>
    </ol>

    <p class="tray">{w.tray}</p>
  </main>
  <footer class="foot">
    <Button onclick={() => finish('settings')}>{w.openSettings}</Button>
    <span class="spacer"></span>
    <Button variant="primary" onclick={() => finish()}>{w.start}</Button>
  </footer>
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
    flex-direction: column;
    gap: 22px;
    padding: 26px 32px;
  }
  .hero {
    display: flex;
    align-items: center;
    gap: 16px;
  }
  .lead {
    margin: 4px 0 0;
    font: var(--body);
    color: var(--text-secondary);
  }
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .steps li {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 14px;
    border-radius: var(--radius-lg);
    background: var(--bg-sunken);
  }
  .num {
    flex-shrink: 0;
    width: 28px;
    height: 28px;
    border-radius: 14px;
    background: var(--accent);
    color: var(--text-on-marker);
    font: 600 14px/28px var(--font);
    text-align: center;
  }
  .txt {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .txt b {
    font: var(--body-medium);
    color: var(--text-primary);
  }
  .txt small {
    font: var(--caption);
    color: var(--text-secondary);
  }
  .tray {
    margin: 0;
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 32px 20px;
  }
  .spacer {
    flex: 1;
  }
</style>
