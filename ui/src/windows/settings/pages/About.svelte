<!-- Настройки / О программе (Figma 16:3139): logo, version, updates
     (`version.json` in the GitHub repo, see src-tauri/src/update.rs), links. -->
<script lang="ts">
  import { t } from '../../../lib/i18n.svelte';
  import { onMount } from 'svelte';
  import { inApp, openUrl, updateCheck, updateInstall, type UpdateCheck } from '../../../lib/ipc';
  import { settings, updateSettings } from '../../../lib/settings.svelte';
  import Button from '../../../lib/ui/Button.svelte';
  import Choice from '../../../lib/ui/Choice.svelte';
  import IconButton from '../../../lib/ui/IconButton.svelte';
  import Logo from '../../../lib/ui/Logo.svelte';
  import Switch from '../../../lib/ui/Switch.svelte';
  import Group from '../Group.svelte';
  import Row from '../Row.svelte';

  const REPO = 'https://github.com/Helisy420/HeliLingo';
  const s = $derived(t());
  const a = $derived(s.set.about);
  /** idle → checking → latest / found → installing (→ the app restarts) / failed. */
  let phase = $state<'idle' | 'checking' | 'latest' | 'found' | 'installing' | 'failed'>('idle');
  let found = $state<UpdateCheck | null>(null);
  let progress = $state(0);
  let error = $state('');

  async function check() {
    phase = 'checking';
    try {
      found = await updateCheck();
      phase = found.available ? 'found' : 'latest';
    } catch (e) {
      error = String(e);
      phase = 'failed';
    }
  }

  async function install() {
    phase = 'installing';
    progress = 0;
    try {
      await updateInstall();
    } catch (e) {
      error = String(e) === 'checksum' ? a.checksum : String(e);
      phase = 'failed';
    }
  }

  onMount(() => {
    if (!inApp) {
      if (new URLSearchParams(location.search).has('update')) void check();
      return;
    }
    const offs: (() => void)[] = [];
    void import('@tauri-apps/api/event').then(async ({ listen }) => {
      offs.push(await listen<{ done: number; total: number }>('update:progress', (e) => (progress = e.payload.total ? e.payload.done / e.payload.total : 0)));
      offs.push(
        await listen<UpdateCheck>('update:available', (e) => {
          found = e.payload;
          if (phase === 'idle') phase = 'found';
        }),
      );
    });
    return () => offs.forEach((f) => f());
  });

  const notes = $derived(found ? (settings.uiLanguage === 'en' ? found.latest.notes.en : found.latest.notes.ru) || found.latest.notes.en : '');
  const mb = (n: number) => (n / 1024 / 1024).toLocaleString(settings.uiLanguage === 'en' ? 'en-US' : 'ru-RU', { maximumFractionDigits: 1 });
</script>

<div class="page">
  <h1 class="h-page">{s.set.nav.about}</h1>

  <div class="app">
    <Logo size={64} />
    <div class="name">
      <span class="hero">HeliLingo</span>
      <span class="ver">{a.version(__APP_VERSION__, __BUILD_DATE__)}</span>
    </div>
  </div>

  <div class="buttons">
    <Button variant="primary" icon="refresh" disabled={phase === 'checking' || phase === 'installing'} onclick={check}>
      {phase === 'checking' ? a.checking : a.checkUpdates}
    </Button>
    <Button onclick={() => openUrl(`${REPO}/releases`)}>{a.whatsNew}</Button>
    {#if phase === 'latest'}<span class="note ok">{a.upToDate}</span>{/if}
    {#if phase === 'failed'}<span class="note err">{a.failed(error)}</span>{/if}
  </div>

  {#if found && (phase === 'found' || phase === 'installing')}
    <section class="update">
      <div class="u-head">
        <span class="u-title">{a.available(found.latest.version)}</span>
        <span class="u-meta">{found.latest.date}{found.latest.size ? ` · ${mb(found.latest.size)} ${s.files.mb}` : ''}</span>
      </div>
      {#if notes}<p class="u-notes">{notes}</p>{/if}
      {#if phase === 'installing'}
        <span class="u-bar"><span style:width="{Math.round(progress * 100)}%"></span></span>
        <span class="u-meta">{a.installing(Math.round(progress * 100))}</span>
      {:else}
        <div class="u-actions">
          <Button variant="primary" size="sm" icon="download" onclick={install}>{a.install}</Button>
          {#if found.latest.page}<Button size="sm" onclick={() => openUrl(found!.latest.page)}>{a.releasePage}</Button>{/if}
        </div>
      {/if}
    </section>
  {/if}

  <Group title={a.updates}>
    <Row label={a.autoCheck}>
      <Switch checked={settings.autoUpdate} label={a.autoCheck} onchange={(v) => updateSettings({ autoUpdate: v })} />
    </Row>
    <Row label={a.channel}>
      <Choice
        value={settings.updateChannel}
        width={140}
        options={[
          { value: 'stable', label: a.stable },
          { value: 'beta', label: a.beta },
        ]}
        onchange={(v) => updateSettings({ updateChannel: v })}
      />
    </Row>
  </Group>

  <Group title={a.links}>
    <Row label={a.licenses}>
      <IconButton icon="chev-r" label={a.licenses} onclick={() => openUrl(`${REPO}/blob/main/THIRD-PARTY.md`)} />
    </Row>
    <Row label={a.source}>
      <IconButton icon="link" label={a.source} onclick={() => openUrl(REPO)} />
    </Row>
    <Row label={a.report}>
      <IconButton icon="link" label={a.report} onclick={() => openUrl(`${REPO}/issues`)} />
    </Row>
  </Group>

  <p class="copy">© 2026 HeliLingo</p>
</div>

<style>
  .app {
    display: flex;
    align-items: center;
    gap: 18px;
  }
  .name {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .hero {
    font: 600 24px/30px var(--font);
    letter-spacing: -0.015em;
    color: var(--text-primary);
  }
  .ver {
    font: var(--caption);
    color: var(--text-secondary);
  }
  .buttons {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .note {
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .note.ok {
    color: var(--status-ok);
  }
  .note.err {
    color: var(--danger);
  }
  .update {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px 16px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--line-soft);
    background: var(--bg-sunken);
  }
  .u-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  .u-title {
    font: 600 14px/20px var(--font);
    color: var(--text-primary);
  }
  .u-meta {
    font: var(--caption);
    color: var(--text-secondary);
  }
  .u-notes {
    margin: 0;
    font: var(--body);
    color: var(--text-primary);
    white-space: pre-line;
  }
  .u-actions {
    display: flex;
    gap: 8px;
  }
  .u-bar {
    display: flex;
    height: 6px;
    border-radius: 3px;
    background: var(--bg-field);
    overflow: hidden;
  }
  .u-bar span {
    background: var(--accent);
    transition: width 0.2s;
  }
  .copy {
    margin: 0;
    font: var(--caption);
    color: var(--text-tertiary);
  }
</style>
