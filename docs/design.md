# Design — HeliLingo v2.5

Source: Figma `HeliLingo — редизайн`, key `6KVEWhFnxuyuMjFQQDYiOT`, page
`0:1 HeliLingo UI`. Read 2026-10-14. Screenshots are not kept in this repository.

Font: **Golos Text** (the Figma file already uses it). Weights 400 / 500 / 600.

## Themes

Three themes, same layout, chosen in Settings → General → Тема
(Белая / Тёмная / Синяя / Системная). "Системная" = white or dark from Windows.

| Section | Theme | Accent |
|---|---|---|
| `16:2237` 1 · Тёмно-синяя | navy (`blue`) | periwinkle `#8e96ff` |
| `16:508` 2 · Тёмная | dark | amber `#ffd04d` |
| `10:2` 3 · Белая | light | amber `#ffc933` |

## Tokens (Figma variables)

| Token | Navy | Dark | Light |
|---|---|---|---|
| `bg/surface` | `#0f1331` | `#17191d` | `#fafbfc` |
| `bg/sunken` | `#151a42` | `#1d2025` | `#f1f3f5` |
| `bg/field` | `#1a2050` | `#20242a` | `#ffffff` |
| `bg/disabled` | `#171c48` | `#22262c` | `#eef0f3` |
| `state/hover` | `#1c2256` | `#23272e` | `#eef0f3` |
| `state/pressed` | `#252c6b` | `#2c3139` | `#e1e5e9` |
| `line/soft` | `#242b5f` | `#272b32` | `#e1e5e9` |
| `line/strong` | `#363f7c` | `#3a404a` | `#cbd1d8` |
| `text/primary` | `#eef0ff` | `#eceef1` | `#14181f` |
| `text/secondary` | `#a3aad6` | `#a0a9b5` | `#566170` |
| `text/tertiary` | `#6e77ad` | `#6f7885` | `#8a94a1` |
| `text/disabled` | `#4b5493` | `#4c535f` | `#b1b8c2` |
| `text/inverse` | `#0f1331` | `#17191d` | `#fafbfc` |
| `text/on-marker` | `#0b0e2a` | `#14181f` | `#14181f` |
| `text/on-danger` | `#0f1331` | `#17191d` | `#ffffff` |
| `ink/fill` | `#eef0ff` | `#eceef1` | `#14181f` |
| `accent/marker` | `#8e96ff` | `#ffd04d` | `#ffc933` |
| `accent/marker-hover` | `#a4aaff` | `#ffdb70` | `#f5ba16` |
| `accent/marker-pressed` | `#7c84f2` | `#f2c23a` | `#e6a800` |
| `accent/marker-soft` | `#8e96ff42` | `#ffd04d38` | `#ffc93366` |
| `danger` | `#ff7a93` | `#ff7078` | `#d6424b` |
| `danger-soft` | `#ff7a9329` | `#ff707829` | `#d6424b1f` |
| `status/ok` | `#4cc38a` | `#4cc38a` | `#2e9e6b` |

Type styles (all Golos Text):

| Style | Size / line | Weight | Letter spacing |
|---|---|---|---|
| `Heading/Page` | 22 / 28 | 600 | −1 |
| `Title/Section` | 16 / 24 | 600 | −1 |
| `Body/Default` | 14 / 20 | 400 | 0 |
| `Body/Medium` | 14 / 20 | 500 | 0 |
| `Caption` | 12 / 16 | 400 | 0 |
| `Caption/Medium` | 12 / 16 | 500 | 0 |

Radii, spacing and shadows: not exposed as variables — read them from
`get_design_context` per component when building it.

## Screens (node ids; navy / dark / light)

| Screen | Navy | Dark | Light | Window |
|---|---|---|---|---|
| Элементы интерфейса (buttons, icon buttons, switches, segmented, fields, tags, progress, KeyCap) | `16:2238` | `16:509` | `10:3` | component sheet |
| Быстрый перевод (680×303) — source, translation, «Другие варианты», Esc, Заменить выделенное, Копировать, engine chip, pin | `16:2547` | `16:818` | `12:24` | Quick window / popup |
| Трей-меню (324×331) — on/off, translate selection, direction, open translator, history, pause 1 h, settings, exit | `16:2611` | `16:882` | `12:88` | tray menu |
| Выпадающие списки — language picker (search, detect, recent, all), engine picker (cloud / offline), UI language | `16:2668` | `16:939` | `12:152` | dropdowns |
| Переводчик (компакт) / Текст (540×500) | `16:3243` | `16:1514` | `15:71` | main translator |
| Переводчик (компакт) / Изображения | `16:3331` | `16:1602` | `15:163` | main translator, image tab |
| Переводчик (компакт) / Список языков | `16:3393` | `16:1664` | `15:225` | main translator, language open |
| Настройки / Общие (800×660) | `16:2768` | `16:1039` | `13:33` | settings |
| Настройки / Провайдеры | `16:2894` | `16:1165` | `13:162` | settings |
| Настройки / Офлайн и ускорение | `16:3521` | `16:1792` | `16:57` | settings |
| Настройки / Контекст и режимы | `16:3670` | `16:1941` | `16:206` | settings |
| Настройки / Горячие клавиши | `16:3014` | `16:1285` | `14:46` | settings |
| Настройки / Статистика | `16:3774` | `16:2045` | `16:315` | settings |
| Настройки / О программе | `16:3139` | `16:1410` | `14:304` | settings |

Library frame `2:2`: Switch `2:3`, KeyCap `2:8`, Select `2:11`, NavItem `2:16`,
Logo `2:21`, KeyCap icons `15:70`.

## Not in Figma — build from existing components

The user decided (2026-10-14): compose these in code from the Figma components
and tokens, then show screenshots for review.

- Double-Ctrl popup: word variant (alternatives, part of speech) and sentence
  variant (pick words, Copy word / Copy all / Replace), compact style.
- Ultra mode window (fragments with language, original, translation).
- History panel (search, starred, reopen, copy, delete).
- First-run welcome / "How it works".
- Settings → Offline: OPUS-MT, Argos, NLLB 600M / 1.3B rows (only TranslateGemma
  4B / 12B and OPUS-MT are drawn; use the same row pattern).

## Notes from the frames

- Settings sidebar: Общие, Провайдеры, Офлайн и ускорение, Контекст и режимы,
  Горячие клавиши, Статистика, О программе.
- Providers page: cloud engines with on/off + "по умолчанию" tag, key fields
  with eye toggle, «Добавить ключ»; «Запасной вариант» (switch on error,
  response timeout); «Проверка» (test every engine).
- Offline page: device (Авто / Процессор / Видеокарта, GPU name + VRAM), CPU
  threads slider, precision (Авто / int8 / int8+fp16 / float32), model rows
  with installed / progress / download / delete.
- About page: version + build date, check for updates, what's new, update
  channel (Стабильный), auto-check toggle, licences, source code, report bug.
- Status bar of the translator: history, settings, "● Офлайн готов".
