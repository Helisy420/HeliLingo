// Typed wrappers for the Rust commands (src-tauri/src/commands.rs).
// Components call these, never `invoke` with raw strings. In a plain
// browser (vite dev, headless screenshots) they answer with canned data.

import { Channel, invoke } from '@tauri-apps/api/core';

/** Inside the app, or a plain browser. */
export const inApp = '__TAURI_INTERNALS__' in window;

export type Surface = 'popup' | 'quick' | 'translator' | 'ultra' | 'paste';

/** hl_core::EngineError, as serialized. */
export type EngineError =
  | { kind: 'noKey' }
  | { kind: 'unauthorized' }
  | { kind: 'paymentRequired' }
  | { kind: 'rateLimited' }
  | { kind: 'unsupportedPair' }
  | { kind: 'network'; detail: string }
  | { kind: 'timeout' }
  | { kind: 'server'; detail: string }
  | { kind: 'noEngines' }
  | { kind: 'other'; detail: string };

/** hl_core::DictGroup: translations of a word for one part of speech. */
export type DictEntry = { word: string; gloss: string[] };
export type DictGroup = { pos: string; terms: string[]; entries?: DictEntry[] };

/** hl_core::TranslateEvent, as serialized. */
export type TranslateEvent =
  | { type: 'planned'; id: number; sourceLang: string | null; target: string }
  | { type: 'started'; id: number; engine: string }
  | { type: 'delta'; id: number; text: string }
  | { type: 'replace'; id: number; engine: string; text: string }
  | {
      type: 'done';
      id: number;
      engine: string;
      text: string;
      sourceLang: string | null;
      alternatives: string[];
      /** Dictionary groups of a word, by part of speech. */
      dict: DictGroup[];
      transcription: string | null;
      examples: string[];
      cached: boolean;
    }
  | { type: 'error'; id: number; kind: EngineError };

export type TranslateArgs = {
  text: string;
  source: string;
  target: string;
  surface: Surface;
  /** Engine picked in the window; goes first in the chain. */
  engine?: string;
};

/** src-tauri state::EngineInfo. */
export type EngineInfo = {
  id: string;
  name: string;
  /** Offline models carry their own note; cloud notes are in i18n. */
  note?: string;
  group: 'cloud' | 'offline';
  enabled: boolean;
  implemented: boolean;
  /** Works only with a key. */
  needsKey: boolean;
  /** Takes a key but works without one. */
  keyOptional: boolean;
  /** OpenRouter without a chosen model. */
  needsModel: boolean;
  keyHint: string | null;
};

/** Usable in a window's engine picker. */
export function usable(e: EngineInfo): boolean {
  return e.enabled && e.implemented && (!e.needsKey || !!e.keyHint) && !e.needsModel;
}

/** hl_providers::openrouter::ModelInfo. Prices: USD per million tokens. */
export type ModelInfo = { id: string; name: string; context: number; priceIn: number; priceOut: number };

export const openrouterModels = (refresh = false) =>
  call<ModelInfo[]>('openrouter_models', { refresh }, () => [
    { id: 'google/gemma-3-27b-it:free', name: 'Google: Gemma 3 27B (free)', context: 96000, priceIn: 0, priceOut: 0 },
    { id: 'openai/gpt-5-mini', name: 'OpenAI: GPT-5 Mini', context: 400000, priceIn: 0.25, priceOut: 2 },
    { id: 'anthropic/claude-haiku-5.5', name: 'Anthropic: Claude Haiku 5.5', context: 200000, priceIn: 1, priceOut: 5 },
    { id: 'deepseek/deepseek-v3.2', name: 'DeepSeek: DeepSeek V3.2', context: 163840, priceIn: 0.27, priceOut: 0.4 },
  ]);

/** hl_cache::history::Item. */
export type HistoryItem = {
  id: number;
  sourceText: string;
  text: string;
  sourceLang: string | null;
  targetLang: string;
  engine: string;
  surface: string;
  starred: boolean;
  created: number;
};

/** hl_cache::stats::Summary. */
export type StatsSummary = {
  translations: number;
  previous: number;
  chars: number;
  streak: number;
  bestStreak: number;
  features: Record<string, number>;
  pairs: [string, number][];
  words: [string, string, number][];
  /** OpenRouter tokens sent and received. */
  tokensIn: number;
  tokensOut: number;
};

