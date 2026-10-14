//! The Windows file dialogs and Explorer, for «Перевод файлов»: choose
//! files, save a translated one, show a file in its folder. The dialogs
//! block; call them on their own thread.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT, FileOpenDialog, FileSaveDialog,
    IFileDialog, IFileOpenDialog, IFileSaveDialog, IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
};
use windows::core::{HSTRING, PCWSTR};

fn path_of(item: &IShellItem) -> Option<PathBuf> {
    unsafe {
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from)
    }
}

fn start_in(dialog: &IFileDialog, dir: &Path) {
    let mut p = dir;
    while !p.exists() {
        match p.parent() {
            Some(parent) => p = parent,
            None => return,
        }
    }
    unsafe {
        if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(p.as_os_str()), None) {
            let _ = dialog.SetFolder(&item);
        }
    }
}

/// «Выбрать файлы»: several files of the given patterns ("*.docx;*.pdf").
pub fn open_files(title: &str, filter_name: &str, patterns: &str) -> Vec<PathBuf> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        let Ok(dialog) = CoCreateInstance::<_, IFileOpenDialog>(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) else {
            return Vec::new();
        };
        let Ok(options) = dialog.GetOptions() else { return Vec::new() };
        let _ = dialog.SetOptions(options | FOS_ALLOWMULTISELECT | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM);
        let _ = dialog.SetTitle(&HSTRING::from(title));
        let (name, spec) = (HSTRING::from(filter_name), HSTRING::from(patterns));
        let filters = [COMDLG_FILTERSPEC { pszName: PCWSTR(name.as_ptr()), pszSpec: PCWSTR(spec.as_ptr()) }];
        let _ = dialog.SetFileTypes(&filters);
        if dialog.Show(None).is_err() {
            return Vec::new();
        }
        let Ok(items) = dialog.GetResults() else { return Vec::new() };
        let n = items.GetCount().unwrap_or(0);
        (0..n).filter_map(|i| items.GetItemAt(i).ok().and_then(|it| path_of(&it))).collect()
    }
}

/// «Скачать»: where to save `name`, starting in `dir`. `None` = cancelled.
pub fn save_file(title: &str, dir: &Path, name: &str) -> Option<PathBuf> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog.SetOptions(options | FOS_OVERWRITEPROMPT | FOS_FORCEFILESYSTEM).ok()?;
        dialog.SetTitle(&HSTRING::from(title)).ok()?;
        dialog.SetFileName(&HSTRING::from(name)).ok()?;
        if let Some(ext) = Path::new(name).extension().and_then(|e| e.to_str()) {
            let _ = dialog.SetDefaultExtension(&HSTRING::from(ext));
        }
        start_in(&dialog, dir);
        dialog.Show(None).ok()?;
        path_of(&dialog.GetResult().ok()?)
    }
}

/// Opens Explorer with `path` selected.
pub fn reveal(path: &Path) {
    let arg = format!("/select,\"{}\"", path.display());
    let _ = std::process::Command::new("explorer.exe").raw_arg(arg).spawn();
}
