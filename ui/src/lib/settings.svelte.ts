// Settings shared by every window: loaded once, updated live when any
// window (or the Rust side) changes them. Mirrors src-tauri/src/settings.rs.

import { setLanguage } from './i18n.svelte';
import { inApp } from './ipc';

export type Theme = 'light' | 'dark' | 'blue' | 'system';

export type Hotkeys = {
  translateSelection: string;
  quick: string;
  translator: string;
  ultra: string;
  copyTranslation: string;
  screenshot: string;
  /** «Перевод окна»: the capture overlay starts in «Окно». Off by default. */
  windowCapture: string;
  pasteTranslation: string;
  doubleCopy: boolean;
};

/** Settings → Баблы и Вики. */
export type BubbleSettings = {
  /** Show 2–3 lines and «Развернуть» for long translations. */
  collapseLong: boolean;
  /** Esc or a click outside closes the bubble. */
  closeOnOutside: boolean;
  /** Wikipedia language: the word's, the translation's, or the interface's. */
  wikiLang: 'word' | 'translation' | 'interface';
  wikiSize: 'short' | 'long';
  wikiImage: boolean;
  placement: 'selection' | 'cursor' | 'center';
  /** Appearance delay, ms. */
  delayMs: number;
  /** For single words: dictionary, Wikipedia, or both. */
  wordMode: 'dictionary' | 'wiki' | 'both' | 'none';
  /** Compact bubbles: part of speech by words. */
  showPos: boolean;
  /** Compact bubbles: other translations of a word (also …, dictionary). */
  showVariants: boolean;
  /** 0 = never. */
  autoCloseSecs: number;
  keepOnHover: boolean;
  rememberPinned: boolean;
  /** Text size, px. */
  fontSize: number;
  /** Background opacity, %. */
  opacity: number;
  animation: boolean;
};

/** Quick translate layout (Settings → Баблы и Вики → Быстрый перевод). */
/** Settings → «Изображения и файлы» → «Область экрана». */
export type ImagesSettings = {
  mode: 'over' | 'plate' | 'side' | 'subtitles';
  /** «Прозрачность подложки», %. */
  backdropOpacity: number;
  altOriginal: boolean;
  /** The area's text language and target ('' = «Переводить на»). */
  sourceLang: string;
  targetLang: string;
  /** Engine for areas (offline models too); null = the usual chain. */
  engine: string | null;
  /** «Субтитры»: text size, px. */
  subtitleSize: number;
};

/** Settings → «Изображения и файлы» → «Файлы». */
export type FilesSettings = {
  format: 'same' | 'docx' | 'txt';
  keepFormatting: boolean;
  bilingual: boolean;
  /** Empty = Documents\HeliLingo. */
  outDir: string;
};

export type QuickSettings = {
  /** 'window': language bar + input + result; 'line': one launcher-like line. */
  layout: 'window' | 'line';
  suggestions: boolean;
  history: boolean;
  /** Icon-only buttons with tooltips instead of labelled ones. */
  miniButtons: boolean;
};

export type Settings = {
  uiLanguage: 'ru' | 'en';
  theme: Theme;
  autostart: boolean;
  animations: boolean;
  hardwareAcceleration: boolean;
  targetLang: string;
  sourceLang: string;
  secondLang: string;
  saveHistory: boolean;
  /** Bubble style (Settings → Баблы и Вики): the full card, translation only, or
      translation with mini-buttons. 'compact' is the old name of 'translation'. */
  popupStyle: 'full' | 'translation' | 'buttons' | 'compact';
  bubble: BubbleSettings;
  quick: QuickSettings;
  images: ImagesSettings;
  files: FilesSettings;
  popupCtrlC: 'copyClose' | 'copy' | 'ignore';
  readAloud: boolean;
  showWiki: boolean;
  ignoredApps: string[];
  skipFullscreen: boolean;
  /** Phone / passport / card numbers drawn as •••• (display only). */
  maskNumbers: boolean;
  engines: { id: string; enabled: boolean }[];
  fallback: boolean;
  timeoutSecs: number;
  googleMode: 'auto' | 'web' | 'api';
  openrouterModel: string | null;
  yandexFolder: string;
  bingRegion: string;
  offline: {
    enabled: boolean;
    device: 'auto' | 'cpu' | 'gpu';
    threads: number;
    precision: 'auto' | 'int8' | 'int8Float16' | 'float32';
    modelDir: string;
    tiers: string[];
    screenOcr: 'windows' | 'granite';
  };
  contextEnabled: boolean;
  contextFragments: number;
  programmerMode: boolean;
  /** «Расширение VS Code»: the local API for editors. */
  editorApi: boolean;
  ultraDetectEach: boolean;
  ultraSkipTarget: boolean;
  ultraWindow: boolean;
  hotkeys: Hotkeys;
  doubleTapMs: number;
  keepStats: boolean;
  autoUpdate: boolean;
  updateChannel: 'stable' | 'beta';
  welcomed: boolean;
};

/** Same as Settings::default() in Rust; used before the real ones load and
    in a plain browser. */