export type Action =
  | 'translateSelection'
  | 'quick'
  | 'translator'
  | 'ultra'
  | 'copyTranslation'
  | 'screenshot'
  | 'windowCapture'
  | 'pasteTranslation'
  | 'settings'
  | 'history';

function call<T>(cmd: string, args: Record<string, unknown> | undefined, fallback: () => T): Promise<T> {
  return inApp ? invoke<T>(cmd, args) : Promise.resolve(fallback());
}

/** Starts a translation; events arrive in `onEvent`. Resolves to the request id. */
export function translate(args: TranslateArgs, onEvent: (e: TranslateEvent) => void): Promise<number> {
  if (!inApp) return mock.translate(args, onEvent);
  const channel = new Channel<TranslateEvent>();
  channel.onmessage = onEvent;
  return invoke<number>('translate', { args, onEvent: channel });
}

export function cancel(surface: Surface): Promise<void> {
  if (!inApp) return mock.cancel();
  return invoke('cancel', { surface });
}

export const engines = () => call<EngineInfo[]>('engines', undefined, () => mock.engines);

/** Pastes `text` over the selection in the app that was in front before
    this window opened (the window hides first). */
export const replaceSelection = (text: string) => call<void>('replace_selection', { text }, () => {});

export const setKey = (engine: string, key: string) => call<void>('set_key', { engine, key }, () => {});
export const runAction = (action: Action) => call<void>('run_action', { action }, () => {});
export const quit = () => call<void>('quit', undefined, () => {});
export const openSettings = (page: string) => call<void>('open_settings', { page }, () => {});

export const historyList = (query: string, starred: boolean) =>
  call<HistoryItem[]>('history_list', { query, starred }, () =>
    mock.history.filter(
      (h) =>
        (!starred || h.starred) &&
        (!query || (h.sourceText + h.text).toLowerCase().includes(query.toLowerCase())),
    ),
  );
export const historyStar = (id: number, starred: boolean) => call<void>('history_star', { id, starred }, () => {});
export const historyDelete = (id: number) => call<void>('history_delete', { id }, () => {});
export const historyClear = () => call<void>('history_clear', undefined, () => {});

export const statsSummary = (period: 'week' | 'month' | 'all') =>
  call<StatsSummary>('stats_summary', { period }, () => mock.stats);
export const statsReset = () => call<void>('stats_reset', undefined, () => {});
export const cacheClear = () => call<void>('cache_clear', undefined, () => {});

