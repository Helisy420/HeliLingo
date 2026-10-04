//! Settings → Statistics (Figma 329:838): period switch, three tiles
//! (translations, characters, days in a row), the Functions and Languages
//! cards, the most translated words with "To glossary", and the footer
//! (stored locally, Keep statistics, Reset). Numbers come from
//! [`crate::stats::summary`]; `--preview settings-stats` shows sample data
//! kept in memory only.

use std::path::Path;
use std::time::{Duration, Instant};

use egui::{Galley, Ui};

use super::*;
use crate::i18n::plural;
use crate::stats::{Period, Summary};

/// "≈ N pages": characters per standard page.
const CHARS_PER_PAGE: u64 = 1800;
/// How long Reset stays armed, and how long "Added N words" shows.
const CONFIRM: Duration = Duration::from_secs(4);
/// The summary is re-read this often while the page is open.
const REFRESH: Duration = Duration::from_secs(2);
const PERIODS: [Period; 3] = [Period::Week, Period::Month, Period::All];

/// State of the Statistics page.
pub struct StatsView {
    pub period: Period,
    /// `--preview settings-stats`: sample numbers instead of stats.json;
    /// Reset and "To glossary" then touch nothing on disk.
    pub sample: bool,
    sample_cleared: bool,
    cache: Option<(Period, Instant, Summary)>,
    reset_armed: Option<Instant>,
    /// "Added N words" after "To glossary".
    note: Option<(String, Instant)>,
}

impl Default for StatsView {
    fn default() -> Self {
        Self { period: Period::Week, sample: false, sample_cleared: false, cache: None, reset_armed: None, note: None }
    }
}

impl StatsView {
    fn summary(&mut self) -> Summary {
        if self.sample {
            return if self.sample_cleared { Summary::default() } else { sample_summary(self.period) };
        }
        let fresh = self
            .cache
            .as_ref()
            .is_some_and(|(p, at, _)| *p == self.period && at.elapsed() < REFRESH);
        if !fresh {
            self.cache = Some((self.period, Instant::now(), crate::stats::summary(self.period)));
        }
        self.cache.as_ref().map(|(_, _, s)| s.clone()).unwrap_or_default()
    }
}

pub(super) fn show(ui: &mut Ui, s: &mut Settings, state: &mut SettingsState) -> Option<SettingsAction> {
    let mut action = None;
    let v = &mut state.stats;
    let sum = v.summary();
    let w = ui.available_width();

    // Title and period.
    ui.allocate_ui_with_layout(vec2(w, 25.0), Layout::left_to_right(Align::Center), |ui| {
        text(ui, tr("Statistics"), 16.0, Weight::SemiBold, TEXT);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let active = PERIODS.iter().position(|p| *p == v.period).unwrap_or(0);
            if let Some(i) = segmented_sm(ui, &[tr("Week"), tr("Month"), tr("All time")], active, &[], "") {
                v.period = PERIODS[i];
            }
        });
    });

    tiles(ui, &sum, v.period);
    function_and_language_cards(ui, &sum);
    words_card(ui, &sum, v);
    if let Some(a) = footer(ui, s, v) {
        action = Some(a);
    }
    if v.reset_armed.is_some() || v.note.is_some() {
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    }
    action
}

// ------------------------------------------------------------------ tiles

