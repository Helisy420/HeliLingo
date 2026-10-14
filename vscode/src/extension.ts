// HeliLingo for VS Code: translation through the HeliLingo app running on
// this PC (its engines, offline models, glossary and cache).
//
// - «Перевести выделенное» (Ctrl+Alt+H): by `helilingo.selection.action` —
//   a hover with Заменить / Копировать / Вставить ниже (default), replace,
//   insert below, copy or a notification. Without a selection: the comment
//   or string under the cursor.
// - «Заменить выделенное переводом» (Ctrl+Alt+Shift+H), «Вставить перевод
//   ниже», «Копировать перевод»: each a command of its own, so any of them
//   can get its own shortcut.
// - Hovering a comment or a prose string shows its translation.
// - «Меню» (Ctrl+Alt+J, or a click on the status bar item): the actions,
//   switches, language and translator pickers, settings and shortcuts.

import * as vscode from 'vscode';
import { engines, NotRunning, status, translate, type Translation } from './api';
import { commentPrefix, findText } from './comments';

const ru = vscode.env.language.toLowerCase().startsWith('ru');
const T = ru
  ? {
      notRunning: 'HeliLingo не запущен или выключено «Расширение VS Code» (Настройки → Контекст и режимы).',
      failed: (e: string) => `HeliLingo не смог перевести: ${e}`,
      nothing: 'Выделите текст или поставьте курсор в комментарий.',
      replace: 'Заменить',
      copy: 'Копировать',
      insert: 'Вставить ниже',
      copied: 'Перевод скопирован',
      connected: (v: string, t: string) => `HeliLingo ${v} подключён · перевод на ${t}`,
      tipOff: 'HeliLingo не запущен — щёлкните для меню',
      on: 'вкл.',
      off: 'выкл.',
      translating: 'HeliLingo переводит…',
      menu: 'HeliLingo',
      actions: 'Действия',
      switches: 'Настройки',
      translateSel: 'Перевести выделенное',
      replaceSel: 'Заменить выделенное переводом',
      insertSel: 'Вставить перевод ниже',
      copySel: 'Копировать перевод',
      hover: 'Перевод комментариев при наведении',
      strings: 'Переводить и строки',
      target: 'Язык перевода',
      engine: 'Переводчик',
      action: 'Что делает Ctrl+Alt+H',
      settings: 'Все настройки расширения…',
      keys: 'Сочетания клавиш…',
      check: 'Проверить подключение',
      asApp: 'Как в HeliLingo',
      pickTarget: 'На какой язык переводить',
      pickEngine: 'Какой переводчик пробовать первым',
      pickAction: 'Что делать с переводом выделенного',
      actionNames: {
        hover: 'Подсказка с кнопками',
        replace: 'Сразу заменить',
        insertBelow: 'Вставить ниже',
        copy: 'Скопировать',
        notification: 'Уведомление',
      } as Record<string, string>,
      offline: 'офлайн',
    }
  : {
      notRunning: 'HeliLingo isn’t running, or «VS Code extension» is off (Settings → Context & modes).',
      failed: (e: string) => `HeliLingo couldn’t translate: ${e}`,
      nothing: 'Select some text or put the cursor in a comment.',
      replace: 'Replace',
      copy: 'Copy',
      insert: 'Insert below',
      copied: 'Translation copied',
      connected: (v: string, t: string) => `HeliLingo ${v} connected · translating to ${t}`,
      tipOff: 'HeliLingo isn’t running — click for the menu',
      on: 'on',
      off: 'off',
      translating: 'HeliLingo is translating…',
      menu: 'HeliLingo',
      actions: 'Actions',
      switches: 'Settings',
      translateSel: 'Translate selection',
      replaceSel: 'Replace selection with translation',
      insertSel: 'Insert translation below',
      copySel: 'Copy translation',
      hover: 'Translate comments on hover',
      strings: 'Translate strings too',
      target: 'Target language',
      engine: 'Translator',
      action: 'What Ctrl+Alt+H does',
      settings: 'All extension settings…',
      keys: 'Keyboard shortcuts…',
      check: 'Check the connection',
      asApp: 'As in HeliLingo',
      pickTarget: 'Language to translate into',
      pickEngine: 'Translator to try first',
      pickAction: 'What to do with a selection’s translation',
      actionNames: {
        hover: 'Hover with buttons',
        replace: 'Replace right away',
        insertBelow: 'Insert below',
        copy: 'Copy',
        notification: 'Notification',
      } as Record<string, string>,
      offline: 'offline',
    };