/** Browser-only stand-ins (the Figma sample data). */
const mock = (() => {
  let timer: ReturnType<typeof setInterval> | undefined;
  let next = 0;
  const answers: Record<string, [string, string[]]> = {
    'Where is the nearest pharmacy?': ['Где ближайшая аптека?', ['Где находится ближайшая аптека?', 'Где тут рядом аптека?']],
    'The meeting has been moved to Thursday. Could you send me the updated slides before then?': [
      'Встречу перенесли на четверг. Не могли бы вы прислать мне обновлённые слайды до этого?',
      [],
    ],
    effectively: ['эффективно', ['действенно', 'фактически', 'в сущности']],
    cat: ['кошка', ['кот', 'котёнок', 'кошачий']],
    car: ['машина', ['автомобиль', 'вагон']],
  };
  const dicts: Record<string, DictGroup[]> = {
    effectively: [{ pos: 'adverb', terms: ['эффективно', 'действенно', 'фактически', 'в сущности'] }],
    cat: [
      { pos: 'noun', terms: ['кошка', 'кот', 'котёнок', 'котик'] },
      { pos: 'adjective', terms: ['кошачий'] },
    ],
    car: [
      {
        pos: 'noun',
        terms: ['машина', 'автомобиль', 'вагон', 'тачка'],
        entries: [
          { word: 'автомобиль', gloss: ['car', 'vehicle', 'automobile'] },
          { word: 'машина', gloss: ['machine', 'car', 'engine'] },
          { word: 'вагон', gloss: ['car', 'carriage', 'coach'] },
          { word: 'тачка', gloss: ['wheelbarrow', 'car'] },
          { word: 'тележка', gloss: ['truck', 'cart', 'trolley'] },
        ],
      },
    ],
  };
  const extras: Record<string, { transcription: string; examples: string[] }> = {
    car: { transcription: 'kɑː', examples: ["we're going by car", 'she drove up in a car'] },
  };
  const t = Math.floor(Date.now() / 1000);
  return {
    /** The Figma sample banner (49:192), block by block. */
    sampleBlocks: {
      'Free shipping on orders over $50': 'Бесплатная доставка при заказе от 50 $',
      'Order today and get your package in 2–3 business days. Offer ends Sunday.':
        'Закажите сегодня — посылка придёт за 2–3 рабочих дня. Предложение действует до воскресенья.',
      'Add to cart': 'В корзину',
    } as Record<string, string>,
    engines: [
      { id: 'google', name: 'Google', group: 'cloud', enabled: true, implemented: true, needsKey: false, keyOptional: true, needsModel: false, keyHint: null },
      { id: 'yandex', name: 'Yandex', group: 'cloud', enabled: true, implemented: true, needsKey: false, keyOptional: true, needsModel: false, keyHint: null },
      { id: 'deepl', name: 'DeepL', group: 'cloud', enabled: true, implemented: true, needsKey: true, keyOptional: false, needsModel: false, keyHint: '•••• 4f2a' },
      { id: 'bing', name: 'Bing', group: 'cloud', enabled: true, implemented: true, needsKey: false, keyOptional: true, needsModel: false, keyHint: null },
      { id: 'openrouter', name: 'OpenRouter', group: 'cloud', enabled: true, implemented: true, needsKey: true, keyOptional: false, needsModel: false, keyHint: '•••• 9c1e' },
    ] as EngineInfo[],
    history: [
      { id: 3, sourceText: 'Where is the nearest pharmacy?', text: 'Где ближайшая аптека?', sourceLang: 'en', targetLang: 'ru', engine: 'google', surface: 'quick', starred: true, created: t - 120 },
      { id: 2, sourceText: 'effectively', text: 'эффективно', sourceLang: 'en', targetLang: 'ru', engine: 'google', surface: 'popup', starred: false, created: t - 3600 },
      { id: 1, sourceText: 'The meeting has been moved to Thursday.', text: 'Встречу перенесли на четверг.', sourceLang: 'en', targetLang: 'ru', engine: 'google', surface: 'translator', starred: false, created: t - 90000 },
    ] as HistoryItem[],
    stats: {
      translations: 233,
      previous: 0,
      chars: 39916,
      streak: 5,
      bestStreak: 5,
      features: { popup: 140, quick: 92, translator: 1, images: 0, ultra: 0 },
      pairs: [['en>ru', 110], ['ru>en', 107], ['uk>ru', 7], ['de>ru', 5], ['fr>ru', 4]],
      words: [['helilingo', 'ХелиЛинго', 5], ['до', 'До', 3], ['фыв', 'фыв', 2], ['1,000', '1000', 1], ['bytes', 'байты', 1], ['confidence', 'Уверенность', 1]],
      tokensIn: 18420,
      tokensOut: 2310,
    } as StatsSummary,
    translate(args: TranslateArgs, onEvent: (e: TranslateEvent) => void): Promise<number> {
      clearInterval(timer);
      const id = ++next;
      const [text, alternatives] = answers[args.text.trim()] ?? [`[${args.target}] ${args.text}`, []];
      const words = text.split(/(?<= )/);
      let i = 0;
      onEvent({ type: 'planned', id, sourceLang: 'en', target: args.target });
      onEvent({ type: 'started', id, engine: 'google' });
      timer = setInterval(() => {
        if (i < words.length) {
          onEvent({ type: 'delta', id, text: words[i++]! });
        } else {
          clearInterval(timer);
          const dict = dicts[args.text.trim()] ?? [];
          const x = extras[args.text.trim()];
          onEvent({
            type: 'done',
            id,
            engine: 'google',
            text,
            sourceLang: 'en',
            alternatives,
            dict,
            transcription: x?.transcription ?? null,
            examples: x?.examples ?? [],
            cached: false,
          });
        }
      }, 40);
      return Promise.resolve(id);
    },
    cancel(): Promise<void> {
      clearInterval(timer);
      return Promise.resolve();
    },
  };
})();

/** Tray switch: shortcuts on/off and pauses. */
export type PowerInfo = { enabled: boolean; pausedSecs: number };
export const power = () => call<PowerInfo>('power', undefined, () => ({ enabled: true, pausedSecs: 0 }));
export const setEnabled = (enabled: boolean) =>
  call<PowerInfo>('set_enabled', { enabled }, () => ({ enabled, pausedSecs: 0 }));
export const pause = (minutes: number) =>
  call<PowerInfo>('pause', { minutes }, () => ({ enabled: true, pausedSecs: minutes * 60 }));