/// `Показатели`: three equal tiles (`px 10 py 6`, radius 10, gap 8): 10px
/// label, 16px value, 10px caption.
fn tiles(ui: &mut Ui, sum: &Summary, period: Period) {
    let w = ui.available_width();
    let (prev_caption, prev_color) = change_caption(sum.translations, sum.previous, period);
    let pages = if sum.chars == 0 { 0 } else { ((sum.chars + CHARS_PER_PAGE / 2) / CHARS_PER_PAGE).max(1) };
    let pages_word = if crate::i18n::is_russian() {
        plural(pages as usize, "страница", "страницы", "страниц")
    } else if pages == 1 {
        "page"
    } else {
        "pages"
    };
    let items = [
        (tr("Translations"), group_thousands(sum.translations), prev_caption, prev_color),
        (tr("Characters"), group_thousands(sum.chars), format!("≈ {} {pages_word}", group_thousands(pages)), OK),
        (
            tr("Days in a row"),
            sum.streak.to_string(),
            trf("record — {n}", &[("n", &sum.best_streak.to_string())]),
            OK,
        ),
    ];
    let tile_w = (w - 16.0) / 3.0;
    let galleys: Vec<_> = items
        .iter()
        .map(|(label, value, caption, color)| {
            let room = tile_w - 20.0;
            (
                truncated(ui, label, font(10.0, Weight::Medium), TEXT_3, room),
                truncated(ui, value, font(16.0, Weight::SemiBold), TEXT, room),
                truncated(ui, caption, font(10.0, Weight::Medium), *color, room),
            )
        })
        .collect();
    let content_h = galleys
        .iter()
        .map(|(a, b, c)| a.size().y + b.size().y + c.size().y + 2.0)
        .fold(0.0, f32::max);
    let (rect, _) = ui.allocate_exact_size(vec2(w, content_h + 12.0), Sense::hover());
    for (i, (label, value, caption)) in galleys.into_iter().enumerate() {
        let tile = Rect::from_min_size(pos2(rect.left() + i as f32 * (tile_w + 8.0), rect.top()), vec2(tile_w, rect.height()));
        card_bg(ui, tile);
        let x = tile.left() + 10.0;
        let mut y = tile.top() + 6.0;
        for g in [label, value, caption] {
            let h = g.size().y;
            ui.painter().galley(pos2(x, y), g, TEXT);
            y += h + 1.0;
        }
    }
}

/// "↑ 12% vs last week" (green), "↓ 5% …" (amber), or a muted note.
fn change_caption(now: u64, previous: Option<u64>, period: Period) -> (String, Color32) {
    let Some(prev) = previous else { return (tr("in all time").to_owned(), TEXT_3) };
    let week = period == Period::Week;
    if prev == 0 {
        let none = if week { tr("none the week before") } else { tr("none the month before") };
        return (none.to_owned(), TEXT_3);
    }
    let pct = percent_change(now, prev);
    let n = pct.unsigned_abs().to_string();
    let args = [("n", n.as_str())];
    match (pct >= 0, week) {
        (true, true) => (trf("↑ {n}% vs last week", &args), OK),
        (true, false) => (trf("↑ {n}% vs last month", &args), OK),
        (false, true) => (trf("↓ {n}% vs last week", &args), WARN),
        (false, false) => (trf("↓ {n}% vs last month", &args), WARN),
    }
}

/// Change from `prev` to `now` in whole percent (`prev` > 0).
fn percent_change(now: u64, prev: u64) -> i64 {
    ((now as f64 - prev as f64) / prev as f64 * 100.0).round() as i64
}

/// `1 284` (Russian) / `1,284` (English).
fn group_thousands(n: u64) -> String {
    group_with(n, if crate::i18n::is_russian() { '\u{a0}' } else { ',' })
}

fn group_with(n: u64, sep: char) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

/// A card plate: panel fill, border, radius 10.
fn card_bg(ui: &Ui, rect: Rect) {
    ui.painter().rect_filled(rect, 10, PANEL);
    ui.painter().rect_stroke(rect, 10, BORDER_STROKE, egui::StrokeKind::Inside);
}

/// A card header: 12 SemiBold title on the left.
fn card_title(ui: &Ui, rect: Rect, title: &str) -> f32 {
    let g = ui.painter().layout_no_wrap(title.to_owned(), font(12.0, Weight::SemiBold), TEXT);
    let h = g.size().y;
    ui.painter().galley(pos2(rect.left() + 10.0, rect.top() + 8.0), g, TEXT);
    h
}

/// Right-aligned galley at `right`, vertically centred on `cy`.
fn paint_right(ui: &Ui, g: std::sync::Arc<Galley>, right: f32, cy: f32) {
    let pos = pos2(right - g.size().x, cy - g.size().y / 2.0);
    ui.painter().galley(pos, g, TEXT);
}

fn paint_left(ui: &Ui, g: std::sync::Arc<Galley>, left: f32, cy: f32) -> f32 {
    let w = g.size().x;
    ui.painter().galley(pos2(left, cy - g.size().y / 2.0), g, TEXT);
    w
}

// ------------------------------------------------------- functions, langs

const LINE_H: f32 = 14.0;
const LINE_GAP: f32 = 6.0;

