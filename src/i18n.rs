//! UI strings. The English text is the key: `tr("Copy")` returns it as is
//! in English, or its Russian translation from the tables below. Russian is
//! the default interface language (Settings → General → Interface language).
//!
//! All Russian strings live in this file, one table per surface. A string
//! missing from the tables falls back to English. Placeholders such as
//! `{n}` are filled in with [`trf`].

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static RUSSIAN: AtomicBool = AtomicBool::new(true);

/// Interface languages offered in Settings: (code, name in that language).
pub const UI_LANGUAGES: &[(&str, &str)] = &[("ru", "Русский"), ("en", "English")];

pub fn set_lang(code: &str) {
    RUSSIAN.store(code != "en", Ordering::Relaxed);
}

pub fn is_russian() -> bool {
    RUSSIAN.load(Ordering::Relaxed)
}

fn table() -> &'static HashMap<&'static str, &'static str> {
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| {
        [COMMON, POPUP, TRAY, WELCOME, SETTINGS, PROVIDERS, RECORDING, ZOOM, ENGINE, MAIN_WINDOW, HISTORY, QUICK, ULTRA, LINKS, OFFLINE]
            .iter()
            .flat_map(|t| t.iter().copied())
            .collect()
    })
}

/// The UI string for `en` in the current interface language.
pub fn tr(en: &'static str) -> &'static str {
    if !is_russian() {
        return en;
    }
    table().get(en).copied().unwrap_or(en)
}

/// [`tr`] for text that isn't a literal, e.g. an engine error message
/// (`Error::Other`): translated when it is a known UI string.
pub fn tr_text(en: &str) -> &str {
    if !is_russian() {
        return en;
    }
    table().get(en).copied().unwrap_or(en)
}

/// [`tr`] with `{name}` placeholders replaced: `trf("Copy {n} words", &[("n", "3")])`.
pub fn trf(en: &'static str, args: &[(&str, &str)]) -> String {
    let mut s = tr(en).to_owned();
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// Russian plural form: `plural(n, "фрагмент", "фрагмента", "фрагментов")`.
/// English callers pass the same word for `few` and `many`.
pub fn plural<'a>(n: usize, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    let (m10, m100) = (n % 10, n % 100);
    if m10 == 1 && m100 != 11 {
        one
    } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
        few
    } else {
        many
    }
}

type Table = &'static [(&'static str, &'static str)];

/// Shared words: buttons, states, key names.
const COMMON: Table = &[
    ("Copy", "Копировать"),
    ("Copied", "Скопировано"),
    ("Replace", "Заменить"),
    ("close", "закрыть"),
    ("Close", "Закрыть"),
    ("Minimize", "Свернуть"),
    ("Listen", "Прослушать"),
    ("Retry", "Повторить"),
    ("Auto-detect", "Определять автоматически"),
    ("Detect language", "Определить язык"),
    ("Search languages", "Поиск языка"),
    ("Search", "Поиск"),
    ("No languages found", "Языки не найдены"),
    ("Never", "Никогда"),
    ("{n} s", "{n} с"),
    ("Double-tap Ctrl", "Двойное нажатие Ctrl"),
    ("Double-tap Alt", "Двойное нажатие Alt"),
    ("Double-tap Shift", "Двойное нажатие Shift"),
    ("Translating…", "Перевод…"),
];