export type Check = { id: string; ok: boolean; error: EngineError | null; ms: number };
export const checkEngines = () =>
  call<Check[]>('check_engines', undefined, () => [{ id: 'google', ok: true, error: null, ms: 412 }]);
export const glossaryCount = () => call<number>('glossary_count', undefined, () => 2);
export const openGlossary = () => call<void>('open_glossary', undefined, () => {});
export const openUrl = (url: string) => call<void>('open_url', { url }, () => void window.open(url, '_blank'));

/** A translation without a window or history (example sentences). */
export const translateQuiet = (text: string, source: string, target: string) =>
  call<string>('translate_quiet', { text, source, target }, () =>
    text
      .replace("we're going by car", 'мы едем на машине')
      .replace('she drove up in a car', 'она подъехала на машине'),
  );

/** The bubble's pin and drag (the page has no focus, so Rust moves it). */
export const popupPin = (pinned: boolean) => call<void>('popup_pin', { pinned }, () => {});
export const popupPinned = () => call<boolean>('popup_pinned', undefined, () => false);
export const popupDrag = () => call<void>('popup_drag', undefined, () => {});
export const popupHide = () => call<void>('popup_hide', undefined, () => {});

/** update::Check. */
export type UpdateRelease = {
  version: string;
  date: string;
  notes: { ru: string; en: string };
  url: string;
  sha256: string;
  size: number;
  page: string;
};
export type UpdateCheck = { current: string; latest: UpdateRelease; available: boolean };
export const updateCheck = () =>
  call<UpdateCheck>('update_check', undefined, () => ({
    current: '2.5.0',
    available: true,
    latest: {
      version: '2.6.0',
      date: '2026-11-01',
      notes: { ru: 'Живой перевод областей стал быстрее.', en: 'Live area translation is faster.' },
      url: '',
      sha256: '',
      size: 67_000_000,
      page: 'https://github.com/Helisy420/HeliLingo/releases',
    },
  }));
/** Downloads, checks and swaps in the update, then restarts. */
export const updateInstall = () => call<void>('update_install', undefined, () => {});

export type NetStatus = { online: boolean; offlineReady: boolean };
export const netStatus = () => call<NetStatus>('net_status', undefined, () => ({ online: navigator.onLine, offlineReady: false }));

/** hl_providers::wiki::Outcome. */
export type WikiArticle = { title: string; extract: string; url: string; thumbnail: string | null };
export type WikiOutcome =
  | { kind: 'found'; article: WikiArticle }
  | { kind: 'notFound'; searchUrl: string }
  | { kind: 'failed'; searchUrl: string };

export const wikiLookup = (source: string, sourceLang: string | null, translation: string | null, target: string) =>
  call<WikiOutcome>('wiki_lookup', { source, sourceLang, translation, target }, () => ({
    kind: 'found',
    article: {
      title: 'Кошка',
      extract:
        'Ко́шка — домашнее животное, одно из наиболее популярных «животных-компаньонов». С точки зрения научной систематики домашняя кошка — млекопитающее семейства кошачьих отряда хищных.',
      url: 'https://ru.wikipedia.org/wiki/Кошка',
      thumbnail: null,
    },
  }));

/** hl_offline::manager::Status. */
export type ModelStatus =
  | { state: 'notInstalled'; partial: boolean; size: number }
  | { state: 'downloading'; done: number; total: number }
  | { state: 'installed'; size: number }
  | { state: 'failed'; error: 'network' | 'checksum' | 'disk' | 'space' | 'source' | 'incomplete' | 'cancelled' };

export type OfflineModel = { id: string; name: string; status: ModelStatus; enabled: boolean; loaded: boolean };
export type Gpu = { name: string; vendor: number; vram: number; driver: string | null; cuda: [number, number] | null };
export type OfflineStatus = { gpu: Gpu | null; backend: string; cores: number; models: OfflineModel[] };