const LANGS: [string, string][] = [
  ['ru', 'Русский'],
  ['en', 'English'],
  ['uk', 'Українська'],
  ['de', 'Deutsch'],
  ['fr', 'Français'],
  ['es', 'Español'],
  ['it', 'Italiano'],
  ['pt', 'Português'],
  ['pl', 'Polski'],
  ['tr', 'Türkçe'],
  ['ja', '日本語'],
  ['zh-CN', '中文'],
  ['ko', '한국어'],
];

type Action = 'hover' | 'replace' | 'insertBelow' | 'copy' | 'notification';

const cfg = () => vscode.workspace.getConfiguration('helilingo');
const opt = {
  target: () => cfg().get<string>('targetLanguage', '').trim(),
  source: () => cfg().get<string>('sourceLanguage', 'auto').trim(),
  engine: () => cfg().get<string>('engine', '').trim(),
  action: () => cfg().get<Action>('selection.action', 'hover'),
  asComment: () => cfg().get<boolean>('insertBelow.asComment', true),
  hover: () => cfg().get<boolean>('hover.enabled', true),
  strings: () => cfg().get<boolean>('hover.strings', true),
  minLetters: () => cfg().get<number>('hover.minLetters', 3),
  hoverLangs: () => cfg().get<string[]>('hover.languages', []),
  details: () => cfg().get<boolean>('hover.showDetails', true),
  statusBar: () => cfg().get<boolean>('statusBar', true),
  errors: () => cfg().get<boolean>('showErrors', true),
};
const set = (key: string, value: unknown) => cfg().update(key, value, vscode.ConfigurationTarget.Global);

/** Translations by text and options (hovers ask again and again). */
const cache = new Map<string, Translation>();
async function cached(text: string): Promise<Translation> {
  const o = { target: opt.target(), source: opt.source(), engine: opt.engine() };
  const key = `${o.target}\u0000${o.source}\u0000${o.engine}\u0000${text}`;
  const hit = cache.get(key);
  if (hit) return hit;
  const r = await translate(text, o);
  if (cache.size > 500) cache.delete(cache.keys().next().value!);
  cache.set(key, r);
  return r;
}

function complain(e: unknown) {
  if (!opt.errors()) return;
  if (e instanceof NotRunning) void vscode.window.showWarningMessage(T.notRunning);
  else void vscode.window.showErrorMessage(T.failed(e instanceof Error ? e.message : String(e)));
}

const code = (c: string | null | undefined) => (c ?? '?').split('-')[0]!.toUpperCase();
const letters = (s: string) => (s.match(/\p{L}/gu) ?? []).length;

/** The hover's body: the translation, a footer, and the action links. */
function card(r: Translation, uri: vscode.Uri, range: vscode.Range): vscode.MarkdownString {
  const args = (cmd: string) =>
    `command:${cmd}?${encodeURIComponent(JSON.stringify([{ uri: uri.toString(), range: [range.start.line, range.start.character, range.end.line, range.end.character], text: r.text }]))}`;
  const md = new vscode.MarkdownString(undefined, true);
  md.isTrusted = { enabledCommands: ['helilingo.apply', 'helilingo.copy', 'helilingo.insertBelow'] };
  md.appendText(r.text);
  const details = opt.details() ? `$(globe) ${code(r.sourceLang)} → ${code(r.target)} · ${r.engine} &nbsp;&nbsp; ` : '';
  md.appendMarkdown(
    `\n\n---\n\n${details}[${T.replace}](${args('helilingo.apply')}) · [${T.copy}](${args('helilingo.copy')}) · [${T.insert}](${args('helilingo.insertBelow')})`,
  );
  return md;
}

type Args = { uri: string; range: [number, number, number, number]; text: string };
const toRange = (a: Args) => new vscode.Range(a.range[0], a.range[1], a.range[2], a.range[3]);

/** The selection's translation waiting to be shown by `showHover`. */
let pending: { uri: string; range: vscode.Range; result: Translation } | null = null;

/** The selection (or the comment / string under the cursor) and its text. */
function target(ed: vscode.TextEditor): { range: vscode.Range; text: string } | null {
  const sel = ed.selection;
  const text = ed.document.getText(sel);
  if (text.trim()) return { range: sel, text };
  const found = findText(ed.document, sel.active, true);
  return found ? { range: found.range, text: found.text } : null;
}

const busy = <R>(f: () => Thenable<R>) =>
  vscode.window.withProgress({ location: vscode.ProgressLocation.Window, title: T.translating }, f);

