<script lang="ts">
  let {
    group = $bindable(),
    value,
    disabled = false,
    label,
  }: { group?: string; value: string; disabled?: boolean; label?: string } = $props();
  const on = $derived(group === value);
</script>

<button class="rb" class:on role="radio" aria-checked={on} aria-label={label} {disabled} onclick={() => (group = value)}>
  {#if on}<span class="dot"></span>{/if}
</button>

<style>
  /* Figma Radio: 20×20; on = accent with an 8 px on-marker dot. */
  .rb {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--bg-field);
    box-shadow: inset 0 0 0 1.5px var(--line-strong);
    flex-shrink: 0;
  }
  .on {
    background: var(--accent);
    box-shadow: none;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-on-marker);
  }
  .rb:focus-visible {
    outline: 2px solid var(--text-primary);
    outline-offset: 2px;
  }
</style>
