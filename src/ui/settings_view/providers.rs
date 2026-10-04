//! Settings → Providers (Figma 329:166): the ordered provider list with
//! drag-to-reorder, status badges, enable switches and, for the expanded
//! row, its API key fields.

use egui::{CursorIcon, Ui};

use super::*;

/// Where each provider hands out keys.
fn key_url(kind: ProviderKind) -> Option<&'static str> {
    match kind {
        ProviderKind::Google => Some("https://console.cloud.google.com/apis/credentials"),
        ProviderKind::Yandex => Some("https://yandex.cloud/docs/translate/operations/sa-api-key"),
        ProviderKind::Bing => Some("https://portal.azure.com/#create/Microsoft.CognitiveServicesTextTranslation"),
        ProviderKind::DeepL => Some("https://www.deepl.com/your-account/keys"),
        _ => None,
    }
}

fn name(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Google => tr("Google Translate"),
        ProviderKind::Yandex => tr("Yandex Translate"),
        ProviderKind::Bing => "Bing (Microsoft)",
        ProviderKind::DeepL => "DeepL",
        ProviderKind::Argos => "Argos Translate",
        other => other.short_name(),
    }
}

/// The key a provider row reports in its subtitle, if one is set.
fn key_hint(kind: ProviderKind, s: &Settings) -> Option<String> {
    let k = &s.keys;
    match kind {
        ProviderKind::Google => k.google.is_set().then(|| k.google.hint()),
        ProviderKind::Yandex => k.yandex.is_set().then(|| k.yandex.hint()),
        ProviderKind::Bing => k.azure.is_set().then(|| k.azure.hint()),
        ProviderKind::DeepL => k.deepl_key().map(|key| crate::settings::Secret::new(key).hint()),
        _ => None,
    }
}

fn subtitle(kind: ProviderKind, s: &Settings) -> String {
    if kind.is_offline() {
        return if offline::installed(s, kind) { tr("offline · installed") } else { tr("offline · not installed") }.to_owned();
    }
    match key_hint(kind, s) {
        Some(hint) => format!("{} · {hint}", tr("API key set")),
        None if kind == ProviderKind::Bing => tr("Azure Translator key required").to_owned(),
        None if kind == ProviderKind::DeepL => tr("API key required").to_owned(),
        None => tr("no key needed").to_owned(),
    }
}

/// Takes part in the chain: a key when one is needed, installed for an
/// offline tier, and not an online one in "offline only" mode.
fn usable(kind: ProviderKind, s: &Settings) -> bool {
    if kind.is_offline() {
        offline::installed(s, kind)
    } else {
        !s.offline_only && s.keys.usable(kind)
    }
}