/// The translation popup.
const POPUP: Table = &[
    ("Pick this translation", "Выбрать этот перевод"),
    // Parts of speech from the dictionary (Google returns them in English).
    ("noun", "существительное"),
    ("verb", "глагол"),
    ("adjective", "прилагательное"),
    ("adverb", "наречие"),
    ("pronoun", "местоимение"),
    ("preposition", "предлог"),
    ("conjunction", "союз"),
    ("interjection", "междометие"),
    ("particle", "частица"),
    ("article", "артикль"),
    ("abbreviation", "сокращение"),
    ("phrase", "фраза"),
    ("prefix", "приставка"),
    ("suffix", "суффикс"),
    ("auxiliary verb", "вспомогательный глагол"),
    ("also", "также"),
    ("All meanings", "Все значения"),
    ("Fewer meanings", "Меньше значений"),
    ("Copy “{w}”", "Копировать «{w}»"),
    ("Replace the selection with “{w}”", "Заменить выделенное на «{w}»"),
    ("Replace the selection with the translation", "Заменить выделенное переводом"),
    ("Copy all", "Копировать всё"),
    ("Copy word", "Копировать слово"),
    ("Copy {n} words", "Копировать слова"),
    ("Click to copy · Shift+click to replace", "Щелчок — копировать · Shift+щелчок — заменить"),
    ("Can’t reach the translation service", "Сервис перевода недоступен"),
    ("Check your connection and try again", "Проверьте подключение и повторите"),
    ("The translation service is busy", "Сервис перевода занят"),
    ("Wait a moment and try again", "Подождите немного и повторите"),
    ("Translation failed", "Не удалось перевести"),
    ("The API key was rejected", "API-ключ отклонён"),
    ("Empty response", "Пустой ответ"),
];

/// The tray menu.
const TRAY: Table = &[
    ("On", "Вкл"),
    ("Paused", "Пауза"),
    ("Translate selection", "Перевести выделенное"),
    ("Translate to", "Переводить на"),
    ("Pause for 1 hour", "Пауза на 1 час"),
    ("Resume", "Возобновить"),
    ("Open translator", "Открыть переводчик"),
    ("Settings…", "Настройки…"),
    ("Quit", "Выход"),
    ("HeliLingo — paused", "HeliLingo — на паузе"),
    ("HeliLingo — {s} to translate", "HeliLingo — {s} для перевода"),
];

/// "How it works" intro.
const WELCOME: Table = &[
    ("Select a word", "Выделите слово"),
    ("Or a sentence — anywhere, in any app", "Или предложение — где угодно, в любом приложении"),
    ("Two quick presses, under half a second", "Два быстрых нажатия, меньше чем за полсекунды"),
    ("Press {s}", "Нажмите {s}"),
    ("Works in any app", "Работает в любом приложении"),
    ("Read it, carry on", "Прочитайте и продолжайте"),
    ("Appears next to the word. Esc or click away closes it", "Появляется рядом со словом. Esc или щелчок мимо закрывает"),
    ("Got it", "Понятно"),
];

