<!-- A settings group: Body/Strong title over a 1 px strong line, then rows.
     `data-group` feeds the section tabs at the top of the page. -->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let { title, children }: { title: string; children: Snippet } = $props();
</script>

<section class="group" data-group={title}>
  <h3 class="title">{title}</h3>
  {@render children()}
</section>

<style>
  .group {
    display: flex;
    flex-direction: column;
    width: 100%;
  }
  .title {
    margin: 0;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--line-strong);
    font: 600 14px/20px var(--font);
    color: var(--text-primary);
  }
</style>
