// Sentence splitting for the translator's sentence pairs — the same rules
// as hl_core::text::segments (lines, then sentence punctuation before a
// space, not after abbreviations or initials).

const ABBREVIATIONS = new Set([
  'mr', 'mrs', 'ms', 'dr', 'prof', 'st', 'vs', 'etc', 'e.g', 'i.e', 'approx', 'no', 'fig', 'vol', 'jr', 'sr',
  'inc', 'ltd', 'co', 'т', 'е', 'д', 'т.е', 'т.д', 'т.п', 'г', 'гг', 'ул', 'др', 'пр', 'см', 'стр', 'рис',
  'им', 'св', 'тыс', 'млн', 'млрд', 'руб', 'коп', 'z.b', 'bzw', 'usw',
]);

/** Pieces of `text` that concatenate back to it exactly. Each piece is a
    sentence with the whitespace after it. */
export function sentences(text: string): string[] {
  const out: string[] = [];
  let start = 0;
  const chars = [...text];
  let pos = 0; // in UTF-16 units
  const at: number[] = [];
  for (const c of chars) {
    at.push(pos);
    pos += c.length;
  }
  at.push(pos);
  for (let i = 0; i < chars.length; i++) {
    const c = chars[i]!;
    let end = -1;
    if (c === '\n') {
      end = i + 1;
    } else if ('.!?…。！？'.includes(c)) {
      let j = i + 1;
      while (j < chars.length && '.!?…"\'»”’)]'.includes(chars[j]!)) j++;
      const next = chars[j];
      const cjk = '。！？'.includes(c);
      if (cjk || next === undefined || /\s/.test(next)) {
        if (c === '.' && next !== undefined && isAbbreviation(text.slice(at[start], at[i]))) continue;
        end = j;
      }
    }
    if (end >= 0) {
      while (end < chars.length && /[ \t]/.test(chars[end]!)) end++;
      if (end < chars.length && chars[end] === '\n' && c !== '\n') end++;
      out.push(text.slice(at[start], at[end]));
      start = end;
      i = end - 1;
    }
  }
  if (start < chars.length) out.push(text.slice(at[start]));
  return out;
}

function isAbbreviation(before: string): boolean {
  const m = /([\p{L}.]+)$/u.exec(before);
  if (!m) return false;
  const word = m[1]!;
  if ([...word].length === 1) return true;
  return ABBREVIATIONS.has(word.toLowerCase());
}