/// `Функции и языки`: two cards side by side, same height.
fn function_and_language_cards(ui: &mut Ui, sum: &Summary) {
    let w = ui.available_width();
    let head = 15.0;
    let functions_h = 8.0 + head + 5.0 * (LINE_GAP + LINE_H) + 9.0;
    let rows = sum.pairs.len().clamp(1, 4) as f32;
    let langs_h = 8.0 + head + LINE_GAP + 5.0 + rows * (LINE_GAP + LINE_H) + 9.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, functions_h.max(langs_h)), Sense::hover());
    let card_w = (w - 8.0) / 2.0;
    let left = Rect::from_min_size(rect.min, vec2(card_w, rect.height()));
    let right = Rect::from_min_size(pos2(left.right() + 8.0, rect.top()), vec2(card_w, rect.height()));
    functions_card(ui, left, sum);
    languages_card(ui, right, sum);
}

/// `Карточка / Функции`: a bar per feature, scaled to the busiest one.
fn functions_card(ui: &Ui, rect: Rect, sum: &Summary) {
    card_bg(ui, rect);
    let th = card_title(ui, rect, tr("Functions"));
    let uses = ui.painter().layout_no_wrap(tr("uses").to_owned(), font(10.0, Weight::Regular), TEXT_3);
    paint_right(ui, uses, rect.right() - 10.0, rect.top() + 8.0 + th / 2.0);
    let labels = [tr("Double Ctrl"), tr("Quick window"), tr("Translator window"), tr("Images"), tr("«Ultra»")];
    let max = sum.features.iter().copied().max().unwrap_or(0).max(1);
    let mut y = rect.top() + 8.0 + 15.0 + LINE_GAP;
    for (label, n) in labels.iter().zip(sum.features) {
        let cy = y + LINE_H / 2.0;
        let x = rect.left() + 10.0;
        let g = truncated(ui, label, font(11.0, Weight::Medium), TEXT_2, 92.0);
        paint_left(ui, g, x, cy);
        let count = ui.painter().layout_no_wrap(group_thousands(n), font(11.0, Weight::SemiBold), TEXT);
        paint_right(ui, count, rect.right() - 10.0, cy);
        let track = Rect::from_min_max(pos2(x + 92.0 + 8.0, cy - 2.0), pos2(rect.right() - 10.0 - 30.0 - 8.0, cy + 2.0));
        ui.painter().rect_filled(track, 2, SWITCH_OFF_TRACK);
        if n > 0 {
            let fill_w = (track.width() * n as f32 / max as f32).max(4.0);
            ui.painter().rect_filled(Rect::from_min_size(track.min, vec2(fill_w, 4.0)), 2, ACCENT);
        }
        y += LINE_H + LINE_GAP;
    }
}

/// Top pairs as percentages (the rest summed as "Other"), for the
/// Languages card: (label, percent, share for the stacked bar).
fn language_rows(sum: &Summary) -> Vec<(String, u64, u64)> {
    let total: u64 = sum.pairs.iter().map(|p| p.2).sum();
    if total == 0 {
        return Vec::new();
    }
    let pct = |n: u64| (n * 100 + total / 2) / total;
    let mut rows: Vec<(String, u64, u64)> = sum
        .pairs
        .iter()
        .take(3)
        .map(|(from, to, n)| (format!("{} → {}", lang_name(from), lang_name(to)), pct(*n), *n))
        .collect();
    if sum.pairs.len() > 3 {
        let rest: u64 = sum.pairs.iter().skip(3).map(|p| p.2).sum();
        let shown: u64 = rows.iter().map(|r| r.1).sum();
        rows.push((tr("Other").to_owned(), 100u64.saturating_sub(shown), rest));
    }
    rows
}