/// Settings window (all pages except Providers).
const SETTINGS: Table = &[
    ("Settings", "Настройки"),
    // Sidebar and General.
    ("General", "Общие"),
    ("Providers", "Провайдеры"),
    ("Offline & acceleration", "Офлайн и ускорение"),
    ("Context & modes", "Контекст и режимы"),
    ("Hotkeys", "Горячие клавиши"),
    ("About", "О программе"),
    ("Interface", "Интерфейс"),
    ("Interface language", "Язык интерфейса"),
    ("Start with Windows", "Запускать вместе с Windows"),
    ("Translation", "Перевод"),
    ("Translate from", "Переводить с"),
    ("If the text is already in {lang}", "Если текст уже на {lang}"),
    ("Translate to English", "Перевести на английский"),
    ("Translate to Russian", "Перевести на русский"),
    ("Leave as is", "Оставить как есть"),
    ("Popup", "Всплывающее окно"),
    ("Style", "Стиль"),
    ("Compact", "Компактный"),
    ("Full", "Полный"),
    ("Hide after", "Скрывать через"),
    ("Read translation aloud", "Озвучивать перевод"),
    ("Shortcuts are ignored in", "Сочетания не работают"),
    ("Fullscreen apps", "В полноэкранных приложениях"),
    ("Games and videos in full screen", "Игры и видео на весь экран"),
    ("These apps", "В этих приложениях"),
    ("While one of them is in front, the shortcuts do nothing", "Пока одно из них на переднем плане, сочетания ничего не делают"),
    ("Add app…", "Добавить…"),
    ("Type a name, e.g. game.exe", "Введите имя, например game.exe"),
    ("No other apps are open", "Других открытых приложений нет"),
    ("Remove", "Убрать"),
    // Hotkeys.
    ("Click a shortcut to change it.", "Нажмите на сочетание, чтобы изменить."),
    ("Shortcuts", "Сочетания"),
    ("In the popup next to the word", "Во всплывающем окне рядом со словом"),
    ("Quick window", "Быстрое окно"),
    ("Type text — the translation appears at once", "Введите текст — перевод появляется сразу"),
    ("A selection in several languages", "Выделенный блок на нескольких языках"),
    ("Screen area", "Область экрана"),
    ("Translate text in a picture", "Перевод текста на картинке"),
    ("Main window", "Главное окно"),
    ("Unload", "Выгрузить"),
    ("Loaded", "Загружена"),
    ("Free the memory it uses; it loads again when used", "Освободить занятую память; модель загрузится снова при использовании"),
    ("Other services", "Другие сервисы"),
    ("In the compact popup: up to 3 translations from your other services, pick one", "В компактном окне: до 3 переводов от других сервисов, можно выбрать"),
    ("List", "Списком"),
    ("Menu", "Меню"),
    ("Use this translation", "Использовать этот перевод"),
    ("Other services' translations", "Переводы других сервисов"),
    ("Developer", "Разработчик"),
    ("Developer: {name}", "Разработчик: {name}"),
    ("Hub", "Хаб"),
    ("Translate and paste", "Перевести и вставить"),
    ("Replaces the selection with its translation, no window", "Заменяет выделенное переводом, без окон"),
    ("Ctrl + C twice", "Ctrl + C дважды"),
    ("Press Ctrl + C twice to translate what you copied, like in DeepL", "Дважды нажмите Ctrl + C, чтобы перевести скопированное, как в DeepL"),
    ("Record shortcut…", "Записать сочетание…"),
    ("Press keys…", "Нажмите клавиши…"),
    ("Turn off", "Выключить"),
    ("Off", "Выкл"),
    ("Double press", "Двойное нажатие"),
    ("Interval between presses", "Интервал между нажатиями"),
    ("Lower — fewer accidental triggers", "Меньше — реже случайные срабатывания"),
    ("{n} ms", "{n} мс"),
    // Offline & acceleration.
    ("Local models work without the internet and run on the processor or graphics card.", "Локальные модели работают без интернета и считают на процессоре или видеокарте."),
    ("Models", "Модели"),
    ("98 MB · lightweight, starts fast", "98 МБ · лёгкая, быстро стартует"),
    ("Not downloaded", "Не скачана"),
    // Context & modes.
    ("Detect the language of each fragment", "Определять язык каждого фрагмента"),
    ("One selection can mix several languages", "В одном выделении может быть несколько языков"),
    ("Leave fragments in {lang} as is", "Не трогать фрагменты на {lang}"),
    ("Text already in the target language stays as it is", "Текст на целевом языке остаётся как есть"),
    ("Show the Ultra window", "Показывать окно «Ультра»"),
    ("Otherwise the result replaces the selected text right away", "Иначе результат сразу заменит выделенный текст"),
    // About.
    ("Version {v}", "Версия {v}"),
    ("Select text in any app, double-tap Ctrl and read the translation right next to it.", "Выделите текст в любом приложении, дважды нажмите Ctrl и читайте перевод прямо рядом с ним."),
    ("Privacy", "Конфиденциальность"),
    ("Selected text is sent for translation and not stored", "Выделенный текст отправляется на перевод и не сохраняется"),
    ("Only your own translation history keeps it, if it is turned on in General", "Его хранит только ваша история переводов, если она включена в разделе «Общие»"),
    ("API keys are encrypted with Windows DPAPI", "API-ключи зашифрованы с помощью Windows DPAPI"),
    ("Only your Windows account on this PC can read them", "Прочитать их может только ваша учётная запись Windows на этом компьютере"),
    ("Settings are stored in", "Настройки хранятся в"),
    ("Open folder", "Открыть папку"),
    ("HeliLingo — settings", "HeliLingo — настройки"),
    ("Statistics", "Статистика"),
    ("Theme", "Тема"),
    ("Dark", "Тёмная"),
    ("Light", "Светлая"),
    ("As in Windows", "Как в системе"),
    ("Coming later", "Появится позже"),
    ("Animations", "Анимации"),
    ("Smooth fades and slides for popups, windows and pages", "Плавное появление всплывающих окон, окон и страниц"),
    ("Hardware acceleration", "Аппаратное ускорение"),
    (
        "Off: lighter rendering for weak or remote PCs (no animations). Applies after restart.",
        "Выкл.: облегчённая отрисовка для слабых и удалённых ПК (без анимаций). Применится после перезапуска",
    ),
    ("Ctrl+C in the popup", "Ctrl+C во всплывающем окне"),
    ("Copy and close", "Копировать и закрыть"),
    ("Copy only", "Только копировать"),
    ("Ignore", "Не перехватывать"),
    ("Show Wiki button", "Кнопка «Wiki»"),
    ("Wikipedia article for a word, in popups and windows", "Статья Википедии о слове во всплывающих окнах и окнах перевода"),
    // Hotkeys: the whole-screen shortcut.
    ("Whole screen", "Весь экран"),
    ("Translate everything on the monitor", "Перевод всего текста на мониторе"),
    // Context & modes: programmer mode.
    ("How to translate code and selections in several languages.", "Как переводить код и выделения на нескольких языках."),
    ("Code", "Код"),
    ("Programmer mode", "Режим программиста"),
    ("In code, translate only strings and comments", "В коде переводить только строки и комментарии"),
    // Statistics.
    ("Week", "Неделя"),
    ("Month", "Месяц"),
    ("All time", "Всё время"),
    ("Translations", "Переводов"),
    ("Characters", "Знаков"),
    ("Days in a row", "Дней подряд"),
    ("record — {n}", "рекорд — {n}"),
    ("in all time", "за всё время"),
    ("none the week before", "неделей ранее — ни одного"),
    ("none the month before", "месяцем ранее — ни одного"),
    ("↑ {n}% vs last week", "↑ {n}% к прошлой неделе"),
    ("↑ {n}% vs last month", "↑ {n}% к прошлому месяцу"),
    ("↓ {n}% vs last week", "↓ {n}% к прошлой неделе"),
    ("↓ {n}% vs last month", "↓ {n}% к прошлому месяцу"),
    ("Functions", "Функции"),
    ("uses", "использований"),
    ("Double Ctrl", "Двойной Ctrl"),
    ("Translator window", "Окно перевода"),
    ("«Ultra»", "«Ультра»"),
    ("Languages", "Языки"),
    ("most often: {lang}", "чаще всего: {lang}"),
    ("Other", "Другие"),
    ("Language pairs will appear after a few translations", "Языковые пары появятся после нескольких переводов"),
    ("Most translated words", "Чаще всего переводимые слова"),
    ("To glossary", "В глоссарий"),
    ("Adds these words to glossary.tsv in the app folder", "Добавляет эти слова в glossary.tsv в папке приложения"),
    ("Added {n} words", "Добавлено слов: {n}"),
    ("Already in the glossary", "Уже в глоссарии"),
    ("Words you translate one at a time will appear here", "Здесь появятся слова, которые вы переводите по одному"),
    ("Stored only on this PC", "Хранится только на этом ПК"),
    ("Keep statistics", "Вести статистику"),
    ("Reset", "Сбросить"),
    ("Click again to reset", "Нажмите ещё раз, чтобы сбросить"),
];

