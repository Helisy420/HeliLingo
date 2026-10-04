//! The user's glossary: %APPDATA%\HeliLingo\glossary.tsv, one
//! `term<TAB>translation` per line. Statistics → "To glossary" may write it
//! too, so reading is lenient: a BOM, blank lines, `#` comments, extra
//! columns and lines without a translation are all fine.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

pub const FILE: &str = "glossary.tsv";

pub fn path() -> Option<PathBuf> {
    Some(crate::settings::data_dir()?.join(FILE))
}

/// `term → translation` pairs from TSV text.
pub fn parse(text: &str) -> Vec<(String, String)> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| {
            let mut cols = l.split('\t').map(str::trim).filter(|c| !c.is_empty());
            let term = cols.next()?;
            let translation = cols.next()?;
            Some((term.to_owned(), translation.to_owned()))
        })
        .collect()
}

/// The glossary, re-read when the file changes.
pub fn load() -> Vec<(String, String)> {
    type Cached = Option<(Option<SystemTime>, Vec<(String, String)>)>;
    static CACHE: Mutex<Cached> = Mutex::new(None);
    let Some(p) = path() else { return Vec::new() };
    let mtime = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
    let mut cache = CACHE.lock().unwrap();
    if let Some((t, list)) = cache.as_ref()
        && *t == mtime
    {
        return list.clone();
    }
    let list = std::fs::read_to_string(&p).map(|s| parse(&s)).unwrap_or_default();
    *cache = Some((mtime, list.clone()));
    list
}

pub fn count() -> usize {
    load().len()
}

/// Entries whose term occurs in `text` (case-insensitive), longest first,
/// at most 20: what goes into the prompt.
pub fn relevant(text: &str) -> Vec<(String, String)> {
    filter(&load(), text)
}

fn filter(all: &[(String, String)], text: &str) -> Vec<(String, String)> {
    let lower = text.to_lowercase();
    let mut hits: Vec<(String, String)> = all
        .iter()
        .filter(|(term, _)| lower.contains(&term.to_lowercase()))
        .cloned()
        .collect();
    hits.sort_by_key(|(t, _)| std::cmp::Reverse(t.chars().count()));
    hits.truncate(20);
    hits
}

/// Opens the file in the default editor, creating it with a header
/// comment when it doesn't exist yet.
pub fn open() {
    let Some(p) = path() else { return };
    if !p.exists() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(&p, "# term<TAB>translation\n");
    }
    // .tsv often has no default app: fall back to Notepad.
    std::thread::spawn(move || unsafe {
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;
        use windows::core::{HSTRING, w};
        let file = HSTRING::from(p.as_os_str());
        let r = ShellExecuteW(None, w!("open"), &file, None, None, SW_SHOW);
        if r.0 as isize <= 32 {
            let arg = HSTRING::from(format!("\"{}\"", p.display()));
            ShellExecuteW(None, w!("open"), w!("notepad.exe"), &arg, None, SW_SHOW);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lenient_tsv() {
        let text = "\u{feff}# header\nrelease\tрелиз\n\nbranch\tветка\textra\tcols\nlonely\n \t \nmerge request\t мерж-реквест \n";
        let g = parse(text);
        assert_eq!(
            g,
            [
                ("release".to_owned(), "релиз".to_owned()),
                ("branch".to_owned(), "ветка".to_owned()),
                ("merge request".to_owned(), "мерж-реквест".to_owned()),
            ]
        );
        let hits = filter(&g, "Open a Merge Request for this branch");
        assert_eq!(hits.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["merge request", "branch"]);
    }
}
