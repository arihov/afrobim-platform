//! The Appearance dialog: the interface look (dark glass, light glass, solid) and its scale. A
//! change applies at once and is kept in the preferences file.

use crate::app::AfroBimApp;
use crate::theme::{Look, Theme, SCALES};
use crate::ui_kit::{self, Dialog, Footer, Level};

impl AfroBimApp {
    pub(crate) fn appearance_window(&mut self, ctx: &egui::Context) {
        if !self.appearance_open {
            return;
        }
        let mut theme: Theme = self.theme;
        let out = Dialog::new("Appearance", crate::glyphs::by_name("APPEARANCE")).subtitle("Look and scale of the interface").width(420.0).resizable(false).help("The look and the scale are kept for the next time AfroBIM starts. Drawing colours (layers, signs, results) do not change with the look.").show(
            ctx,
            |ui| {
                ui_kit::section(ui, "Look", |ui| {
                    ui_kit::form(ui, "look", |f| {
                        f.row("Interface", |ui| {
                            let mut i = Look::ALL.iter().position(|l| *l == theme.look).unwrap_or(0);
                            let labels: Vec<&str> = Look::ALL.iter().map(|l| l.label()).collect();
                            if ui_kit::segmented(ui, &mut i, &labels) {
                                theme.look = Look::ALL[i];
                            }
                        });
                        if theme.look == Look::Solid {
                            f.row("Solid palette", |ui| {
                                let mut i = if theme.solid_dark { 0 } else { 1 };
                                if ui_kit::segmented(ui, &mut i, &["Dark", "Light"]) {
                                    theme.solid_dark = i == 0;
                                }
                            });
                        }
                    });
                    ui.add_space(4.0);
                    ui_kit::callout(ui, Level::Info, "Glass is a translucent tint over the drawing, so the model shows faintly through a floating window. It is not a live blur: the renderer does not offer one. Solid turns the translucency off.");
                });
                ui_kit::section(ui, "Scale", |ui| {
                    ui_kit::form(ui, "scale", |f| {
                        f.row("Interface scale", |ui| {
                            let mut i = SCALES.iter().position(|s| (*s - theme.scale).abs() < 0.01).unwrap_or(1);
                            let labels: Vec<String> = SCALES.iter().map(|s| format!("{:.0}%", s * 100.0)).collect();
                            let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
                            if ui_kit::combo(ui, "scale", &mut i, &refs) {
                                theme.scale = SCALES[i];
                            }
                        });
                    });
                });
            },
            |ui| Footer::new("Done").cancel(None).note("Changes apply at once").show(ui),
        );
        if theme != self.theme {
            self.set_theme(ctx, theme);
        }
        if out.closed || out.inner.is_some_and(|(_, f)| f.primary || f.cancel) {
            self.appearance_open = false;
        }
    }
}