/// Zoom viewer of the Images tab.
const ZOOM: Table = &[
    ("Enlarge", "Увеличить"),
    ("Original", "Оригинал"),
    ("Fit", "По размеру"),
];

/// Shortcut recording on the Hotkeys page.
const RECORDING: Table = &[
    ("Press a chord (Ctrl + Alt + A) or a mouse side button, press it twice for ×2, or tap Ctrl, Alt or Shift twice. Esc cancels, Backspace turns the shortcut off.",
        "Нажмите сочетание (Ctrl + Alt + A) или боковую кнопку мыши, нажмите дважды для ×2 или дважды коснитесь Ctrl, Alt или Shift. Esc — отмена, Backspace — выключить сочетание."),
    ("{keys} was used by “{other}” — that shortcut is now off", "{keys} было у «{other}» — то сочетание теперь выключено"),
];

/// Settings → Providers.
const PROVIDERS: Table = &[
    ("Translation providers", "Провайдеры перевода"),
    (
        "Translation goes top to bottom. If a provider doesn't answer or has no key, the next one is used.",
        "Перевод идёт сверху вниз. Если провайдер не отвечает или нет ключа — берётся следующий.",
    ),
    ("Google Translate", "Google Переводчик"),
    ("Yandex Translate", "Яндекс Переводчик"),
    ("no key needed", "ключ не нужен"),
    ("API key set", "API-ключ задан"),
    ("API key required", "нужен API-ключ"),
    ("offline · not installed", "офлайн · не установлен"),
    ("Primary", "Основной"),
    ("Fallback", "Резерв"),
    ("Last fallback", "Последний резерв"),
    ("No key", "Нет ключа"),
    ("Check", "Проверить"),
    ("Checking…", "Проверка…"),
    ("Works", "Работает"),
    ("Where to get a key", "Где взять ключ"),
    ("Cloud Translation API key (optional)", "API-ключ Cloud Translation (необязательно)"),
    ("Without a key the free public Google endpoint is used.", "Без ключа используется бесплатный публичный сервис Google."),
    ("Folder ID", "ID каталога"),
    (
        "The folder ID is needed for user API keys; service account keys work without it.",
        "ID каталога нужен для пользовательских ключей; ключам сервисного аккаунта он не нужен.",
    ),
    ("Azure Translator key required", "нужен ключ Azure Translator"),
    ("Region", "Регион"),
    ("Paste the Yandex Cloud API key", "Вставьте API-ключ Yandex Cloud"),
    ("Paste the Azure Translator key", "Вставьте ключ Azure Translator"),
    ("Paste the DeepL API key", "Вставьте API-ключ DeepL"),
    ("Follows from the key", "Определяется по ключу"),
    ("Google mode", "Режим Google"),
    ("Auto", "Авто"),
    ("API only", "Только API"),
    ("Web", "Веб"),
    (
        "If Google answers “busy”, HeliLingo switches to other Google web addresses.",
        "Если Google отвечает «занят», HeliLingo переключается на другие веб-адреса Google.",
    ),
    (
        "Drag to change the order. Keys are kept in the protected Windows storage.",
        "Порядок меняется перетаскиванием. Ключи хранятся в защищённом хранилище Windows.",
    ),
    ("e.g. westeurope", "например, westeurope"),
    ("Plan", "Тариф"),
    (
        "Free — up to 500,000 characters a month. The key is stored encrypted by Windows.",
        "Free — до 500 000 знаков в месяц. Ключ хранится в защищённом хранилище Windows.",
    ),
];