const GB = 1 << 30;
export const offlineStatus = () =>
  call<OfflineStatus>('offline_status', undefined, () => ({
    gpu: { name: 'NVIDIA GeForce RTX 4060', vendor: 0x10de, vram: 8 * GB, driver: '591.86', cuda: [13, 0] },
    backend: 'CUDA 13',
    cores: 8,
    models: [
      { id: 'opus', name: 'OPUS-MT', status: { state: 'installed', size: 161_000_000 }, enabled: true, loaded: true },
      { id: 'argos', name: 'Argos Translate', status: { state: 'notInstalled', partial: false, size: 352_000_000 }, enabled: false, loaded: false },
      { id: 'nllb600', name: 'NLLB-200 600M', status: { state: 'downloading', done: 263_000_000, total: 630_000_000 }, enabled: false, loaded: false },
      { id: 'nllb13', name: 'NLLB-200 1.3B', status: { state: 'notInstalled', partial: false, size: 1_388_000_000 }, enabled: false, loaded: false },
      { id: 'gemma4', name: 'TranslateGemma 4B', status: { state: 'failed', error: 'network' }, enabled: false, loaded: false },
      { id: 'gemma12', name: 'TranslateGemma 12B', status: { state: 'notInstalled', partial: false, size: 7_878_000_000 }, enabled: false, loaded: false },
      { id: 'granite', name: 'Granite Vision 4.1', status: { state: 'notInstalled', partial: false, size: 3_839_000_000 }, enabled: false, loaded: false },
    ],
  }));
export type ModelDir = { path: string; isDefault: boolean; free: number | null };
export const offlineDir = () =>
  call<ModelDir>('offline_dir', undefined, () => ({ path: 'C:\\Users\\you\\AppData\\Local\\HeliLingo\\models', isDefault: true, free: 112 * GB }));
export const offlinePickDir = () => call<string | null>('offline_pick_dir', undefined, () => null);

/** «Папка для результатов» (Settings → Изображения и файлы). */
export type FilesDir = { path: string; isDefault: boolean };
export const filesDir = () =>
  call<FilesDir>('files_dir', undefined, () => ({ path: 'C:\\Users\\you\\Documents\\HeliLingo', isDefault: true }));
export const filesPickDir = () => call<string | null>('files_pick_dir', undefined, () => null);
export const offlineOpenDir = () => call<void>('offline_open_dir', undefined, () => {});
export const offlineInstall = (id: string) => call<void>('offline_install', { id }, () => {});
export const offlineCancel = (id: string) => call<void>('offline_cancel', { id }, () => {});
export const offlineDelete = (id: string) => call<void>('offline_delete', { id }, () => {});

/** hl_win::ocr types. */
export type OcrBlock = { text: string; bounds: { x: number; y: number; w: number; h: number }; lines: number; lineHeight: number };
export type OcrResult = { blocks: OcrBlock[]; language: string; width: number; height: number };
export type OcrError = { kind: 'noLanguage'; detail: string | null } | { kind: 'badImage' } | { kind: 'failed'; detail: string };

/** Recognizes text in an encoded image (raw bytes, no JSON copy). */
export function ocrImage(bytes: Uint8Array, lang: string): Promise<OcrResult> {
  if (!inApp)
    return Promise.resolve({
      blocks: [{ text: 'The meeting has been moved to Thursday.', bounds: { x: 20, y: 20, w: 300, h: 24 }, lines: 1, lineHeight: 24 }],
      language: 'en-US',
      width: 400,
      height: 100,
    });
  return invoke<OcrResult>('ocr_image', bytes, { headers: { lang } });
}

export type UltraFragment = { text: string; sep: string; lang: string | null };
export type UltraEvent =
  | { type: 'split'; fragments: UltraFragment[]; target: string }
  | { type: 'translated'; index: number; text: string; engine: string }
  | { type: 'unchanged'; index: number }
  | { type: 'failed'; index: number; error: EngineError }
  | { type: 'done' };

/** Ultra mode on `text`; per-fragment results arrive in `onEvent`. */
export function ultra(text: string, onEvent: (e: UltraEvent) => void): Promise<number> {
  if (!inApp) {
    const parts = [
      { text: 'The build failed again.', sep: ' ', lang: 'en' },
      { text: 'Посмотри логи, пожалуйста.', sep: '\n', lang: 'ru' },
      { text: 'Der Server antwortet nicht.', sep: '', lang: 'de' },
    ];
    onEvent({ type: 'split', fragments: parts, target: 'ru' });
    setTimeout(() => onEvent({ type: 'translated', index: 0, text: 'Сборка снова упала.', engine: 'google' }), 200);
    setTimeout(() => onEvent({ type: 'unchanged', index: 1 }), 250);
    setTimeout(() => onEvent({ type: 'translated', index: 2, text: 'Сервер не отвечает.', engine: 'google' }), 400);
    setTimeout(() => onEvent({ type: 'done' }), 450);
    return Promise.resolve(1);
  }
  const channel = new Channel<UltraEvent>();
  channel.onmessage = onEvent;
  return invoke<number>('ultra', { text, onEvent: channel });
}

