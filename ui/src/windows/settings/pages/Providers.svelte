<!-- Настройки / Провайдеры (Figma 16:2894): cloud engines in order (on/off,
     key, «по умолчанию» on the first working one), fallback, check.
     Under an engine, when it applies: Yandex folder id and Azure region
     (with a key), the OpenRouter model (a searchable list). -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { engineName, engineNote, t } from '../../../lib/i18n.svelte';
  import { checkEngines, engines, setKey, usable, type Check, type EngineInfo } from '../../../lib/ipc';
  import { inApp } from '../../../lib/ipc';
  import { settings, updateSettings } from '../../../lib/settings.svelte';
  import Badge from '../../../lib/ui/Badge.svelte';
  import Button from '../../../lib/ui/Button.svelte';
  import Choice from '../../../lib/ui/Choice.svelte';
  import Field from '../../../lib/ui/Field.svelte';
  import IconButton from '../../../lib/ui/IconButton.svelte';
  import ModelMenu from '../../../lib/ui/ModelMenu.svelte';
  import SelectButton from '../../../lib/ui/SelectButton.svelte';
  import Switch from '../../../lib/ui/Switch.svelte';
  import Group from '../Group.svelte';
  import Row from '../Row.svelte';

  const s = $derived(t());
  const p = $derived(s.set.providers);

  let list = $state<EngineInfo[]>([]);
  /** Engine whose key field is open for typing. */
  let editing = $state<string | null>(null);
  let draft = $state('');
  let checks = $state<Check[] | null>(null);
  let checking = $state(false);
  let modelsOpen = $state(false);
  let folder = $state(settings.yandexFolder);
  let region = $state(settings.bingRegion);

  // Settings can arrive after the page opens.
  $effect(() => {
    folder = settings.yandexFolder;
  });
  $effect(() => {
    region = settings.bingRegion;
  });

  /** Saves a text setting a moment after typing stops. */
  function debounced(save: (v: string) => void) {
    let id: ReturnType<typeof setTimeout> | undefined;
    return (v: string) => {
      clearTimeout(id);
      id = setTimeout(() => save(v.trim()), 500);
    };
  }
  const saveFolder = debounced((v) => void updateSettings({ yandexFolder: v }));
  const saveRegion = debounced((v) => void updateSettings({ bingRegion: v }));

  function pickModel(id: string) {
    modelsOpen = false;
    void updateSettings({ openrouterModel: id }).then(load);
  }

  const firstWorking = $derived(list.find((e) => usable(e))?.id);

  async function load() {
    list = await engines();
  }

  onMount(() => {
    void load();
    if (!inApp) return;
    let off = () => {};
    void import('@tauri-apps/api/event').then(async ({ listen }) => {
      off = await listen('engines', () => void load());
    });
    return () => off();
  });

  function setEnabled(id: string, on: boolean) {
    const next = settings.engines.map((e) => (e.id === id ? { ...e, enabled: on } : e));
    void updateSettings({ engines: next }).then(load);
  }

  function move(id: string, by: -1 | 1) {
    const next = [...settings.engines];
    const i = next.findIndex((e) => e.id === id);
    const j = i + by;
    if (i < 0 || j < 0 || j >= next.length) return;
    [next[i], next[j]] = [next[j]!, next[i]!];
    void updateSettings({ engines: next }).then(load);
  }

  async function saveKey(id: string) {
    await setKey(id, draft);
    editing = null;
    draft = '';
    await load();
  }

  async function check() {
    checking = true;
    checks = await checkEngines();
    checking = false;
  }

  const allOk = $derived(checks !== null && checks.length > 0 && checks.every((c) => c.ok));
</script>

