<!-- One row of a Menu: 32 px (44 with a note), 10 px sides, radius 8.
     Selected: soft accent, medium weight and a check. -->
<script lang="ts">
  import Icon from './Icon.svelte';

  let {
    label,
    note,
    icon,
    trailing,
    selected = false,
    active = false,
    disabled = false,
    onpick,
  }: {
    label: string;
    /** Grey second line. */
    note?: string;
    /** 18 px leading icon. */
    icon?: string;
    /** Grey text on the right: a language code, «офлайн». */
    trailing?: string;
    selected?: boolean;
    /** Keyboard highlight. */
    active?: boolean;
    disabled?: boolean;
    onpick: () => void;
  } = $props();
</script>

<button
  class="item"
  class:tall={!!note}
  class:selected
  class:active
  {disabled}
  role="option"
  aria-selected={selected}
  onclick={onpick}
>
  {#if icon}<span class="lead"><Icon name={icon} size={18} /></span>{/if}
  <span class="text">
    <span class="label">{label}</span>
    {#if note}<span class="note">{note}</span>{/if}
  </span>
  <span class="spacer"></span>
  {#if selected}
    <Icon name="check-16" />
  {:else if trailing}
    <span class="trail">{trailing}</span>
  {/if}
</button>

<style>
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    border-radius: var(--radius);
    color: var(--text-primary);
    text-align: left;
    flex-shrink: 0;
  }
  .tall {
    height: 44px;
  }
  .item:hover:not(:disabled),
  .active {
    background: var(--state-hover);
  }
  .selected,
  .selected:hover:not(:disabled) {
    background: var(--accent-soft);
  }
  .item:disabled {
    opacity: 0.5;
  }
  .lead {
    display: flex;
    color: var(--text-primary);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .label {
    font: var(--body);
    white-space: nowrap;
  }
  .selected .label {
    font: var(--body-medium);
  }
  .note {
    font: var(--caption);
    color: var(--text-secondary);
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
  }
  .trail {
    font: var(--caption);
    color: var(--text-tertiary);
  }
</style>
