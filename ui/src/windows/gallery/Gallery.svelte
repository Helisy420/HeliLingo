<!-- Dev-only component sheet, laid out like the Figma frame «Элементы интерфейса»
     (16:2238 / 16:509 / 10:3) so the two can be compared side by side.
     ?theme=blue|dark|light picks the theme. -->
<script lang="ts">
  import Badge from '../../lib/ui/Badge.svelte';
  import Bits from '../../lib/ui/Bits.svelte';
  import Button from '../../lib/ui/Button.svelte';
  import Checkbox from '../../lib/ui/Checkbox.svelte';
  import Field from '../../lib/ui/Field.svelte';
  import Icon from '../../lib/ui/Icon.svelte';
  import IconButton from '../../lib/ui/IconButton.svelte';
  import Radio from '../../lib/ui/Radio.svelte';
  import Segmented from '../../lib/ui/Segmented.svelte';
  import SelectButton from '../../lib/ui/SelectButton.svelte';
  import Switch from '../../lib/ui/Switch.svelte';

  const theme = new URLSearchParams(location.search).get('theme') ?? 'blue';
  document.documentElement.dataset.theme = theme;

  const states = ['По умолчанию', 'Наведение', 'Нажатие', 'Фокус', 'Отключена'];
  const demos = [undefined, 'hover', 'pressed', 'focus', undefined] as const;
  const rows = [
    { variant: 'primary', name: 'Основная', text: 'Перевести' },
    { variant: 'secondary', name: 'Вторичная', text: 'Отмена' },
    { variant: 'ghost', name: 'Прозрачная', text: 'История' },
    { variant: 'danger', name: 'Опасная', text: 'Очистить' },
  ] as const;

  const iconStates: [string, { demo?: 'hover' | 'pressed'; active?: boolean; disabled?: boolean }][] = [
    ['обычная', {}],
    ['наведение', { demo: 'hover' }],
    ['нажатие', { demo: 'pressed' }],
    ['включена', { active: true }],
    ['отключена', { disabled: true }],
  ];

  let mode = $state('text');
  let themeSeg = $state('blue');
</script>

