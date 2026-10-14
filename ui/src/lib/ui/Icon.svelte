<script module lang="ts">
  // Figma icons (16 px line icons, colour = currentColor).
  const files = import.meta.glob('../../assets/icons/*.svg', {
    query: '?raw',
    import: 'default',
    eager: true,
  }) as Record<string, string>;
  const icons: Record<string, string> = {};
  for (const [path, svg] of Object.entries(files)) {
    icons[path.slice(path.lastIndexOf('/') + 1, -4)] = svg;
  }
  export const iconNames = Object.keys(icons);
</script>

<script lang="ts">
  let { name, size = 16 }: { name: string; size?: number } = $props();
</script>

<span class="icon" style:width="{size}px" style:height="{size}px" aria-hidden="true">{@html icons[name] ?? ''}</span>

<style>
  .icon {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
  }
  .icon :global(svg) {
    display: block;
  }
</style>