/// Status badge per provider, in list order: Primary for the first enabled
/// usable one, Last fallback for the last, Fallback in between; "No key"
/// when a needed key is missing, "Not installed" for Argos.
fn badges(entries: &[ProviderEntry], s: &Settings) -> Vec<Option<(&'static str, Color32, Color32, bool)>> {
    let usable: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.enabled && usable(e.kind, s))
        .map(|(i, _)| i)
        .collect();
    entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            if e.kind.is_offline() && !offline::installed(s, e.kind) {
                Some((tr("Not installed"), BADGE_MUTED.0, BADGE_MUTED.1, false))
            } else if s.offline_only && !e.kind.is_offline() {
                Some((tr("Offline only"), BADGE_MUTED.0, BADGE_MUTED.1, false))
            } else if !s.keys.usable(e.kind) {
                Some((tr("No key"), WARN, WARN_BG, true))
            } else if usable.first() == Some(&i) {
                Some((tr("Primary"), ACCENT, SELECTED, false))
            } else if usable.last() == Some(&i) {
                Some((tr("Last fallback"), BADGE_MUTED.0, BADGE_MUTED.1, false))
            } else if usable.contains(&i) {
                Some((tr("Fallback"), BADGE_MUTED.0, BADGE_MUTED.1, false))
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn show(ui: &mut Ui, s: &mut Settings, state: &mut SettingsState) -> Option<SettingsAction> {
    let mut action = None;
    page_title(
        ui,
        tr("Translation providers"),
        Some(tr("Translation goes top to bottom. If a provider doesn't answer or has no key, the next one is used.")),
    );

    let entries = state.drag_order.clone().unwrap_or_else(|| s.providers.clone());
    let badges = badges(&entries, s);
    let mut rows: Vec<Rect> = Vec::new();
    let mut grip_released = false;

    group(ui, |ui| {
        let n = entries.len();
        for (i, entry) in entries.iter().enumerate() {
            let kind = entry.kind;
            let top = ui.cursor().min.y;
            let header = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 38.0));
            let dragged = state.drag == Some(kind);
            let row_bg = ui.painter().add(egui::Shape::Noop);
            let click = ui.interact(header, Id::new(("qt-provider-row", kind)), Sense::click());
            ui.allocate_rect(header, Sense::hover());

            // Grip (drag to reorder) and number.
            let grip = Rect::from_min_size(pos2(header.left() + 10.0, header.center().y - 7.0), vec2(14.0, 14.0));
            let grip_resp = ui
                .interact(grip.expand(5.0), Id::new(("qt-provider-grip", kind)), Sense::drag())
                .on_hover_cursor(if dragged { CursorIcon::Grabbing } else { CursorIcon::Grab });
            Image::new(icons::GRIP).paint_at(ui, grip);
            if grip_resp.drag_started() {
                state.drag = Some(kind);
                state.drag_order = Some(s.providers.clone());
                state.open = None;
            }
            if grip_resp.drag_stopped() {
                grip_released = true;
            }
            let num = ui.painter().layout_no_wrap((i + 1).to_string(), font(11.0, Weight::Medium), TEXT_3);
            let num_x = grip.right() + 10.0;
            let num_w = num.size().x;
            ui.painter().galley(pos2(num_x, header.center().y - num.size().y / 2.0), num, TEXT_3);

            // Switch and badge on the right.
            let mut right = ui.new_child(
                UiBuilder::new()
                    .max_rect(Rect::from_min_max(header.min, header.max - vec2(12.0, 0.0)))
                    .layout(Layout::right_to_left(Align::Center)),
            );
            right.spacing_mut().item_spacing.x = 10.0;
            let mut on = entry.enabled;
            if switch(&mut right, &mut on).clicked() {
                if let Some(e) = s.providers.iter_mut().find(|e| e.kind == kind) {
                    e.enabled = on;
                }
                action = Some(SettingsAction::Changed);
            }
            if let Some((label, fg, bg, dot)) = badges[i] {
                badge(&mut right, label, fg, bg, dot.then_some(icons::DOT_WARN));
            }
            let text_right = right.min_rect().left() - 10.0;

            // Name and subtitle.
            let col_x = num_x + num_w + 10.0;
            let w = (text_right - col_x).max(40.0);
            let title = truncated(ui, name(kind), font(12.5, Weight::Medium), TEXT, w);
            let sub = truncated(ui, &subtitle(kind, s), font(10.5, Weight::Regular), TEXT_3, w);
            let h = title.size().y + 1.0 + sub.size().y;
            let y = header.center().y - h / 2.0;
            let th = title.size().y;
            ui.painter().galley(pos2(col_x, y), title, TEXT);
            ui.painter().galley(pos2(col_x, y + th + 1.0), sub, TEXT_3);

            if click.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                state.expanded = if state.expanded == Some(kind) { None } else { Some(kind) };
                state.open = None;
            }

            if state.expanded == Some(kind) {
                Frame::new()
                    .inner_margin(Margin { left: 46, right: 12, top: 0, bottom: 12 })
                    .show(ui, |ui| {
                        ui.set_width(header.width() - 58.0);
                        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                        if let Some(a) = key_panel(ui, kind, s, state) {
                            action = Some(a);
                        }
                    });
            }

            let row_rect = Rect::from_x_y_ranges(header.x_range(), top..=ui.cursor().min.y);
            if dragged {
                ui.painter().set(row_bg, egui::Shape::rect_filled(row_rect, 0, HOVER));
            }
            if i + 1 < n {
                ui.painter().hline(row_rect.x_range(), row_rect.bottom() - 0.5, Stroke::new(1.0, BORDER));
            }
            rows.push(row_rect);
        }
    });

    // Live reorder: swap with a neighbour once the pointer passes its middle.
    if let (Some(kind), Some(order)) = (state.drag, state.drag_order.as_mut()) {
        let pointer = ui.ctx().pointer_interact_pos().map(|p| p.y);
        if let (Some(y), Some(j)) = (pointer, order.iter().position(|e| e.kind == kind)) {
            if j > 0 && y < rows[j - 1].center().y {
                order.swap(j, j - 1);
            } else if j + 1 < order.len() && y > rows[j + 1].center().y {
                order.swap(j, j + 1);
            }
        }
        let released = grip_released || !ui.ctx().input(|i| i.pointer.primary_down());
        if released {
            let order = state.drag_order.take().unwrap_or_default();
            state.drag = None;
            if order.iter().map(|e| e.kind).ne(s.providers.iter().map(|e| e.kind)) {
                // Keep the enabled flags from the live settings.
                let enabled = |k| s.providers.iter().find(|e| e.kind == k).is_some_and(|e| e.enabled);
                s.providers = order.iter().map(|e| ProviderEntry { kind: e.kind, enabled: enabled(e.kind) }).collect();
                action = Some(SettingsAction::Changed);
            }
        }
    }

    group(ui, |ui| {
        let sub = tr("Use only the installed offline models; nothing leaves this PC");
        row(ui, tr("Offline only"), Some(sub), 32.0, true, |ui| {
            changed(switch(ui, &mut s.offline_only), &mut action);
        });
    });
    footnote_with(ui, icons::LOCK12, tr("Drag to change the order. Keys are kept in the protected Windows storage."));
    action
}

