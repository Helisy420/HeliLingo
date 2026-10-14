// Which comment or string is under the cursor, by the language's comment
// syntax (VS Code doesn't tell extensions about tokens). Line comments
// that follow each other are read as one paragraph; block comments and
// docstrings as a whole; strings only when they look like prose.

import * as vscode from 'vscode';

const SLASHES = ['javascript', 'typescript', 'javascriptreact', 'typescriptreact', 'java', 'c', 'cpp', 'csharp', 'go', 'rust', 'swift', 'kotlin', 'scala', 'dart', 'php', 'jsonc', 'less', 'scss', 'groovy', 'objective-c', 'objective-cpp', 'zig', 'vue', 'svelte', 'glsl', 'hlsl', 'proto'];
const HASH = ['python', 'ruby', 'shellscript', 'perl', 'r', 'yaml', 'toml', 'dockerfile', 'makefile', 'powershell', 'elixir', 'julia', 'coffeescript', 'nim', 'crystal', 'cmake', 'nix', 'gdscript'];

function lineTokens(lang: string): string[] {
  if (SLASHES.includes(lang)) return ['//'];
  if (HASH.includes(lang)) return ['#'];
  if (['sql', 'lua', 'haskell', 'elm', 'ada'].includes(lang)) return ['--'];
  if (['latex', 'tex', 'erlang', 'matlab'].includes(lang)) return ['%'];
  if (['clojure', 'lisp', 'scheme', 'ini', 'asm'].includes(lang)) return [';'];
  if (lang === 'vb') return ["'"];
  if (lang === 'bat') return ['::', 'REM ', 'rem '];
  return ['//', '#'];
}

function blockTokens(lang: string): [string, string][] {
  const out: [string, string][] = [];
  if (SLASHES.includes(lang) || ['css', 'sql'].includes(lang)) out.push(['/*', '*/']);
  if (['html', 'xml', 'markdown', 'vue', 'svelte', 'php'].includes(lang)) out.push(['<!--', '-->']);
  if (lang === 'python') out.push(['"""', '"""'], ["'''", "'''"]);
  if (lang === 'lua') out.push(['--[[', ']]']);
  if (lang === 'haskell') out.push(['{-', '-}']);
  if (out.length === 0) out.push(['/*', '*/']);
  return out;
}

export type Found = { text: string; range: vscode.Range; kind: 'comment' | 'string' };

/** Where a line comment starts on `line` (outside strings), or -1. */
function commentStart(line: string, tokens: string[]): number {
  let quote: string | null = null;
  for (let i = 0; i < line.length; i++) {
    const c = line[i]!;
    if (quote) {
      if (c === '\\') i++;
      else if (c === quote) quote = null;
      continue;
    }
    if (c === '"' || c === '`' || (c === "'" && !tokens.includes("'"))) {
      quote = c;
      continue;
    }
    if (tokens.some((t) => line.startsWith(t, i))) return i;
  }
  return -1;
}