/// `Карточка / Языки`: "most often" badge, stacked bar, top pairs.
fn languages_card(ui: &Ui, rect: Rect, sum: &Summary) {
    card_bg(ui, rect);
    let th = card_title(ui, rect, tr("Languages"));
    let head_cy = rect.top() + 8.0 + th / 2.0;
    if let Some(src) = sum.top_source() {
        let name = lang_name(src);
        let name = if crate::i18n::is_russian() { name.to_lowercase() } else { name };
        let room = rect.width() - 20.0 - 60.0 - 12.0;
        let g = truncated(ui, &trf("most often: {lang}", &[("lang", &name)]), font(10.0, Weight::Medium), ACCENT, room);
        let plate = Rect::from_min_max(
            pos2(rect.right() - 10.0 - g.size().x - 12.0, head_cy - g.size().y / 2.0 - 1.0),
            pos2(rect.right() - 10.0, head_cy + g.size().y / 2.0 + 1.0),
        );
        ui.painter().rect_filled(plate, 8, SELECTED);
        ui.painter().galley(plate.min + vec2(6.0, 1.0), g, ACCENT);
    }
    let rows = language_rows(sum);
    let x = rect.left() + 10.0;
    let inner_w = rect.width() - 20.0;
    let mut y = rect.top() + 8.0 + 15.0 + LINE_GAP;
    if rows.is_empty() {
        let g = ui.painter().layout(
            tr("Language pairs will appear after a few translations").to_owned(),
            font(11.0, Weight::Regular),
            TEXT_3,
            inner_w,
        );
        ui.painter().galley(pos2(x, y), g, TEXT_3);
        return;
    }
    // Stacked bar: 5px, gap 2, segments by share.
    let total: u64 = rows.iter().map(|r| r.2).sum::<u64>().max(1);
    let gaps = 2.0 * (rows.len() as f32 - 1.0);
    let mut bx = x;
    for (i, (_, _, n)) in rows.iter().enumerate() {
        let seg_w = ((inner_w - gaps) * *n as f32 / total as f32).max(2.0);
        let seg = Rect::from_min_size(pos2(bx, y), vec2(seg_w, 5.0));
        ui.painter().rect_filled(seg, 2, STATS_LANG[i.min(3)]);
        bx += seg_w + 2.0;
    }
    y += 5.0 + LINE_GAP;
    for (i, (label, pct, _)) in rows.iter().enumerate() {
        let cy = y + LINE_H / 2.0;
        ui.painter().circle_filled(pos2(x + 3.5, cy), 3.5, STATS_LANG[i.min(3)]);
        let value = ui.painter().layout_no_wrap(format!("{pct}%"), font(11.0, Weight::SemiBold), TEXT);
        let vw = value.size().x;
        paint_right(ui, value, rect.right() - 10.0, cy);
        let g = truncated(ui, label, font(11.0, Weight::Medium), TEXT_2, inner_w - 7.0 - 8.0 - vw - 8.0);
        paint_left(ui, g, x + 7.0 + 8.0, cy);
        y += LINE_H + LINE_GAP;
    }
}

// ------------------------------------------------------------------ words

