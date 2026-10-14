//! What shortcuts, the tray and the hooks can do.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::windows;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    /// Translate the selection in the popup.
    TranslateSelection,
    Quick,
    Translator,
    Ultra,
    /// Translate the selection and copy it, no window.
    CopyTranslation,
    Screenshot,
    /// The capture overlay in «Окно» mode.
    WindowCapture,
    /// Translate the selection and paste it over, no window.
    PasteTranslation,
    Settings,
    History,
}

pub fn run(app: &AppHandle, action: Action) {
    tracing::debug!(?action, "action");
    match action {
        Action::Quick => windows::show_quick(app, None),
        Action::Translator => windows::show(app, windows::TRANSLATOR),
        Action::Settings => windows::show(app, windows::SETTINGS),
        Action::History => windows::show(app, windows::HISTORY),
        Action::TranslateSelection | Action::CopyTranslation | Action::PasteTranslation => {
            crate::selection::run(app, action)
        }
        Action::Ultra => windows::show(app, windows::ULTRA),
        Action::Screenshot => crate::capture::start(app, crate::capture::Mode::Area),
        Action::WindowCapture => crate::capture::start(app, crate::capture::Mode::Window),
    }
}
