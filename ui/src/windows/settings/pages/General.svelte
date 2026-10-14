<!-- Настройки / Общие (Figma 16:2768): interface, translation, popup, data
     and exceptions. -->
<script lang="ts">
  import { t } from '../../../lib/i18n.svelte';
  import { cacheClear, historyClear } from '../../../lib/ipc';
  import { langLower, langOn } from '../../../lib/langs';
  import { settings, updateSettings } from '../../../lib/settings.svelte';
  import Button from '../../../lib/ui/Button.svelte';
  import Choice from '../../../lib/ui/Choice.svelte';
  import Field from '../../../lib/ui/Field.svelte';
  import Masked from '../../../lib/ui/Masked.svelte';
  import LangChoice from '../../../lib/ui/LangChoice.svelte';
  import Segmented from '../../../lib/ui/Segmented.svelte';
  import Switch from '../../../lib/ui/Switch.svelte';
  import Group from '../Group.svelte';
  import Row from '../Row.svelte';

  const s = $derived(t());
  const g = $derived(s.set.general);
  let done = $state<'history' | 'cache' | null>(null);
  let apps = $state(settings.ignoredApps.join(', '));

  function flash(what: 'history' | 'cache') {
    done = what;
    setTimeout(() => (done = null), 1500);
  }

  function saveApps() {
    const list = apps
      .split(/[,;\n]/)
      .map((a) => a.trim())
      .filter(Boolean);
    void updateSettings({ ignoredApps: list });
  }
</script>

<div class="page">
  <h1 class="h-page">{s.set.nav.general}</h1>

  <Group title={g.interface}>
    <Row label={g.uiLanguage}>
      <Choice
        value={settings.uiLanguage}
        options={[
          { value: 'ru', label: 'Русский' },
          { value: 'en', label: 'English' },
        ]}
        onchange={(v) => updateSettings({ uiLanguage: v })}
      />
    </Row>
    <Row label={g.theme}>
      <Segmented
        value={settings.theme}
        options={[
          { value: 'light', label: g.themeLight, icon: 'sun' },
          { value: 'dark', label: g.themeDark, icon: 'moon' },
          { value: 'blue', label: g.themeBlue, icon: 'drop' },
          { value: 'system', label: g.themeSystem, icon: 'monitor' },
        ]}
        onchange={(v) => updateSettings({ theme: v as typeof settings.theme })}
      />
    </Row>
    <Row label={g.autostart}>
      <Switch checked={settings.autostart} label={g.autostart} onchange={(v) => updateSettings({ autostart: v })} />
    </Row>
    <Row label={g.animations} note={g.animationsNote}>
      <Switch checked={settings.animations} label={g.animations} onchange={(v) => updateSettings({ animations: v })} />
    </Row>
    <Row label={g.hardware} note={g.hardwareNote}>
      <Switch
        checked={settings.hardwareAcceleration}
        label={g.hardware}
        onchange={(v) => updateSettings({ hardwareAcceleration: v })}
      />
    </Row>
  </Group>

  <Group title={g.translation}>
    <Row label={g.targetLang}>
      <LangChoice value={settings.targetLang} onchange={(c) => updateSettings({ targetLang: c })} />
    </Row>
    <Row label={g.sourceLang}>
      <LangChoice value={settings.sourceLang} allowAuto width={236} onchange={(c) => updateSettings({ sourceLang: c })} />
    </Row>
    <Row label={g.alreadyInLang(langOn(settings.targetLang))}>
      <LangChoice
        value={settings.secondLang}
        width={236}
        label={g.translateTo(langLower(settings.secondLang))}
        onchange={(c) => updateSettings({ secondLang: c })}
      />
    </Row>
  </Group>

  <Group title={g.popup}>
    <Row label={g.ctrlC}>
      <Choice
        value={settings.popupCtrlC}
        width={216}
        options={[
          { value: 'copyClose', label: g.ctrlCCopyClose },
          { value: 'copy', label: g.ctrlCCopy },
          { value: 'ignore', label: g.ctrlCIgnore },
        ]}
        onchange={(v) => updateSettings({ popupCtrlC: v })}
      />
    </Row>
    <Row label={g.readAloud} note={g.readAloudNote}>
      <Switch checked={settings.readAloud} label={g.readAloud} onchange={(v) => updateSettings({ readAloud: v })} />
    </Row>
  </Group>

  <Group title={g.privacy}>
    <Row label={g.maskNumbers} note={g.maskNumbersNote}>
      <Switch checked={settings.maskNumbers} label={g.maskNumbers} onchange={(v) => updateSettings({ maskNumbers: v })} />
    </Row>
    {#if settings.maskNumbers}
      <p class="mask-sample"><Masked text={g.maskSample} /></p>
    {/if}
  </Group>

  <Group title={g.data}>
    <Row label={g.saveHistory}>
      <Button
        variant="danger"
        size="sm"
        icon={done === 'history' ? 'check' : 'trash'}
        onclick={() => historyClear().then(() => flash('history'))}
      >
        {done === 'history' ? g.cleared : g.clearHistory}
      </Button>
      <Switch checked={settings.saveHistory} label={g.saveHistory} onchange={(v) => updateSettings({ saveHistory: v })} />
    </Row>
    <Row label={g.clearCache}>
      <Button size="sm" icon={done === 'cache' ? 'check' : 'trash'} onclick={() => cacheClear().then(() => flash('cache'))}>
        {done === 'cache' ? g.cleared : s.clear}
      </Button>
    </Row>
  </Group>

  <Group title={g.apps}>
    <Row label={g.skipFullscreen} note={g.skipFullscreenNote}>
      <Switch
        checked={settings.skipFullscreen}
        label={g.skipFullscreen}
        onchange={(v) => updateSettings({ skipFullscreen: v })}
      />
    </Row>
    <Row label={g.ignoredApps} note={g.ignoredAppsNote}>
      <span class="apps" onfocusout={saveApps}>
        <Field bind:value={apps} placeholder="game.exe" width={190} />
      </span>
    </Row>
  </Group>
</div>

<style>
  .mask-sample {
    margin: 10px 0 0;
    padding: 10px 14px;
    border-radius: var(--radius);
    background: var(--bg-sunken);
    font: var(--body);
    color: var(--text-secondary);
  }
  .apps {
    display: contents;
  }
</style>