/// `Частые слова`: the six most translated words in two columns, with
/// "To glossary".
fn words_card(ui: &mut Ui, sum: &Summary, v: &mut StatsView) {
    let w = ui.available_width();
    let words: Vec<&(String, String, u64)> = sum.words.iter().take(6).collect();
    let lines = if words.is_empty() { 1.0 } else { words.len().div_ceil(2) as f32 };
    let body_h = if words.is_empty() { 16.0 } else { lines * 20.0 + (lines - 1.0) * 2.0 };
    let h = 8.0 + 18.0 + 5.0 + body_h + 8.0;
    let rect = Rect::from_min_size(ui.cursor().min, vec2(w, h));
    ui.allocate_rect(rect, Sense::hover());
    card_bg(ui, rect);
    let head_cy = rect.top() + 8.0 + 9.0;
    let title = ui.painter().layout_no_wrap(tr("Most translated words").to_owned(), font(12.0, Weight::SemiBold), TEXT);
    paint_left(ui, title, rect.left() + 10.0, head_cy);

    // "To glossary" (or what it did) on the right.
    let note = v.note.as_ref().filter(|(_, at)| at.elapsed() < CONFIRM).map(|(t, _)| t.clone());
    if note.is_none() {
        v.note = None;
    }
    let link_rect = Rect::from_min_max(pos2(rect.left() + 10.0, rect.top() + 8.0), pos2(rect.right() - 10.0, rect.top() + 26.0));
    let mut head = ui.new_child(UiBuilder::new().max_rect(link_rect).layout(Layout::right_to_left(Align::Center)));
    if let Some(t) = note {
        text(&mut head, t, 11.5, Weight::Medium, OK);
    } else if !words.is_empty() && link_button(&mut head, None, tr("To glossary")).on_hover_text(tr("Adds these words to glossary.tsv in the app folder")).clicked() {
        let added = if v.sample { words.len() } else { export_glossary(&sum.words[..words.len()]).unwrap_or(0) };
        let t = if added == 0 {
            tr("Already in the glossary").to_owned()
        } else {
            trf("Added {n} words", &[("n", &added.to_string())])
        };
        v.note = Some((t, Instant::now()));
    }

    let top = rect.top() + 8.0 + 18.0 + 5.0;
    if words.is_empty() {
        let g = ui.painter().layout(
            tr("Words you translate one at a time will appear here").to_owned(),
            font(11.0, Weight::Regular),
            TEXT_3,
            w - 20.0,
        );
        ui.painter().galley(pos2(rect.left() + 10.0, top), g, TEXT_3);
        return;
    }
    let col_w = (w - 20.0 - 20.0) / 2.0;
    let per_col = words.len().div_ceil(2);
    for (i, (word, translation, n)) in words.iter().map(|t| (&t.0, &t.1, t.2)).enumerate() {
        let (col, line) = (i / per_col, i % per_col);
        let x = rect.left() + 10.0 + col as f32 * (col_w + 20.0);
        let cy = top + line as f32 * 22.0 + 10.0;
        let right = x + col_w;
        let idx = ui.painter().layout_no_wrap((i + 1).to_string(), font(10.0, Weight::Medium), TEXT_3);
        paint_left(ui, idx, x, cy);
        let count = ui.painter().layout_no_wrap(format!("{n}×"), font(10.5, Weight::Medium), TEXT_3);
        let cw = count.size().x;
        paint_right(ui, count, right, cy);
        let mut wx = x + 10.0 + 8.0;
        let room = right - cw - 8.0 - wx;
        let wg = truncated(ui, word, font(11.5, Weight::SemiBold), TEXT, room * 0.6);
        wx += paint_left(ui, wg, wx, cy) + 8.0;
        if !translation.is_empty() {
            let tg = truncated(ui, &format!("→ {translation}"), font(11.0, Weight::Regular), TEXT_2, right - cw - 8.0 - wx);
            paint_left(ui, tg, wx, cy);
        }
    }
}

// ----------------------------------------------------------------- footer

/// Lock + "Stored only on this PC" on the left; "Keep statistics" switch
/// and Reset (click twice) on the right.
fn footer(ui: &mut Ui, s: &mut Settings, v: &mut StatsView) -> Option<SettingsAction> {
    let mut action = None;
    let w = ui.available_width();
    ui.allocate_ui_with_layout(vec2(w, 18.0), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        icon(ui, icons::LOCK12, vec2(12.0, 12.0));
        text(ui, tr("Stored only on this PC"), 10.5, Weight::Regular, TEXT_3);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let armed = v.reset_armed.is_some_and(|t| t.elapsed() < CONFIRM);
            if !armed {
                v.reset_armed = None;
            }
            let label = if armed { tr("Click again to reset") } else { tr("Reset") };
            if link_button(ui, None, label).clicked() {
                if armed {
                    if v.sample {
                        v.sample_cleared = true;
                    } else {
                        crate::stats::reset();
                    }
                    v.cache = None;
                    v.reset_armed = None;
                } else {
                    v.reset_armed = Some(Instant::now());
                }
            }
            changed(switch(ui, &mut s.keep_stats), &mut action);
            text(ui, tr("Keep statistics"), 11.0, Weight::Medium, TEXT_2);
        });
    });
    action
}

// --------------------------------------------------------------- glossary

/// Adds `word<TAB>translation` lines to %APPDATA%\HeliLingo\glossary.tsv,
/// skipping words already there. Returns how many were added.
fn export_glossary(words: &[(String, String, u64)]) -> std::io::Result<usize> {
    let dir = crate::settings::data_dir().ok_or_else(|| std::io::Error::other("no APPDATA"))?;
    write_glossary(&dir.join("glossary.tsv"), words)
}

