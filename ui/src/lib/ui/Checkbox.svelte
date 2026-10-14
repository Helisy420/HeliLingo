<script lang="ts">
  import Icon from './Icon.svelte';

  let {
    checked = $bindable(false),
    disabled = false,
    label,
  }: { checked?: boolean; disabled?: boolean; label?: string } = $props();
</script>

<button
  class="cb"
  class:on={checked}
  role="checkbox"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  onclick={() => (checked = !checked)}
>
  {#if checked}<Icon name="check" size={14} />{/if}
</button>

<style>
  /* Figma Checkbox: 20×20, radius 5; off = field + 1.5 px strong border. */
  .cb {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: 5px;
    background: var(--bg-field);
    box-shadow: inset 0 0 0 1.5px var(--line-strong);
    flex-shrink: 0;
  }
  .on {
    background: var(--accent);
    box-shadow: none;
    color: var(--text-on-marker);
  }
  .cb:focus-visible {
    outline: 2px solid var(--text-primary);
    outline-offset: 2px;
  }
</style>