async function insertBelow(doc: vscode.TextDocument, range: vscode.Range, text: string) {
  const end = range.end;
  const line = doc.lineAt(end.line);
  const prefix = opt.asComment() ? commentPrefix(doc, end.line) : '';
  const indent = prefix || line.text.slice(0, line.firstNonWhitespaceCharacterIndex);
  const edit = new vscode.WorkspaceEdit();
  edit.insert(doc.uri, line.range.end, `\n${indent}${text}`);
  await vscode.workspace.applyEdit(edit);
}

async function run(action: Action) {
  const ed = vscode.window.activeTextEditor;
  if (!ed) return;
  if (action === 'replace') {
    const sels = ed.selections.filter((s) => !s.isEmpty);
    if (!sels.length) return void vscode.window.showInformationMessage(T.nothing);
    try {
      const results = await busy(() => Promise.all(sels.map((s) => cached(ed.document.getText(s)))));
      await ed.edit((b) => sels.forEach((s, i) => b.replace(s, results[i]!.text)));
    } catch (e) {
      complain(e);
    }
    return;
  }
  const t = target(ed);
  if (!t) return void vscode.window.showInformationMessage(T.nothing);
  try {
    const result = await busy(() => cached(t.text));
    if (action === 'hover') {
      pending = { uri: ed.document.uri.toString(), range: t.range, result };
      await vscode.commands.executeCommand('editor.action.showHover');
    } else if (action === 'insertBelow') {
      await insertBelow(ed.document, t.range, result.text);
    } else if (action === 'copy') {
      await vscode.env.clipboard.writeText(result.text);
      void vscode.window.setStatusBarMessage(T.copied, 2000);
    } else {
      const pick = await vscode.window.showInformationMessage(result.text, T.replace, T.copy, T.insert);
      if (pick === T.replace) await ed.edit((b) => b.replace(t.range, result.text));
      if (pick === T.copy) await vscode.env.clipboard.writeText(result.text);
      if (pick === T.insert) await insertBelow(ed.document, t.range, result.text);
    }
  } catch (e) {
    complain(e);
  }
}

async function pickTarget() {
  const now = opt.target();
  const items = [{ label: T.asApp, code: '' }, ...LANGS.map(([c, n]) => ({ label: n, description: c, code: c }))].map((i) => ({
    ...i,
    picked: i.code === now,
    label: i.code === now ? `$(check) ${i.label}` : i.label,
  }));
  const pick = await vscode.window.showQuickPick(items, { title: T.pickTarget });
  if (pick) await set('targetLanguage', pick.code);
}

async function pickEngine() {
  let list: { id: string; name: string; group: string }[] = [];
  try {
    list = await engines();
  } catch (e) {
    return complain(e);
  }
  const now = opt.engine();
  const items = [
    { label: T.asApp, id: '', description: '' },
    ...list.map((e) => ({ label: e.name, id: e.id, description: e.group === 'offline' ? T.offline : '' })),
  ].map((i) => ({ ...i, label: i.id === now ? `$(check) ${i.label}` : i.label }));
  const pick = await vscode.window.showQuickPick(items, { title: T.pickEngine });
  if (pick) await set('engine', pick.id);
}

async function pickAction() {
  const now = opt.action();
  const items = (['hover', 'replace', 'insertBelow', 'copy', 'notification'] as Action[]).map((a) => ({
    label: a === now ? `$(check) ${T.actionNames[a]}` : T.actionNames[a]!,
    value: a,
  }));
  const pick = await vscode.window.showQuickPick(items, { title: T.pickAction });
  if (pick) await set('selection.action', pick.value);
}

async function menu() {
  const langName = (c: string) => (c ? (LANGS.find(([k]) => k === c)?.[1] ?? c) : T.asApp);
  type Item = vscode.QuickPickItem & { run?: () => unknown };
  const sep = (label: string): Item => ({ label, kind: vscode.QuickPickItemKind.Separator });
  const items: Item[] = [
    sep(T.actions),
    { label: `$(comment-discussion) ${T.translateSel}`, description: 'Ctrl+Alt+H', run: () => run(opt.action()) },
    { label: `$(replace) ${T.replaceSel}`, description: 'Ctrl+Alt+Shift+H', run: () => run('replace') },
    { label: `$(insert) ${T.insertSel}`, run: () => run('insertBelow') },
    { label: `$(copy) ${T.copySel}`, run: () => run('copy') },
    sep(T.switches),
    { label: `$(globe) ${T.target}`, description: langName(opt.target()), run: pickTarget },
    { label: `$(server-process) ${T.engine}`, description: opt.engine() || T.asApp, run: pickEngine },
    { label: `$(play) ${T.action}`, description: T.actionNames[opt.action()], run: pickAction },
    { label: `$(eye) ${T.hover}`, description: opt.hover() ? T.on : T.off, run: () => set('hover.enabled', !opt.hover()) },
    { label: `$(quote) ${T.strings}`, description: opt.strings() ? T.on : T.off, run: () => set('hover.strings', !opt.strings()) },
    { label: `$(settings-gear) ${T.settings}`, run: () => vscode.commands.executeCommand('helilingo.openSettings') },
    { label: `$(keyboard) ${T.keys}`, run: () => vscode.commands.executeCommand('helilingo.openKeybindings') },
    { label: `$(plug) ${T.check}`, run: () => vscode.commands.executeCommand('helilingo.status') },
  ];
  const pick = await vscode.window.showQuickPick(items, { title: T.menu });
  await pick?.run?.();
}

