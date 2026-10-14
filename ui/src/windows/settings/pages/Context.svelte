<!-- Настройки / Контекст и режимы (Figma 16:3670): context, glossary,
     programmer mode, Ultra mode. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../../../lib/i18n.svelte';
  import { glossaryCount, openGlossary } from '../../../lib/ipc';
  import { langOn } from '../../../lib/langs';
  import { settings, updateSettings } from '../../../lib/settings.svelte';
  import Button from '../../../lib/ui/Button.svelte';
  import Slider from '../../../lib/ui/Slider.svelte';
  import Switch from '../../../lib/ui/Switch.svelte';
  import Group from '../Group.svelte';
  import Row from '../Row.svelte';

  const s = $derived(t());
  const c = $derived(s.set.context);
  let terms = $state(0);

  onMount(() => {
    void glossaryCount().then((n) => (terms = n));
    // The glossary is edited in Notepad: recount when the window is focused again.
    const recount = () => void glossaryCount().then((n) => (terms = n));
    window.addEventListener('focus', recount);
    return () => window.removeEventListener('focus', recount);
  });

  const termsNote = $derived(`${s.set.context.glossaryNote}. ${s.set.context.terms(terms)}`);
</script>

<div class="page">
  <div class="page-head">
    <h1 class="h-page">{s.set.nav.context}</h1>
    <p class="lead">{c.lead}</p>
  </div>

  <Group title={c.context}>
    <Row label={c.withContext} note={c.withContextNote}>
      <Switch checked={settings.contextEnabled} label={c.withContext} onchange={(v) => updateSettings({ contextEnabled: v })} />
    </Row>
    <Row label={c.fragments}>
      <Slider
        value={settings.contextFragments}
        min={1}
        max={10}
        label={c.fragments}
        onchange={(v) => updateSettings({ contextFragments: v })}
      />
      <span class="value">{settings.contextFragments}</span>
    </Row>
    <Row label={c.glossary} note={termsNote}>
      <Button size="sm" onclick={() => openGlossary()}>{c.editGlossary}</Button>
    </Row>
  </Group>

  <Group title={c.code}>
    <Row label={c.programmer} note={c.programmerNote}>
      <Switch checked={settings.programmerMode} label={c.programmer} onchange={(v) => updateSettings({ programmerMode: v })} />
    </Row>
    <Row label={c.editorApi} note={c.editorApiNote}>
      <Switch checked={settings.editorApi} label={c.editorApi} onchange={(v) => updateSettings({ editorApi: v })} />
    </Row>
  </Group>

  <Group title={c.ultra}>
    <Row label={c.detectEach} note={c.detectEachNote}>
      <Switch checked={settings.ultraDetectEach} label={c.detectEach} onchange={(v) => updateSettings({ ultraDetectEach: v })} />
    </Row>
    <Row
      label={c.skipTargetOn(langOn(settings.targetLang))}
      note={c.skipTargetNote}
    >
      <Switch checked={settings.ultraSkipTarget} label={c.skipTarget} onchange={(v) => updateSettings({ ultraSkipTarget: v })} />
    </Row>
    <Row label={c.showUltra} note={c.showUltraNote}>
      <Switch checked={settings.ultraWindow} label={c.showUltra} onchange={(v) => updateSettings({ ultraWindow: v })} />
    </Row>
  </Group>
</div>

<style>
  .value {
    min-width: 16px;
    text-align: right;
    font: var(--body-medium);
    color: var(--text-primary);
  }
</style>
