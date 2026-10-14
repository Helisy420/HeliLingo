// «Скрывать номера» (Settings → Общие → Конфиденциальность): phone, passport
// and card numbers in the history (the History window and Quick's history
// list) are drawn as ••••, so screen sharing or a demo doesn't show them.
// Bubbles, Quick's result, the translator and Ultra show the text as is.
// Only the display changes: copy, Replace, the stored history and what goes
// to translators keep the real text.
//
// A "number" is a run of 9–19 digits, optionally starting with "+", whose
// groups are joined by single spaces, hyphens or brackets:
//   +7 111 111 11 11   +71111111111   8 (111) 111-11-11   1111 111111
// Dates (2026-10-14), money (1 000 000), versions and short numbers have
// fewer digits; lists of years (1990 2000 2010) are left alone.

const SEP = new Set([' ', ' ', '-', '(', ')']);
const isDigit = (c: string | undefined) => c !== undefined && c >= '0' && c <= '9';
const isWordChar = (c: string | undefined) => c !== undefined && /[\p{L}\p{N}_]/u.test(c);

/** [start, end) ranges of numbers to hide in `text` (UTF-16 indexes). */
export function maskRanges(text: string): [number, number][] {
  const out: [number, number][] = [];
  let i = 0;
  while (i < text.length) {
    const plus = text[i] === '+' && (isDigit(text[i + 1]) || (text[i + 1] === ' ' && isDigit(text[i + 2])));
    const opener = text[i] === '(' && isDigit(text[i + 1]);
    if (!(isDigit(text[i]) || plus || opener) || isWordChar(text[i - 1])) {
      i++;
      continue;
    }
    const start = i;
    let digits = 0;
    let end = i;
    let j = i;
    const groups: string[] = [];
    let group = '';
    if (plus) j++;
    while (j < text.length) {
      const c = text[j]!;
      if (isDigit(c)) {
        digits++;
        group += c;
        j++;
        end = j;
        continue;
      }
      // Up to two separators in a row ("8 (111)", ") 111"), then a digit.
      let k = j;
      while (k < text.length && k - j < 2 && SEP.has(text[k]!)) k++;
      if (k > j && isDigit(text[k])) {
        if (group) groups.push(group);
        group = '';
        j = k;
        continue;
      }
      // An opening bracket right at the start.
      if (c === '(' && digits === 0 && isDigit(text[j + 1])) {
        j++;
        continue;
      }
      break;
    }
    if (group) groups.push(group);
    const years = groups.length > 1 && groups.every((g) => /^(19|20)\d\d$/.test(g));
    if (digits >= 9 && digits <= 19 && !isWordChar(text[end]) && !years) {
      // Keep a closing bracket that belongs to the number.
      out.push([start, end]);
    }
    i = Math.max(end, i + 1);
  }
  return out;
}

/** The text with the digits of every masked number turned into bullets. */
export function maskText(text: string): string {
  let out = '';
  let last = 0;
  for (const [a, b] of maskRanges(text)) {
    out += text.slice(last, a) + text.slice(a, b).replace(/\d/g, '•');
    last = b;
  }
  return out + text.slice(last);
}

/** Pieces of `text[from, from + len)` for drawing: plain or masked. */
export function pieces(text: string, ranges: [number, number][], from = 0): { t: string; masked: boolean }[] {
  const end = from + text.length;
  const out: { t: string; masked: boolean }[] = [];
  let pos = from;
  for (const [a, b] of ranges) {
    if (b <= pos || a >= end) continue;
    const s = Math.max(a, pos);
    const e = Math.min(b, end);
    if (s > pos) out.push({ t: text.slice(pos - from, s - from), masked: false });
    out.push({ t: text.slice(s - from, e - from), masked: true });
    pos = e;
  }
  if (pos < end) out.push({ t: text.slice(pos - from), masked: false });
  return out;
}
