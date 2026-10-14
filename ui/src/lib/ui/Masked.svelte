<!-- History text with phone / passport / card numbers drawn as •••• when
     «Скрывать номера» is on. Hovering a number (or holding Alt in a focused
     window) shows it; copying the text gives the real digits. -->
<script lang="ts" module>
  /** Alt held: every number shows (focused windows only). */
  let alt = $state(false);
  if (typeof window !== 'undefined') {
    window.addEventListener('keydown', (e) => e.key === 'Alt' && (alt = true));
    window.addEventListener('keyup', (e) => e.key === 'Alt' && (alt = false));
    window.addEventListener('blur', () => (alt = false));
  }
</script>

<script lang="ts">
  import { maskRanges, pieces } from '../mask';
  import { settings } from '../settings.svelte';

  let { text }: { text: string } = $props();

  const parts = $derived(settings.maskNumbers ? pieces(text, maskRanges(text)) : [{ t: text, masked: false }]);
</script>

{#each parts as p, i (i)}{#if p.masked && !alt}<span class="mask"
      ><span class="dots">{p.t.replace(/\d/g, '•')}</span><span class="real">{p.t}</span></span
    >{:else}{p.t}{/if}{/each}

<style>
  .mask {
    border-radius: 4px;
    background: color-mix(in srgb, var(--text-tertiary) 18%, transparent);
    cursor: default;
  }
  .real {
    display: none;
  }
  .mask:hover .real {
    display: inline;
  }
  .mask:hover .dots {
    display: none;
  }
</style>
