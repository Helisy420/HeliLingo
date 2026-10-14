# TODO — HeliLingo v2.5

Rules and architecture: [AGENTS.md](AGENTS.md). Design: [docs/design.md](docs/design.md),
section 4 screenshots (kept outside the public repo).
Tick items as they land. **❓** = needs the user's answer first. **[~]** = partly done.
**⏸** = waiting for the user's approval — don't start.

Status 2026-10-14: every cloud provider works (Google, Yandex, Bing keyless; DeepL and
OpenRouter by key); offline models download, verify and translate (OPUS-MT tested end to
end; the llama-server path is built but not yet run for real); screen-area capture, UI
Automation selection, Wikipedia card and network status are in. Next: section 4 of Figma —
bubbles, Quick translate variants, «Баблы и Вики» settings.

## 0. Decisions

- [x] Engines: Google, Yandex, DeepL, Bing + OpenRouter + offline.
- [x] Hybrid routing: hedged chain, first text wins (AGENTS.md).
- [x] Offline: OPUS-MT, Argos, NLLB 600M / 1.3B (ct2rs), TranslateGemma 4B / 12B and
  Granite Vision (llama-server). Opt-in; a model downloads only on «Скачать».
- [x] No installer: one `helilingo.exe`. No data migration from older versions.
- [x] Frontend: Svelte 5 + Vite. Font: Golos Text. Three themes from Figma.
- [x] Interface languages: RU and EN only.
- [x] OpenRouter: model picker, no default model.
- [x] Updates from GitHub `Helisy420/HeliLingo` behind a flag (repo not ready).
- [x] lingua with 10 languages + script rules; local detection only picks the target.
- [x] Shortcuts follow Figma: Quick = Ctrl+Shift+L, translator = Ctrl+Alt+T.
- [x] No older HeliLingo version is a reference any more (2026-10-14).
- [x] IBM Granite Vision 4.1 as an optional offline screen reader (English-trained).
- [ ] ❓ App icon (navy square + periwinkle sun) is my own — needs the user's OK.

## 1. Design

- [x] Figma screens, node ids, tokens for 3 themes → `docs/design.md`.
- [x] Icons as SVG → `ui/src/assets/icons/`; logo → app icon.
- [x] Section 4 («Баблы, быстрый перевод, файлы, изображения») read; screenshots in
  kept outside the public repo.

## 2. Scaffold — done

- [x] Cargo workspace (`hl-core`, `hl-cache`, `hl-detect`, `hl-pipeline`, `hl-providers`,
  `hl-offline`, `hl-win`) + `src-tauri` on Tauri 2.12; `ui/` Svelte 5 + Vite + TS strict.
- [x] Windows, tray, single-instance, autostart (release only), logging, settings with
  live `settings` event, keys in Credential Manager, `cargo tauri build --no-bundle`.

## 3. Core pipeline — done

- [x] Types + events, request manager, hedged router, text/code splitting, lingua,
  moka + redb cache, history, stats, prompts, CLI.
- [x] Dictionary groups by part of speech travel with `Done` and the cache.
- [x] Translate with context: last N fragments per window → DeepL `context`,
  OpenRouter / Gemma prompts (cache not read while context is on).
- [x] Glossary file re-read on change, used by OpenRouter and Gemma.
- [x] Google endpoint cool-downs saved across restarts.

## 4. Providers — done

- [x] Google: keyless endpoints + Cloud key; mode Auto / Web / API applied.
- [x] Yandex: keyless + dictionary (parallel, with the local language guess); Cloud key
  + folder id.
- [x] Bing: keyless bing.com session (renewed on expiry); Azure key + region.
- [x] DeepL: key, Free vs Pro host, context.
- [x] OpenRouter: SSE streaming, mid-stream errors, `[DONE]`, usage counted, model
  picker (live list, search, price per 1M tokens, cached a day).
- [x] Wikipedia summaries (REST API, target wiki first, cached for the session).
- [x] OpenRouter token usage → Statistics page: tokens sent / received per day, shown for
  the chosen period (2026-10-14).

## 5. Offline — mostly done

