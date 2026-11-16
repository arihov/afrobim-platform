//! The command palette (Ctrl+K): a search box over every command, showing icon, name, aliases and
//! help. Up and Down move, Enter runs, Escape or a click outside closes. A command of another
//! workspace runs like any other: the palette lists everything.

use egui::{vec2, Align2, Id, Key, Order, RichText, Sense};

use crate::app::AfroBimApp;
use crate::commands::{CommandInfo, COMMANDS};
use crate::theme;

#[derive(Default)]
pub struct PaletteState {
    pub query: String,
    pub selected: usize,
    pub was_open: bool,
}

/// How well a command matches the words of a query: every word must match somewhere, and the
/// score is the sum of the best match of each word (name first, then alias, then help text).
pub fn score(query: &str, c: &CommandInfo) -> Option<u32> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Some(0);
    }
    let name = c.name.to_lowercase();
    let aliases: Vec<String> = c.aliases.iter().map(|a| a.to_lowercase()).collect();
    let help = c.help.to_lowercase();
    let mut total = 0;
    for w in &words {
        let s = if name == *w {
            1000
        } else if aliases.iter().any(|a| a == w) {
            900
        } else if name.starts_with(w.as_str()) {
            800
        } else if aliases.iter().any(|a| a.starts_with(w.as_str())) {
            700
        } else if name.contains(w.as_str()) {
            500
        } else if aliases.iter().any(|a| a.contains(w.as_str())) {
            400
        } else if help.contains(w.as_str()) {
            100
        } else {
            return None;
        };
        total += s;
    }
    Some(total)
}

/// The commands matching a query, best first (all of them, in table order, for an empty query).
pub fn search(query: &str) -> Vec<&'static CommandInfo> {
    let mut found: Vec<(u32, usize, &'static CommandInfo)> = COMMANDS.iter().enumerate().filter_map(|(i, c)| score(query, c).map(|s| (s, i, c))).collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    found.into_iter().map(|(_, _, c)| c).collect()
}