export function activate(context: vscode.ExtensionContext) {
  const bar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 90);
  bar.command = 'helilingo.menu';
  context.subscriptions.push(bar);

  const refresh = async () => {
    if (!opt.statusBar()) return bar.hide();
    try {
      const s = await status();
      bar.text = '$(globe) HeliLingo';
      bar.tooltip = T.connected(s.version, opt.target() || s.target);
    } catch {
      bar.text = '$(debug-disconnect) HeliLingo';
      bar.tooltip = T.tipOff;
    }
    bar.show();
  };
  void refresh();
  const timer = setInterval(refresh, 30_000);
  context.subscriptions.push({ dispose: () => clearInterval(timer) });

  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration('helilingo')) void refresh();
    }),

    vscode.commands.registerCommand('helilingo.status', async () => {
      try {
        const s = await status();
        void vscode.window.showInformationMessage(T.connected(s.version, opt.target() || s.target));
      } catch (e) {
        if (e instanceof NotRunning) void vscode.window.showWarningMessage(T.notRunning);
        else complain(e);
      }
      void refresh();
    }),
    vscode.commands.registerCommand('helilingo.translateSelection', () => run(opt.action())),
    vscode.commands.registerCommand('helilingo.replaceSelection', () => run('replace')),
    vscode.commands.registerCommand('helilingo.insertBelowSelection', () => run('insertBelow')),
    vscode.commands.registerCommand('helilingo.copySelection', () => run('copy')),
    vscode.commands.registerCommand('helilingo.menu', menu),
    vscode.commands.registerCommand('helilingo.pickTarget', pickTarget),
    vscode.commands.registerCommand('helilingo.pickEngine', pickEngine),
    vscode.commands.registerCommand('helilingo.toggleHover', () => set('hover.enabled', !opt.hover())),
    vscode.commands.registerCommand('helilingo.openSettings', () =>
      vscode.commands.executeCommand('workbench.action.openSettings', `@ext:${context.extension.id}`),
    ),
    vscode.commands.registerCommand('helilingo.openKeybindings', () =>
      vscode.commands.executeCommand('workbench.action.openGlobalKeybindings', 'helilingo'),
    ),

    // Hover links.
    vscode.commands.registerCommand('helilingo.apply', async (a: Args) => {
      const edit = new vscode.WorkspaceEdit();
      edit.replace(vscode.Uri.parse(a.uri), toRange(a), a.text);
      await vscode.workspace.applyEdit(edit);
    }),
    vscode.commands.registerCommand('helilingo.copy', async (a: Args) => {
      await vscode.env.clipboard.writeText(a.text);
      void vscode.window.setStatusBarMessage(T.copied, 2000);
    }),
    vscode.commands.registerCommand('helilingo.insertBelow', async (a: Args) => {
      const doc = await vscode.workspace.openTextDocument(vscode.Uri.parse(a.uri));
      await insertBelow(doc, toRange(a), a.text);
    }),

    vscode.window.onDidChangeTextEditorSelection(() => (pending = null)),

    vscode.languages.registerHoverProvider({ scheme: '*' }, {
      async provideHover(doc, pos, token) {
        if (pending && pending.uri === doc.uri.toString() && pending.range.contains(pos)) {
          return new vscode.Hover(card(pending.result, doc.uri, pending.range), pending.range);
        }
        if (!opt.hover()) return undefined;
        const only = opt.hoverLangs();
        if (only.length && !only.includes(doc.languageId)) return undefined;
        const found = findText(doc, pos, opt.strings());
        if (!found || letters(found.text) < opt.minLetters()) return undefined;
        try {
          const r = await cached(found.text);
          if (token.isCancellationRequested) return undefined;
          // Nothing to tell: the comment came back the same.
          if (r.text.trim() === found.text.trim()) return undefined;
          return new vscode.Hover(card(r, doc.uri, found.range), found.range);
        } catch {
          return undefined;
        }
      },
    }),
  );
}

export function deactivate() {}