export const DEFAULTS: Settings = {
  uiLanguage: 'ru',
  theme: 'blue',
  autostart: true,
  animations: true,
  hardwareAcceleration: true,
  targetLang: 'ru',
  sourceLang: 'auto',
  secondLang: 'en',
  saveHistory: true,
  popupStyle: 'full',
  bubble: {
    collapseLong: true,
    closeOnOutside: true,
    wikiLang: 'word',
    wikiSize: 'short',
    wikiImage: false,
    placement: 'selection',
    delayMs: 0,
    wordMode: 'dictionary',
    showPos: true,
    showVariants: true,
    autoCloseSecs: 0,
    keepOnHover: true,
    rememberPinned: false,
    fontSize: 15,
    opacity: 100,
    animation: true,
  },
  quick: { layout: 'window', suggestions: true, history: false, miniButtons: false },
  images: {
    mode: 'over',
    backdropOpacity: 80,
    altOriginal: true,
    sourceLang: 'auto',
    targetLang: '',
    engine: null,
    subtitleSize: 26,
  },
  files: { format: 'same', keepFormatting: true, bilingual: false, outDir: '' },
  popupCtrlC: 'copyClose',
  readAloud: false,
  showWiki: true,
  ignoredApps: [],
  skipFullscreen: true,
  maskNumbers: false,
  engines: ['google', 'yandex', 'deepl', 'bing', 'openrouter'].map((id) => ({
    id,
    enabled: id === 'google' || id === 'yandex',
  })),
  fallback: true,
  timeoutSecs: 5,
  googleMode: 'auto',
  openrouterModel: null,
  yandexFolder: '',
  bingRegion: '',
  offline: { enabled: false, device: 'auto', threads: 0, precision: 'auto', modelDir: '', tiers: [], screenOcr: 'windows' },
  contextEnabled: false,
  contextFragments: 5,
  programmerMode: false,
  editorApi: true,
  ultraDetectEach: true,
  ultraSkipTarget: false,
  ultraWindow: true,
  hotkeys: {
    translateSelection: 'Ctrl×2',
    quick: 'Ctrl+Shift+L',
    translator: 'Ctrl+Alt+T',
    ultra: 'Ctrl+Shift+S',
    copyTranslation: 'Ctrl+Shift+C',
    screenshot: 'Ctrl+Shift+O',
    windowCapture: '',
    pasteTranslation: 'Ctrl+Alt+R',
    doubleCopy: true,
  },
  doubleTapMs: 400,
  keepStats: true,
  autoUpdate: true,
  updateChannel: 'stable',
  welcomed: false,
};

/** The live settings. Read fields directly in markup / `$derived`. */
export const settings: Settings = $state(structuredClone(DEFAULTS));

function assign(next: Settings) {
  Object.assign(settings, next);
  applyLook();
}

const dark = matchMedia('(prefers-color-scheme: dark)');

/** Theme on <html> and the interface language. */
function applyLook() {
  const theme = settings.theme === 'system' ? (dark.matches ? 'dark' : 'light') : settings.theme;
  document.documentElement.dataset.theme = theme;
  document.documentElement.lang = settings.uiLanguage;
  document.documentElement.classList.toggle('no-anim', !settings.animations);
  setLanguage(settings.uiLanguage);
}

/** Loads settings and keeps them in sync. A `?theme=` URL parameter
    overrides the theme (screenshots). */
export async function loadSettings(): Promise<void> {
  const forced = new URLSearchParams(location.search).get('theme') as Theme | null;
  if (inApp) {
    const { invoke } = await import('@tauri-apps/api/core');
    const { listen } = await import('@tauri-apps/api/event');
    assign(await invoke<Settings>('get_settings'));
    await listen<Settings>('settings', (e) => assign(e.payload));
  }
  if (forced) settings.theme = forced;
  const lang = new URLSearchParams(location.search).get('lang');
  if (lang === 'en' || lang === 'ru') settings.uiLanguage = lang;
  // Browser previews only: `?patch={"offline":{"enabled":true}}` (screenshots).
  const patch = new URLSearchParams(location.search).get('patch');
  if (!inApp && patch) {
    try {
      merge(settings as unknown as Record<string, unknown>, JSON.parse(patch) as Record<string, unknown>);
    } catch {
      // A bad patch just isn't applied.
    }
  }
  applyLook();
  dark.addEventListener('change', applyLook);
}

/** Changes some fields; every window gets the result. */
export async function updateSettings(patch: Partial<Settings> | Record<string, unknown>): Promise<void> {
  if (!inApp) {
    // Plain browser: deep-merge locally so pages stay usable.
    merge(settings as unknown as Record<string, unknown>, patch as Record<string, unknown>);
    applyLook();
    return;
  }
  const { invoke } = await import('@tauri-apps/api/core');
  assign(await invoke<Settings>('update_settings', { patch }));
}

function merge(base: Record<string, unknown>, patch: Record<string, unknown>) {
  for (const [k, v] of Object.entries(patch)) {
    const b = base[k];
    if (v && typeof v === 'object' && !Array.isArray(v) && b && typeof b === 'object' && !Array.isArray(b)) {
      merge(b as Record<string, unknown>, v as Record<string, unknown>);
    } else {
      base[k] = v;
    }
  }
}
