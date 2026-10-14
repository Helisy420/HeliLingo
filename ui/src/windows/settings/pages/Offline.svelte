<!-- Настройки / Офлайн и ускорение (Figma 16:3521). Offline is opt-in: until
     the switch is on, the rest stays hidden and nothing is downloaded. Then:
     this PC (graphics card, llama.cpp build), device / threads / precision,
     the models (download with progress, cancel, use, delete) and who reads
     text from images (Windows OCR or Granite Vision). -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { fileSize, t } from '../../../lib/i18n.svelte';
  import {
    offlineCancel,
    offlineDelete,
    offlineDir,
    offlineInstall,
    offlineOpenDir,
    offlinePickDir,
    offlineStatus,
    type ModelDir,
    type OfflineModel,
    type OfflineStatus,
  } from '../../../lib/ipc';
  import { settings, updateSettings } from '../../../lib/settings.svelte';
  import Badge from '../../../lib/ui/Badge.svelte';
  import Bits from '../../../lib/ui/Bits.svelte';
  import Button from '../../../lib/ui/Button.svelte';
  import IconButton from '../../../lib/ui/IconButton.svelte';
  import Segmented from '../../../lib/ui/Segmented.svelte';
  import Slider from '../../../lib/ui/Slider.svelte';
  import Switch from '../../../lib/ui/Switch.svelte';
  import Group from '../Group.svelte';
  import Row from '../Row.svelte';

  const s = $derived(t());
  const o = $derived(s.set.offline);

  let info = $state<OfflineStatus | null>(null);
  const cores = $derived(info?.cores ?? Math.max(2, Math.floor((navigator.hardwareConcurrency || 8) / 2)));
  const translators = $derived(info?.models.filter((m) => m.id !== 'granite') ?? []);
  const granite = $derived(info?.models.find((m) => m.id === 'granite'));
  const graniteReady = $derived(granite?.status.state === 'installed');

  let dir = $state<ModelDir | null>(null);

  async function refresh() {
    [info, dir] = await Promise.all([offlineStatus(), offlineDir()]);
  }

  async function pickDir() {
    const path = await offlinePickDir();
    if (path) await set({ modelDir: path }).then(refresh);
  }

  /** A long path → its start and end ("C:\Users\…\HeliLingo\models"). */
  function shortPath(p: string): string {
    if (p.length <= 34) return p;
    const parts = p.split('\\');
    return `${parts.slice(0, 2).join('\\')}\\…\\${parts.slice(-2).join('\\')}`;
  }

  // Poll while the page is open: fast during a download, slow otherwise.
  onMount(() => {
    let alive = true;
    const tick = async () => {
      if (!alive) return;
      if (settings.offline.enabled) await refresh();
      const busy = info?.models.some((m) => m.status.state === 'downloading');
      setTimeout(() => void tick(), busy ? 500 : 3000);
    };
    void tick();
    return () => (alive = false);
  });

  const set = (patch: Partial<typeof settings.offline>) => updateSettings({ offline: patch });

  function use(m: OfflineModel, on: boolean) {
    const rest = settings.offline.tiers.filter((x) => x !== m.id);
    const tiers = on ? [...rest, m.id] : rest;
    // Catalogue order: fastest first.
    const order = info?.models.map((x) => x.id) ?? [];
    tiers.sort((a, b) => order.indexOf(a) - order.indexOf(b));
    void set({ tiers }).then(refresh);
  }

  async function act(f: (id: string) => Promise<void>, id: string) {
    await f(id);
    await refresh();
  }

  function note(m: OfflineModel): string {
    const text = o.notes[m.id] ?? '';
    const st = m.status;
    const size = st.state === 'installed' || st.state === 'notInstalled' ? fileSize(st.size) : '';
    return size ? `${size} · ${text}` : text;
  }

  const percent = (done: number, total: number) => `${Math.floor((done / Math.max(1, total)) * 100)}%`;

  const gpuLine = $derived(
    info?.gpu ? o.gpuLine(info.gpu.name, (info.gpu.vram / 2 ** 30).toFixed(0), info.backend) : o.noGpu,
  );
</script>