/// The expanded part of a provider row: key fields, check, link, notes
/// (DeepL as in the Figma frame: key + Check, then plan and link).
fn key_panel(ui: &mut Ui, kind: ProviderKind, s: &mut Settings, state: &mut SettingsState) -> Option<SettingsAction> {
    let mut action = None;
    match kind {
        ProviderKind::Google => {
            let hint = tr("Cloud Translation API key (optional)");
            key_row(ui, kind, s.keys.google.text_mut(), hint, state, &mut action);
            google_mode(ui, s, &mut action);
            link_row(ui, kind);
            note(ui, tr("Without a key the free public Google endpoint is used."));
        }
        ProviderKind::Yandex => {
            key_row(ui, kind, s.keys.yandex.text_mut(), tr("Paste the Yandex Cloud API key"), state, &mut action);
            labeled_field(ui, tr("Folder ID"), &mut s.keys.yandex_folder, "b1g…", kind, state, &mut action);
            link_row(ui, kind);
            note(ui, tr("The folder ID is needed for user API keys; service account keys work without it."));
        }
        ProviderKind::Bing => {
            key_row(ui, kind, s.keys.azure.text_mut(), tr("Paste the Azure Translator key"), state, &mut action);
            labeled_field(ui, tr("Region"), &mut s.keys.azure_region, tr("e.g. westeurope"), kind, state, &mut action);
            link_row(ui, kind);
        }
        ProviderKind::DeepL => {
            key_row(ui, kind, s.keys.deepl.text_mut(), tr("Paste the DeepL API key"), state, &mut action);
            // Plan follows from the key (Free keys end in ":fx"); not a choice.
            let plan = match s.keys.deepl_key() {
                Some(k) if k.ends_with(":fx") => 0,
                Some(_) => 1,
                None => usize::MAX,
            };
            let w = ui.available_width();
            ui.allocate_ui_with_layout(vec2(w, 24.0), Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                text(ui, tr("Plan"), 11.0, Weight::Medium, TEXT_2);
                segmented_sm(ui, &["Free", "Pro"], plan, &[false, false], tr("Follows from the key"));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| link(ui, kind));
            });
            note(ui, tr("Free — up to 500,000 characters a month. The key is stored encrypted by Windows."));
        }
        _ => {
            note(ui, tr("Offline model: install it in Settings → Offline & acceleration."));
        }
    }
    action
}