- [x] Catalog pinned to HF commits + SHA-256 (all links re-checked 2026-10-14).
- [x] Resumable downloads, progress, cancel, delete, free-space check, markers.
- [x] ct2 worker process (`--offline-worker`), OPUS-MT / Argos (pivot) / NLLB.
  OPUS-MT tested end to end (download 157 MB, en→ru 0.7 s warm).
- [x] llama-server: release for the PC (CUDA / Vulkan / CPU), supervisor, Job object,
  streamed TranslateGemma, 15-min idle stop, Granite with `--mmproj`.
- [x] GPU + VRAM (DXGI), layers fitted to free VRAM; «Офлайн готов» from the real state.
- [x] Settings page: download / progress / cancel / use / delete, screen OCR choice.
- [x] Per-sentence deltas from ct2 workers: the first sentence alone, then batches of 4,
  each streamed to the window (tested with OPUS-MT).
- [x] Model folder: shown with free space, change (Windows folder picker), back to default,
  open in Explorer.

## 6. Windows integration

**[~] = built and in the code, compiles, unit-tested, but never run end to end on a real
desktop** (checked 2026-10-14: every item below exists). An agent can't test them: that
needs real key presses on the user's desktop (AGENTS.md rule 9). The user's own tests so
far found the chord-shortcut freeze and the Quick sliver (both fixed below).
Needs testing on a real desktop:
- [x] Hook thread + watchdog: double Ctrl, double Ctrl+C and Mouse4 **confirmed working by
  the user on their desktop (2026-10-15)**. Still to try: Mouse5, popup Esc / Ctrl+C /
  click-away.
- [x] Shortcuts can be removed with a × next to «Изменить» (Backspace while changing
  still works).
- [~] Selection: UI Automation first, clipboard fallback (restored).
- [~] Translate-and-copy / translate-and-paste; Ultra from the selection.
- [~] «Скриншот области» overlay (Ctrl+Shift+O) → translator Images tab.
  pin + drag (the hook follows the moved card), pinned place remembered across restarts.
- [x] Chords, fullscreen / ignore-list suppression, Replace, OCR, speech, network status,
  Wikipedia card in the popup, Esc / click-away obey «Закрывать по Esc и клику вне бабла».
- [x] Code review of the hook, placement and click-through after §7 (2026-10-14): lock
  order safe (one thread), Esc passes through when closing is off, click-through matches
  tao's own flags.
- [x] Freeze fixed (2026-10-14, user's test build): chord shortcuts (Ctrl+Alt+T, Ctrl+Shift+S,
  Ctrl+Shift+O) ran on the main thread and created the webview there — a deadlock on
  Windows. Actions now run off the main thread; focusing a window also focuses its webview
  so Quick gets the keys. Quick itself is always shown from the main thread. **Re-test on
  the real desktop.**
- [x] Quick cut down to a sliver after switching Окно ↔ Строка: the window was sized from
  the old, detached card (0 × 0). `fitWindow` now measures the card shown now (2026-10-14).
- [x] Mouse shortcuts with modifiers or pressed twice: Ctrl+Alt+Mouse5, Mouse4, Mouse4×2
  can be recorded on «Горячие клавиши» (the hook already matched them).

## 7. Section 4 — bubbles and Quick translate (Figma 25:167) — built, screenshot-checked

Decided 2026-10-14: bubbles are **styles**. Settings shows every style as a live
preview (the current full card included); the user picks which to use. No icon next to
the selection (the Figma row was an accident). Quick layouts are chosen in a
«Быстрый перевод» group on the same page, also with previews.

- [x] «Только перевод» (34:199) and «С кнопками» (35:167): speak, copy, expand to the card.
- [x] Long translation: three lines + «Развернуть» (34:222), scrolling + «Свернуть»
  (34:243); hover highlights a sentence, a click copies it.
- [x] Word + Wikipedia: compact (34:266) and expanded with picture and «CC BY-SA» (34:301);
  short or detailed summary, the word's / translation's / interface language first.