<div class="page">
  <div class="page-head">
    <h1 class="h-page">{s.set.nav.offline}</h1>
    <p class="lead">{o.lead}</p>
  </div>

  <Group title={o.enable}>
    <Row label={o.enable} note={settings.offline.enabled ? o.enableOnNote : o.enableNote}>
      <Switch checked={settings.offline.enabled} label={o.enable} onchange={(v) => void set({ enabled: v }).then(refresh)} />
    </Row>
  </Group>

  {#if settings.offline.enabled}
    <Group title={o.device}>
      <Row label={o.hardware} note={gpuLine} />
      <Row label={o.where}>
        <Segmented
          value={settings.offline.device}
          options={[
            { value: 'auto', label: o.auto },
            { value: 'cpu', label: o.cpu },
            { value: 'gpu', label: o.gpu },
          ]}
          onchange={(v) => set({ device: v as 'auto' | 'cpu' | 'gpu' })}
        />
      </Row>
      <Row label={o.threads} note={o.threadsNote}>
        <Slider value={settings.offline.threads} min={0} max={Math.max(cores * 2, 4)} label={o.threads} onchange={(v) => set({ threads: v })} />
        <span class="value">{settings.offline.threads === 0 ? `${o.threadsAuto} (${cores})` : settings.offline.threads}</span>
      </Row>
      <Row label={o.precision} note={o.precisionNote}>
        <Segmented
          value={settings.offline.precision}
          options={[
            { value: 'auto', label: o.auto },
            { value: 'int8', label: 'int8' },
            { value: 'int8Float16', label: 'int8 + fp16' },
            { value: 'float32', label: 'float32' },
          ]}
          onchange={(v) => set({ precision: v as typeof settings.offline.precision })}
        />
      </Row>
    </Group>

    <Group title={o.models}>
      <p class="group-note">{o.modelsNote}</p>
      {#if dir}
        <Row label={o.folder} note={`${shortPath(dir.path)} · ${o.folderNote(dir.free === null ? '—' : fileSize(dir.free))}`}>
          {#if !dir.isDefault}
            <Button size="sm" onclick={() => void set({ modelDir: '' }).then(refresh)}>{o.byDefault}</Button>
          {/if}
          <IconButton icon="folder" label={o.openFolder} onclick={() => void offlineOpenDir()} />
          <span title={o.folderMoveNote}><Button size="sm" onclick={pickDir}>{o.change}</Button></span>
        </Row>
      {/if}
      {#each translators as m (m.id)}
        {@render model(m)}
      {/each}
    </Group>

    <Group title={o.screen}>
      <Row label={o.screenOcr} note={graniteReady ? o.screenOcrNote : o.graniteNeeded}>
        <Segmented
          value={graniteReady ? settings.offline.screenOcr : 'windows'}
          options={[
            { value: 'windows', label: o.ocrWindows },
            { value: 'granite', label: o.ocrGranite, disabled: !graniteReady },
          ]}
          onchange={(v) => void set({ screenOcr: v as 'windows' | 'granite' }).then(refresh)}
        />
      </Row>
      {#if granite}
        {@render model(granite)}
      {/if}
    </Group>
  {/if}
</div>

{#snippet model(m: OfflineModel)}
  <div class="model" class:failed={m.status.state === 'failed'}>
    <Row
      label={m.name}
      note={m.status.state === 'failed'
        ? undefined
        : m.status.state === 'downloading'
          ? `${fileSize(m.status.done)} / ${fileSize(m.status.total)}`
          : note(m)}
    >
      {#if m.status.state === 'downloading'}
        <span class="progress">
          <Bits kind="progress" width={120} value={m.status.done / Math.max(1, m.status.total)} />
          <span class="pct">{percent(m.status.done, m.status.total)}</span>
        </span>
        <Button size="sm" onclick={() => act(offlineCancel, m.id)}>{o.cancel}</Button>
      {:else if m.status.state === 'installed'}
        {#if m.loaded}<Badge dot>{o.inMemory}</Badge>{/if}
        <IconButton icon="trash" label={o.remove} onclick={() => act(offlineDelete, m.id)} />
        {#if m.id !== 'granite'}
          <Switch checked={m.enabled} label={`${o.use}: ${m.name}`} onchange={(v) => use(m, v)} />
        {/if}
      {:else if m.status.state === 'failed'}
        <Button size="sm" icon="download" onclick={() => act(offlineInstall, m.id)}>{o.retry}</Button>
      {:else}
        <Button size="sm" icon="download" onclick={() => act(offlineInstall, m.id)}>
          {m.status.partial ? o.resume : o.download}
        </Button>
      {/if}
    </Row>
    {#if m.status.state === 'failed'}
      <p class="err">{o.errors[m.status.error] ?? m.status.error}</p>
    {/if}
  </div>
{/snippet}

<style>
  .value {
    min-width: 64px;
    text-align: right;
    font: var(--body-medium);
    color: var(--text-primary);
    white-space: nowrap;
  }
  .group-note {
    margin: 10px 0 0;
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .progress {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .pct {
    min-width: 34px;
    text-align: right;
    font: var(--caption-medium);
    color: var(--text-secondary);
    font-variant-numeric: tabular-nums;
  }
  /* The error goes where the row's note would be. */
  .err {
    position: absolute;
    left: 0;
    bottom: 14px;
    margin: 0;
    font: var(--caption);
    color: var(--danger);
  }
  .model {
    position: relative;
  }
  .failed :global(.row) {
    padding-bottom: 32px;
  }
</style>