export const openInTranslator = (text: string) => call<void>('open_in_translator', { text }, () => {});

/** Clipboard write through Windows (the browser API needs window focus). */
export const copyText = (text: string) => call<void>('copy_text', { text }, () => void navigator.clipboard.writeText(text));

// ── «Перевод файлов» and the screen-area results ─────────────────────

/** files::FileInfo. */
export type FileInfo = { path: string; name: string; size: number; kind: string | null; tooBig: boolean };

/** files::FileEvent. */
export type FileEvent =
  | { type: 'read'; units: number; pages: number | null; groups: number }
  | { type: 'reading'; page: number; pages: number }
  | { type: 'progress'; done: number; total: number; group: number }
  | { type: 'done'; out: string; name: string; sourceLang: string | null }
  | { type: 'error'; kind: string };

const SAMPLE_FILES: FileInfo[] = [
  { path: 'C:\\Docs\\Договор_поставки.docx', name: 'Договор_поставки.docx', size: 2_516_582, kind: 'DOCX', tooBig: false },
  { path: 'C:\\Docs\\Presentation_Q3.pptx', name: 'Presentation_Q3.pptx', size: 5_242_880, kind: 'PPTX', tooBig: false },
  { path: 'C:\\Docs\\invoice_0312.pdf', name: 'invoice_0312.pdf', size: 389_120, kind: 'PDF', tooBig: false },
];

export const filesInfo = (paths: string[]) =>
  call<FileInfo[]>('files_info', { paths }, () => SAMPLE_FILES.filter((f) => paths.includes(f.path)));
export const filesChoose = () => call<FileInfo[]>('files_choose', undefined, () => SAMPLE_FILES);
export const filesFromClipboard = () => call<FileInfo[]>('files_from_clipboard', undefined, () => []);
export const fileCancel = (id: number) => call<void>('file_cancel', { id }, () => {});
export const fileSave = (out: string, name: string) => call<string | null>('file_save', { out, name }, () => null);
export const filesSaveAll = (items: [string, string][]) => call<void>('files_save_all', { items }, () => {});
export const filesOpenDir = () => call<void>('files_open_dir', undefined, () => {});

/** Translates a file; events arrive in `onEvent`. In a browser: a demo run. */
export function fileTranslate(
  id: number,
  path: string,
  source: string,
  target: string,
  onEvent: (e: FileEvent) => void,
): Promise<void> {
  if (!inApp) {
    const groups = path.endsWith('.pptx') ? 22 : 0;
    onEvent({ type: 'read', units: 40, pages: groups ? null : 12, groups });
    let done = 0;
    const t = setInterval(() => {
      done = Math.min(40, done + 4);
      onEvent({ type: 'progress', done, total: 40, group: groups ? Math.ceil((done / 40) * groups) : 0 });
      if (done >= 40) {
        clearInterval(t);
        onEvent({ type: 'done', out: path, name: path.split('\\').pop()!, sourceLang: 'en' });
      }
    }, 400);
    return Promise.resolve();
  }
  const channel = new Channel<FileEvent>();
  channel.onmessage = onEvent;
  return invoke('file_translate', { id, path, source, target, onEvent: channel });
}

export type BatchResult = { texts: string[]; engine: string; sourceLang: string | null };
/** Many lines at once (the blocks of a screen area). */
export const translateBatch = (texts: string[], source: string, target: string, engine: string | null = null) =>
  call<BatchResult>('translate_batch', { texts, source, target, engine }, () => ({
    texts: texts.map((t) => mock.sampleBlocks[t] ?? `[${target}] ${t}`),
    engine: 'google',
    sourceLang: 'en',
  }));

/** An image is on the clipboard (for the Images banner). */
export const clipboardHasImage = () => call<boolean>('clipboard_has_image', undefined, () => true);
/** The clipboard's image as PNG bytes. */
export async function clipboardImage(): Promise<ArrayBuffer | null> {
  if (!inApp) return null;
  return invoke<ArrayBuffer>('clipboard_image').catch(() => null);
}
/** A dropped image file's bytes (≤ 10 MB). */
export async function imageFile(path: string): Promise<ArrayBuffer | null> {
  if (!inApp) return null;
  return invoke<ArrayBuffer>('image_file', { path }).catch(() => null);
}
