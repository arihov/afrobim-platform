// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The About > Licences window: the end-user licence, the project licence, the notices for
//! third-party work and the generated list of Rust dependency licences. The texts are compiled in
//! from the files at the repository root, so they cannot be missing from a running build.

use eframe::egui::{self, RichText};

use crate::ui_kit;

/// The end-user licence agreement.
pub const EULA: &str = include_str!("../../../../EULA.md");
/// The proprietary licence of AfroBIM's own code.
pub const LICENCE: &str = include_str!("../../../../LICENSE");
/// Notices for the work AfroBIM ports or bundles (acadrust, ACadSharp, BIMCraft, fonts, tiles).
pub const NOTICE: &str = include_str!("../../../../NOTICE");
/// Every Rust dependency with its SPDX licence and licence text (generated).
pub const THIRD_PARTY: &str = include_str!("../../../../THIRD-PARTY-NOTICES.md");

const TABS: [&str; 4] = ["EULA", "Notices", "Third-party licences", "Source licence"];

/// The state of the window: whether it is open and which text is shown.
#[derive(Default)]
pub struct Licences {
    pub open: bool,
    tab: usize,
    /// The lines of the long third-party text, split once.
    lines: Vec<&'static str>,
}

fn text_of(tab: usize) -> &'static str {
    match tab {
        0 => EULA,
        1 => NOTICE,
        2 => THIRD_PARTY,
        _ => LICENCE,
    }
}

impl Licences {
    /// Draws the window when it is open.
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        if self.lines.is_empty() {
            self.lines = THIRD_PARTY.lines().collect();
        }
        let mut tab = self.tab;
        let lines = &self.lines;
        let out = ui_kit::Dialog::titled("Licences").subtitle("Licence agreement and notices for this build").width(720.0).show_body(ctx, |ui| {
            ui_kit::tabs(ui, &mut tab, &TABS);
            ui.add_space(6.0);
            if tab == 2 {
                // About 18,000 lines: draw only the rows in view.
                let row = ui.text_style_height(&egui::TextStyle::Monospace);
                egui::ScrollArea::both().id_salt("licences-third-party").max_height(420.0).auto_shrink([false, false]).show_rows(ui, row, lines.len(), |ui, range| {
                    for line in &lines[range] {
                        ui.add(egui::Label::new(RichText::new(*line).monospace()).wrap_mode(egui::TextWrapMode::Extend));
                    }
                });
            } else {
                egui::ScrollArea::vertical().id_salt(("licences-text", tab)).max_height(420.0).auto_shrink([false, false]).show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(text_of(tab)).monospace().size(12.0)).wrap());
                });
            }
        });
        self.tab = tab;
        if out.closed {
            self.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_texts_are_compiled_in() {
        assert!(EULA.contains("Draft for review by a Ugandan advocate"));
        assert!(EULA.contains("Uganda"));
        assert!(LICENCE.contains("All rights reserved"));
        assert!(NOTICE.contains("acadrust"));
        assert!(NOTICE.contains("ACadSharp"));
        assert!(NOTICE.contains("BIMCraft"));
        assert!(THIRD_PARTY.contains("Third-party notices"));
        assert!(THIRD_PARTY.lines().count() > 1000);
    }

    #[test]
    fn every_tab_has_text() {
        for t in 0..TABS.len() {
            assert!(!text_of(t).trim().is_empty(), "tab {t}");
        }
    }

    #[test]
    fn the_window_draws_headless() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        crate::theme::apply(&ctx, &crate::theme::Theme::default());
        let mut l = Licences { open: true, ..Default::default() };
        for tab in 0..TABS.len() {
            l.tab = tab;
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))), ..Default::default() };
            // The first pass sets the fonts up; the window is drawn on the second.
            for _ in 0..2 {
                let _ = ctx.run(input.clone(), |ctx| l.show(ctx));
            }
        }
        assert!(l.open);
    }
}