impl AfroBimApp {
    pub(crate) fn palette_window(&mut self, ctx: &egui::Context) {
        if !self.palette_open {
            self.palette.was_open = false;
            return;
        }
        let t = theme::tokens(ctx);
        let first = !self.palette.was_open;
        if first {
            self.palette.query.clear();
            self.palette.selected = 0;
            self.palette.was_open = true;
        }
        let results = search(&self.palette.query);
        let n = results.len();
        let (down, up, enter, esc) = ctx.input_mut(|i| (i.consume_key(egui::Modifiers::NONE, Key::ArrowDown), i.consume_key(egui::Modifiers::NONE, Key::ArrowUp), i.consume_key(egui::Modifiers::NONE, Key::Enter), i.consume_key(egui::Modifiers::NONE, Key::Escape)));
        if down && n > 0 {
            self.palette.selected = (self.palette.selected + 1) % n;
        }
        if up && n > 0 {
            self.palette.selected = (self.palette.selected + n - 1) % n;
        }
        self.palette.selected = self.palette.selected.min(n.saturating_sub(1));
        let mut run = None;
        let moved = down || up;
        let area = egui::Area::new(Id::new("afrobim-palette")).order(Order::Foreground).anchor(Align2::CENTER_TOP, vec2(0.0, 80.0)).show(ctx, |ui| {
            theme::window_frame(ctx).show(ui, |ui| {
                ui.set_width(640.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(crate::glyphs::ui::SEARCH).font(theme::icon_font(18.0)).color(t.text_muted));
                    let r = ui.add(egui::TextEdit::singleline(&mut self.palette.query).hint_text("Search commands by name, alias or description").desired_width(f32::INFINITY).font(egui::TextStyle::Heading).frame(false));
                    if first {
                        r.request_focus();
                    } else if !r.has_focus() {
                        r.request_focus();
                    }
                    if r.changed() {
                        self.palette.selected = 0;
                    }
                });
                let line = ui.available_rect_before_wrap();
                ui.painter().hline(line.x_range(), ui.cursor().top() + 2.0, egui::Stroke::new(1.0, t.border));
                ui.add_space(6.0);
                if n == 0 {
                    ui.label(RichText::new("No command matches.").color(t.text_muted));
                }
                egui::ScrollArea::vertical().max_height(420.0).auto_shrink([false, true]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    for (i, c) in results.iter().take(80).enumerate() {
                        let on = i == self.palette.selected;
                        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                        if on {
                            ui.painter().rect_filled(rect, theme::RADIUS_CONTROL as f32, t.selection);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(rect, theme::RADIUS_CONTROL as f32, t.raised_hover);
                        }
                        let p = ui.painter();
                        p.text(egui::pos2(rect.left() + 18.0, rect.center().y), Align2::CENTER_CENTER, crate::glyphs::of(c), theme::icon_font(17.0), if on { t.text } else { t.accent_text });
                        let name_galley = p.layout_no_wrap(c.name.to_string(), egui::FontId::monospace(13.0), t.text);
                        let name_w = name_galley.size().x;
                        p.galley(egui::pos2(rect.left() + 38.0, rect.center().y - name_galley.size().y / 2.0), name_galley, t.text);
                        let mut x = rect.left() + 38.0 + name_w + 10.0;
                        if !c.aliases.is_empty() {
                            let a = p.layout_no_wrap(c.aliases.join(" "), egui::FontId::monospace(11.5), t.text_muted);
                            let w = a.size().x;
                            p.galley(egui::pos2(x, rect.center().y - a.size().y / 2.0), a, t.text_muted);
                            x += w + 12.0;
                        }
                        let keys = crate::ribbon::shortcut(c.action);
                        let right = if keys.is_empty() { rect.right() - 8.0 } else { rect.right() - 8.0 - 76.0 };
                        if !keys.is_empty() {
                            p.text(egui::pos2(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, keys, egui::FontId::monospace(11.5), t.text_muted);
                        }
                        // The help, cut to what is left of the row.
                        let room = (right - x).max(0.0);
                        let help = p.layout(c.help.to_string(), egui::FontId::proportional(12.0), t.text_muted, f32::INFINITY);
                        if room > 40.0 {
                            let text = crate::statusbar::fit_message(c.help, room, &|s| p.layout_no_wrap(s.to_string(), egui::FontId::proportional(12.0), t.text_muted).size().x);
                            p.text(egui::pos2(x, rect.center().y), Align2::LEFT_CENTER, text, egui::FontId::proportional(12.0), t.text_muted);
                        }
                        let _ = help;
                        if on && moved {
                            resp.scroll_to_me(None);
                        }
                        if resp.clicked() {
                            run = Some(*c);
                        }
                    }
                });
                ui.add_space(4.0);
                ui.label(RichText::new("Up and Down to choose  ·  Enter to run  ·  Esc to close").size(theme::FONT_SMALL).color(t.text_muted));
            });
        });
        if enter {
            run = results.get(self.palette.selected).copied().or(run);
        }
        let clicked_outside = !first && ctx.input(|i| i.pointer.any_pressed()) && ctx.pointer_interact_pos().is_some_and(|p| !area.response.rect.contains(p));
        if esc || clicked_outside {
            self.palette_open = false;
        }
        if let Some(c) = run {
            self.palette_open = false;
            self.run(ctx, c.action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exact_name_or_alias_ranks_first() {
        assert_eq!(search("layer")[0].name, "LAYER");
        assert_eq!(search("la")[0].name, "LAYER");
        assert_eq!(search("move")[0].name, "MOVE");
        assert_eq!(search("boq")[0].name, "QUANTITIES");
    }

    #[test]
    fn every_word_must_match_and_the_help_text_is_searched() {
        let r = search("survey points");
        assert!(r.iter().any(|c| c.name == "SURVEY"));
        assert!(search("zzzzqqq").is_empty());
        // A word found only in the help text still finds the command, below name matches.
        assert!(search("critical path").iter().any(|c| c.name == "SCHEDULE"));
    }

    #[test]
    fn an_empty_query_lists_every_command_in_table_order() {
        let all = search("");
        assert_eq!(all.len(), COMMANDS.len());
        assert_eq!(all[0].name, COMMANDS[0].name);
    }
}
