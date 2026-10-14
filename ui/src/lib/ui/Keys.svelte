<!-- A shortcut as keycaps: "Ctrl×2" → Ctrl Ctrl, "Ctrl+Shift+L" → Ctrl ⇧ L,
     "Ctrl+Alt+Mouse5" → Ctrl Alt M5, "Ctrl+Mouse4×2" → Ctrl M4 M4. -->
<script lang="ts">
  import Bits from './Bits.svelte';

  let { shortcut }: { shortcut: string } = $props();

  const keys = $derived(split(shortcut));

  function split(h: string): string[] {
    if (!h) return [];
    const double = h.endsWith('×2');
    const parts = h
      .replace('×2', '')
      .split('+')
      .map((k) => (k === 'Shift' ? '⇧' : k === 'Mouse4' ? 'M4' : k === 'Mouse5' ? 'M5' : k));
    // Pressed twice: the last key (the button or the lone modifier) again.
    return double ? [...parts, parts[parts.length - 1]!] : parts;
  }
</script>

<span class="keys">{#each keys as k, i (i)}<Bits kind="key">{k}</Bits>{/each}</span>

<style>
  .keys {
    display: inline-flex;
    gap: 4px;
  }
</style>
