<!-- A preview stage for settings: a soft "desktop" (a few faint text lines
     and a window edge) so transparency and size read naturally, the live
     component on top, and an optional switch between samples. Not in
     Figma (the user asked for neat previews): built from the tokens. -->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    label,
    samples,
    sample = $bindable(),
    minHeight = 120,
    children,
  }: {
    label: string;
    /** Sample switch: value → caption. */
    samples?: { value: string; label: string }[];
    sample?: string;
    minHeight?: number;
    children: Snippet;
  } = $props();
</script>

<figure class="preview">
  <figcaption class="bar">
    <span class="label">{label}</span>
    <span class="spacer"></span>
    {#if samples}
      <span class="samples" role="radiogroup">
        {#each samples as s (s.value)}
          <button class="sample" class:on={s.value === sample} role="radio" aria-checked={s.value === sample} onclick={() => (sample = s.value)}
            >{s.label}</button
          >
        {/each}
      </span>
    {/if}
  </figcaption>
  <div class="stage" style:min-height="{minHeight}px">
    <!-- The "page" behind: lines like text in another app. -->
    <div class="page" aria-hidden="true">
      <span style:width="62%"></span>
      <span style:width="88%"></span>
      <span class="sel" style:width="34%"></span>
      <span style:width="71%"></span>
      <span style:width="80%"></span>
      <span style:width="46%"></span>
    </div>
    <div class="content">
      {@render children()}
    </div>
  </div>
</figure>

<style>
  .preview {
    margin: 14px 0 4px;
    border-radius: 14px;
    box-shadow: inset 0 0 0 1px var(--line-soft);
    background: var(--bg-sunken);
    overflow: hidden;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px 0 14px;
  }
  .label {
    font: var(--caption-medium);
    color: var(--text-tertiary);
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }
  .spacer {
    flex: 1;
  }
  .samples {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--bg-surface) 60%, transparent);
  }
  .sample {
    padding: 3px 10px;
    border-radius: 7px;
    font: var(--caption-medium);
    color: var(--text-secondary);
  }
  .sample:hover {
    color: var(--text-primary);
  }
  .sample.on {
    background: var(--bg-surface);
    color: var(--text-primary);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
  }
  .stage {
    position: relative;
    display: grid;
    place-items: center;
    padding: 18px 20px 22px;
  }
  .page {
    position: absolute;
    inset: 14px 24px;
    display: flex;
    flex-direction: column;
    gap: 11px;
    pointer-events: none;
  }
  .page span {
    height: 7px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--text-tertiary) 22%, transparent);
  }
  .page .sel {
    background: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .content {
    position: relative;
    max-width: 100%;
  }
</style>
