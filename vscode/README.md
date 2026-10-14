# HeliLingo for VS Code

Translation in the editor through the **HeliLingo** app running on this PC — the
same engines (Google, Yandex, Bing, DeepL, OpenRouter, offline models), glossary
and cache.

- **Перевести выделенное / Translate selection** — `Ctrl+Alt+H`: the translation in
  a hover next to the text, with *Заменить / Replace*, *Копировать / Copy* and
  *Вставить ниже / Insert below*. Without a selection — the comment or string under
  the cursor.
- **Заменить выделенное переводом / Replace selection** — `Ctrl+Alt+Shift+H`, for
  every selection at once. Write a comment in Russian, get it in English.
- **Hover** a comment or a prose string to read its translation (turn off with
  *HeliLingo: Toggle translation of comments on hover*).
- The status bar shows whether HeliLingo is reachable.

Text already in the target language goes to HeliLingo's second language
(«Если текст уже на русском → английский»), so Russian comments come out in
English and English ones in Russian with no setting at all.

## Needs

HeliLingo 2.5 running, with **Settings → Контекст и режимы → Расширение VS Code**
on (it is by default). The app listens on 127.0.0.1 only, on a random port, and
asks for a secret key it writes to `%LOCALAPPDATA%\HeliLingo\api.json`.

## Settings

- `helilingo.targetLanguage` — `en`, `ru`, … (empty: as in HeliLingo).
- `helilingo.hover.enabled`, `helilingo.hover.strings`.
