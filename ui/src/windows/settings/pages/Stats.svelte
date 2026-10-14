<!-- Настройки / Статистика (Figma 16:3774): period switch, three totals,
     features and languages, most translated words, OpenRouter tokens (not
     in Figma: shown once OpenRouter is on or has been used), keep/reset. -->
<script lang="ts">
  import { t } from '../../../lib/i18n.svelte';
  import { statsReset, statsSummary, type StatsSummary } from '../../../lib/ipc';
  import { langName } from '../../../lib/langs';
  import { settings, updateSettings } from '../../../lib/settings.svelte';
  import Bits from '../../../lib/ui/Bits.svelte';
  import Button from '../../../lib/ui/Button.svelte';
  import Icon from '../../../lib/ui/Icon.svelte';
  import Segmented from '../../../lib/ui/Segmented.svelte';
  import Switch from '../../../lib/ui/Switch.svelte';

  const s = $derived(t());
  const st = $derived(s.set.stats);
  let period = $state<'week' | 'month' | 'all'>('week');
  let data = $state<StatsSummary | null>(null);

  $effect(() => {
    const p = period;
    void statsSummary(p).then((d) => (data = d));
  });

  const features = ['popup', 'quick', 'translator', 'images', 'ultra'];
  const maxFeature = $derived(Math.max(1, ...features.map((f) => data?.features[f] ?? 0)));

  /** Top three pairs plus «Другие», as shares. */
  const pairs = $derived.by(() => {
    if (!data || data.translations === 0) return [];
    const total = data.pairs.reduce((a, [, n]) => a + n, 0) || 1;
    const top = data.pairs.slice(0, 3).map(([p, n]) => ({ label: pairLabel(p), share: n / total }));
    const rest = data.pairs.slice(3).reduce((a, [, n]) => a + n, 0);
    if (rest > 0) top.push({ label: st.other, share: rest / total });
    return top;
  });
  const shades = ['var(--accent)', 'var(--text-tertiary)', 'var(--line-strong)', 'var(--line-strong)'];

  function pairLabel(p: string) {
    const [a, b] = p.split('>');
    return `${a === 'auto' ? '?' : langName(a!)} → ${langName(b!)}`;
  }

  const openrouter = $derived(settings.engines.some((e) => e.id === 'openrouter' && e.enabled));

  const fmt = (n: number) => n.toLocaleString(settings.uiLanguage === 'en' ? 'en-US' : 'ru-RU');

  async function reset() {
    await statsReset();
    data = await statsSummary(period);
  }
</script>

