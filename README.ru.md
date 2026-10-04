<p align="center">
  <img src="assets/brand/sun96.svg" width="96" height="96" alt="HeliLingo">
</p>

<h1 align="center">HeliLingo</h1>

<p align="center">
  <b>Выделите текст где угодно. Дважды нажмите Ctrl. Читайте перевод.</b><br>
  Быстрый нативный переводчик для Windows, который живёт в трее.
</p>

<p align="center">
  <a href="README.md">English</a> ·
  <a href="#возможности">Возможности</a> ·
  <a href="#скачать">Скачать</a> ·
  <a href="#сборка-из-исходников">Сборка</a> ·
  <a href="#горячие-клавиши">Клавиши</a>
</p>

<p align="center">
  <img alt="Windows 10/11" src="https://img.shields.io/badge/Windows-10%20%7C%2011-5b6ee1?style=flat-square">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-2024-5b6ee1?style=flat-square&logo=rust&logoColor=white">
  <img alt="egui" src="https://img.shields.io/badge/UI-egui-5b6ee1?style=flat-square">
  <img alt="Версия" src="https://img.shields.io/badge/версия-2.0.0-5b6ee1?style=flat-square">
</p>

<p align="center">
  <img src="docs/screenshots/word.png" width="300" alt="Перевод слова">
  &nbsp;
  <img src="docs/screenshots/sentence.png" width="400" alt="Перевод предложения">
</p>

---

## Возможности

- **Перевод на месте.** Выделите текст в любой программе, дважды нажмите **Ctrl**, и рядом появится окошко с переводом. Оно не забирает фокус, так что выделение остаётся на месте.
- **Замена в один клик.** Кнопка «Заменить» подставляет перевод вместо выделенного текста прямо в исходной программе; буфер обмена потом восстанавливается. А **Ctrl+Alt+R** переводит и вставляет вообще без окна.
- **Слово или всё предложение.** Нажмите на слово в переводе (Shift+клик или протяжка для нескольких) и скопируйте или замените только его. Для слов показываются часть речи и другие варианты.
- **Быстрое окно** (**Ctrl+Alt+T**): перевод появляется по мере ввода.
- **Режим «Ультра»** (**Ctrl+Alt+U**): текст на нескольких языках делится на фрагменты, и каждый определяется и переводится отдельно.
- **Картинки и области экрана.** Перевод текста на изображении, в области экрана, в окне или на всём мониторе через распознавание Windows (OCR).
- **Несколько сервисов с подстраховкой.** Google и Яндекс работают без ключей, Bing (Azure) и DeepL с вашими ключами. Если один не ответил, перевод берёт следующий.
- **Офлайн-модели.** OPUS-MT, Argos, NLLB-200 (600M / 1.3B) и TranslateGemma (4B / 12B) работают на вашем компьютере, на процессоре или видеокарте.
- **Режим программиста.** В исходном коде переводятся только строки и комментарии, сам код не меняется ни на байт.
- **История, статистика, глоссарий**, статьи из Википедии, озвучка, интерфейс на русском и английском.
- **Приватность.** API-ключи шифруются средствами Windows (DPAPI); история и статистика хранятся только у вас и отключаются.

<p align="center">
  <img src="docs/screenshots/quick.png" width="600" alt="Быстрое окно">
</p>

<p align="center">
  <img src="docs/screenshots/main.png" width="49%" alt="Окно переводчика">
  <img src="docs/screenshots/main-image-result.png" width="49%" alt="Перевод изображения">
</p>

<p align="center">
  <img src="docs/screenshots/ultra.png" width="49%" alt="Режим «Ультра»">
  <img src="docs/screenshots/settings-providers.png" width="49%" alt="Настройки: провайдеры">
</p>

## Скачать