/// Errors and messages from the translation engine (`translate::Error::Other`;
/// "The API key was rejected" and "Empty response" are in POPUP).
const ENGINE: Table = &[
    ("No API key", "Нет API-ключа"),
    ("Folder ID required", "Нужен ID каталога"),
    ("Not installed", "Не установлен"),
    ("Quota exceeded", "Лимит исчерпан"),
    ("The service rejected the request", "Сервис отклонил запрос"),
    ("The service returned an error", "Сервис вернул ошибку"),
    ("Unexpected response", "Неожиданный ответ сервиса"),
    ("Network error", "Ошибка сети"),
    ("No translation service is available", "Нет доступных сервисов перевода"),
];

/// Main translator window (text and image tabs).
const MAIN_WINDOW: Table = &[
    ("Translator", "Перевод"),
    ("Text", "Текст"),
    ("Images", "Изображения"),
    ("More languages", "Другие языки"),
    ("Swap source and target", "Поменять языки местами"),
    ("Enter text", "Введите текст"),
    ("Clear", "Очистить"),
    ("detected: {lang}", "определён: {lang}"),
    ("Translation service", "Сервис перевода"),
    ("Any provider", "Любой сервис"),
    ("Add to favourites", "Добавить в избранное"),
    ("Remove from favourites", "Убрать из избранного"),
    ("{s} — translate selected text in any app", "{s} — перевод выделенного текста в любом приложении"),
    // Images tab.
    ("Drag an image here", "Перетащите изображение сюда"),
    ("Drop the image to translate it", "Отпустите, чтобы перевести изображение"),
    ("or paste from the clipboard", "или вставьте из буфера"),
    ("Choose file", "Выбрать файл"),
    ("Recognition: Windows OCR", "Распознавание: Windows OCR"),
    ("PNG, JPG, WEBP · up to 20 MB", "PNG, JPG, WEBP · до 20 МБ"),
    ("The translation will appear here", "Здесь появится перевод"),
    ("The text in the picture will be replaced in {lang}", "Текст на картинке будет заменён на {lang}"),
    ("Opening the image…", "Открываем изображение…"),
    ("Recognizing text…", "Распознаём текст…"),
    ("translating…", "переводим…"),
    ("No translation", "Перевода нет"),
    ("Another image", "Другое изображение"),
    ("As picture", "Картинкой"),
    ("As text", "Текстом"),
    ("Save the picture", "Сохранить картинку"),
    ("Save the text", "Сохранить текст"),
    ("Choose an image", "Выберите изображение"),
    ("Image", "Изображение"),
    ("translated", "перевод"),
    ("translation", "перевод"),
    ("The image is larger than 20 MB", "Изображение больше 20 МБ"),
    ("Choose a smaller image", "Выберите изображение поменьше"),
    ("Can’t open this image", "Не удаётся открыть изображение"),
    ("Use a PNG, JPG or WEBP image", "Используйте PNG, JPG или WEBP"),
    ("Can’t read the file", "Не удаётся прочитать файл"),
    ("Check that the file still exists", "Проверьте, что файл существует"),
    ("Windows can’t recognise {lang} text", "Windows не распознаёт текст на языке «{lang}»"),
    (
        "Add the {lang} language with “Optical character recognition” in Windows Settings → Time & language → Language & region",
        "Добавьте язык «{lang}» с компонентом «Оптическое распознавание символов» в Параметрах Windows → Время и язык → Язык и регион",
    ),
    ("No text recognition language is installed", "Не установлен язык для распознавания текста"),
    (
        "Add a language with “Optical character recognition” in Windows Settings → Time & language → Language & region",
        "Добавьте язык с компонентом «Оптическое распознавание символов» в Параметрах Windows → Время и язык → Язык и регион",
    ),
    ("Text recognition failed", "Не удалось распознать текст"),
    ("No text found in the picture", "На картинке не найден текст"),
    (
        "Try a sharper or larger image, or choose the source language",
        "Попробуйте более чёткое или крупное изображение либо выберите язык оригинала",
    ),
    ("Couldn’t capture the screen", "Не удалось снять экран"),
    ("Try again", "Попробуйте ещё раз"),
    ("There is no picture on the clipboard", "В буфере обмена нет картинки"),
    ("Copy an image or a screenshot, then press Ctrl+V", "Скопируйте изображение или снимок экрана и нажмите Ctrl+V"),
    ("Drag to select the text to translate · Esc to cancel", "Выделите область с текстом · Esc — отмена"),
];

