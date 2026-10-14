<!-- A settings-row dropdown (30 px, Body/Default) that opens the language
     list. `label` overrides the button text ("Перевести на английский"). -->
<script lang="ts">
  import { t } from '../i18n.svelte';
  import { langName } from '../langs';
  import Icon from './Icon.svelte';
  import LangMenu from './LangMenu.svelte';

  let {
    value,
    allowAuto = false,
    width = 156,
    label,
    onchange,
  }: {
    value: string;
    allowAuto?: boolean;
    width?: number;
    label?: string;
    onchange: (code: string) => void;
  } = $props();

  const s = $derived(t());
  let open = $state(false);
  const text = $derived(label ?? (value === 'auto' ? s.set.general.sourceAuto : langName(value)));
</script>

<span class="anchor" data-menu-anchor>
  <button class="choice" class:open style:width="{width}px" onclick={() => (open = !open)} aria-expanded={open}>
    <span class="text">{text}</span>
    <Icon name={open ? 'chev-u' : 'chev-d'} />
  </button>
  {#if open}
    <LangMenu
      {value}
      {allowAuto}
      align="right"
      onpick={(c) => {
        open = false;
        if (c !== value) onchange(c);
      }}
      onclose={() => (open = false)}
    />
  {/if}
</span>

<style>
  .anchor {
    position: relative;
    display: inline-flex;
  }
  .choice {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 10px 0 12px;
    border-radius: var(--radius);
    background: var(--bg-field);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    font: var(--body);
    color: var(--text-primary);
  }
  .choice :global(.icon) {
    color: var(--text-secondary);
  }
  .choice:hover {
    background: var(--state-hover);
  }
  .choice.open {
    background: var(--bg-field);
    box-shadow: inset 0 0 0 1.5px var(--text-primary);
  }
  .text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }
</style>
