<!-- Настройки / Горячие клавиши (Figma 16:3014): each action with its keys
     and «Изменить». Changing: press the new chord (Esc cancels, Backspace
     turns the shortcut off); pressing a lone modifier twice sets a double
     tap (Ctrl×2). Mouse side buttons work too, with modifiers held
     (Ctrl+Alt+Mouse5) or pressed twice (Mouse4×2). -->
<script lang="ts">
  import { t } from '../../../lib/i18n.svelte';
  import { DEFAULTS, settings, updateSettings, type Hotkeys } from '../../../lib/settings.svelte';
  import Button from '../../../lib/ui/Button.svelte';
  import IconButton from '../../../lib/ui/IconButton.svelte';
  import Keys from '../../../lib/ui/Keys.svelte';
  import Slider from '../../../lib/ui/Slider.svelte';
  import Switch from '../../../lib/ui/Switch.svelte';
  import Group from '../Group.svelte';
  import Row from '../Row.svelte';

  type Key = Exclude<keyof Hotkeys, 'doubleCopy'>;
  const s = $derived(t());
  const h = $derived(s.set.hotkeys);

  let editing = $state<Key | null>(null);
  /** Last lone modifier press, for double taps. */
  let lastMod: { key: string; at: number } | null = null;

  const MODS: Record<string, string> = { Control: 'Ctrl', Shift: 'Shift', Alt: 'Alt' };

  function keyName(e: KeyboardEvent): string | null {
    if (e.code.startsWith('Key')) return e.code.slice(3);
    if (e.code.startsWith('Digit')) return e.code.slice(5);
    if (/^F\d+$/.test(e.code)) return e.code;
    const named: Record<string, string> = {
      Space: 'Space',
      Enter: 'Enter',
      Tab: 'Tab',
      Backquote: '`',
      Minus: '-',
      Equal: '=',
      BracketLeft: '[',
      BracketRight: ']',
      Semicolon: ';',
      Quote: "'",
      Comma: ',',
      Period: '.',
      Slash: '/',
      Insert: 'Insert',
      Home: 'Home',
      End: 'End',
      PageUp: 'PageUp',
      PageDown: 'PageDown',
    };
    return named[e.code] ?? null;
  }

  function save(key: Key, value: string) {
    editing = null;
    void updateSettings({ hotkeys: { [key]: value } });
  }

  function onKey(e: KeyboardEvent) {
    if (!editing) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape') {
      editing = null;
      return;
    }
    if (e.key === 'Backspace' || e.key === 'Delete') {
      save(editing, '');
      return;
    }
    const mod = MODS[e.key];
    if (mod) {
      // A lone modifier pressed twice quickly = double tap.
      const now = performance.now();
      if (lastMod && lastMod.key === mod && now - lastMod.at < settings.doubleTapMs) {
        save(editing, `${mod}×2`);
        lastMod = null;
      } else {
        lastMod = { key: mod, at: now };
      }
      return;
    }
    const name = keyName(e);
    if (!name) return;
    const parts = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift'].filter(Boolean) as string[];
    // Plain keys would fire while typing anywhere: need a modifier (or F-keys).
    if (!parts.length && !/^F\d+$/.test(name)) return;
    save(editing, [...parts, name].join('+'));
  }

  /** A side button just pressed: saved after the double-tap time unless
      the same button (with the same modifiers) comes again → «×2». */
  let pendingMouse = $state<{ combo: string; timer: ReturnType<typeof setTimeout> } | null>(null);

  function onMouse(e: MouseEvent) {
    if (e.button !== 3 && e.button !== 4) return;
    // WebView2 turns side buttons into Back / Forward: never here.
    e.preventDefault();
    if (!editing) return;
    e.stopPropagation();
    const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift'].filter(Boolean) as string[];
    const combo = [...mods, e.button === 3 ? 'Mouse4' : 'Mouse5'].join('+');
    const key = editing;
    if (pendingMouse) clearTimeout(pendingMouse.timer);
    if (pendingMouse?.combo === combo) {
      pendingMouse = null;
      save(key, `${combo}×2`);
      return;
    }
    pendingMouse = {
      combo,
      timer: setTimeout(() => {
        pendingMouse = null;
        if (editing === key) save(key, combo);
      }, settings.doubleTapMs),
    };
  }

  function noNavigate(e: MouseEvent) {
    if (e.button === 3 || e.button === 4) e.preventDefault();
  }

  const rows = $derived<{ group: string; items: { key: Key; label: string; note?: string }[] }[]>([
    {
      group: h.translation,
      items: [
        { key: 'translateSelection', label: h.translateSelection, note: h.translateSelectionNote },
        { key: 'quick', label: h.quick, note: h.quickNote },
        { key: 'translator', label: h.translator },
      ],
    },
    {
      group: h.actions,
      items: [
        { key: 'ultra', label: h.ultra },
        { key: 'copyTranslation', label: h.copyTranslation },
        { key: 'screenshot', label: h.screenshot },
        { key: 'windowCapture', label: h.windowCapture },
        { key: 'pasteTranslation', label: h.pasteTranslation },
      ],
    },
  ]);
</script>

<svelte:window onkeydowncapture={onKey} onmousedowncapture={onMouse} onmouseupcapture={noNavigate} onauxclick={noNavigate} />

<div class="page">
  <h1 class="h-page">{s.set.nav.hotkeys}</h1>

  {#each rows as g (g.group)}
    <Group title={g.group}>
      {#each g.items as r (r.key)}
        <Row label={r.label} note={r.note}>
          {#if editing === r.key && pendingMouse}
            <Keys shortcut={pendingMouse.combo} />
          {:else if editing === r.key}
            <span class="press">{h.press}</span>
          {:else if settings.hotkeys[r.key]}
            <Keys shortcut={settings.hotkeys[r.key]} />
          {:else}
            <span class="off">{h.off}</span>
          {/if}
          <Button variant="ghost" size="sm" onclick={() => (editing = editing === r.key ? null : r.key)}>
            {editing === r.key ? s.set.offline.cancel : h.change}
          </Button>
          {#if settings.hotkeys[r.key] && editing !== r.key}
            <!-- Unbinding: the same as Backspace while changing. -->
            <IconButton size="sm" icon="x" label={h.remove} onclick={() => save(r.key, '')} />
          {/if}
        </Row>
      {/each}
    </Group>
  {/each}

  <Group title={h.doubleCopy}>
    <Row label={h.doubleCopy} note={h.doubleCopyNote}>
      <Switch
        checked={settings.hotkeys.doubleCopy}
        label={h.doubleCopy}
        onchange={(v) => updateSettings({ hotkeys: { doubleCopy: v } })}
      />
    </Row>
    <Row label={h.doubleTap}>
      <Slider
        value={settings.doubleTapMs}
        min={200}
        max={800}
        step={50}
        label={h.doubleTap}
        onchange={(v) => updateSettings({ doubleTapMs: v })}
      />
      <span class="value">{s.ms(settings.doubleTapMs)}</span>
    </Row>
  </Group>

  <div>
    <Button size="sm" icon="history" onclick={() => updateSettings({ hotkeys: DEFAULTS.hotkeys, doubleTapMs: DEFAULTS.doubleTapMs })}>
      {h.reset}
    </Button>
  </div>
</div>

<style>
  .press {
    font: var(--caption-medium);
    color: var(--accent);
  }
  .off {
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .value {
    min-width: 56px;
    text-align: right;
    font: var(--body-medium);
    color: var(--text-primary);
  }
</style>