/// History panel and history-related settings.
const HISTORY: Table = &[
    ("History", "История"),
    ("Save translation history", "Сохранять историю переводов"),
    ("Saved translations", "Сохранённые переводы"),
    ("Clear history ({n})", "Очистить историю ({n})"),
    ("Click again to clear", "Нажмите ещё раз, чтобы очистить"),
    ("Search history", "Поиск по истории"),
    ("All", "Все"),
    ("Starred", "Избранное"),
    ("Clear history", "Очистить историю"),
    ("Click again to clear the history", "Нажмите ещё раз, чтобы очистить"),
    ("History is empty", "История пуста"),
    ("Your translations will appear here", "Здесь появятся ваши переводы"),
    ("No starred translations", "В избранном пока пусто"),
    ("Star a translation to keep it here", "Отметьте перевод звёздочкой, чтобы сохранить его здесь"),
    ("Nothing found", "Ничего не найдено"),
    ("Try another word", "Попробуйте другое слово"),
    ("Delete", "Удалить"),
    ("{n} entries", "Записей: {n}"),
    ("just now", "только что"),
    ("{n} min ago", "{n} мин назад"),
    ("{n} h ago", "{n} ч назад"),
    ("yesterday", "вчера"),
    ("{n} d ago", "{n} дн. назад"),
    ("Ultra", "Ультра"),
];