- [x] Dictionary: meanings with glosses (Google entries / Yandex meanings), transcription,
  examples (Google) translated after the bubble shows, «Все значения (N)» (51:231).
- [x] Pin, drag, remember the pinned place; copy "winks" the bubble (no tick).
- [x] Auto-close (paused while hovered), appear animation, text size, background opacity.

Quick translate variants:
- [x] Mini-buttons with tooltips (36:167).
- [x] Without suggestions (43:172).
- [x] Suggestions + history list (43:235).
- [x] History only when the field is empty: search, filters Все / Слова / Фразы / Избранное,
  ↑↓, Enter pastes, Tab (or Enter on nothing found) translates the query (43:354).
- [x] Line mode like a launcher: empty / translation / long text (47:180, 47:191, 47:216).

Settings → «Баблы и Вики» (36:243, 53:240), with live previews (sample switch Фраза /
Длинный текст / Слово, a faint "desktop" behind so opacity reads):
- [x] Вид бабла (Только перевод / С кнопками / Карточка), сворачивать длинный перевод,
  закрывать по Esc и клику вне бабла.
- [x] Вики-справка: on/off, язык справки, объём справки, картинка из статьи.
- [x] Появление: как открывать бабл (writes the double-Ctrl / double-Ctrl+C shortcuts), где
  показывать, задержка появления (default 0 s for speed; Figma shows 0.3 s).
- [x] Для одиночных слов: Словарь / Вики / Оба, with a preview.
- [x] Автозакрытие: закрывать через, не закрывать при наведении, запоминать место.
- [x] Оформление: размер текста, прозрачность фона, анимация появления, with a preview.
- [x] Быстрый перевод: вид окна (Окно / Строка), другие варианты, история, мини-кнопки,
  with a drawn preview of the window.

Other UI:
- [x] Popup word mode: dictionary groups by part of speech.
- [x] Compact bubbles (Только перевод / С кнопками): switches «Части речи» and «Другие
  переводы слова» under «Что показывать для слов» (on by default; off = just the
  translation).
- [x] Settings: search field over the navigation (Ctrl+F; jumps to the row and flashes
  it); the open page's groups are listed under it in the navigation (three or more).
- [x] «Для одиночных слов» → «Показывать: Словарь / Вики / Оба / Ничего» (Ничего = just
  the translation).
- [x] Duplicate settings removed: «Как открывать бабл» (Баблы и Вики) wrote the same
  shortcuts as «Горячие клавиши»; shortcuts now live only there.
- [x] Hard-coded Russian moved to i18n (offline notes, glossary count, «мс», window titles).
- [x] Screenshot pass in all 3 themes (dark full pass; light + blue for changed pages).

## 8. Later (Figma section 4, left for later by the user)

- [x] Files: compact translator «Перевод файлов» — empty / drag-over / queue with progress,
  result format, keep formatting, bilingual document (46:178, 46:252, 46:313). Done
  2026-10-14, locally: `hl-docs` reads DOCX / PPTX / XLSX (shared and inline strings) /
  TXT / SRT and rebuilds them (only the text parts change; a paragraph's translation goes
  into its first run); PDF pages are rendered by Windows.Data.Pdf and read by Windows OCR
  (result: DOCX or TXT). Units are translated in batches by the user's engine chain
  (lines kept; a batch that comes back merged is redone line by line). «Скачать» = save
  dialog, «Скачать всё» = into «Папка для результатов». Checked with python-docx /
  python-pptx / openpyxl files and an Edge-printed PDF; keyless Google kept 7/7 lines.
  Limits: a paragraph mixing bold and plain runs gets the first run's look; OCR can miss
  words on table lines in PDFs.
  ❓ DeepL's document API (perfect layout, PDF in and out) bills at least 50 000 characters
  per document — not used until the user decides.
- [x] Images: ways to add — paste (banner when the clipboard has a picture), area, window,
  file, drag and drop (48:180).
- [x] Area overlay from Figma: handles, size label, mode bar Область / Окно / Весь экран,
  Esc / Enter (48:297). «Окно» highlights the window under the pointer (DWM frame bounds).
  «Перевод окна» shortcut: on «Горячие клавиши», empty by default.