Скачайте `helilingo.exe` из [последнего релиза](https://github.com/Helisy420/HeliLingo/releases/latest) и запустите: один файл, установка не нужна. Программа появится в трее; настройки лежат в `%APPDATA%\HeliLingo`.

## Сборка из исходников

### Что нужно

| Что | Зачем |
|---|---|
| **Windows 10 или 11**, x64 | Программа использует API Win32 и WinRT |
| **[Rust](https://rustup.rs/)** (stable, 1.85 или новее, MSVC) | Edition 2024 |
| **[Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)** с набором *Разработка классических приложений на C++* | Линкер MSVC и CMake |
| CMake | Сборка CTranslate2 и SentencePiece для офлайн-моделей; CMake из Build Tools подхватывается автоматически |

Если CMake лежит в другом месте, укажите путь перед сборкой:

```bash
set CMAKE=C:\путь\к\cmake.exe
```

### Сборка и запуск

```bash
git clone https://github.com/Helisy420/HeliLingo.git
cd HeliLingo
cargo build --release
```

Результат: `target\release\helilingo.exe`. Один файл, всё слинковано статически, ничего доустанавливать не нужно. Первая сборка идёт долго, потому что CTranslate2 компилируется из исходников.

По желанию: `cargo build --release --features offline-cuda` собирает офлайн-модели с CUDA (нужен CUDA Toolkit).

## Горячие клавиши

Все меняются в «Настройки → Горячие клавиши». Любое сочетание можно назначить и на боковые кнопки мыши (**Mouse 4** / **Mouse 5**).

| Сочетание | Что делает |
|---|---|
| Двойное **Ctrl** | Перевод выделенного текста в окошке рядом |
| **Ctrl+Alt+T** | Быстрое окно: перевод по мере ввода |
| **Ctrl+Alt+U** | Режим «Ультра» для выделения |
| **Ctrl+Shift+S** | Перевод области экрана или окна |
| **Ctrl+Shift+A** | Перевод всего монитора под курсором |
| **Ctrl+Alt+Q** | Главное окно переводчика |
| **Ctrl+Alt+R** | Перевести и вставить: выделение заменяется переводом |
| **Ctrl+C** дважды | Перевод только что скопированного (как в DeepL) |

Сочетания не срабатывают, пока на переднем плане полноэкранная программа (игры, видео) или программа из списка исключений.

### В окошке перевода

| Вид | Копировать | Заменить |
|---|---|---|
| **Слово** | Значок копирования копирует выбранный вариант; нажмите на любой вариант, чтобы выбрать | **Заменить** подставляет выбранный перевод вместо выделения |
| **Предложение** | **Копировать** копирует всё; выберите слова кликом или протяжкой, затем **Копировать слово** | **Заменить** подставляет выбранные слова или весь перевод |
| **Компактный** | Клик по переводу | Shift+клик по переводу |

`Esc` или клик в другом месте закрывает окошко; **Ctrl+C**, пока оно открыто, копирует перевод.

## Сервисы перевода

«Настройки → Провайдеры»: перетаскивайте, чтобы изменить порядок, включайте и выключайте. Перевод идёт по включённым сервисам сверху вниз: при любой ошибке берётся следующий.

| Сервис | Ключ |
|---|---|
| **Google** | Не нужен (публичные веб-адреса). По желанию ключ Google Cloud Translation |
| **Яндекс** | Не нужен. По желанию ключ Yandex Cloud |
| **Bing (Microsoft)** | Ключ Azure Translator + регион |
| **DeepL** | API-ключ (бесплатные ключи с `:fx` тоже подходят) |
| **Офлайн-модели** | Не нужен: скачиваются в «Настройки → Офлайн и ускорение» |

Ключи хранятся только в `%APPDATA%\HeliLingo\settings.json`, зашифрованы DPAPI для вашей учётной записи и никогда не пишутся в логи.

## Где хранятся данные

Всё лежит в `%APPDATA%\HeliLingo`:

| Файл | Что внутри |
|---|---|
| `settings.json` | Настройки и зашифрованные ключи |
| `history.json` | История переводов (до 1000 записей, можно отключить) |
| `stats.json` | Локальная статистика (можно отключить) |
| `glossary.tsv` | Ваш глоссарий |

Офлайн-модели по умолчанию ставятся в `%LOCALAPPDATA%\HeliLingo\Models` (папку можно сменить).

## Для разработчиков

```bash
cargo run -- --preview word        # также: sentence compact loading offline tray languages welcome
cargo run -- --preview quick       # также: ultra wiki main main-image main-image-result main-history capture
cargo run -- --preview settings-providers   # также: -hotkeys -stats -offline -modes -about
cargo run -- --translate "effectively" ru --provider google
cargo run -- --check deepl         # проверка ключа из settings.json
```

Режим предпросмотра ничего не пишет в историю и статистику.

<details>
<summary>Структура проекта</summary>

```
src/
  main.rs          точка входа, флаги командной строки
  app.rs           логика: хук → выделение → перевод → окошко; трей, окна
  app/             жизненный цикл быстрого окна, «Ультра», настроек и главного окна
  translate/       цепочка сервисов, Google / Яндекс / Bing / DeepL, языки, кэш
  offline/         офлайн-модели: каталог, загрузка, CTranslate2, llama.cpp
  ui/              окошко перевода, меню трея, настройки, быстрое окно, «Ультра», главное окно
  win/             Win32: хуки, буфер обмена, ввод, озвучка, DPAPI, OCR, захват экрана
  settings.rs      сохранённые настройки
  history.rs       история переводов
  stats.rs         статистика
  code.rs          режим программиста
  wiki.rs          статьи из Википедии
  i18n.rs          строки интерфейса на русском и английском
  theme.rs         цвета, шрифт Montserrat, стиль egui
assets/            иконки, логотип, шрифт Montserrat
```

</details>

## Ограничения

- Только Windows.
- При копировании и вставке сохраняются и восстанавливаются только текст, картинки, файлы, HTML и RTF из буфера обмена.
- В терминалах программа использует Ctrl+Insert / Shift+Insert, чтобы не прерывать запущенный процесс.
- Для озвучки нужен голос Windows для языка, для OCR нужен языковой пакет Windows.
- Пока есть только тёмная тема.

## Благодарности

- Шрифт интерфейса: [Montserrat](https://fonts.google.com/specimen/Montserrat), лицензия SIL Open Font License (см. [`assets/fonts/OFL.txt`](assets/fonts/OFL.txt)).
- Сделано на [egui](https://github.com/emilk/egui), [CTranslate2](https://github.com/OpenNMT/CTranslate2) через [ct2rs](https://github.com/jkawamoto/ctranslate2-rs) и [llama.cpp](https://github.com/ggml-org/llama.cpp).

<p align="center">
  Автор: <a href="https://t.me/helisy">Helisy</a> · <a href="https://t.me/helisy420">Telegram</a> · <a href="https://github.com/Helisy420">GitHub</a>
</p>
