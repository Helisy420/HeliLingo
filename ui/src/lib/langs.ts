// Languages, same codes and order as hl_core::lang::LANGUAGES.

import { isEnglish } from './i18n.svelte';

/** [code, Russian name, English name] */
export const LANGUAGES: [string, string, string][] = [
  ['en', 'Английский', 'English'],
  ['ru', 'Русский', 'Russian'],
  ['uk', 'Украинский', 'Ukrainian'],
  ['de', 'Немецкий', 'German'],
  ['fr', 'Французский', 'French'],
  ['es', 'Испанский', 'Spanish'],
  ['it', 'Итальянский', 'Italian'],
  ['pt', 'Португальский', 'Portuguese'],
  ['nl', 'Нидерландский', 'Dutch'],
  ['pl', 'Польский', 'Polish'],
  ['cs', 'Чешский', 'Czech'],
  ['sk', 'Словацкий', 'Slovak'],
  ['sv', 'Шведский', 'Swedish'],
  ['da', 'Датский', 'Danish'],
  ['no', 'Норвежский', 'Norwegian'],
  ['fi', 'Финский', 'Finnish'],
  ['et', 'Эстонский', 'Estonian'],
  ['lv', 'Латышский', 'Latvian'],
  ['lt', 'Литовский', 'Lithuanian'],
  ['hu', 'Венгерский', 'Hungarian'],
  ['ro', 'Румынский', 'Romanian'],
  ['bg', 'Болгарский', 'Bulgarian'],
  ['el', 'Греческий', 'Greek'],
  ['tr', 'Турецкий', 'Turkish'],
  ['be', 'Белорусский', 'Belarusian'],
  ['kk', 'Казахский', 'Kazakh'],
  ['ka', 'Грузинский', 'Georgian'],
  ['hy', 'Армянский', 'Armenian'],
  ['he', 'Иврит', 'Hebrew'],
  ['ar', 'Арабский', 'Arabic'],
  ['fa', 'Персидский', 'Persian'],
  ['hi', 'Хинди', 'Hindi'],
  ['th', 'Тайский', 'Thai'],
  ['vi', 'Вьетнамский', 'Vietnamese'],
  ['id', 'Индонезийский', 'Indonesian'],
  ['ja', 'Японский', 'Japanese'],
  ['ko', 'Корейский', 'Korean'],
  ['zh-CN', 'Китайский', 'Chinese'],
];

export function langName(code: string): string {
  if (code === 'auto') return isEnglish() ? 'Detect language' : 'Определить язык';
  const l = LANGUAGES.find(([c]) => c === code) ?? LANGUAGES.find(([c]) => c.split('-')[0] === code.split('-')[0]);
  return l ? (isEnglish() ? l[2] : l[1]) : code.toUpperCase();
}

/** Picker search: the code's start, or part of either name ("ger", "нем", "de"). */
export function langMatches(code: string, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q || code.toLowerCase().startsWith(q)) return true;
  const l = LANGUAGES.find(([c]) => c === code);
  return !!l && (l[1].toLowerCase().includes(q) || l[2].toLowerCase().includes(q));
}

export function sameLang(a: string, b: string): boolean {
  return a.split('-')[0]!.toLowerCase() === b.split('-')[0]!.toLowerCase();
}

/** Recently picked languages, newest first (a per-PC convenience). */
const RECENT_KEY = 'hl.recentLangs';

export function recentLangs(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]');
    return Array.isArray(v) ? v.filter((c) => typeof c === 'string').slice(0, 3) : [];
  } catch {
    return [];
  }
}

export function rememberLang(code: string) {
  if (code === 'auto') return;
  try {
    const list = [code, ...recentLangs().filter((c) => c !== code)].slice(0, 3);
    localStorage.setItem(RECENT_KEY, JSON.stringify(list));
  } catch {
    // Storage off: the list just stays empty.
  }
}

/** Russian prepositional case for "на …м" ("Если текст уже на русском"). */
const PREPOSITIONAL: Record<string, string> = {
  ru: 'русском',
  en: 'английском',
  uk: 'украинском',
  de: 'немецком',
  fr: 'французском',
  es: 'испанском',
  it: 'итальянском',
  pt: 'португальском',
  pl: 'польском',
  zh: 'китайском',
  ja: 'японском',
  ko: 'корейском',
  tr: 'турецком',
  be: 'белорусском',
  kk: 'казахском',
};

/** "русском" (ru) / "Russian" (en) — the language after "on/in". */
export function langOn(code: string): string {
  if (isEnglish()) return langName(code);
  return PREPOSITIONAL[code.split('-')[0]!] ?? `языке «${langName(code)}»`;
}

/** "английский" — a language name inside a sentence (Russian lowercase). */
export function langLower(code: string): string {
  return isEnglish() ? langName(code) : langName(code).toLowerCase();
}
