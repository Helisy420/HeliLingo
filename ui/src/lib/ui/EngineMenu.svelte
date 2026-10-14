<!-- Engine list (Figma «Движок перевода»): «Облако» and «Без интернета»
     groups, name + note per row. Engines that aren't ready are greyed. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { engineName, engineNote, t } from '../i18n.svelte';
  import { engines, usable, type EngineInfo } from '../ipc';
  import Menu from './Menu.svelte';
  import MenuItem from './MenuItem.svelte';

  let {
    value,
    align = 'right',
    onpick,
    onclose,
  }: {
    value: string;
    align?: 'left' | 'right';
    onpick: (id: string) => void;
    onclose: () => void;
  } = $props();

  const s = $derived(t());
  let list = $state<EngineInfo[]>([]);
  const icons: Record<string, string> = { google: 'globe-18', yandex: 'globe-18', openrouter: 'cloud-18' };

  onMount(() => {
    void engines().then((l) => (list = l.filter((e) => e.enabled)));
    const esc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onclose();
      }
    };
    window.addEventListener('keydown', esc, true);
    return () => window.removeEventListener('keydown', esc, true);
  });

  const cloud = $derived(list.filter((e) => e.group === 'cloud'));
  const offline = $derived(list.filter((e) => e.group === 'offline'));
</script>

<Menu width={269} {align} {onclose}>
  <div class="caption-row">{s.cloud}</div>
  {#each cloud as e (e.id)}
    <MenuItem
      label={engineName(e.id)}
      note={!e.implemented
        ? `${engineNote(e)} · ${s.notReady}`
        : e.needsKey && !e.keyHint
          ? s.noKeyYet
          : e.needsModel
            ? s.noModelYet
            : engineNote(e)}
      icon={icons[e.id] ?? 'key-18'}
      selected={e.id === value}
      disabled={!usable(e)}
      onpick={() => onpick(e.id)}
    />
  {/each}
  {#if offline.length}
    <div class="caption-row">{s.offline}</div>
    {#each offline as e (e.id)}
      <MenuItem
        label={e.name}
        note={engineNote(e)}
        icon="cpu-18"
        trailing={s.offlineTag}
        selected={e.id === value}
        disabled={!usable(e)}
        onpick={() => onpick(e.id)}
      />
    {/each}
  {/if}
</Menu>