/// Quick translation window.
const QUICK: Table = &[
    ("Detect", "Автоопределение"),
    ("Swap languages", "Поменять языки местами"),
    ("Automatic", "Автоматически"),
    ("Type or paste text", "Введите или вставьте текст"),
    ("copy and close", "копировать и закрыть"),
];

/// Ultra mode window.
const ULTRA: Table = &[
    ("Ultra mode", "Режим «Ультра»"),
    ("unchanged", "без изменений"),
    ("Cancelled", "Отменено"),
    ("Detecting languages…", "Определяем языки…"),
    ("Replace selection", "Заменить выделенное"),
];

/// Google Translate and Wikipedia links, whole-screen and window capture.
const LINKS: Table = &[
    ("Open in Google Translate", "Открыть в Google Переводчике"),
    ("Wikipedia article", "Статья в Википедии"),
    ("Searching Wikipedia…", "Ищем в Википедии…"),
    ("Open in Wikipedia", "Открыть в Википедии"),
    ("Search Wikipedia", "Искать в Википедии"),
    ("Wikipedia is unavailable", "Википедия недоступна"),
    ("Entire screen", "Весь экран"),
    ("Translate the text on the whole screen", "Перевести текст на всём экране"),
    ("Area", "Область"),
    ("Window", "Окно"),
    ("Drag over the text to translate", "Выделите область с текстом"),
    ("Click the window to translate", "Щёлкните по окну с текстом"),
    ("cancel", "отмена"),
];