/// Google mode: Auto / API only / Web, with its caption.
fn google_mode(ui: &mut Ui, s: &mut Settings, action: &mut Option<SettingsAction>) {
    use crate::settings::GoogleMode;
    const MODES: [GoogleMode; 3] = [GoogleMode::Auto, GoogleMode::Api, GoogleMode::Web];
    let w = ui.available_width();
    ui.allocate_ui_with_layout(vec2(w, 24.0), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        text(ui, tr("Google mode"), 11.0, Weight::Medium, TEXT_2);
        let active = MODES.iter().position(|m| *m == s.google_mode).unwrap_or(0);
        if let Some(i) = segmented_sm(ui, &[tr("Auto"), tr("API only"), tr("Web")], active, &[], "") {
            s.google_mode = MODES[i];
            *action = Some(SettingsAction::Changed);
        }
    });
    note(ui, tr("If Google answers “busy”, HeliLingo switches to other Google web addresses."));
}

/// A secondary plain field (folder ID, region) with its label on the left.
fn labeled_field(
    ui: &mut Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    kind: ProviderKind,
    state: &mut SettingsState,
    action: &mut Option<SettingsAction>,
) {
    let w = ui.available_width();
    ui.allocate_ui_with_layout(vec2(w, 30.0), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let g = ui.painter().layout_no_wrap(label.to_owned(), font(11.0, Weight::Medium), TEXT_2);
        let label_w = g.size().x.max(80.0);
        let (r, _) = ui.allocate_exact_size(vec2(label_w, 30.0), Sense::hover());
        ui.painter().galley(pos2(r.left(), r.center().y - g.size().y / 2.0), g, TEXT_2);
        if key_field(ui, value, hint, w - label_w - 10.0, false, false).changed() {
            state.checks.remove(&kind);
            *action = Some(SettingsAction::Changed);
        }
    });
}

/// Key input (lock icon, masked) + "Check", then the check result.
fn key_row(
    ui: &mut Ui,
    kind: ProviderKind,
    key: &mut String,
    hint: &str,
    state: &mut SettingsState,
    action: &mut Option<SettingsAction>,
) {
    let check = tr("Check");
    let btn_w = ui.painter().layout_no_wrap(check.to_owned(), font(11.5, Weight::Medium), TEXT).size().x + 20.0;
    let w = ui.available_width();
    ui.allocate_ui_with_layout(vec2(w, 30.0), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if key_field(ui, key, hint, w - btn_w - 8.0, true, true).changed() {
            state.checks.remove(&kind);
            *action = Some(SettingsAction::Changed);
        }
        let running = state.checks.get(&kind) == Some(&CheckStatus::Running);
        if outline_button(ui, check, 30.0).clicked() && !running {
            state.checks.insert(kind, CheckStatus::Running);
            *action = Some(SettingsAction::Check(kind));
        }
    });
    match state.checks.get(&kind) {
        None => {}
        Some(CheckStatus::Running) => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.add(egui::Spinner::new().size(11.0).color(ACCENT));
                text(ui, tr("Checking…"), 11.0, Weight::Medium, TEXT_2);
            });
        }
        Some(CheckStatus::Ok) => {
            badge(ui, tr("Works"), OK, OK_BG, Some(icons::DOT_OK));
        }
        Some(CheckStatus::Failed(e)) => {
            ui.add(egui::Label::new(rt(e.as_str(), 11.0, Weight::Medium, WARN)).wrap().selectable(false));
        }
    }
}

/// "Where to get a key" (opens the provider's console in the browser).
fn link(ui: &mut Ui, kind: ProviderKind) {
    if let Some(url) = key_url(kind)
        && link_button(ui, Some(icons::EXTERNAL12), tr("Where to get a key"))
            .on_hover_text(url)
            .clicked()
    {
        crate::win::open_url(url);
    }
}

fn link_row(ui: &mut Ui, kind: ProviderKind) {
    ui.horizontal(|ui| link(ui, kind));
}

fn note(ui: &mut Ui, t: &str) {
    ui.add(egui::Label::new(rt(t, 10.5, Weight::Regular, TEXT_3)).wrap().selectable(false));
}