<div class="page">
  <h1 class="h-page">{s.set.nav.providers}</h1>

  <Group title={p.cloudEngines}>
    {#each list as e, i (e.id)}
      <div class="engine">
        <!-- Order: in the page's left gutter, shown on hover. -->
        <span class="order">
          <IconButton icon="chev-u" label={p.moveUp} disabled={i === 0} onclick={() => move(e.id, -1)} />
          <IconButton icon="chev-d" label={p.moveDown} disabled={i === list.length - 1} onclick={() => move(e.id, 1)} />
        </span>
        <Row label={engineName(e.id)} note={engineNote(e)}>
          {#if e.id === firstWorking}
            <Badge tone="accent">{p.default}</Badge>
          {:else if !e.implemented}
            <span title={p.soon}><Badge>{s.notReady}</Badge></span>
          {/if}
          {#if e.needsKey || e.keyOptional}
            {#if editing === e.id}
              <span class="key-edit">
                <Field bind:value={draft} icon="key" password placeholder={p.keyPlaceholder} width={190} />
                <Button variant="primary" size="sm" onclick={() => saveKey(e.id)}>{p.save}</Button>
              </span>
            {:else if e.keyHint}
              <!-- Figma: the stored key as a masked field; a click replaces it. -->
              <button class="key" onclick={() => ((editing = e.id), (draft = ''))} title={p.removeKey}>
                <Field value={e.keyHint} icon="key" width={150} />
              </button>
            {:else if e.needsKey}
              <Button size="sm" icon="key-14" onclick={() => ((editing = e.id), (draft = ''))}>{p.addKey}</Button>
            {:else}
              <IconButton icon="key" label={p.ownKey} onclick={() => ((editing = e.id), (draft = ''))} />
            {/if}
          {/if}
          <Switch checked={e.enabled} label={engineName(e.id)} onchange={(v) => setEnabled(e.id, v)} />
        </Row>
        {#if e.id === 'yandex' && e.keyHint}
          <div class="sub">
            <Row label={p.folderId} note={p.folderNote}>
              <Field bind:value={folder} placeholder="b1g…" width={190} oninput={() => saveFolder(folder)} />
            </Row>
          </div>
        {:else if e.id === 'bing' && e.keyHint}
          <div class="sub">
            <Row label={p.region} note={p.regionNote}>
              <Field bind:value={region} placeholder="westeurope" width={190} oninput={() => saveRegion(region)} />
            </Row>
          </div>
        {:else if e.id === 'openrouter'}
          <div class="sub">
            <Row label={p.model} note={p.modelNote}>
              <span class="anchor" data-menu-anchor>
                <SelectButton open={modelsOpen} width={240} onclick={() => (modelsOpen = !modelsOpen)}>
                  {settings.openrouterModel ?? p.chooseModel}
                </SelectButton>
                {#if modelsOpen}
                  <ModelMenu value={settings.openrouterModel} onpick={pickModel} onclose={() => (modelsOpen = false)} />
                {/if}
              </span>
            </Row>
          </div>
        {/if}
      </div>
    {/each}
  </Group>

  <Group title={p.fallback}>
    <Row label={p.switchOnError} note={p.switchOnErrorNote}>
      <Switch checked={settings.fallback} label={p.switchOnError} onchange={(v) => updateSettings({ fallback: v })} />
    </Row>
    <Row label={p.timeout}>
      <Choice
        value={settings.timeoutSecs}
        width={125}
        options={[2, 3, 5, 10, 15, 30].map((n) => ({ value: n, label: p.seconds(n) }))}
        onchange={(v) => updateSettings({ timeoutSecs: v })}
      />
    </Row>
    <Row label={p.googleMode}>
      <Choice
        value={settings.googleMode}
        width={125}
        options={[
          { value: 'auto', label: p.googleAuto },
          { value: 'web', label: p.googleWeb },
          { value: 'api', label: p.googleApi },
        ]}
        onchange={(v) => updateSettings({ googleMode: v })}
      />
    </Row>
  </Group>

  <Group title={p.check}>
    <Row label={p.checkConnection} note={p.checkNote}>
      {#if checking}
        <Badge>{p.checking}</Badge>
      {:else if checks}
        <Badge dot={allOk}>{allOk ? p.works : p.fails}</Badge>
      {/if}
      <Button size="sm" icon="check" onclick={check} disabled={checking}>{p.checkButton}</Button>
    </Row>
    {#if checks}
      {#each checks as c (c.id)}
        <div class="result">
          <span class="dot" class:ok={c.ok}></span>
          <span class="name">{engineName(c.id)}</span>
          <span class="detail">{c.ok ? `${c.ms} ms` : s.errors[c.error?.kind ?? 'other']}</span>
        </div>
      {/each}
    {/if}
  </Group>
</div>

<style>
  .engine {
    position: relative;
  }
  .order {
    position: absolute;
    left: -30px;
    top: 10px;
    display: flex;
    flex-direction: column;
    opacity: 0;
    transition: opacity 120ms;
  }
  .engine:hover .order,
  .engine:focus-within .order {
    opacity: 1;
  }
  .order :global(.ib) {
    width: 24px;
    height: 24px;
  }
  .sub {
    padding-left: 16px;
  }
  .anchor {
    position: relative;
    display: inline-flex;
  }
  .key-edit {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .key {
    display: contents;
  }
  .key :global(.field) {
    cursor: pointer;
  }
  .key :global(input) {
    pointer-events: none;
  }
  .result {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
    font: var(--body);
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--danger);
  }
  .dot.ok {
    background: var(--status-ok);
  }
  .name {
    color: var(--text-primary);
  }
  .detail {
    color: var(--text-secondary);
    font: var(--caption);
  }
</style>
