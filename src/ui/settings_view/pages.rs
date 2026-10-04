//! The smaller pages: Context & modes (Figma 329:541: context for the
//! offline models and DeepL, programmer mode and the Ultra section) and About (brand kit 326:405 mark and
//! wordmark, same compact style).

use egui::Ui;

use super::*;

pub(super) fn modes(ui: &mut Ui, s: &mut Settings) -> Option<SettingsAction> {
    let mut action = None;
    page_title(
        ui,
        tr("Context & modes"),
        Some(tr("How to translate code and selections in several languages.")),
    );
    section(ui, tr("Context"), |ui| {
        let sub = tr("Gemma models and DeepL see the previous fragments and their translations");
        row(ui, tr("Translate with context"), Some(sub), 32.0, false, |ui| {
            changed(switch(ui, &mut s.use_context), &mut action);
        });
        let n = s.context_fragments.to_string();
        row(ui, tr("Fragments to remember"), None, 190.0, false, |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let (r, _) = ui.allocate_exact_size(vec2(24.0, 14.0), Sense::hover());
            let g = ui.painter().layout_no_wrap(n, font(11.5, Weight::SemiBold), TEXT);
            ui.painter().galley(pos2(r.right() - g.size().x, r.center().y - g.size().y / 2.0), g, TEXT);
            if s.use_context {
                let resp = slider(ui, &mut s.context_fragments, crate::settings::CONTEXT_FRAGMENTS_RANGE, 1, 130.0);
                if resp.drag_stopped() || resp.clicked() {
                    action = Some(SettingsAction::Changed);
                }
            } else {
                ui.add_enabled_ui(false, |ui| {
                    let mut v = s.context_fragments;
                    slider(ui, &mut v, crate::settings::CONTEXT_FRAGMENTS_RANGE, 1, 130.0);
                });
            }
        });
        let count = crate::offline::glossary::count();
        let sub = trf("{n} terms · term and translation separated by a tab", &[("n", &count.to_string())]);
        row(ui, tr("Glossary"), Some(&sub), 120.0, true, |ui| {
            if link_button(ui, None, tr("Edit glossary")).clicked() {
                crate::offline::glossary::open();
            }
        });
    });
    section(ui, tr("Code"), |ui| {
        let sub = tr("In code, translate only strings and comments");
        row(ui, tr("Programmer mode"), Some(sub), 32.0, true, |ui| {
            changed(switch(ui, &mut s.programmer_mode), &mut action);
        });
    });
    section(ui, tr("Ultra mode"), |ui| {
        let skip = trf("Leave fragments in {lang} as is", &[("lang", &lang_in(&s.target))]);
        let u = &mut s.ultra;
        let rows: [(&str, &str, &mut bool); 3] = [
            (tr("Detect the language of each fragment"), tr("One selection can mix several languages"), &mut u.detect_each),
            (&skip, tr("Text already in the target language stays as it is"), &mut u.skip_target),
            (tr("Show the Ultra window"), tr("Otherwise the result replaces the selected text right away"), &mut u.show_window),
        ];
        for (i, (title, sub, value)) in rows.into_iter().enumerate() {
            row(ui, title, Some(sub), 32.0, i == 2, |ui| {
                changed(switch(ui, value), &mut action);
            });
        }
    });
    action
}

pub(super) fn about(ui: &mut Ui) -> Option<SettingsAction> {
    page_title(ui, tr("About"), None);
    group(ui, |ui| {
        let w = ui.available_width();
        // The mark, the "HeliLingo" wordmark ("Heli" text, "Lingo" accent)
        // and the version.
        let (head, _) = ui.allocate_exact_size(vec2(w, 60.0), Sense::hover());
        let mark = Rect::from_min_size(pos2(head.left() + 12.0, head.center().y - 18.0), vec2(36.0, 36.0));
        Image::new(icons::SUN).paint_at(ui, mark);
        let mut job = egui::text::LayoutJob::default();
        let fmt = |color| egui::TextFormat { font_id: font(16.0, Weight::SemiBold), color, ..Default::default() };
        let (heli, lingo) = crate::APP_NAME.split_at(4);
        job.append(heli, 0.0, fmt(TEXT));
        job.append(lingo, 0.0, fmt(ACCENT));
        let name = ui.painter().layout_job(job);
        let version = ui.painter().layout_no_wrap(
            trf("Version {v}", &[("v", env!("CARGO_PKG_VERSION"))]),
            font(11.0, Weight::Regular),
            TEXT_3,
        );
        let h = name.size().y + 1.0 + version.size().y;
        let (x, y) = (mark.right() + 12.0, head.center().y - h / 2.0);
        let nh = name.size().y;
        ui.painter().galley(pos2(x, y), name, TEXT);
        ui.painter().galley(pos2(x, y + nh + 1.0), version, TEXT_3);
        ui.painter().hline(ui.min_rect().x_range(), ui.cursor().min.y - 0.5, Stroke::new(1.0, BORDER));
        Frame::new().inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(w - 24.0);
            ui.add(
                egui::Label::new(rt(
                    tr("Select text in any app, double-tap Ctrl and read the translation right next to it."),
                    12.0,
                    Weight::Regular,
                    TEXT_2,
                ))
                .wrap()
                .selectable(false),
            );
        });
    });

    // Developer and links: (title, link text shown, URL).
    section(ui, tr("Developer"), |ui| {
        let developer = trf("Developer: {name}", &[("name", "Helisy")]);
        let links = [
            (developer.as_str(), "t.me/helisy", "https://t.me/helisy"),
            (tr("Hub"), "t.me/helisy420", "https://t.me/helisy420"),
            ("GitHub", "github.com/Helisy420", "https://github.com/Helisy420"),
        ];
        let n = links.len();
        for (i, (title, label, url)) in links.into_iter().enumerate() {
            row(ui, title, None, 170.0, i + 1 == n, |ui| {
                if link_button(ui, Some(icons::EXTERNAL12), label).on_hover_text(url).clicked() {
                    crate::win::open_url(url);
                }
            });
        }
    });

    section(ui, tr("Privacy"), |ui| {
        row(
            ui,
            tr("Selected text is sent for translation and not stored"),
            Some(tr("Only your own translation history keeps it, if it is turned on in General")),
            0.0,
            false,
            |_| {},
        );
        row(
            ui,
            tr("API keys are encrypted with Windows DPAPI"),
            Some(tr("Only your Windows account on this PC can read them")),
            0.0,
            false,
            |_| {},
        );
        let dir = std::env::var("APPDATA")
            .map(|a| format!(r"{a}\HeliLingo"))
            .unwrap_or_else(|_| r"%APPDATA%\HeliLingo".into());
        let file = format!(r"{dir}\settings.json");
        row(ui, tr("Settings are stored in"), Some(&file), 100.0, true, |ui| {
            if link_button(ui, None, tr("Open folder")).clicked() {
                crate::win::open_url(&dir);
            }
        });
    });
    None
}
