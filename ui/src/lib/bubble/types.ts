// What a bubble shows and how (Settings → «Баблы и Вики»). The double-Ctrl
// popup and the settings previews draw the same `Bubble` component.

import type { DictGroup, EngineError, WikiOutcome } from '../ipc';

export type BubbleData = {
  /** The selected text. */
  source: string;
  text: string;
  sourceLang: string | null;
  target: string;
  engine: string;
  busy: boolean;
  error: EngineError | null;
  /** "Nothing is selected" and the like. */
  notice: string;
  alternatives: string[];
  dict: DictGroup[];
  transcription: string | null;
  /** Example sentences (source language) and their translations once known. */
  examples: { text: string; tr: string | null }[];
};

export type BubbleStyle = 'full' | 'translation' | 'buttons';

export type BubbleLook = {
  style: BubbleStyle;
  collapseLong: boolean;
  wordMode: 'dictionary' | 'wiki' | 'both' | 'none';
  /** Compact styles: part of speech by words. */
  pos: boolean;
  /** Compact styles: other translations of a word (also …, dictionary). */
  variants: boolean;
  /** Wikipedia summaries on («Справка из Википедии»). */
  wiki: boolean;
  wikiImage: boolean;
  fontSize: number;
  opacity: number;
  animation: boolean;
};

/** The Wikipedia part: off, loading, or the answer. */
export type WikiState = 'off' | 'loading' | WikiOutcome;

export type BubbleActions = {
  copy: (text?: string) => void;
  replace: (text?: string) => void;
  speak: () => void;
  close: () => void;
  pin: () => void;
  /** «W»: show or hide the Wikipedia card. */
  wiki: () => void;
  openUrl: (url: string) => void;
  settings: () => void;
  google: () => void;
  /** Start dragging the window by the bubble's grip. */
  drag: () => void;
};

export const EMPTY: BubbleData = {
  source: '',
  text: '',
  sourceLang: null,
  target: 'ru',
  engine: 'google',
  busy: false,
  error: null,
  notice: '',
  alternatives: [],
  dict: [],
  transcription: null,
  examples: [],
};

/** «Для одиночных слов» wants the dictionary / the Wikipedia card. */
export const wantsDict = (mode: BubbleLook['wordMode']) => mode === 'dictionary' || mode === 'both';
export const wantsWiki = (mode: BubbleLook['wordMode']) => mode === 'wiki' || mode === 'both';

/** A word or a short term (dictionary and Wikipedia make sense for it). */
export function isWordLike(source: string): boolean {
  const s = source.trim();
  return s.length > 0 && s.split(/\s+/).length <= 3 && s.length <= 40;
}

/** Sample bubbles for the settings previews. */
export function samples(ru: boolean): Record<'short' | 'long' | 'word', BubbleData> {
  const base = { ...EMPTY, sourceLang: 'en', target: 'ru' };
  return {
    short: { ...base, source: 'Where is the nearest pharmacy?', text: ru ? 'Где ближайшая аптека?' : 'Where is the nearest pharmacy?' },
    long: {
      ...base,
      source: 'The meeting has been moved to Thursday. Could you send me the updated slides before then?',
      text:
        'Встречу перенесли на четверг. Не могли бы вы прислать мне обновлённые слайды до этого? Я хотел бы просмотреть раздел с ценами вместе с командой и убедиться, что цифры совпадают с последним прогнозом. Если что-то непонятно, дайте знать — мы можем коротко созвониться завтра утром.',
    },
    word: {
      ...base,
      source: 'car',
      text: 'машина',
      transcription: 'kɑː',
      alternatives: ['автомобиль', 'вагон'],
      dict: [
        {
          pos: 'noun',
          terms: ['машина', 'автомобиль', 'вагон'],
          entries: [
            { word: 'автомобиль', gloss: ['car', 'vehicle', 'automobile'] },
            { word: 'машина', gloss: ['machine', 'car', 'engine'] },
            { word: 'вагон', gloss: ['car', 'carriage', 'coach'] },
            { word: 'тачка', gloss: ['wheelbarrow', 'car'] },
          ],
        },
      ],
      examples: [
        { text: "we're going by car", tr: 'мы едем на машине' },
        { text: 'she drove up in a car', tr: 'она подъехала на машине' },
      ],
    },
  };
}

/** The Wikipedia answer the previews show. */
export const SAMPLE_WIKI: WikiOutcome = {
  kind: 'found',
  article: {
    title: 'Автомобиль',
    extract:
      'Автомоби́ль — моторное дорожное и внедорожное транспортное средство, используемое для перевозки людей или грузов. Основное назначение автомобиля заключается в совершении транспортной работы.',
    url: 'https://ru.wikipedia.org/wiki/Автомобиль',
    thumbnail: null,
  },
};
