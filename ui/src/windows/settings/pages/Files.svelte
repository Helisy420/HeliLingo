<!-- Настройки / Изображения и файлы (Figma 53:562): how a translated screen
     area is shown, and what translated documents look like and where they
     go. The Figma frame also has «Выбрать область» / «Выбрать окно»
     shortcut rows: shortcuts live only on «Горячие клавиши» (one place per
     setting), so they aren't repeated here. The views and the file
     translator these settings drive are TODO §8. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../../../lib/i18n.svelte';
  import { filesDir, filesPickDir, type FilesDir } from '../../../lib/ipc';
  import {
    settings,
    updateSettings,
    type FilesSettings,
    type ImagesSettings,
  } from '../../../lib/settings.svelte';
  import Button from '../../../lib/ui/Button.svelte';
  import Choice from '../../../lib/ui/Choice.svelte';
  import Segmented from '../../../lib/ui/Segmented.svelte';
  import Slider from '../../../lib/ui/Slider.svelte';
  import Switch from '../../../lib/ui/Switch.svelte';
  import Group from '../Group.svelte';
  import Row from '../Row.svelte';

  const s = $derived(t());
  const f = $derived(s.set.files);

  const setI = (patch: Partial<ImagesSettings>) => updateSettings({ images: patch });
  const setF = (patch: Partial<FilesSettings>) => updateSettings({ files: patch });

  let dir = $state<FilesDir | null>(null);
  const refresh = async () => (dir = await filesDir());
  onMount(refresh);

  async function pickDir() {
    const path = await filesPickDir();
    if (path) await setF({ outDir: path }).then(refresh);
  }

  /** A long path → its start and end ("C:\Users\…\Documents\HeliLingo"). */
  function shortPath(p: string): string {
    if (p.length <= 48) return p;
    const parts = p.split('\\');
    return `${parts.slice(0, 2).join('\\')}\\…\\${parts.slice(-2).join('\\')}`;
  }
</script>

<div class="page">
  <div class="page-head">
    <h1 class="h-page">{s.set.nav.files}</h1>
    <p class="lead">{f.lead}</p>
  </div>

  <Group title={f.area}>
    <Row label={f.mode}>
      <Segmented
        value={settings.images.mode}
        options={[
          { value: 'over', label: f.modes.over!, icon: 'layers' },
          { value: 'plate', label: f.modes.plate!, icon: 'plate' },
          { value: 'side', label: f.modes.side!, icon: 'columns' },
        ]}
        onchange={(v) => setI({ mode: v as ImagesSettings['mode'] })}
      />
    </Row>
    <Row label={f.backdrop}>
      <Slider
        value={settings.images.backdropOpacity}
        min={30}
        max={100}
        step={5}
        label={f.backdrop}
        onchange={(v) => setI({ backdropOpacity: v })}
      />
      <span class="value">{settings.images.backdropOpacity} %</span>
    </Row>
    <Row label={f.altOriginal}>
      <Switch checked={settings.images.altOriginal} label={f.altOriginal} onchange={(v) => setI({ altOriginal: v })} />
    </Row>
  </Group>

  <Group title={f.files}>
    <Row label={f.format}>
      <Choice
        value={settings.files.format}
        width={190}
        options={(['same', 'docx', 'txt'] as const).map((v) => ({ value: v, label: f.formats[v]! }))}
        onchange={(v) => setF({ format: v })}
      />
    </Row>
    <Row label={f.keepFormatting}>
      <Switch
        checked={settings.files.keepFormatting}
        label={f.keepFormatting}
        onchange={(v) => setF({ keepFormatting: v })}
      />
    </Row>
    <Row label={f.bilingual} note={f.bilingualNote}>
      <Switch checked={settings.files.bilingual} label={f.bilingual} onchange={(v) => setF({ bilingual: v })} />
    </Row>
    <Row label={f.outDir} note={dir ? (dir.isDefault ? f.defaultDir : shortPath(dir.path)) : ' '}>
      <Button size="sm" onclick={pickDir}>{f.change}</Button>
    </Row>
  </Group>
</div>

<style>
  .value {
    min-width: 40px;
    text-align: right;
    font: var(--body-medium);
    color: var(--text-primary);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
</style>
