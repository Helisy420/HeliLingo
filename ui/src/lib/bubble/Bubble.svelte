<!-- The translation bubble in every style (Figma section 4, «Баблы»):

     - full: the card — language pair, translation, pick words, Replace / Copy;
     - translation: only the translation (34:199); a long one collapses to
       three lines with «Развернуть» (34:222) and scrolls when expanded (34:243);
     - buttons: the same with mini-buttons — speak, copy, expand (35:167).

     A word or short term gets a word row (translation, part of speech,
     «также …») and, by «Для одиночных слов», the dictionary (51:231), the
     Wikipedia card (34:266 / 34:301) or both. Copying "winks" the bubble.

     Pure view: the double-Ctrl popup feeds it live data, Settings feeds it
     samples. `card` is the bubble's outer element (the popup sizes its
     window from it). -->
<script lang="ts">
  import { engineName, posName, posShort, t } from '../i18n.svelte';
  import { langName, sameLang } from '../langs';
  import { sentences } from '../text';
  import Button from '../ui/Button.svelte';
  import Icon from '../ui/Icon.svelte';
  import IconButton from '../ui/IconButton.svelte';
  import { isWordLike, wantsDict, type BubbleActions, type BubbleData, type BubbleLook, type WikiState } from './types';

  let {
    data,
    look,
    wiki = 'off',
    pinned = false,
    winked = false,
    actions,
    card = $bindable(),
    preview = false,
  }: {
    data: BubbleData;
    look: BubbleLook;
    wiki?: WikiState;
    pinned?: boolean;
    /** Just copied: play the wink. */
    winked?: boolean;
    actions: BubbleActions;
    card?: HTMLElement;
    /** In Settings: no window to size, no hover-only controls. */
    preview?: boolean;
  } = $props();

  const s = $derived(t());
  const p = $derived(s.popup);

  /** The expand button turns a small bubble into the full card. */
  let forcedFull = $state(false);
  let expanded = $state(false);
  let allMeanings = $state(false);
  let wikiBig = $state(false);
  let hovered = $state<number | null>(null);
  /** Picked words in the full card's sentence: [from, to] token indexes. */
  let pick = $state<[number, number] | null>(null);
  let dragFrom: number | null = null;
  let textBox = $state<HTMLElement>();
  let overflowing = $state(false);

  // A new translation starts small again.
  $effect(() => {
    void data.source;
    forcedFull = false;
    expanded = false;
    allMeanings = false;
    wikiBig = false;
    pick = null;
  });

  const style = $derived(forcedFull ? 'full' : look.style);
  const word = $derived(isWordLike(data.source) && !data.notice && !data.error);
  const failed = $derived(data.notice || (data.error ? s.errors[data.error.kind] : ''));

  /** Dictionary groups without the shown translation itself. */
  const groups = $derived(
    data.dict
      .map((g) => ({ ...g, terms: g.terms.filter((w) => w.toLowerCase() !== data.text.trim().toLowerCase()) }))
      .filter((g) => g.terms.length || g.entries?.length),
  );
  const entries = $derived(groups.flatMap((g) => (g.entries ?? []).map((e) => ({ ...e, pos: g.pos }))));
  const firstPos = $derived(data.dict[0]?.pos ?? '');
  const also = $derived(
    (groups[0]?.terms ?? data.alternatives).filter((w) => w.toLowerCase() !== data.text.trim().toLowerCase()).slice(0, 2),
  );

  // Compact styles can leave out the other translations (the dictionary
  // included) and the part of speech; the full card always has them.
  const showDict = $derived(word && look.variants && wantsDict(look.wordMode) && (entries.length > 0 || groups.length > 0));
  const showWiki = $derived(wiki !== 'off' && !data.notice && !data.error);

  /** Sentences of the translation (hover highlights, click copies one). */
  const parts = $derived(sentences(data.text));
  const long = $derived(data.text.length > 90 || data.text.includes('\n'));

  // Does the collapsed text hide anything?
  $effect(() => {
    void data.text;
    void expanded;
    const el = textBox;
    if (!el) return;
    requestAnimationFrame(() => (overflowing = el.scrollHeight > el.clientHeight + 2));
  });

  /** Full card: words and the spaces between them, for picking. */
  const tokens = $derived(data.text.split(/(\s+)/));
  const picked = $derived.by(() => {
    if (!pick) return '';
    const [a, b] = pick[0] <= pick[1] ? pick : [pick[1], pick[0]];
    return tokens
      .slice(a, b + 1)
      .join('')
      .replace(/^[\s«»"“”'(]+|[\s«»"“”'.,;:!?)]+$/g, '');
  });
  function wordDown(i: number, e: MouseEvent) {
    if (e.shiftKey && pick) pick = [pick[0], i];
    else {
      pick = pick && pick[0] === i && pick[1] === i ? null : [i, i];
      dragFrom = i;
    }
  }
  function inPick(i: number) {
    if (!pick) return false;
    const [a, b] = pick[0] <= pick[1] ? pick : [pick[1], pick[0]];
    return i >= a && i <= b;
  }

  const vars = $derived(`--bub-font: ${look.fontSize}px; --bub-alpha: ${look.opacity}%;`);
  const shownMeanings = $derived(allMeanings ? entries : entries.slice(0, 3));

  function wikiLink() {
    if (typeof wiki === 'object') actions.openUrl(wiki.kind === 'found' ? wiki.article.url : wiki.searchUrl);
  }
</script>

<svelte:window onmouseup={() => (dragFrom = null)} />

{#snippet wikiCard(big: boolean)}
  <section class="wiki" class:big aria-live="polite">
    <header class="wiki-head">
      <Icon name="book" />
      <span>{p.wiki}</span>
      <span class="spacer"></span>
      {#if big}
        <IconButton size="sm" icon="gear" label={p.bubbleSettings} onclick={actions.settings} />
      {:else}
        <IconButton size="sm" icon="link" label={p.readWiki} onclick={wikiLink} />
      {/if}
    </header>
    {#if wiki === 'loading'}
      <p class="wiki-note">{p.wikiLoading}</p>
    {:else if typeof wiki === 'object' && wiki.kind === 'found'}
      <button class="wiki-body" onclick={() => (wikiBig = !wikiBig)} title={big ? p.collapse : p.more}>
        {#if big && look.wikiImage}
          <span class="wiki-img">
            {#if wiki.article.thumbnail}
              <img src={wiki.article.thumbnail} alt="" />
            {:else}
              <Icon name="image" />
            {/if}
          </span>
        {/if}
        <span class="wiki-text">
          {#if big}<strong class="wiki-title">{wiki.article.title}</strong>{/if}
          <span class="wiki-extract" class:clamp2={!big} class:clamp6={big}>{wiki.article.extract}</span>
        </span>
      </button>
      {#if big}
        <footer class="wiki-foot">
          <button class="link" onclick={wikiLink}>{p.readWiki}<Icon name="link" size={14} /></button>
          <span class="spacer"></span>
          <span class="license">CC BY-SA</span>
        </footer>
      {/if}
    {:else if typeof wiki === 'object'}
      <p class="wiki-note">{wiki.kind === 'notFound' ? p.wikiNotFound : p.wikiFailed}</p>
      <button class="link" onclick={wikiLink}>{p.wikiSearch}<Icon name="link" size={14} /></button>
    {/if}
  </section>
{/snippet}

{#snippet dictionary()}
  <ol class="meanings">
    {#each shownMeanings as m, i (m.word + i)}
      <li>
        <span class="num">{i + 1}</span>
        <span class="m-body">
          <button class="m-word" onclick={() => actions.copy(m.word)} title={s.copy}>{m.word}</button>
          {#if m.gloss.length}<span class="m-gloss">{m.gloss.join(', ')}</span>{/if}
        </span>
      </li>
    {:else}
      {#each groups as g (g.pos)}
        <li class="plain">
          {#if g.pos && look.pos}<span class="m-pos">{posName(g.pos)}</span>{/if}
          <span class="m-gloss">{g.terms.join(', ')}</span>
        </li>
      {/each}
    {/each}
  </ol>
  {#if data.examples.length}
    <div class="examples">
      {#each data.examples.slice(0, 2) as ex (ex.text)}
        <p class="example">{ex.text}{#if ex.tr}{' '}<span class="ex-tr">→ {ex.tr}</span>{/if}</p>
      {/each}
    </div>
  {/if}
  <footer class="dict-foot">
    {#if entries.length > 3}
      <button class="more" onclick={() => (allMeanings = !allMeanings)}>
        {allMeanings ? p.fewer : p.allMeaningsN(entries.length)}
        <Icon name={allMeanings ? 'chev-u' : 'chev-d'} />
      </button>
    {/if}
    <span class="spacer"></span>
    <IconButton size="sm" icon="gear" label={p.bubbleSettings} onclick={actions.settings} />
  </footer>
{/snippet}

{#snippet tools(withExpand: boolean)}
  <span class="tools">
    <IconButton size="sm" icon="speaker" label={s.speak} disabled={!data.text} onclick={actions.speak} />
    <IconButton size="sm" icon="copy" label={s.copy} disabled={!data.text} onclick={() => actions.copy()} />
    {#if withExpand}
      <IconButton size="sm" icon="expand" label={p.expand} onclick={() => (forcedFull = true)} />
    {/if}
  </span>
{/snippet}

{#if style === 'full'}
  <!-- The card. -->
  <div
    class="card full"
    class:word
    class:winked
    class:still={!look.animation}
    class:preview
    style={vars}
    bind:this={card}
  >
    <header class="head">
      <!-- The empty part of the header drags the bubble. -->
      <span class="grip" role="presentation" onmousedown={(e) => e.button === 0 && actions.drag()}>
        <span class="pair">
          {data.sourceLang ? langName(data.sourceLang) : s.autoTitle} → {langName(data.target)}
        </span>
        <span class="engine">· {engineName(data.engine)}</span>
      </span>
      <IconButton icon="speaker" label={s.speak} disabled={!data.text} onclick={actions.speak} />
      {#if look.wiki}
        <button class="w" class:on={wiki !== 'off'} title={p.wiki} aria-label={p.wiki} onclick={actions.wiki}>W</button>
      {/if}
      <IconButton icon="link" label={p.openGoogle} onclick={actions.google} />
      <IconButton icon="pin" label={pinned ? p.unpin : p.pin} active={pinned} onclick={actions.pin} />
      <IconButton icon="x" label={s.closeWindow} onclick={actions.close} />
    </header>

    {#if failed}
      <p class="notice" class:error={!!data.error}>{failed}</p>
    {:else if word}
      <div class="word-block">
        <span class="srcword">{data.source}{#if data.transcription} <span class="ts">/{data.transcription}/</span>{/if}</span>
        <p class="big" class:pending={data.busy && !data.text}>{#if data.text}{data.text}{:else}&nbsp;{/if}</p>
      </div>
      {#if groups.length}
        <div class="dict">
          {#each groups.slice(0, 3) as g (g.pos)}
            {#if g.terms.length}
              <div class="group">
                {#if g.pos}<span class="pos">{posName(g.pos)}</span>{/if}
                <div class="alts">
                  {#each g.terms.slice(0, 5) as alt (alt)}
                    <button class="chip" onclick={() => actions.copy(alt)}>{alt}</button>
                  {/each}
                </div>
              </div>
            {/if}
          {/each}
        </div>
      {:else if data.alternatives.length}
        <div class="alts">
          {#each data.alternatives.slice(0, 6) as alt (alt)}
            <button class="chip" onclick={() => actions.copy(alt)}>{alt}</button>
          {/each}
        </div>
      {/if}
    {:else}
      <p class="sentence" class:pending={data.busy && !data.text}>
        {#each tokens as tok, i (i)}{#if /\S/.test(tok)}<span
              class="tok"
              class:on={inPick(i)}
              role="presentation"
              onmousedown={(e) => wordDown(i, e)}
              onmouseenter={() => dragFrom !== null && (pick = [dragFrom, i])}>{tok}</span
            >{:else}{tok}{/if}{/each}{#if !data.text}&nbsp;{/if}
      </p>
    {/if}

    {#if showWiki}
      {@render wikiCard(true)}
    {/if}

    {#if !failed}
      <footer class="foot">
        {#if !word}<span class="src">{data.source}</span>{/if}
        <span class="spacer"></span>
        <Button size="sm" icon="replace" disabled={!data.text || data.busy} onclick={() => actions.replace(picked || undefined)}
          >{s.replaceSelection}</Button
        >
        <Button variant="primary" size="sm" icon="copy" disabled={!data.text} onclick={() => actions.copy(picked || undefined)}>
          {picked ? (picked.includes(' ') ? p.copyWords : p.copyWord) : s.copy}
        </Button>
      </footer>
    {/if}
  </div>
{:else if failed}
  <div class="card mini" class:still={!look.animation} style={vars} bind:this={card}>
    <p class="notice" class:error={!!data.error}>{failed}</p>
  </div>
{:else if word && (showDict || showWiki)}
  <!-- Word: translation row, then dictionary and/or Wikipedia. -->
  <div class="card wordcard" class:winked class:still={!look.animation} style={vars} bind:this={card}>
    <div class="row">
      <button class="wordbox" class:pending={data.busy && !data.text} onclick={() => actions.copy()} title={p.copyHint}>
        <span class="w-text">{#if data.text}{data.text}{:else}&nbsp;{/if}</span>
        {#if firstPos && look.pos}<span class="pos-chip">{posShort(firstPos)}</span>{/if}
        {#if also.length && !showDict && look.variants}<span class="also">{p.also}: {also.join(', ')}</span>{/if}
      </button>
      {@render tools(look.style === 'buttons' || !preview)}
    </div>
    {#if showDict}
      {#if data.transcription}
        <p class="src-line">{data.source} <span class="ts">/{data.transcription}/</span></p>
      {/if}
      {@render dictionary()}
    {/if}
    {#if showWiki}
      {@render wikiCard(wikiBig)}
    {/if}
  </div>
{:else if long}
  <!-- Long translation: sentences; collapsed to three lines or scrolling. -->
  <div class="card longcard" class:winked class:still={!look.animation} style={vars} bind:this={card}>
    <div
      class="sents"
      class:collapsed={look.collapseLong && !expanded}
      class:open={expanded || !look.collapseLong}
      class:fade={look.collapseLong && !expanded && overflowing}
      bind:this={textBox}
    >
      {#each parts as part, i (i)}
        <button
          class="sent"
          class:hl={(hovered ?? 0) === i}
          class:pending={data.busy && !data.text}
          onmouseenter={() => (hovered = i)}
          onmouseleave={() => (hovered = null)}
          onclick={(e) => (e.shiftKey ? actions.replace() : actions.copy(part.trim()))}
          title={p.sentenceHint}>{part}</button
        >
      {/each}
    </div>
    <footer class="long-foot">
      {#if look.collapseLong && (overflowing || expanded)}
        <button class="more" onclick={() => (expanded = !expanded)}>
          {expanded ? p.collapse : p.expand}
          <Icon name={expanded ? 'chev-u' : 'chev-d'} />
        </button>
      {/if}
      <span class="spacer"></span>
      {@render tools(look.style === 'buttons')}
    </footer>
  </div>
{:else}
  <!-- Short: the translation in a pill (+ mini-buttons). -->
  <div class="card mini" class:winked class:still={!look.animation} style={vars} bind:this={card}>
    <button
      class="pill"
      class:pending={data.busy && !data.text}
      title={p.copyHint}
      onclick={(e) => (e.shiftKey ? actions.replace() : actions.copy())}
      >{#if data.text.trim()}{data.text.trim()}{:else}&nbsp;{/if}</button
    >
    {#if look.style === 'buttons'}
      {@render tools(true)}
    {/if}
  </div>
{/if}

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 7px;
    border-radius: 14px;
    background: color-mix(in srgb, var(--bg-surface) var(--bub-alpha, 100%), transparent);
    border: 1px solid var(--line-soft);
    box-shadow:
      0 2px 6px rgba(0, 0, 0, 0.12),
      0 16px 40px -6px rgba(0, 0, 0, 0.2);
    color: var(--text-primary);
  }
  .spacer {
    flex: 1;
  }
  .pending {
    animation: pulse 1s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }

  /* ── Full card ─────────────────────────────────────────── */
  .full {
    width: 460px;
    gap: 10px;
    padding: 10px 12px 12px;
  }
  .full.word {
    width: 380px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-right: -4px;
  }
  .grip {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 4px;
    cursor: default;
    overflow: hidden;
  }
  .pair {
    font: var(--caption-medium);
    color: var(--text-secondary);
    white-space: nowrap;
  }
  .engine {
    font: var(--caption);
    color: var(--text-tertiary);
    white-space: nowrap;
  }
  .w {
    width: 32px;
    height: 32px;
    border-radius: var(--radius);
    font: 600 14px/20px var(--font);
    color: var(--text-secondary);
  }
  .w:hover,
  .w.on {
    background: var(--state-hover);
    color: var(--text-primary);
  }
  .word-block {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .srcword {
    padding: 0 8px;
    font: var(--caption);
    color: var(--text-tertiary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ts {
    color: var(--text-tertiary);
    font-family: var(--font);
  }
  .big {
    margin: 0;
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    font: 500 calc(var(--bub-font) + 7px) / 1.36 var(--font);
    user-select: text;
  }
  .sentence {
    margin: 0;
    padding: 4px 8px;
    max-height: 260px;
    overflow-y: auto;
    font: 400 calc(var(--bub-font) + 1px) / 1.5 var(--font);
    white-space: pre-wrap;
    overflow-wrap: break-word;
  }
  .tok {
    border-radius: 4px;
    cursor: pointer;
  }
  .tok:hover {
    background: var(--state-hover);
  }
  .tok.on {
    background: var(--accent-soft);
    box-shadow: 0 0 0 2px var(--accent-soft);
  }
  .dict {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .pos {
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .alts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    padding: 4px 10px;
    border-radius: var(--radius);
    background: var(--bg-field);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    font: var(--body);
    color: var(--text-secondary);
  }
  .chip:hover {
    background: var(--state-hover);
    color: var(--text-primary);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-top: 8px;
    border-top: 1px solid var(--line-soft);
  }
  .src {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .notice {
    margin: 0;
    padding: 6px 10px;
    font: var(--body);
    color: var(--text-secondary);
  }
  .notice.error {
    color: var(--danger);
  }

  /* ── Short pill ────────────────────────────────────────── */
  .mini {
    flex-direction: row;
    align-items: center;
    gap: 4px;
    /* max-content, not fit-content: the window is sized from the card, so
       fit-content would wrap letter by letter after a short word. */
    width: max-content;
    max-width: 420px;
  }
  .pill {
    min-width: 0;
    padding: 6px 12px;
    border-radius: var(--radius);
    background: var(--accent-soft);
    font: 400 var(--bub-font) / 1.6 var(--font);
    text-align: left;
    white-space: pre-wrap;
    overflow-wrap: break-word;
  }
  .pill:hover {
    background: color-mix(in srgb, var(--accent-soft) 100%, var(--state-hover));
  }
  .tools {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }

  /* ── Long text ─────────────────────────────────────────── */
  .longcard {
    width: 420px;
    gap: 2px;
  }
  .sents {
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow: hidden;
  }
  .sents.collapsed {
    /* The first sentence block + about two more lines. */
    max-height: calc((var(--bub-font) * 1.6 + 12px) + (var(--bub-font) * 1.6 * 2) + 12px);
  }
  .sents.fade {
    mask-image: linear-gradient(to bottom, #000 70%, transparent);
  }
  .sents.open {
    max-height: 250px;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--line-strong) transparent;
  }
  .sent {
    display: block;
    padding: 6px 10px;
    border-radius: var(--radius);
    font: 400 var(--bub-font) / 1.6 var(--font);
    text-align: left;
    white-space: pre-wrap;
    overflow-wrap: break-word;
    transition: background 120ms;
  }
  .sent.hl {
    background: var(--accent-soft);
  }
  .long-foot,
  .dict-foot {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 28px;
    padding: 0 2px 0 8px;
  }
  .more {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font: var(--caption-medium);
    color: var(--text-secondary);
  }
  .more:hover {
    color: var(--text-primary);
  }

  /* ── Word ──────────────────────────────────────────────── */
  .wordcard {
    width: 420px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .wordbox {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: var(--radius);
    background: var(--accent-soft);
    text-align: left;
    overflow: hidden;
  }
  .w-text {
    font: 400 calc(var(--bub-font) + 5px) / 1.4 var(--font);
    white-space: nowrap;
  }
  .pos-chip {
    padding: 1px 7px;
    border-radius: 6px;
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    font: var(--caption-medium);
    color: var(--text-primary);
    white-space: nowrap;
  }
  .also {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: var(--caption);
    color: var(--text-secondary);
  }
  .src-line {
    margin: 0 8px;
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .meanings {
    margin: 0;
    padding: 0 8px;
    list-style: none;
    display: flex;
    flex-direction: column;
  }
  .meanings li {
    display: flex;
    gap: 10px;
    padding: 7px 0;
    border-bottom: 1px solid var(--line-soft);
  }
  .meanings li:last-child {
    border-bottom: 0;
  }
  .num {
    width: 10px;
    padding-top: 1px;
    font: var(--caption-medium);
    color: var(--text-tertiary);
  }
  .m-body {
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    column-gap: 8px;
  }
  .m-word {
    font: 600 var(--bub-font) / 1.4 var(--font);
    color: var(--text-primary);
  }
  .m-word:hover {
    text-decoration: underline;
  }
  .m-gloss,
  .m-pos {
    font: var(--caption);
    color: var(--text-secondary);
  }
  .m-pos {
    color: var(--text-tertiary);
  }
  .examples {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0 8px;
    padding: 6px 10px;
    border-radius: var(--radius);
    background: var(--bg-sunken);
  }
  .example {
    margin: 0;
    font: var(--caption);
    color: var(--text-secondary);
  }
  .ex-tr {
    color: var(--text-tertiary);
  }

  /* ── Wikipedia ─────────────────────────────────────────── */
  .wiki {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px 10px 12px;
    border-radius: 12px;
    background: var(--bg-sunken);
  }
  .full .wiki {
    padding: 8px 10px 10px 12px;
  }
  .wiki-head {
    display: flex;
    align-items: center;
    gap: 6px;
    font: var(--caption-medium);
    color: var(--text-secondary);
    margin: -2px -4px -2px 0;
  }
  .wiki-body {
    display: flex;
    gap: 14px;
    text-align: left;
  }
  .wiki-img {
    flex-shrink: 0;
    width: 72px;
    height: 72px;
    display: grid;
    place-items: center;
    border-radius: var(--radius);
    background: var(--bg-field);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    color: var(--text-tertiary);
    overflow: hidden;
  }
  .wiki-img img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .wiki-text {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .wiki-title {
    font: var(--body-medium);
    color: var(--text-primary);
  }
  .wiki-extract {
    font: 400 calc(var(--bub-font) - 1px) / 1.5 var(--font);
    color: var(--text-secondary);
    display: -webkit-box;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .clamp2 {
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }
  .clamp6 {
    -webkit-line-clamp: 6;
    line-clamp: 6;
  }
  .wiki-note {
    margin: 0;
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .wiki-foot {
    display: flex;
    align-items: center;
  }
  .link {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font: var(--caption-medium);
    color: var(--text-primary);
  }
  .link:hover {
    text-decoration: underline;
  }
  .license {
    font: var(--caption);
    color: var(--text-tertiary);
  }

  /* ── Copy wink (no tick: the size never changes) ───────── */
  .winked {
    animation: wink 380ms cubic-bezier(0.3, 0.7, 0.4, 1);
  }
  .winked :is(.pill, .wordbox, .big) {
    animation: flash 750ms ease-out;
  }
  @keyframes wink {
    35% {
      transform: scale(0.965);
    }
  }
  @keyframes flash {
    0%,
    30% {
      filter: brightness(1.35);
    }
    100% {
      filter: none;
    }
  }
  .still,
  .still :is(.pill, .wordbox, .big) {
    animation: none;
  }
  @media (prefers-reduced-motion: reduce) {
    .winked,
    .winked :is(.pill, .wordbox, .big) {
      animation: none;
    }
  }
</style>
