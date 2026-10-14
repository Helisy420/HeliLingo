import assert from 'node:assert/strict';
import { findText } from '../src/comments';
import { doc, Position } from './vscode-stub';

const at = (src: string, lang: string, line: number, ch: number, strings = true) =>
  findText(doc(src, lang) as never, new Position(line, ch) as never, strings)?.text ?? null;

// Consecutive line comments read as one paragraph.
const ts = 'const a = 1;\n// Считает сумму\n// двух чисел\nfunction add() {}';
assert.equal(at(ts, 'typescript', 1, 5), 'Считает сумму двух чисел');
// Code isn't a comment.
assert.equal(at(ts, 'typescript', 0, 3), null);
// A trailing comment stands alone; "//" inside a string isn't one.
assert.equal(at('let u = "http://x"; // адрес сервера', 'javascript', 0, 25), 'адрес сервера');
assert.equal(at('let u = "http://x";', 'javascript', 0, 12, false), null);
// Block comments and JSDoc.
const block = '/**\n * Возвращает пользователя\n * по идентификатору\n */\nfn()';
assert.equal(at(block, 'typescript', 1, 6), 'Возвращает пользователя по идентификатору');
assert.equal(at(block, 'typescript', 4, 1), null);
// Python: # comments and docstrings.
assert.equal(at('x = 1  # счётчик попыток', 'python', 0, 12), 'счётчик попыток');
assert.equal(at('def f():\n    """Load the config file."""\n    pass', 'python', 1, 10), 'Load the config file.');
// Strings: prose yes, identifiers no.
assert.equal(at('alert("Файл не найден")', 'javascript', 0, 10), 'Файл не найден');
assert.equal(at('obj["userId"]', 'javascript', 0, 7), null);
assert.equal(at('alert("Файл не найден")', 'javascript', 0, 10, false), null);
console.log('comments: ok');

// «Вставить ниже» keeps the comment marker.
import { commentPrefix } from '../src/comments';
const pre = (src: string, lang: string, line: number) => commentPrefix(doc(src, lang) as never, line);
assert.equal(pre('    // Считает сумму', 'typescript', 0), '    // ');
assert.equal(pre('# счётчик', 'python', 0), '# ');
assert.equal(pre(' * Возвращает', 'typescript', 0), ' * ');
assert.equal(pre('let x = 1;', 'typescript', 0), '');
console.log('prefix: ok');
