<!-- Настройки / Баблы и Вики (Figma 36:243, 53:240). Every choice that
     changes a look has a live preview drawn with the real `Bubble`
     component (the previews aren't in Figma — the user asked for them to
     look finished). The double-Ctrl bubble's styles are previewed and
     picked here; so is the Quick translate layout. -->
<script lang="ts">
  import Bubble from '../../../lib/bubble/Bubble.svelte';
  import { SAMPLE_WIKI, samples, wantsWiki, type BubbleActions, type BubbleLook } from '../../../lib/bubble/types';
  import { isEnglish, t } from '../../../lib/i18n.svelte';
  import { settings, updateSettings, type BubbleSettings, type QuickSettings } from '../../../lib/settings.svelte';
  import Choice from '../../../lib/ui/Choice.svelte';
  import Segmented from '../../../lib/ui/Segmented.svelte';
  import Slider from '../../../lib/ui/Slider.svelte';
  import Switch from '../../../lib/ui/Switch.svelte';
  import Group from '../Group.svelte';
  import Preview from '../Preview.svelte';
  import QuickSketch from '../QuickSketch.svelte';
  import Row from '../Row.svelte';

  const s = $derived(t());
  const b = $derived(s.set.bubbles);
  const sample = $derived(samples(!isEnglish()));

  const setB = (patch: Partial<BubbleSettings>) => updateSettings({ bubble: patch });
  const setQ = (patch: Partial<QuickSettings>) => updateSettings({ quick: patch });

  const style = $derived(settings.popupStyle === 'compact' ? 'translation' : settings.popupStyle);
  const look = $derived<BubbleLook>({
    style,
    collapseLong: settings.bubble.collapseLong,
    wordMode: settings.bubble.wordMode,
    pos: settings.bubble.showPos,
    variants: settings.bubble.showVariants,
    wiki: settings.showWiki,
    wikiImage: settings.bubble.wikiImage,
    fontSize: settings.bubble.fontSize,
    opacity: settings.bubble.opacity,
    animation: settings.bubble.animation,
  });

  /** Previews react to clicks the way the real bubble does (copy winks). */
  let winked = $state(false);
  const actions: BubbleActions = {
    copy: () => {
      winked = false;
      requestAnimationFrame(() => (winked = true));
      setTimeout(() => (winked = false), 800);
    },
    replace: () => {},
    speak: () => {},
    close: () => {},
    pin: () => {},
    wiki: () => {},
    openUrl: () => {},
    settings: () => {},
    google: () => {},
    drag: () => {},
  };

  let styleSample = $state<'short' | 'long' | 'word'>('short');
  let lookSample = $state<'short' | 'long'>('short');

  const quickSample = $derived(isEnglish() ? 'Где ближайшая аптека?' : 'Where is the nearest pharmacy?');
  const quickResult = $derived(isEnglish() ? 'Where is the nearest pharmacy?' : 'Где ближайшая аптека?');
</script>

