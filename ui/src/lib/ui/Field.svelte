<script lang="ts">
  import { t } from '../i18n.svelte';
  import Icon from './Icon.svelte';

  let {
    value = $bindable(''),
    placeholder = '',
    icon,
    password = false,
    error,
    hint,
    disabled = false,
    width,
    demo,
    oninput,
    onkeydown,
  }: {
    value?: string;
    placeholder?: string;
    /** Leading icon: "search", "key", … */
    icon?: string;
    /** Masked, with an eye button to show it. */
    password?: boolean;
    /** Red border and this text under the field. */
    error?: string;
    /** Grey text under the field. */
    hint?: string;
    disabled?: boolean;
    width?: number;
    /** Gallery only: draw a pseudo-state. */
    demo?: 'hover' | 'focus';
    oninput?: (e: Event) => void;
    onkeydown?: (e: KeyboardEvent) => void;
  } = $props();

  let shown = $state(false);
</script>

<div class="wrap" style:width={width ? `${width}px` : undefined}>
  <label class="field {demo ?? ''}" class:error={!!error} class:disabled>
    {#if icon}<span class="lead"><Icon name={icon} /></span>{/if}
    <input
      type={password && !shown ? 'password' : 'text'}
      bind:value
      {placeholder}
      {disabled}
      {oninput}
      {onkeydown}
      spellcheck="false"
      autocomplete="off"
    />
    {#if password}
      <button class="eye" type="button" aria-label={t().showPassword} onclick={() => (shown = !shown)}>
        <Icon name="eye" />
      </button>
    {/if}
  </label>
  {#if error}
    <span class="note err">{error}</span>
  {:else if hint}
    <span class="note">{hint}</span>
  {/if}
</div>

<style>
  /* Figma Field/*: 36 px, 12 px sides, radius 8, gap 8. Hover: tertiary
     border; focus: 1.5 px primary; error: 1.5 px danger. */
  .wrap {
    display: inline-flex;
    flex-direction: column;
    gap: 6px;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 12px;
    border-radius: var(--radius);
    background: var(--bg-field);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    cursor: text;
  }
  .field:hover,
  .field.hover {
    box-shadow: inset 0 0 0 1px var(--text-tertiary);
  }
  .field:focus-within,
  .field.focus {
    box-shadow: inset 0 0 0 1.5px var(--text-primary);
  }
  .field.error {
    box-shadow: inset 0 0 0 1.5px var(--danger);
  }
  .field.disabled {
    background: var(--bg-disabled);
    box-shadow: inset 0 0 0 1px var(--line-soft);
    cursor: default;
  }
  input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: none;
    padding: 0;
    font: var(--body);
    color: var(--text-primary);
    user-select: text;
  }
  input::placeholder {
    color: var(--text-tertiary);
  }
  input:disabled,
  input:disabled::placeholder {
    color: var(--text-disabled);
  }
  .lead {
    display: flex;
    color: var(--text-tertiary);
  }
  .eye {
    display: flex;
    color: var(--text-secondary);
  }
  .eye:hover {
    color: var(--text-primary);
  }
  .note {
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .note.err {
    color: var(--danger);
  }
</style>