- [x] Area results over the real screen: «Поверх» (blocks fit their text, opacity slider,
  Alt shows the original), «Плашка» (one card, speak / copy), «Рядом» (numbered markers +
  list, copy each / all) (49:192, 49:244, 49:300). The chosen view is remembered.
  **Needs a real-desktop test** like §6.
- [x] «Окно» found no windows on the first capture after a start: the window list was
  sent while the overlay page was still loading. The page now asks for it
  (`capture_state`; checked: 4 windows on a cold start). The overlay is also re-sized after
  moving to a monitor with another scale.
- [x] Pinned live area (2026-10-15): 📌 keeps the area translating — it's looked at ~3× a
  second and read again when its picture changes and settles (games, JRPG dialogue). The
  overlay is left out of screen captures, lets clicks through except on its controls,
  and gives focus back to the game. Exclusive-fullscreen games can't show overlays —
  borderless / windowed mode needed.
- [x] «Субтитры» view: the translation as a subtitle line at the bottom of the screen
  (size adjustable); the area frame can be hidden (👁).
- [x] Area controls: the bar, the card, the list (also resizable) and the subtitles
  (also wider / narrower) can be dragged; ⚙ picks the text language, the target, the
  translator (offline models included) and the recogniser (Windows OCR / Granite Vision);
  ⟳ reads the area again.
- [x] `helilingo.exe --capture [window]` starts the overlay (game launchers, stream decks).

- [x] Settings → «Изображения и файлы» (53:562): Область экрана (режим, прозрачность
  подложки, оригинал по Alt) and Файлы (формат, форматирование, двуязычный, папка). The
  frame's «Выбрать область» / «Выбрать окно» rows are left out: shortcuts live only on
  «Горячие клавиши», and a window-capture shortcut waits for the window mode (48:297).
  The settings are saved; the features that use them are the items above.

## 9. Privacy

- [x] Number masking: a Settings switch «Скрывать номера» (Общие → Конфиденциальность).
  **Only the history is masked** (the History window and Quick's history list) — decided
  by the user 2026-10-14; bubbles, Quick's result, the translator and Ultra show text as
  is. Formats: passport `1111 111111`, phones `+7 111 111 11 11`, `+71111111111`.
  Masked as •••• on screen, revealed on hover (or while Alt is held); copy and Replace use
  the real number; text sent to translators and the stored history stay unchanged.

## 10. Ship

- [ ] Real-desktop test pass: double Ctrl in Notepad / browser / Word / terminal, Replace,
  tray, Mouse 4/5, fullscreen game, area capture, UIA in Chrome / Word / Terminal.
- [ ] Performance pass vs targets (hotkey → bubble ≤ 50 ms); idle memory.
- [ ] Self-updater `src-tauri/src/update.rs` (GitHub Releases + minisign), flag off.
- [ ] `docs/release.md`; README (EN + RU).
- [ ] When the user's repo is ready: embed the public key, flip the flag, first release.

## 11. VS Code extension (approved 2026-10-15)

- [x] Local API in the app (`src-tauri/src/api.rs`): 127.0.0.1, random port, random token in
  `%LOCALAPPDATA%\HeliLingo\api.json`; `GET /status`, `POST /translate`. On by default,
  switch: Settings → «Контекст и режимы» → «Расширение VS Code». Checked live: no token →
  401; EN → RU and RU → EN (second language) through the user's engine chain.
- [x] Extension `vscode/` → `vscode/helilingo.vsix`: «Перевести выделенное» (Ctrl+Alt+H, a
  hover with Заменить / Копировать / Вставить ниже; without a selection — the comment or
  string under the cursor), «Заменить выделенное переводом» (Ctrl+Alt+Shift+H, every
  selection), translation of comments and prose strings on hover, status bar. RU / EN by
  VS Code's language. Comment finding tested (`npm test`).
- [ ] Try it in VS Code (Extensions → … → Install from VSIX), then publish to the
  Marketplace when the user wants (needs their publisher account).

## ⏸ Waiting for approval — don't start

Nothing waiting.