<div class="page">
  <div class="page-head">
    <h1 class="h-page">{s.set.nav.bubbles}</h1>
    <p class="lead">{b.lead}</p>
  </div>

  <Group title={b.group}>
    <Row label={b.style}>
      <Segmented
        value={style}
        options={[
          { value: 'translation', label: b.styles.translation!, icon: 'languages' },
          { value: 'buttons', label: b.styles.buttons!, icon: 'copy' },
          { value: 'full', label: b.styles.full!, icon: 'appwin-18' },
        ]}
        onchange={(v) => updateSettings({ popupStyle: v as 'full' | 'translation' | 'buttons' })}
      />
    </Row>
    <Preview
      label={b.preview}
      bind:sample={styleSample}
      samples={[
        { value: 'short', label: isEnglish() ? 'Phrase' : 'Фраза' },
        { value: 'long', label: isEnglish() ? 'Long text' : 'Длинный текст' },
        { value: 'word', label: isEnglish() ? 'Word' : 'Слово' },
      ]}
      minHeight={styleSample === 'word' ? 250 : 150}
    >
      <Bubble
        data={sample[styleSample]}
        {look}
        wiki={styleSample === 'word' && settings.showWiki && wantsWiki(settings.bubble.wordMode) ? SAMPLE_WIKI : 'off'}
        {winked}
        {actions}
        preview
      />
    </Preview>
    <Row label={b.collapseLong} note={b.collapseLongNote}>
      <Switch checked={settings.bubble.collapseLong} label={b.collapseLong} onchange={(v) => setB({ collapseLong: v })} />
    </Row>
    <Row label={b.closeOutside}>
      <Switch checked={settings.bubble.closeOnOutside} label={b.closeOutside} onchange={(v) => setB({ closeOnOutside: v })} />
    </Row>
  </Group>

  <Group title={b.wikiGroup}>
    <Row label={b.wiki} note={b.wikiNote}>
      <Switch checked={settings.showWiki} label={b.wiki} onchange={(v) => updateSettings({ showWiki: v })} />
    </Row>
    {#if settings.showWiki}
      <Row label={b.wikiLang}>
        <Choice
          value={settings.bubble.wikiLang}
          width={170}
          options={(['word', 'translation', 'interface'] as const).map((v) => ({ value: v, label: b.wikiLangs[v]! }))}
          onchange={(v) => setB({ wikiLang: v })}
        />
      </Row>
      <Row label={b.wikiSize}>
        <Choice
          value={settings.bubble.wikiSize}
          width={170}
          options={(['short', 'long'] as const).map((v) => ({ value: v, label: b.wikiSizes[v]! }))}
          onchange={(v) => setB({ wikiSize: v })}
        />
      </Row>
      <Row label={b.wikiImage}>
        <Switch checked={settings.bubble.wikiImage} label={b.wikiImage} onchange={(v) => setB({ wikiImage: v })} />
      </Row>
    {/if}
  </Group>

  <!-- How the bubble opens (double Ctrl, double Ctrl+C, mouse buttons)
       is set only on «Горячие клавиши»: one place per setting. -->
  <Group title={b.appear}>
    <Row label={b.placement}>
      <Choice
        value={settings.bubble.placement}
        width={170}
        options={(['selection', 'cursor', 'center'] as const).map((v) => ({ value: v, label: b.placements[v]! }))}
        onchange={(v) => setB({ placement: v })}
      />
    </Row>
    <Row label={b.delay}>
      <Slider value={settings.bubble.delayMs} min={0} max={1000} step={100} label={b.delay} onchange={(v) => setB({ delayMs: v })} />
      <span class="value">{b.seconds(settings.bubble.delayMs / 1000)}</span>
    </Row>
  </Group>

  <Group title={b.words}>
    <Row label={b.wordMode}>
      <Segmented
        value={settings.bubble.wordMode}
        options={[
          { value: 'dictionary', label: b.wordModes.dictionary!, icon: 'languages' },
          { value: 'wiki', label: b.wordModes.wiki!, icon: 'globe', disabled: !settings.showWiki },
          { value: 'both', label: b.wordModes.both!, icon: 'layers', disabled: !settings.showWiki },
          { value: 'none', label: b.wordModes.none!, icon: 'minus-14' },
        ]}
        onchange={(v) => setB({ wordMode: v as BubbleSettings['wordMode'] })}
      />
    </Row>
    {#if style !== 'full' && settings.bubble.wordMode !== 'none'}
      <!-- Compact styles only: the card always shows both. «Ничего» leaves
           nothing for them to hide. -->
      <Row label={b.showPos} note={b.showPosNote}>
        <Switch checked={settings.bubble.showPos} label={b.showPos} onchange={(v) => setB({ showPos: v })} />
      </Row>
      <Row label={b.showVariants} note={b.showVariantsNote}>
        <Switch checked={settings.bubble.showVariants} label={b.showVariants} onchange={(v) => setB({ showVariants: v })} />
      </Row>
    {/if}
    <Preview label={b.preview} minHeight={200}>
        <Bubble
          data={sample.word}
          {look}
          wiki={settings.showWiki && wantsWiki(settings.bubble.wordMode) ? SAMPLE_WIKI : 'off'}
          {winked}
          {actions}
          preview
        />
      </Preview>
  </Group>

  <Group title={b.autoClose}>
    <Row label={b.closeAfter}>
      <Choice
        value={settings.bubble.autoCloseSecs}
        width={170}
        options={[0, 3, 5, 10, 30].map((n) => ({ value: n, label: n ? b.afterSecs(n) : b.never }))}
        onchange={(v) => setB({ autoCloseSecs: v })}
      />
    </Row>
    <Row label={b.keepOnHover}>
      <Switch checked={settings.bubble.keepOnHover} label={b.keepOnHover} onchange={(v) => setB({ keepOnHover: v })} />
    </Row>
    <Row label={b.rememberPinned}>
      <Switch checked={settings.bubble.rememberPinned} label={b.rememberPinned} onchange={(v) => setB({ rememberPinned: v })} />
    </Row>
  </Group>

  <Group title={b.look}>
    <Row label={b.fontSize}>
      <Slider value={settings.bubble.fontSize} min={12} max={20} label={b.fontSize} onchange={(v) => setB({ fontSize: v })} />
      <span class="value">{settings.bubble.fontSize} px</span>
    </Row>
    <Row label={b.opacity}>
      <Slider value={settings.bubble.opacity} min={60} max={100} step={5} label={b.opacity} onchange={(v) => setB({ opacity: v })} />
      <span class="value">{settings.bubble.opacity}%</span>
    </Row>
    <Row label={b.animation}>
      <Switch checked={settings.bubble.animation} label={b.animation} onchange={(v) => setB({ animation: v })} />
    </Row>
    <Preview
      label={b.preview}
      bind:sample={lookSample}
      samples={[
        { value: 'short', label: isEnglish() ? 'Phrase' : 'Фраза' },
        { value: 'long', label: isEnglish() ? 'Long text' : 'Длинный текст' },
      ]}
      minHeight={150}
    >
      <Bubble data={sample[lookSample]} {look} {winked} {actions} preview />
    </Preview>
  </Group>

  <Group title={b.quick}>
    <Row label={b.quickLayout}>
      <Segmented
        value={settings.quick.layout}
        options={[
          { value: 'window', label: b.quickLayouts.window!, icon: 'appwin-18' },
          { value: 'line', label: b.quickLayouts.line!, icon: 'minus-14' },
        ]}
        onchange={(v) => setQ({ layout: v as QuickSettings['layout'] })}
      />
    </Row>
    {#if settings.quick.layout === 'window'}
      <Row label={b.suggestions} note={b.suggestionsNote}>
        <Switch checked={settings.quick.suggestions} label={b.suggestions} onchange={(v) => setQ({ suggestions: v })} />
      </Row>
      <Row label={b.history} note={b.historyNote}>
        <Switch checked={settings.quick.history} label={b.history} onchange={(v) => setQ({ history: v })} />
      </Row>
      <Row label={b.miniButtons} note={b.miniButtonsNote}>
        <Switch checked={settings.quick.miniButtons} label={b.miniButtons} onchange={(v) => setQ({ miniButtons: v })} />
      </Row>
    {/if}
    <Preview label={b.preview} minHeight={settings.quick.layout === 'line' ? 120 : 200}>
      <QuickSketch {...settings.quick} text={quickSample} translation={quickResult} />
    </Preview>
  </Group>
</div>

<style>
  .value {
    min-width: 48px;
    text-align: right;
    font: var(--body-medium);
    color: var(--text-primary);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
</style>