fn write_glossary(path: &Path, words: &[(String, String, u64)]) -> std::io::Result<usize> {
    let existing = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let (merged, added) = merge_glossary(&existing, words);
    if added > 0 {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, merged)?;
    }
    Ok(added)
}

/// The glossary text with the new words appended (one `word\ttranslation`
/// line each; a word already present, in any case, is skipped).
fn merge_glossary(existing: &str, words: &[(String, String, u64)]) -> (String, usize) {
    let clean = |s: &str| s.replace(['\t', '\r', '\n'], " ").trim().to_owned();
    let mut seen: Vec<String> = existing
        .lines()
        .filter_map(|l| l.split('\t').next())
        .map(|w| w.trim().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    let mut out = existing.to_owned();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    let mut added = 0;
    for (word, translation, _) in words {
        let (word, translation) = (clean(word), clean(translation));
        if word.is_empty() || translation.is_empty() || seen.contains(&word.to_lowercase()) {
            continue;
        }
        seen.push(word.to_lowercase());
        out.push_str(&format!("{word}\t{translation}\n"));
        added += 1;
    }
    (out, added)
}

// ------------------------------------------------------------- preview data

/// The Figma frame's numbers (week), scaled up for the longer periods.
fn sample_summary(period: Period) -> Summary {
    let k = match period {
        Period::Week => 1,
        Period::Month => 4,
        Period::All => 11,
    };
    let features = [684, 312, 176, 74, 38].map(|n| n * k);
    let pair = |a: &str, b: &str, n: u64| (a.to_owned(), b.to_owned(), n * k);
    let word = |a: &str, b: &str, n: u64| (a.to_owned(), b.to_owned(), n * k);
    Summary {
        translations: features.iter().sum(),
        previous: (period != Period::All).then_some(1146 * k),
        chars: 96_400 * k,
        streak: 9,
        best_streak: 14,
        features,
        pairs: vec![pair("en", "ru", 912), pair("de", "ru", 180), pair("ru", "en", 116), pair("fr", "ru", 44), pair("es", "ru", 32)],
        words: vec![
            word("however", "однако", 18),
            word("actually", "на самом деле", 14),
            word("deadline", "срок сдачи", 11),
            word("approach", "подход", 9),
            word("implement", "внедрять", 8),
            word("regarding", "относительно", 7),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(a: &str, b: &str) -> (String, String, u64) {
        (a.into(), b.into(), 1)
    }

    #[test]
    fn glossary_merges_without_duplicates() {
        let (text, added) = merge_glossary("however\tоднако", &[w("However", "тем не менее"), w("deadline", "срок\tсдачи")]);
        assert_eq!(added, 1);
        assert_eq!(text, "however\tоднако\ndeadline\tсрок сдачи\n");
        let (again, added) = merge_glossary(&text, &[w("deadline", "срок")]);
        assert_eq!((again.as_str(), added), (text.as_str(), 0));
    }

    #[test]
    fn glossary_file_round_trip() {
        let dir = std::env::temp_dir().join(format!("helilingo-glossary-test-{}", std::process::id()));
        let path = dir.join("glossary.tsv");
        let _ = std::fs::remove_file(&path);
        assert_eq!(write_glossary(&path, &[w("approach", "подход")]).unwrap(), 1);
        assert_eq!(write_glossary(&path, &[w("approach", "подход"), w("implement", "внедрять")]).unwrap(), 1);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "approach\tподход\nimplement\tвнедрять\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn captions_and_numbers() {
        assert_eq!(group_with(96_400, ','), "96,400");
        assert_eq!(group_with(1284, ' '), "1 284");
        assert_eq!(group_with(12, ' '), "12");
        assert_eq!(percent_change(1284, 1146), 12);
        assert_eq!(percent_change(90, 100), -10);
        assert_eq!(change_caption(5, None, Period::All).1, TEXT_3);
        assert_eq!(change_caption(5, Some(0), Period::Week).1, TEXT_3);
        assert_eq!(change_caption(90, Some(100), Period::Week).1, WARN);
    }

    #[test]
    fn language_rows_add_up() {
        let rows = language_rows(&sample_summary(Period::Week));
        assert_eq!(rows.len(), 4);
        assert_eq!(rows.iter().map(|r| r.1).sum::<u64>(), 100);
        assert!(language_rows(&Summary::default()).is_empty());
    }
}
