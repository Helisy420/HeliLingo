<!-- A settings row: label (Body/Medium) and optional note (Caption,
     secondary) on the left, the control on the right; 14 px vertical
     padding, soft line below. -->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    label,
    note,
    children,
  }: { label: string; note?: string; children?: Snippet } = $props();
</script>

<div class="row" data-setting>
  <div class="text">
    <span class="label">{label}</span>
    {#if note}<span class="note">{note}</span>{/if}
  </div>
  {#if children}<div class="control">{@render children()}</div>{/if}
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 14px 0;
    border-bottom: 1px solid var(--line-soft);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }
  .label {
    font: var(--body-medium);
    color: var(--text-primary);
  }
  .note {
    font: var(--caption);
    color: var(--text-secondary);
  }
  .control {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
  }
</style>