<div class="page stats">
  <div class="head">
    <h1 class="h-page">{s.set.nav.stats}</h1>
    <Segmented
      size="sm"
      value={period}
      options={[
        { value: 'week', label: st.week },
        { value: 'month', label: st.month },
        { value: 'all', label: st.all },
      ]}
      onchange={(v) => (period = v as typeof period)}
    />
  </div>

  {#if data}
    <div class="totals">
      <div class="tile">
        <span class="t-label">{st.translations}</span>
        <span class="t-num">{fmt(data.translations)}</span>
        <span class="t-note">
          {period === 'week' ? st.prevWeek(data.previous) : period === 'month' ? st.prevMonth(data.previous) : ' '}
        </span>
      </div>
      <div class="tile">
        <span class="t-label">{st.chars}</span>
        <span class="t-num">{fmt(data.chars)}</span>
        <span class="t-note">{st.pages(Math.max(1, Math.round(data.chars / 1800)))}</span>
      </div>
      <div class="tile">
        <span class="t-label">{st.streak}</span>
        <span class="t-num">{data.streak}</span>
        <span class="t-note">{st.record(data.bestStreak)}</span>
      </div>
    </div>

    <div class="cols">
      <div class="col">
        <div class="col-head"><span>{st.features}</span><span class="spacer"></span><span class="sub">{st.uses}</span></div>
        {#each features as f (f)}
          <div class="line">
            <span>{st.feature[f]}</span>
            <span class="spacer"></span>
            <Bits kind="progress" width={70} value={(data.features[f] ?? 0) / maxFeature} />
            <span class="num">{data.features[f] ?? 0}</span>
          </div>
        {/each}
      </div>
      <div class="col">
        <div class="col-head"><span>{st.languages}</span></div>
        {#if pairs.length}
          <div class="shares">
            {#each pairs as p, i (p.label)}
              <span style:width="{p.share * 100}%" style:background={shades[i]}></span>
            {/each}
          </div>
          {#each pairs as p, i (p.label)}
            <div class="line pair">
              <span class="dot" style:background={shades[i]}></span>
              <span>{p.label}</span>
              <span class="spacer"></span>
              <span class="pct">{Math.round(p.share * 100)}%</span>
            </div>
          {/each}
        {:else}
          <span class="empty">{st.noWords}</span>
        {/if}
      </div>
    </div>

    <section class="words">
      <h3 class="col-head words-head">{st.topWords}</h3>
      {#if data.words.length}
        <div class="word-cols">
          {#each [data.words.slice(0, 3), data.words.slice(3, 6)] as col, c (c)}
            <div class="word-col">
              {#each col as [w, tr, n], i (w)}
                <div class="word">
                  <span class="rank">{c * 3 + i + 1}</span>
                  <span class="w">{w}</span>
                  <span class="tr">→ {tr}</span>
                  <span class="spacer"></span>
                  <span class="times">{n}×</span>
                </div>
              {/each}
            </div>
          {/each}
        </div>
      {:else}
        <span class="empty">{st.noWords}</span>
      {/if}
    </section>

    {#if openrouter || data.tokensIn + data.tokensOut > 0}
      <section class="words">
        <h3 class="col-head words-head">{st.tokens}</h3>
        {#if data.tokensIn + data.tokensOut > 0}
          <div class="tokens">
            <span><strong>{fmt(data.tokensIn)}</strong> {st.tokensIn}</span>
            <span class="sep">·</span>
            <span><strong>{fmt(data.tokensOut)}</strong> {st.tokensOut}</span>
            <span class="sep">·</span>
            <span><strong>{fmt(data.tokensIn + data.tokensOut)}</strong> {st.tokensTotal}</span>
          </div>
        {:else}
          <span class="empty">{st.noTokens}</span>
        {/if}
      </section>
    {/if}
  {/if}

  <div class="foot">
    <Icon name="lock-14" size={14} />
    <span class="local">{st.localOnly}</span>
    <span class="spacer"></span>
    <span class="keep">{st.keep}</span>
    <Switch checked={settings.keepStats} label={st.keep} onchange={(v) => updateSettings({ keepStats: v })} />
    <Button variant="danger" size="sm" icon="trash" onclick={reset}>{st.reset}</Button>
  </div>
</div>

<style>
  .stats {
    gap: 22px !important;
  }
  .head {
    display: flex;
    align-items: flex-start;
    gap: 16px;
  }
  .head h1 {
    flex: 1;
  }
  .totals {
    display: flex;
    gap: 10px;
  }
  .tile {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 14px;
    border-radius: var(--radius-lg);
    background: var(--bg-sunken);
    white-space: nowrap;
  }
  .t-label {
    font: var(--caption);
    color: var(--text-secondary);
  }
  .t-num {
    font: 600 30px/34px var(--font);
    letter-spacing: -0.02em;
    color: var(--text-primary);
  }
  .t-note {
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .cols {
    display: flex;
    gap: 28px;
  }
  .col {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .col-head {
    display: flex;
    align-items: center;
    margin: 0;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--line-strong);
    font: 600 14px/20px var(--font);
    color: var(--text-primary);
  }
  .sub {
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .spacer {
    flex: 1;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 10px;
    font: var(--body);
    color: var(--text-primary);
    white-space: nowrap;
  }
  .num {
    min-width: 24px;
    text-align: right;
    font: var(--body-medium);
  }
  .shares {
    display: flex;
    height: 8px;
    border-radius: 4px;
    overflow: hidden;
    background: var(--bg-sunken);
  }
  .pair {
    gap: 8px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .pct {
    font: var(--body-medium);
    color: var(--text-secondary);
  }
  .words {
    display: flex;
    flex-direction: column;
  }
  .words-head {
    padding-bottom: 10px;
  }
  .word-cols {
    display: flex;
    gap: 28px;
    padding-top: 10px;
  }
  .word-col {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .word {
    display: flex;
    align-items: center;
    gap: 8px;
    white-space: nowrap;
    min-width: 0;
  }
  .rank,
  .times {
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .w {
    font: 600 14px/20px var(--font);
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tr {
    font: var(--caption);
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tokens {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding-top: 10px;
    font: var(--body);
    color: var(--text-secondary);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .tokens strong {
    font: 600 14px/20px var(--font);
    color: var(--text-primary);
  }
  .sep {
    color: var(--text-tertiary);
  }
  .empty {
    padding-top: 10px;
    font: var(--caption);
    color: var(--text-tertiary);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-tertiary);
  }
  .local {
    font: var(--caption);
  }
  .keep {
    font: var(--body);
    color: var(--text-secondary);
  }
</style>