<main class="sheet">
  <h1 class="h-page">Элементы интерфейса</h1>

  <section>
    <h2 class="h-section">Кнопки</h2>
    <div class="row">
      <span class="label"></span>
      {#each states as s (s)}<span class="col caption">{s}</span>{/each}
    </div>
    {#each rows as r (r.variant)}
      <div class="row">
        <span class="label name">{r.name}</span>
        {#each demos as d, i (i)}
          <span class="col cell">
            <Button variant={r.variant} demo={d} disabled={i === 4}>{r.text}</Button>
          </span>
        {/each}
      </div>
    {/each}
    <div class="flow">
      <Button variant="primary" icon="copy">Копировать</Button>
      <Button icon="upload">Выбрать файл</Button>
      <Button variant="primary" icon="scan">Скриншот области</Button>
      <Button variant="ghost" icon="history">История</Button>
      <Button variant="danger" icon="trash">Очистить кэш</Button>
      <Button iconRight="chev-r">Подробнее</Button>
    </div>
    <div class="flow">
      <Button variant="primary" size="sm" icon="check">Применить</Button>
      <Button size="sm">Изменить</Button>
      <Button variant="ghost" size="sm">Сбросить</Button>
      <Button variant="danger" size="sm" icon="trash">Удалить</Button>
    </div>
  </section>

  <section>
    <h2 class="h-section">Кнопки-иконки</h2>
    <div class="flow wide">
      {#each iconStates as [cap, p] (cap)}
        <span class="stack">
          <IconButton icon="copy" label="Копировать" {...p} />
          <span class="caption">{cap}</span>
        </span>
      {/each}
      <span class="flow tight top">
        <IconButton icon="speaker" label="Озвучить" />
        <IconButton icon="clip" label="Вставить" />
        <IconButton icon="history" label="История" />
        <IconButton icon="gear" label="Настройки" />
        <IconButton icon="pin" label="Закрепить" />
        <IconButton icon="x" label="Закрыть" />
        <button class="swap" aria-label="Поменять языки"><Icon name="swap" /></button>
      </span>
    </div>
  </section>

  <section>
    <h2 class="h-section">Переключатели и выбор</h2>
    <div class="flow switches">
      <Switch checked />
      <Switch />
      <Checkbox checked />
      <Checkbox />
      <Radio group="a" value="a" />
      <Radio group="a" value="b" />
      <Segmented
        bind:value={mode}
        options={[
          { value: 'text', label: 'Текст', icon: 'languages' },
          { value: 'image', label: 'Изображения', icon: 'image' },
        ]}
      />
      <Segmented
        bind:value={themeSeg}
        options={[
          { value: 'light', label: 'Белая', icon: 'sun' },
          { value: 'dark', label: 'Тёмная', icon: 'moon' },
          { value: 'blue', label: 'Синяя', icon: 'drop' },
          { value: 'system', label: 'Системная', icon: 'monitor' },
        ]}
      />
    </div>
  </section>

  <section>
    <h2 class="h-section">Поля и списки</h2>
    <div class="flow fields">
      <Field placeholder="Введите текст" hint="обычное" width={230} />
      <Field placeholder="Введите текст" hint="наведение" width={230} demo="hover" />
      <Field value="sk-or-v1-8a3f" hint="ввод" width={230} demo="focus" />
      <Field value="Неверный ключ" error="Проверьте ключ и повторите" width={230} />
      <Field placeholder="Недоступно" hint="отключено" width={230} disabled />
    </div>
    <div class="flow fields">
      <Field placeholder="Найти язык" icon="search" width={230} />
      <Field value="sk-or-v1-0000000000" icon="key" password width={260} />
      <SelectButton width={190}>Русский</SelectButton>
      <SelectButton width={190} demo="hover">Русский</SelectButton>
      <SelectButton width={190} open>Русский</SelectButton>
    </div>
  </section>

  <section>
    <h2 class="h-section">Метки, подсказки, прогресс</h2>
    <div class="flow wide">
      <Badge tone="accent">авто</Badge>
      <Badge dot>офлайн</Badge>
      <Badge>новое</Badge>
      <Bits kind="tooltip">Копировать перевод</Bits>
      <Bits kind="progress" value={0.42} />
      <Bits kind="progress" value={1} />
      <span class="flow keys"><Bits kind="key">Ctrl</Bits><Bits kind="key">Ctrl</Bits></span>
      <Bits kind="chip">Где находится ближайшая аптека?</Bits>
    </div>
  </section>
</main>

<style>
  :global(body) {
    overflow: auto;
    background: var(--bg-sunken);
  }
  .sheet {
    width: 1400px;
    margin: 0;
    padding: 40px;
    display: flex;
    flex-direction: column;
    gap: 36px;
    background: var(--bg-surface);
    border: 1px solid var(--line-soft);
    border-radius: var(--radius-window);
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .row {
    display: flex;
    align-items: center;
  }
  .label {
    width: 110px;
    flex-shrink: 0;
  }
  .name {
    font: var(--body-medium);
    color: var(--text-secondary);
  }
  .col {
    width: 170px;
    flex-shrink: 0;
  }
  .cell {
    height: 44px;
    display: flex;
    align-items: center;
  }
  .flow {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .flow.wide {
    gap: 24px;
  }
  .flow.tight {
    gap: 8px;
  }
  .flow.switches {
    gap: 32px;
  }
  .flow.fields {
    gap: 20px;
    align-items: flex-start;
  }
  .flow.keys {
    gap: 4px;
  }
  .top {
    align-self: flex-start;
  }
  .stack {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .swap {
    width: 36px;
    height: 36px;
    border-radius: 18px;
    background: var(--bg-surface);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--text-primary);
  }
</style>