/** Comment text without its markers ("// ", " * ", "# "). */
function clean(lines: string[]): string {
  return lines
    .map((l) =>
      l
        .replace(/^\s*(\/\/+|#+|--+|;+|%+|'|::|rem\s|\/\*+|\*+\/?|<!--|-->|"""|'''|\{-|-\})/i, '')
        .replace(/(\*\/|-->|"""|'''|-\})\s*$/, '')
        .trim(),
    )
    .filter((l) => l.length > 0)
    .join(' ')
    .replace(/\s+/g, ' ')
    .trim();
}

function lineComment(doc: vscode.TextDocument, pos: vscode.Position): Found | null {
  const tokens = lineTokens(doc.languageId);
  const here = doc.lineAt(pos.line).text;
  const start = commentStart(here, tokens);
  if (start < 0 || pos.character < start) return null;
  const whole = (n: number) => {
    const t = doc.lineAt(n).text;
    const s = commentStart(t, tokens);
    return s >= 0 && t.slice(0, s).trim() === '' ? s : -1;
  };
  let first = pos.line;
  let last = pos.line;
  // A comment of its own line joins its neighbours; a trailing one stands alone.
  if (here.slice(0, start).trim() === '') {
    while (first > 0 && whole(first - 1) >= 0 && pos.line - first < 40) first--;
    while (last < doc.lineCount - 1 && whole(last + 1) >= 0 && last - pos.line < 40) last++;
  }
  const lines: string[] = [];
  for (let n = first; n <= last; n++) {
    const t = doc.lineAt(n).text;
    lines.push(t.slice(n === pos.line ? start : Math.max(0, commentStart(t, tokens))));
  }
  const range = new vscode.Range(first, n0(first === pos.line ? start : whole(first)), last, doc.lineAt(last).text.length);
  return { text: clean(lines), range, kind: 'comment' };
}

const n0 = (n: number) => Math.max(0, n);

function blockComment(doc: vscode.TextDocument, pos: vscode.Position): Found | null {
  const offset = doc.offsetAt(pos);
  const from = doc.offsetAt(new vscode.Position(Math.max(0, pos.line - 200), 0));
  const to = doc.offsetAt(new vscode.Position(Math.min(doc.lineCount - 1, pos.line + 200), 0)) + 2000;
  const text = doc.getText(new vscode.Range(doc.positionAt(from), doc.positionAt(to)));
  const at = offset - from;
  for (const [open, close] of blockTokens(doc.languageId)) {
    const o = text.lastIndexOf(open, at);
    if (o < 0) continue;
    const c = text.indexOf(close, o + open.length);
    // Same marker both ways (docstrings): the cursor must sit between a pair.
    if (c < 0 || c + close.length < at) continue;
    if (open === close) {
      const before = text.slice(0, o).split(open).length - 1;
      if (before % 2 === 1) continue;
    }
    const body = text.slice(o, c + close.length);
    return {
      text: clean(body.split(/\r?\n/)),
      range: new vscode.Range(doc.positionAt(from + o), doc.positionAt(from + c + close.length)),
      kind: 'comment',
    };
  }
  return null;
}

/** A string literal around the cursor that reads like text, not an id. */
function stringAt(doc: vscode.TextDocument, pos: vscode.Position): Found | null {
  const line = doc.lineAt(pos.line).text;
  let quote: string | null = null;
  let start = 0;
  for (let i = 0; i < line.length; i++) {
    const c = line[i]!;
    if (quote) {
      if (c === '\\') {
        i++;
        continue;
      }
      if (c === quote) {
        if (pos.character > start && pos.character <= i) {
          const body = line.slice(start + 1, i);
          const prose = /\p{L}{2,}/u.test(body) && (/\s/.test(body.trim()) || /[^\x00-\x7F]/.test(body));
          return prose ? { text: body.trim(), range: new vscode.Range(pos.line, start, pos.line, i + 1), kind: 'string' } : null;
        }
        quote = null;
      }
      continue;
    }
    if (c === '"' || c === "'" || c === '`') {
      quote = c;
      start = i;
    }
  }
  return null;
}

export function findText(doc: vscode.TextDocument, pos: vscode.Position, strings: boolean): Found | null {
  const found = lineComment(doc, pos) ?? blockComment(doc, pos) ?? (strings ? stringAt(doc, pos) : null);
  return found && /\p{L}{2,}/u.test(found.text) ? found : null;
}

/** The comment marker that starts `line` with its indent ("    // "), or
    "" when the line isn't a line comment — «Вставить ниже» keeps it. */
export function commentPrefix(doc: vscode.TextDocument, line: number): string {
  const text = doc.lineAt(line).text;
  const start = commentStart(text, lineTokens(doc.languageId));
  if (start < 0 || text.slice(0, start).trim() !== '') {
    // A JSDoc / block comment line: " * ".
    const m = /^(\s*\*\s?)(?!\/)/.exec(text);
    return m ? m[1]! : '';
  }
  const m = /^(\s*)(\/\/+|#+|--+|;+|%+|'|::|rem)\s?/i.exec(text);
  return m ? `${m[1]}${m[2]} ` : '';
}