/// Offline models: Settings pages and engine errors.
const OFFLINE: Table = &[
    ("super-mega-fast", "супер-мега-быстрая"),
    ("super-fast", "супер-быстрая"),
    ("fast", "быстрая"),
    ("normal", "нормальная"),
    ("medium (with context)", "средняя (с контекстом)"),
    ("heavy (with context)", "тяжёлая (с контекстом)"),
    ("This offline model doesn't support this language pair", "Эта офлайн-модель не поддерживает эту пару языков"),
    ("The offline model failed", "Ошибка офлайн-модели"),
    ("The local model server didn't start", "Локальный сервер модели не запустился"),
    ("Download failed: no connection", "Загрузка не удалась: нет соединения"),
    ("The file is damaged (checksum mismatch); try again", "Файл повреждён (не совпала контрольная сумма); попробуйте ещё раз"),
    ("Can’t write to the model folder", "Не удаётся записать в папку моделей"),
    ("Not enough free disk space", "Недостаточно места на диске"),
    ("The download source is unavailable", "Источник загрузки недоступен"),
    ("The downloaded model is incomplete", "Загруженная модель неполная"),
    ("GB", "ГБ"),
    ("MB", "МБ"),
    ("English ↔ Russian · the fastest, starts instantly", "английский ↔ русский · самая быстрая, запускается мгновенно"),
    ("Several languages through English · fast", "несколько языков через английский · быстрая"),
    ("200 languages · good quality", "200 языков · хорошее качество"),
    ("200 languages · better quality, slower", "200 языков · качество лучше, медленнее"),
    ("55 languages · uses context and the glossary · graphics card recommended", "55 языков · учитывает контекст и глоссарий · желательна видеокарта"),
    ("55 languages · the best quality · needs about 8 GB of video memory", "55 языков · лучшее качество · нужно около 8 ГБ видеопамяти"),
    ("Device", "Устройство"),
    ("No suitable graphics card found", "Подходящая видеокарта не найдена"),
    ("Run models on", "Где запускать модели"),
    ("Processor", "Процессор"),
    ("Graphics card", "Видеокарта"),
    ("Auto ({n})", "Авто ({n})"),
    ("Processor threads", "Потоки процессора"),
    ("For models on the processor; Auto uses the physical cores", "Для моделей на процессоре; «Авто» — по числу физических ядер"),
    ("Precision", "Точность"),
    ("OPUS-MT, Argos and NLLB. Gemma models use their own", "Для OPUS-MT, Argos и NLLB. У моделей Gemma своя"),
    ("Graphics card only", "Только на видеокарте"),
    ("Storage", "Хранение"),
    ("{size} free on this drive", "свободно на диске: {size}"),
    ("Model folder", "Папка моделей"),
    ("Change…", "Изменить…"),
    ("Folder for offline models", "Папка для офлайн-моделей"),
    (
        "Models download only when you click Download. Gemma models also download the llama.cpp engine once. Already downloaded models stay in the old folder when you change it.",
        "Модели скачиваются, только когда вы нажмёте «Скачать». Для моделей Gemma один раз скачивается движок llama.cpp. При смене папки уже скачанные модели остаются в старой.",
    ),
    ("Downloading {done} of {total}", "Загрузка: {done} из {total}"),
    ("Download", "Скачать"),
    ("Click again to delete", "Нажмите ещё раз, чтобы удалить"),
    ("Installed", "Установлена"),
    ("Error", "Ошибка"),
    ("offline · installed", "офлайн · установлена"),
    ("Offline only", "Только офлайн"),
    ("Use only the installed offline models; nothing leaves this PC", "Только установленные офлайн-модели; текст не покидает компьютер"),
    ("Context", "Контекст"),
    ("Gemma models and DeepL see the previous fragments and their translations", "Модели Gemma и DeepL видят предыдущие фрагменты и их переводы"),
    ("Translate with context", "Переводить с контекстом"),
    ("Fragments to remember", "Сколько фрагментов помнить"),
    ("{n} terms · term and translation separated by a tab", "терминов: {n} · термин и перевод через табуляцию"),
    ("Glossary", "Глоссарий"),
    ("Edit glossary", "Изменить глоссарий"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plural_forms() {
        let f = |n| plural(n, "язык", "языка", "языков");
        assert_eq!([f(1), f(2), f(5), f(11), f(21), f(22), f(112)], ["язык", "языка", "языков", "языков", "язык", "языка", "языков"]);
    }

    #[test]
    fn no_duplicate_keys() {
        let mut seen = std::collections::HashSet::new();
        for t in [COMMON, POPUP, TRAY, WELCOME, SETTINGS, PROVIDERS, RECORDING, ZOOM, ENGINE, MAIN_WINDOW, HISTORY, QUICK, ULTRA, LINKS, OFFLINE] {
            for (en, _) in t {
                assert!(seen.insert(*en), "duplicate key {en}");
            }
        }
    }
}
