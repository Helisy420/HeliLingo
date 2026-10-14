<!-- Режим «Ультра» (no Figma frame: built from the translator's parts): each
     fragment of a mixed-language selection with its language, the original
     and the translation (or «без изменений»); Copy all / Replace selection. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { onShown } from '../../lib/fit';
  import { t } from '../../lib/i18n.svelte';
  import { replaceSelection, ultra, type UltraEvent, type UltraFragment, copyText } from '../../lib/ipc';
  import { langName } from '../../lib/langs';
  import Badge from '../../lib/ui/Badge.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import TitleBar from '../../lib/ui/TitleBar.svelte';

  type Row = UltraFragment & { state: 'pending' | 'translated' | 'unchanged' | 'failed'; out?: string };

  const s = $derived(t());
  const u = $derived(s.ultra);
  let rows = $state<Row[]>([]);
  let target = $state('ru');
  let done = $state(false);
  let copied = $state(false);
  let seq = 0;

  function onEvent(e: UltraEvent) {
    switch (e.type) {
      case 'split':
        target = e.target;
        rows = e.fragments.map((f) => ({ ...f, state: 'pending' }));
        break;
      case 'translated':
        rows[e.index] = { ...rows[e.index]!, state: 'translated', out: e.text };
        break;
      case 'unchanged':
        rows[e.index] = { ...rows[e.index]!, state: 'unchanged' };
        break;
      case 'failed':
        rows[e.index] = { ...rows[e.index]!, state: 'failed' };
        break;
      case 'done':
        done = true;
        break;
    }
  }

  async function start(text: string) {
    rows = [];
    done = false;
    const mine = ++seq;
    await ultra(text, (e) => {
      if (mine === seq) onEvent(e);
    });
  }

  /** The selection rebuilt with every fragment's output. */
  const result = $derived(rows.map((r) => (r.state === 'translated' ? r.out : r.text) + r.sep).join('').trimEnd());

  /** Languages and how many fragments each. */
  const langs = $derived.by(() => {
    const m = new Map<string, number>();
    for (const r of rows) m.set(r.lang ?? '?', (m.get(r.lang ?? '?') ?? 0) + 1);
    return [...m.entries()];
  });

  async function copyAll() {
    await copyText(result);
    copied = true;
    setTimeout(() => (copied = false), 1200);
  }

  onMount(() => {
    const demo = new URLSearchParams(location.search).get('demo');
    if (demo !== null) void start(demo);
    void onShown<string | null>('window:shown', (text) => {
      if (text) void start(text);
    });
  });
</script>

<div class="win">
  <TitleBar icon="layers-18" title={u.title} />
  <div class="summary">
    <span class="count">{rows.length} {u.fragments}</span>
    {#each langs as [l, n] (l)}
      <Badge>{l === '?' ? '?' : langName(l)} · {n}</Badge>
    {/each}
    <span class="spacer"></span>
    <span class="to">→ {langName(target)}</span>
  </div>
  <main class="list">
    {#each rows as r, i (i)}
      <article class="frag">
        <span class="lang">{r.lang ? r.lang.split('-')[0]!.toUpperCase() : '?'}</span>
        <div class="texts">
          <span class="orig">{r.text}</span>
          {#if r.state === 'translated'}
            <span class="out">{r.out ?? ""}</span>
          {:else if r.state === 'unchanged'}
            <span class="same">{u.unchanged}</span>
          {:else if r.state === 'failed'}
            <span class="fail">{u.failed}</span>
          {:else}
            <span class="pending">{u.pending}</span>
          {/if}
        </div>
      </article>
    {/each}
  </main>
  <footer class="foot">
    <span class="spacer"></span>
    <Button icon="replace" disabled={!done} onclick={() => replaceSelection(result)}>{s.replaceSelection}</Button>
    <Button variant="primary" icon={copied ? 'check' : 'copy'} disabled={!done} onclick={copyAll}>
      {copied ? s.copied : u.copyAll}
    </Button>
  </footer>
</div>

<style>
  .win {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg-surface);
  }
  .summary {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 14px 20px 10px;
  }
  .count {
    font: var(--body-medium);
    color: var(--text-primary);
  }
  .to {
    font: var(--caption-medium);
    color: var(--text-secondary);
  }
  .spacer {
    flex: 1;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 20px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .frag {
    display: flex;
    gap: 12px;
    padding: 10px 12px;
    border-radius: var(--radius-lg);
    background: var(--bg-sunken);
  }
  .lang {
    flex-shrink: 0;
    min-width: 28px;
    height: 22px;
    padding: 0 6px;
    border-radius: 5px;
    background: var(--accent-soft);
    font: 500 12px/22px var(--font);
    color: var(--text-primary);
    text-align: center;
  }
  .texts {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .orig {
    font: var(--body);
    color: var(--text-secondary);
    user-select: text;
  }
  .out {
    font: var(--body-medium);
    color: var(--text-primary);
    user-select: text;
  }
  .same,
  .pending {
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .pending {
    animation: pulse 1s ease-in-out infinite;
  }
  .fail {
    font: var(--caption);
    color: var(--danger);
  }
  @keyframes pulse {
    50% {
      opacity: 0.4;
    }
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 60px;
    padding: 0 20px;
    border-top: 1px solid var(--line-soft);
  }
</style>
