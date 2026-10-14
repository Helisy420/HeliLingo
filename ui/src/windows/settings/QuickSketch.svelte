<!-- A small drawing of the Quick window for its settings preview: the parts
     that each choice adds or removes (window or line, other variants,
     history, mini-buttons), in the theme's colours. Text is suggested by
     bars; only the translation is real text so the result reads at once. -->
<script lang="ts">
  import Icon from '../../lib/ui/Icon.svelte';

  let {
    layout,
    suggestions,
    history,
    miniButtons,
    text,
    translation,
  }: {
    layout: 'window' | 'line';
    suggestions: boolean;
    history: boolean;
    miniButtons: boolean;
    text: string;
    translation: string;
  } = $props();
</script>

{#if layout === 'line'}
  <div class="sk line">
    <div class="in">
      <Icon name="languages" />
      <span class="typed">{text}</span>
      <span class="chip">EN → RU</span>
    </div>
    <div class="out">
      <span class="res">{translation}</span>
      <Icon name="speaker" />
      <Icon name="copy" />
    </div>
  </div>
{:else}
  <div class="sk window">
    <div class="bar">
      <span class="pill w60"></span>
      <span class="dot"></span>
      <span class="pill w44"></span>
      <span class="grow"></span>
      <span class="pill w52"></span>
    </div>
    <div class="typed big">{text}</div>
    <div class="result">
      <span class="res">{translation}</span>
      {#if suggestions}
        <span class="alts"><span class="pill w70"></span><span class="pill w54"></span></span>
      {/if}
    </div>
    {#if history}
      <div class="hist">
        {#each [72, 48, 60] as w (w)}
          <span class="hrow"><span class="pill" style:width="{w}%"></span><span class="ico"></span></span>
        {/each}
      </div>
    {/if}
    <div class="foot">
      <span class="key">Esc</span>
      <span class="grow"></span>
      {#if miniButtons}
        <span class="ib"><Icon name="speaker" /></span>
        <span class="ib"><Icon name="replace" /></span>
        <span class="ib accent"><Icon name="copy" /></span>
      {:else}
        <span class="btn"></span>
        <span class="btn accent"></span>
      {/if}
    </div>
  </div>
{/if}

<style>
  .sk {
    width: 400px;
    border-radius: 14px;
    background: var(--bg-surface);
    border: 1px solid var(--line-soft);
    box-shadow: 0 10px 30px -8px rgba(0, 0, 0, 0.25);
    overflow: hidden;
    color: var(--text-tertiary);
    font: var(--body);
  }
  .grow {
    flex: 1;
  }
  .pill {
    display: inline-block;
    height: 14px;
    border-radius: 7px;
    background: var(--bg-field);
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }
  .w44 {
    width: 44px;
  }
  .w52 {
    width: 52px;
  }
  .w54 {
    width: 54px;
  }
  .w60 {
    width: 60px;
  }
  .w70 {
    width: 70px;
  }
  .dot {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px 0;
  }
  .typed {
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .big {
    padding: 8px 12px 10px;
    font-size: 15px;
  }
  .result {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    border-top: 1px solid var(--line-soft);
    background: color-mix(in srgb, var(--bg-sunken) 60%, transparent);
  }
  .res {
    padding: 4px 8px;
    border-radius: 6px;
    background: var(--accent-soft);
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .alts {
    display: flex;
    gap: 6px;
  }
  .hist {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px 10px;
    border-top: 1px solid var(--line-soft);
  }
  .hrow {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 18px;
  }
  .hrow .pill {
    height: 8px;
    box-shadow: none;
    background: color-mix(in srgb, var(--text-tertiary) 30%, transparent);
  }
  .ico {
    margin-left: auto;
    width: 10px;
    height: 10px;
    border-radius: 3px;
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px 8px;
  }
  .key {
    padding: 0 5px;
    border-radius: 4px;
    box-shadow: inset 0 0 0 1px var(--line-strong);
    font: var(--caption);
  }
  .ib {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 6px;
  }
  .ib.accent,
  .btn.accent {
    background: var(--accent);
    color: var(--text-on-marker);
  }
  .btn {
    height: 22px;
    min-width: 60px;
    border-radius: 6px;
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }
  .btn.accent {
    box-shadow: none;
    min-width: 52px;
  }
  /* Line layout. */
  .line .in {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
  }
  .line .typed {
    flex: 1;
  }
  .chip {
    padding: 2px 8px;
    border-radius: 6px;
    box-shadow: inset 0 0 0 1px var(--line-strong);
    font: var(--caption-medium);
    color: var(--text-secondary);
  }
  .line .out {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-top: 1px solid var(--line-soft);
  }
  .line .res {
    flex: 1;
  }
</style>
