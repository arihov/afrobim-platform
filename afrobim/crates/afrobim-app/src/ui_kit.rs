#![allow(dead_code)] // the kit is the public API of the dialogs: not every piece is used yet
//! The dialog kit: shared building blocks so that every window looks and behaves the same.
//!
//! A dialog is `Dialog::new(title, icon).subtitle(..).help(..).width(..).show(ctx, body, footer)`:
//! the glass frame, a header with icon, title, subtitle and a help toggle, a scrolling body and a
//! pinned footer. The body is built from the pieces below:
//!
//! * layout: `section`, `form` (an aligned two-column grid of labelled rows), `advanced`, `tabs`
//! * inputs: `num_field` (unit, validation, message), `text_field`, `combo`, `radio_group`,
//!   `segmented`, `checkbox`, `toggle`, `slider_entry`, `f64_field`
//! * feedback: `callout` (info, warning, error, success), `results_table`
//! * the footer: `Footer` (primary button in the accent colour, secondary buttons, Cancel, Enter
//!   and Escape, and a disabled primary with the reason shown)
//!
//! Colours, radii and spacing come from `theme`, so the kit follows the look the engineer chose.

use egui::{vec2, Align, Color32, Context, FontId, Id, Key, Layout, Order, Response, RichText, Sense, Stroke, Ui, WidgetText};

use crate::theme::{self, icon_font, tokens_ui, Tokens};

// ─── the window ──────────────────────────────────────────────────────────────

/// What `Dialog::show` returns.
pub struct DialogOut<R> {
    /// What the body and the footer returned, when the window was drawn.
    pub inner: Option<R>,
    /// The close button in the header was pressed.
    pub closed: bool,
}

pub struct Dialog {
    title: String,
    /// The text the window's id is made from, when it is not the title (a title with a file name).
    id_title: Option<String>,
    icon: &'static str,
    subtitle: Option<String>,
    help: Option<String>,
    width: f32,
    min_width: f32,
    resizable: bool,
    max_body: Option<f32>,
    pos: Option<egui::Pos2>,
}

impl Dialog {
    pub fn new(title: impl Into<String>, icon: &'static str) -> Self {
        Dialog { title: title.into(), id_title: None, icon, subtitle: None, help: None, width: 460.0, min_width: 340.0, resizable: true, max_body: None, pos: None }
    }
    /// A dialog whose icon is chosen from its title (the same table the window frame uses).
    pub fn titled(title: impl Into<String>) -> Self {
        let title: String = title.into();
        let icon = crate::win::icon_for(&title);
        Dialog::new(title, icon)
    }
    /// The window's identity (and remembered position) when it differs from the title.
    pub fn id_title(mut self, t: impl Into<String>) -> Self {
        self.id_title = Some(t.into());
        self
    }
    pub fn subtitle(mut self, s: impl Into<String>) -> Self {
        self.subtitle = Some(s.into());
        self
    }
    /// Help text shown under the header when the "?" button is on.
    pub fn help(mut self, s: impl Into<String>) -> Self {
        self.help = Some(s.into());
        self
    }
    /// The starting width; the minimum is the smaller of this and 340.
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self.min_width = w.min(340.0);
        self
    }
    /// Where the window opens when it has no remembered position.
    pub fn default_pos(mut self, p: egui::Pos2) -> Self {
        self.pos = Some(p);
        self
    }
    pub fn resizable(mut self, r: bool) -> Self {
        self.resizable = r;
        self
    }
    /// Caps the scrolling body at this height (otherwise as tall as the work area allows).
    pub fn max_body_height(mut self, h: f32) -> Self {
        self.max_body = Some(h);
        self
    }

    /// Shows the dialog. `body` scrolls when it is taller than the area; `footer` is pinned below it.
    pub fn show<R, F>(self, ctx: &Context, body: impl FnOnce(&mut Ui) -> R, footer: impl FnOnce(&mut Ui) -> F) -> DialogOut<(R, F)> {
        let work = crate::win::work_rect(ctx);
        // Whatever a dialog asks for, the body is never taller than the work area leaves room for, so
        // that the footer stays in view and the body scrolls inside.
        let room = (work.height() - 160.0).max(160.0);
        let body_cap = self.max_body.map_or(room, |m| m.min(room));
        let mut closed = false;
        let key = self.id_title.clone().unwrap_or_else(|| self.title.clone());
        let id = Id::new(&key).with("kit-help");
        let mut window = crate::win::plain(ctx, &key);
        if let Some(p) = self.pos {
            window = window.default_pos(p);
        }
        let out = window
            .title_bar(false)
            .collapsible(false)
            .resizable(self.resizable)
            .default_width(self.width)
            .min_width(self.min_width)
            .show(ctx, |ui| {
                let t = tokens_ui(ui);
                let mut help_on = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
                let header = header(ui, &t, self.icon, &self.title, self.subtitle.as_deref(), self.help.is_some(), &mut help_on);
                closed = header;
                ui.data_mut(|d| d.insert_temp(id, help_on));
                if help_on {
                    if let Some(h) = &self.help {
                        callout(ui, Level::Info, h);
                    }
                }
                ui.add_space(theme::SP_XS);
                // A window wider than the work area scrolls sideways as well, so that nothing is out of reach.
                let scroll = if self.width > crate::win::area_rect(ctx).width() - 8.0 { egui::ScrollArea::both() } else { egui::ScrollArea::vertical() };
                let r = scroll.id_salt("kit-body").max_height(body_cap).auto_shrink([false, true]).show(ui, |ui| egui::Frame::NONE.inner_margin(egui::Margin { left: 0, right: 12, top: 0, bottom: 0 }).show(ui, |ui| body(ui)).inner).inner;
                ui.add_space(theme::SP_SM);
                let f = footer(ui);
                (r, f)
            });
        DialogOut { inner: out.and_then(|o| o.inner), closed }
    }

    /// A window whose fields apply as they are typed: the body, and a pinned footer with the given
    /// secondary buttons and one primary "Close" (Enter in a kit field and Escape press it).
    /// `closed` is set when the header's close button or the footer's Close was pressed; the second
    /// value is the index of the secondary button pressed.
    pub fn show_close<R>(self, ctx: &Context, body: impl FnOnce(&mut Ui) -> R, secondary: &[&str]) -> DialogOut<(R, Option<usize>)> {
        let out = self.show(ctx, body, |ui| Footer::new("Close").cancel(None).secondary(secondary).show(ui));
        let footer_close = out.inner.as_ref().is_some_and(|(_, f)| f.primary);
        DialogOut { closed: out.closed || footer_close, inner: out.inner.map(|(r, f)| (r, f.secondary)) }
    }

    /// A viewer or an editor that applies as it is typed: the body and a pinned footer with one
    /// primary "Close" (Enter in a kit field and Escape close it too).
    pub fn show_body<R>(self, ctx: &Context, body: impl FnOnce(&mut Ui) -> R) -> DialogOut<R> {
        let out = self.show_close(ctx, body, &[]);
        DialogOut { inner: out.inner.map(|(r, _)| r), closed: out.closed }
    }

    /// A dialog with no footer at all (a progress window that has its own buttons).
    pub fn show_bare<R>(self, ctx: &Context, body: impl FnOnce(&mut Ui) -> R) -> DialogOut<R> {
        let out = self.show(ctx, body, |_| ());
        DialogOut { inner: out.inner.map(|(r, _)| r), closed: out.closed }
    }
}

/// The header: an icon badge, the title and subtitle, a help toggle and the close button.
/// Returns true when the close button was pressed.
fn header(ui: &mut Ui, t: &Tokens, icon: &str, title: &str, subtitle: Option<&str>, has_help: bool, help_on: &mut bool) -> bool {
    let mut close = false;
    let top = ui.cursor().min;
    ui.horizontal(|ui| {
        let (badge, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
        ui.painter().rect_filled(badge, 9.0, t.flat(t.accent.gamma_multiply(if t.dark { 0.30 } else { 0.14 })));
        ui.painter().text(badge.center(), egui::Align2::CENTER_CENTER, icon, icon_font(19.0), t.accent_text);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(RichText::new(title).size(theme::FONT_HEADING).strong().color(t.text));
            if let Some(s) = subtitle {
                ui.label(RichText::new(s).size(theme::FONT_SMALL).color(t.text_muted));
            }
        });
        ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
            if icon_button(ui, crate::glyphs::ui::CLOSE, "Close (Esc)").clicked() {
                close = true;
            }
            if has_help && icon_button_on(ui, crate::glyphs::ui::HELP, "Help for this dialog", *help_on).clicked() {
                *help_on = !*help_on;
            }
        });
    });
    // The subtle highlight along the very top edge of the window.
    let r = ui.max_rect();
    theme::paint_top_highlight(ui.painter(), egui::Rect::from_min_max(egui::pos2(r.left() - 12.0, top.y - 12.0), egui::pos2(r.right() + 12.0, top.y + 10.0)), t, theme::RADIUS_WINDOW as f32);
    ui.add_space(2.0);
    let line = ui.available_rect_before_wrap();
    ui.painter().hline(line.x_range(), ui.cursor().top(), Stroke::new(1.0_f32, t.border));
    ui.add_space(theme::SP_SM);
    close
}

/// A small square icon-only button with a tooltip.
pub fn icon_button(ui: &mut Ui, glyph: &str, tip: &str) -> Response {
    icon_button_on(ui, glyph, tip, false)
}

pub fn icon_button_on(ui: &mut Ui, glyph: &str, tip: &str, on: bool) -> Response {
    let t = tokens_ui(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    if resp.hovered() || on {
        ui.painter().rect_filled(rect, theme::RADIUS_CONTROL as f32, t.flat(if on { t.raised_active } else { t.raised_hover }));
    }
    let colour = if resp.hovered() || on { t.text } else { t.text_muted };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, glyph, icon_font(15.0), colour);
    resp.on_hover_text(tip)
}

// ─── layout ──────────────────────────────────────────────────────────────────

/// A heading with a thin rule, then the content.
pub fn section<R>(ui: &mut Ui, heading: &str, body: impl FnOnce(&mut Ui) -> R) -> R {
    let t = tokens_ui(ui);
    ui.add_space(theme::SP_SM);
    ui.horizontal(|ui| {
        ui.label(RichText::new(heading.to_uppercase()).size(theme::FONT_SMALL).strong().color(t.text_muted));
        let r = ui.available_rect_before_wrap();
        ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0_f32, t.border));
    });
    ui.add_space(theme::SP_XS);
    body(ui)
}

/// The rows of a form: a label column and a widget column, aligned across the whole form.
pub struct Form<'a> {
    ui: &'a mut Ui,
}

impl Form<'_> {
    /// A row: the label, then whatever `widget` adds. The label sits level with the first line.
    pub fn row<R>(&mut self, label: &str, widget: impl FnOnce(&mut Ui) -> R) -> R {
        let t = tokens_ui(self.ui);
        self.ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.add_space(3.0);
            ui.label(RichText::new(label).color(t.text));
        });
        let r = widget(self.ui);
        self.ui.end_row();
        r
    }
    /// A row with a muted note under the widget.
    pub fn row_note<R>(&mut self, label: &str, note: &str, widget: impl FnOnce(&mut Ui) -> R) -> R {
        self.row(label, |ui| {
            let t = tokens_ui(ui);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                let r = widget(ui);
                ui.label(RichText::new(note).size(theme::FONT_SMALL).color(t.text_muted));
                r
            })
            .inner
        })
    }
    /// Whatever adds its own cells and ends its own row (a helper written for a two-column grid).
    pub fn raw<R>(&mut self, cells: impl FnOnce(&mut Ui) -> R) -> R {
        cells(self.ui)
    }
    /// A row that spans both columns (a callout, a table).
    pub fn full<R>(&mut self, widget: impl FnOnce(&mut Ui) -> R) -> R {
        // A grid cell cannot span, so the first cell is left empty and the content goes in the second.
        self.ui.label("");
        let r = widget(self.ui);
        self.ui.end_row();
        r
    }
}

/// A two-column grid of labelled rows. `id` must be unique in the window.
pub fn form<R>(ui: &mut Ui, id: &str, rows: impl FnOnce(&mut Form) -> R) -> R {
    egui::Grid::new(Id::new(("kit-form", id))).num_columns(2).spacing([theme::SP_LG, 6.0]).min_col_width(130.0).show(ui, |ui| rows(&mut Form { ui })).inner
}

/// An aligned two-column grid for the helpers that add their own label and widget cells.
pub fn grid<R>(ui: &mut Ui, id: &str, cells: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Grid::new(Id::new(("kit-grid", id))).num_columns(2).spacing([theme::SP_LG, 6.0]).min_col_width(130.0).show(ui, cells).inner
}

/// A collapsible "Advanced" block, closed until opened (and remembered while the window lives).
pub fn advanced<R>(ui: &mut Ui, id: &str, body: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let t = tokens_ui(ui);
    egui::CollapsingHeader::new(RichText::new("Advanced").size(theme::FONT_SMALL + 1.0).strong().color(t.text_muted)).id_salt(("kit-adv", id)).default_open(false).show(ui, |ui| body(ui)).body_returned
}

/// Tabs inside a dialog: a row of labels with an underline on the selected one.
pub fn tabs(ui: &mut Ui, selected: &mut usize, labels: &[&str]) {
    let t = tokens_ui(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SP_MD;
        for (i, label) in labels.iter().enumerate() {
            let on = *selected == i;
            let colour = if on { t.accent_text } else { t.text_muted };
            let r = ui.add(egui::Label::new(RichText::new(*label).size(theme::FONT_BODY).strong().color(colour)).sense(Sense::click()));
            if on {
                ui.painter().hline(r.rect.x_range(), r.rect.bottom() + 3.0, Stroke::new(2.0_f32, t.accent_text));
            }
            if r.clicked() {
                *selected = i;
            }
            r.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
    });
    let r = ui.available_rect_before_wrap();
    ui.painter().hline(r.x_range(), ui.cursor().top() + 3.0, Stroke::new(1.0_f32, t.border));
    ui.add_space(theme::SP_MD);
}

// ─── inputs ──────────────────────────────────────────────────────────────────

/// The state of a numeric field after checking its text.
#[derive(Debug, Clone, PartialEq)]
pub enum Check {
    /// Nothing typed (and that is allowed).
    Empty,
    Ok(f64),
    /// Valid, but worth a word.
    Warn(f64, String),
    Bad(String),
}

impl Check {
    pub fn value(&self) -> Option<f64> {
        match self {
            Check::Ok(v) | Check::Warn(v, _) => Some(*v),
            _ => None,
        }
    }
    pub fn is_bad(&self) -> bool {
        matches!(self, Check::Bad(_))
    }
}

/// How a numeric field is checked.
#[derive(Debug, Clone, Copy, Default)]
pub struct NumRule {
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// An empty field is an error.
    pub required: bool,
    /// The value must be more than `min`, not just at least `min`.
    pub exclusive_min: bool,
}

impl NumRule {
    pub fn required() -> Self {
        NumRule { required: true, ..Default::default() }
    }
    pub fn min(mut self, v: f64) -> Self {
        self.min = Some(v);
        self
    }
    pub fn max(mut self, v: f64) -> Self {
        self.max = Some(v);
        self
    }
    pub fn above(mut self, v: f64) -> Self {
        self.min = Some(v);
        self.exclusive_min = true;
        self
    }
}

/// Checks the text of a numeric field against a rule. A comma is read as a decimal point.
pub fn check_number(text: &str, rule: &NumRule) -> Check {
    let s = text.trim();
    if s.is_empty() {
        return if rule.required { Check::Bad("Enter a number".into()) } else { Check::Empty };
    }
    let Ok(v) = s.replace(',', ".").parse::<f64>() else { return Check::Bad("Not a number".into()) };
    if !v.is_finite() {
        return Check::Bad("Not a number".into());
    }
    if let Some(min) = rule.min {
        if v < min || (rule.exclusive_min && v <= min) {
            return Check::Bad(format!("Must be {} {}", if rule.exclusive_min { "more than" } else { "at least" }, trim_num(min)));
        }
    }
    if let Some(max) = rule.max {
        if v > max {
            return Check::Bad(format!("Must be at most {}", trim_num(max)));
        }
    }
    Check::Ok(v)
}

fn trim_num(v: f64) -> String {
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// What `num_field` gives back.
pub struct NumOut {
    pub response: Response,
    pub check: Check,
}

/// A numeric text field with a unit suffix and a validation state: the border turns red and the
/// message shows under the field when the text is not acceptable.
pub fn num_field(ui: &mut Ui, text: &mut String, unit: &str, rule: &NumRule) -> NumOut {
    let t = tokens_ui(ui);
    let check = check_number(text, rule);
    let mut response = None;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let r = ui
                .scope(|ui| {
                    if check.is_bad() {
                        let w = &mut ui.visuals_mut().widgets;
                        w.inactive.bg_stroke = Stroke::new(1.5_f32, t.danger);
                        w.hovered.bg_stroke = Stroke::new(1.5_f32, t.danger);
                        ui.visuals_mut().selection.stroke = Stroke::new(1.5_f32, t.danger);
                    }
                    ui.add(egui::TextEdit::singleline(text).desired_width(96.0).horizontal_align(Align::RIGHT))
                })
                .inner;
            note_enter(ui, &r);
            if !unit.is_empty() {
                ui.label(RichText::new(unit).color(t.text_muted));
            }
            response = Some(r);
        });
        match &check {
            Check::Bad(m) => {
                ui.label(RichText::new(m).size(theme::FONT_SMALL).color(t.danger));
            }
            Check::Warn(_, m) => {
                ui.label(RichText::new(m).size(theme::FONT_SMALL).color(t.warning));
            }
            _ => {}
        }
    });
    NumOut { response: response.expect("the field was added"), check }
}

/// A drag-or-type field for an `f64` kept as a number, with a unit suffix.
pub fn f64_field(ui: &mut Ui, value: &mut f64, unit: &str, range: std::ops::RangeInclusive<f64>, speed: f64) -> Response {
    let t = tokens_ui(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let r = ui.add(egui::DragValue::new(value).range(range).speed(speed).min_decimals(0).max_decimals(4));
        if !unit.is_empty() {
            ui.label(RichText::new(unit).color(t.text_muted));
        }
        r
    })
    .inner
}

/// A whole-number field for a count or a number of decimals.
pub fn decimals_field(ui: &mut Ui, value: &mut u8, range: std::ops::RangeInclusive<u8>) -> Response {
    ui.add(egui::DragValue::new(value).range(range).speed(0.1))
}

/// A single-line text field.
pub fn text_field(ui: &mut Ui, text: &mut String, hint: &str) -> Response {
    let r = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(220.0));
    note_enter(ui, &r);
    r
}

/// A wide single-line text field (a path, a list).
pub fn text_field_wide(ui: &mut Ui, text: &mut String, hint: &str) -> Response {
    let r = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(f32::INFINITY));
    note_enter(ui, &r);
    r
}

/// Puts the keyboard focus in a field the first time its dialog is drawn, so typing can start at once.
pub fn focus_once(ui: &Ui, r: &Response, key: &str) {
    let id = Id::new(("kit-focused", key));
    if !ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false) {
        r.request_focus();
        ui.data_mut(|d| d.insert_temp(id, true));
    }
}

const ENTER_IN_FIELD: &str = "afrobim-kit-enter";

/// Remembers that Enter ended editing in a field, so the footer can treat it as "primary".
fn note_enter(ui: &Ui, r: &Response) {
    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
        ui.ctx().data_mut(|d| d.insert_temp(Id::new(ENTER_IN_FIELD), true));
    }
}

/// A drop-down over `items`; `selected` is an index. Returns true when the choice changed.
pub fn combo(ui: &mut Ui, id: &str, selected: &mut usize, items: &[&str]) -> bool {
    let mut changed = false;
    let shown = items.get(*selected).copied().unwrap_or("");
    egui::ComboBox::from_id_salt(("kit-combo", id)).selected_text(shown).width(220.0).show_ui(ui, |ui| {
        for (i, item) in items.iter().enumerate() {
            changed |= ui.selectable_value(selected, i, *item).changed();
        }
    });
    changed
}

/// A drop-down over owned strings (names that come from the model).
pub fn combo_owned(ui: &mut Ui, id: &str, selected: &mut usize, items: &[String]) -> bool {
    let refs: Vec<&str> = items.iter().map(String::as_str).collect();
    combo(ui, id, selected, &refs)
}

/// Radio buttons, one per line.
pub fn radio_group(ui: &mut Ui, selected: &mut usize, items: &[&str]) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        for (i, item) in items.iter().enumerate() {
            changed |= ui.radio_value(selected, i, *item).changed();
        }
    });
    changed
}

/// A segmented control for two to five exclusive choices.
pub fn segmented(ui: &mut Ui, selected: &mut usize, items: &[&str]) -> bool {
    let t = tokens_ui(ui);
    let mut changed = false;
    let n = items.len();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        for (i, item) in items.iter().enumerate() {
            let on = *selected == i;
            let r = theme::RADIUS_CONTROL;
            let radius = egui::CornerRadius { nw: if i == 0 { r } else { 0 }, sw: if i == 0 { r } else { 0 }, ne: if i + 1 == n { r } else { 0 }, se: if i + 1 == n { r } else { 0 } };
            let (fill, text) = if on { (t.accent, t.text_on_accent) } else { (t.flat(t.raised), t.text) };
            let resp = ui.add(egui::Button::new(RichText::new(*item).color(text)).fill(fill).corner_radius(radius).stroke(Stroke::new(1.0_f32, if on { t.accent } else { t.border })).min_size(vec2(64.0, 26.0)));
            if resp.clicked() && !on {
                *selected = i;
                changed = true;
            }
        }
    });
    changed
}

/// A segmented control over values of any comparable type.
pub fn segmented_of<T: PartialEq + Clone>(ui: &mut Ui, value: &mut T, items: &[(T, &str)]) -> bool {
    let mut i = items.iter().position(|(v, _)| v == value).unwrap_or(0);
    let labels: Vec<&str> = items.iter().map(|(_, l)| *l).collect();
    let changed = segmented(ui, &mut i, &labels);
    if changed {
        *value = items[i].0.clone();
    }
    changed
}

/// Radio buttons over values of any comparable type, one per line.
pub fn radios_of<T: PartialEq + Clone>(ui: &mut Ui, value: &mut T, items: &[(T, &str)]) -> bool {
    let mut i = items.iter().position(|(v, _)| v == value).unwrap_or(0);
    let labels: Vec<&str> = items.iter().map(|(_, l)| *l).collect();
    let changed = radio_group(ui, &mut i, &labels);
    if changed {
        *value = items[i].0.clone();
    }
    changed
}

/// A drop-down over named model elements (alignments, gantries); `sel` is the chosen one.
pub fn element_combo(ui: &mut Ui, id: &str, choices: &[(afrobim_kernel::ElementId, String)], sel: &mut Option<afrobim_kernel::ElementId>) -> bool {
    let mut changed = false;
    let current = choices.iter().find(|c| Some(c.0) == *sel).map(|c| c.1.clone()).unwrap_or_else(|| "(none)".to_string());
    egui::ComboBox::from_id_salt(("kit-element", id)).width(220.0).selected_text(current).show_ui(ui, |ui| {
        for (eid, name) in choices {
            changed |= ui.selectable_value(sel, Some(*eid), name).changed();
        }
    });
    changed
}

/// A heading inside a body: small capitals over a thin rule (a `section` with no closure).
pub fn heading(ui: &mut Ui, text: &str) {
    let t = tokens_ui(ui);
    // In a child layout of its own, so that it is also safe in a grid cell (spacing in a grid panics).
    ui.vertical(|ui| {
        ui.add_space(theme::SP_SM);
        ui.horizontal(|ui| {
            ui.label(RichText::new(text.to_uppercase()).size(theme::FONT_SMALL).strong().color(t.text_muted));
            let r = ui.available_rect_before_wrap();
            ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0_f32, t.border));
        });
        ui.add_space(theme::SP_XS);
    });
}

/// A muted explanatory paragraph (a source, a limit, a note under a form).
pub fn note(ui: &mut Ui, text: impl Into<String>) {
    let t = tokens_ui(ui);
    ui.add(egui::Label::new(RichText::new(text.into()).size(theme::FONT_SMALL).color(t.text_muted)).wrap());
}

/// A checkbox with its label.
pub fn checkbox(ui: &mut Ui, value: &mut bool, label: &str) -> Response {
    ui.checkbox(value, label)
}

/// A toggle switch with its label.
pub fn toggle(ui: &mut Ui, value: &mut bool, label: &str) -> Response {
    let t = tokens_ui(ui);
    ui.horizontal(|ui| {
        let size = vec2(34.0, 18.0);
        let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
        if resp.clicked() {
            *value = !*value;
            resp.mark_changed();
        }
        let how_on = ui.ctx().animate_bool_responsive(resp.id, *value);
        let track = if *value { t.accent } else { t.flat(t.raised_active) };
        ui.painter().rect(rect, 9.0, track, Stroke::new(1.0_f32, if *value { t.accent } else { t.border }), egui::StrokeKind::Inside);
        let cx = egui::lerp((rect.left() + 9.0)..=(rect.right() - 9.0), how_on);
        ui.painter().circle_filled(egui::pos2(cx, rect.center().y), 6.5, if *value { t.text_on_accent } else { t.text });
        if resp.hovered() {
            ui.painter().rect_stroke(rect.expand(1.0), 10.0, Stroke::new(1.0_f32, t.accent_text), egui::StrokeKind::Outside);
        }
        let l = ui.add(egui::Label::new(RichText::new(label).color(t.text)).sense(Sense::click()));
        if l.clicked() {
            *value = !*value;
            resp.mark_changed();
        }
        resp
    })
    .inner
}

/// A slider with a numeric entry beside it.
pub fn slider_entry(ui: &mut Ui, value: &mut f64, range: std::ops::RangeInclusive<f64>, unit: &str) -> bool {
    let t = tokens_ui(ui);
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui.add(egui::Slider::new(value, range.clone()).show_value(false)).changed();
        changed |= ui.add(egui::DragValue::new(value).range(range).speed(0.1).max_decimals(4)).changed();
        if !unit.is_empty() {
            ui.label(RichText::new(unit).color(t.text_muted));
        }
    });
    changed
}

// ─── feedback ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Info,
    Warning,
    Error,
    Success,
}

impl Level {
    fn colour(self, t: &Tokens) -> Color32 {
        match self {
            Level::Info => t.accent_text,
            Level::Warning => t.warning,
            Level::Error => t.danger,
            Level::Success => t.success,
        }
    }
    fn glyph(self) -> &'static str {
        use crate::glyphs::ui as g;
        match self {
            Level::Info => g::INFO,
            Level::Warning => g::WARNING,
            Level::Error => g::ERROR,
            Level::Success => g::OK,
        }
    }
}

/// An inline callout: a tinted box with an icon and wrapped text.
pub fn callout(ui: &mut Ui, level: Level, text: impl Into<String>) -> Response {
    let t = tokens_ui(ui);
    let colour = level.colour(&t);
    let text: String = text.into();
    egui::Frame::new()
        .fill(t.flat(colour.gamma_multiply(if t.dark { 0.16 } else { 0.10 })))
        .stroke(Stroke::new(1.0_f32, colour.gamma_multiply(0.45)))
        .corner_radius(egui::CornerRadius::same(theme::RADIUS_CONTROL))
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.label(RichText::new(level.glyph()).font(icon_font(16.0)).color(colour));
                ui.add(egui::Label::new(RichText::new(text).color(t.text)).wrap());
            });
        })
        .response
}

/// Column alignment of a results table.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Col {
    Text,
    /// Numbers: set in a monospace face so the digits line up.
    Num,
}

/// A results table: a muted header row, striped rows, numbers in monospace.
pub fn results_table(ui: &mut Ui, id: &str, headers: &[&str], cols: &[Col], rows: &[Vec<String>]) {
    let t = tokens_ui(ui);
    egui::Frame::new().fill(t.flat(t.field)).corner_radius(egui::CornerRadius::same(theme::RADIUS_CONTROL)).inner_margin(egui::Margin::symmetric(8, 6)).stroke(Stroke::new(1.0_f32, t.border)).show(ui, |ui| {
        egui::Grid::new(Id::new(("kit-table", id))).num_columns(headers.len()).spacing([theme::SP_LG, 4.0]).striped(true).show(ui, |ui| {
            for (i, h) in headers.iter().enumerate() {
                let l = RichText::new(*h).size(theme::FONT_SMALL).strong().color(t.text_muted);
                if cols.get(i).copied() == Some(Col::Num) {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(l));
                } else {
                    ui.label(l);
                }
            }
            ui.end_row();
            for row in rows {
                for (i, cell) in row.iter().enumerate() {
                    let text = RichText::new(cell).color(t.text);
                    if cols.get(i).copied() == Some(Col::Num) {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(text.monospace()));
                    } else {
                        ui.label(text);
                    }
                }
                ui.end_row();
            }
        });
    });
}

// ─── the footer ──────────────────────────────────────────────────────────────

/// What the footer reports.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FooterOut {
    pub primary: bool,
    pub cancel: bool,
    /// The index of the secondary button pressed.
    pub secondary: Option<usize>,
}

/// The footer bar: a primary button in the accent colour, secondary buttons, and Cancel. Enter
/// presses the primary button and Escape presses Cancel while this dialog is the top window.
pub struct Footer<'a> {
    primary: &'a str,
    primary_icon: Option<&'a str>,
    enabled: bool,
    reason: Option<&'a str>,
    secondary: &'a [&'a str],
    cancel: Option<&'a str>,
    note: Option<&'a str>,
}

impl<'a> Footer<'a> {
    pub fn new(primary: &'a str) -> Self {
        Footer { primary, primary_icon: None, enabled: true, reason: None, secondary: &[], cancel: Some("Cancel"), note: None }
    }
    pub fn icon(mut self, glyph: &'a str) -> Self {
        self.primary_icon = Some(glyph);
        self
    }
    /// Whether the inputs are valid. When not, the primary button is disabled and `reason` is shown.
    pub fn enabled(mut self, ok: bool, reason: &'a str) -> Self {
        self.enabled = ok;
        self.reason = if ok { None } else { Some(reason) };
        self
    }
    pub fn secondary(mut self, labels: &'a [&'a str]) -> Self {
        self.secondary = labels;
        self
    }
    /// The label of the cancel button, or None for a dialog with no cancel ("Close" as primary).
    pub fn cancel(mut self, label: Option<&'a str>) -> Self {
        self.cancel = label;
        self
    }
    /// A muted line at the left of the bar.
    pub fn note(mut self, note: &'a str) -> Self {
        self.note = Some(note);
        self
    }

    pub fn show(self, ui: &mut Ui) -> FooterOut {
        let t = tokens_ui(ui);
        let mut out = FooterOut::default();
        let line = ui.available_rect_before_wrap();
        ui.painter().hline(line.x_range(), ui.cursor().top(), Stroke::new(1.0_f32, t.border));
        ui.add_space(theme::SP_SM);
        let ctx = ui.ctx().clone();
        let top = ctx.memory(|m| m.areas().top_layer_id(Order::Middle));
        let is_top = top == Some(ui.layer_id());
        // The reason the primary is off, or a note, on a line of its own (it can be long).
        if let Some(r) = self.reason {
            ui.add(egui::Label::new(theme::icon_text(crate::glyphs::ui::WARNING, r, theme::FONT_SMALL + 0.5, t.warning)).wrap());
            ui.add_space(2.0);
        } else if let Some(n) = self.note {
            ui.add(egui::Label::new(RichText::new(n).size(theme::FONT_SMALL + 0.5).color(t.text_muted)).wrap());
            ui.add_space(2.0);
        }
        // The buttons are as wide as their labels (at least 80 points); when they do not all fit
        // in a row the secondary ones go on a row of their own above the primary and cancel.
        let width_of = |s: &str, _min: f32| ui.fonts(|f| f.layout_no_wrap(s.to_string(), FontId::proportional(theme::FONT_BODY), Color32::WHITE).size().x) + 20.0;
        let mut need = 96.0_f32.max(width_of(self.primary, 96.0) + if self.primary_icon.is_some() { 22.0 } else { 0.0 }) + 8.0;
        if let Some(c) = self.cancel {
            need += width_of(c, 80.0).max(80.0) + 8.0;
        }
        for s in self.secondary {
            need += width_of(s, 80.0).max(80.0) + 8.0;
        }
        let one_row = need <= ui.available_width();
        let secondary_row = |ui: &mut Ui, out: &mut FooterOut| {
            for (i, s) in self.secondary.iter().enumerate() {
                if ui.add(egui::Button::new(*s).min_size(vec2(80.0, 28.0))).clicked() {
                    out.secondary = Some(i);
                }
            }
        };
        if !one_row && !self.secondary.is_empty() {
            ui.horizontal_wrapped(|ui| secondary_row(ui, &mut out));
        }
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = match self.primary_icon {
                    Some(g) => WidgetText::from(theme::icon_text(g, self.primary, theme::FONT_BODY, t.text_on_accent)),
                    None => WidgetText::from(RichText::new(self.primary).color(t.text_on_accent)),
                };
                let primary = ui.add_enabled(self.enabled, egui::Button::new(label).fill(t.accent).stroke(Stroke::new(1.0_f32, t.accent_text.gamma_multiply(0.6))).min_size(vec2(96.0, 28.0)));
                if primary.clicked() {
                    out.primary = true;
                }
                if let Some(c) = self.cancel {
                    if ui.add(egui::Button::new(c).min_size(vec2(80.0, 28.0))).clicked() {
                        out.cancel = true;
                    }
                }
                if one_row {
                    for (i, s) in self.secondary.iter().enumerate().rev() {
                        if ui.add(egui::Button::new(*s).min_size(vec2(80.0, 28.0))).clicked() {
                            out.secondary = Some(i);
                        }
                    }
                }
            });
        });
        // Enter (in a field, or with nothing focused) presses the primary button; Escape cancels.
        let enter_in_field = ctx.data_mut(|d| d.remove_temp::<bool>(Id::new(ENTER_IN_FIELD))).unwrap_or(false);
        if is_top && self.enabled && enter_in_field {
            out.primary = true;
        }
        if is_top && self.cancel.is_some() && ctx.input(|i| i.key_pressed(Key::Escape)) {
            out.cancel = true;
        }
        if is_top && self.cancel.is_none() && ctx.input(|i| i.key_pressed(Key::Escape)) {
            out.primary = true;
        }
        out
    }
}

/// Fonts for code that draws kit-styled text by hand.
pub fn body_font() -> FontId {
    FontId::proportional(theme::FONT_BODY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_checked_against_their_rule() {
        let rule = NumRule::required().above(0.0).max(100.0);
        assert_eq!(check_number("12.5", &rule), Check::Ok(12.5));
        assert_eq!(check_number(" 7,5 ", &rule).value(), Some(7.5));
        assert!(check_number("", &rule).is_bad());
        assert_eq!(check_number("", &NumRule::default()), Check::Empty);
        assert!(check_number("abc", &rule).is_bad());
        assert!(check_number("0", &rule).is_bad());
        assert!(check_number("-3", &rule).is_bad());
        assert!(check_number("101", &rule).is_bad());
        assert!(check_number("NaN", &rule).is_bad());
        assert!(check_number("inf", &rule).is_bad());
        // At least is inclusive, above is not.
        assert_eq!(check_number("0", &NumRule::default().min(0.0)), Check::Ok(0.0));
        match check_number("-1", &NumRule::default().min(0.0)) {
            Check::Bad(m) => assert_eq!(m, "Must be at least 0"),
            other => panic!("{other:?}"),
        }
    }

    /// Runs one frame of a dialog with the given pointer-free input and returns what the footer said.
    fn frame(ctx: &Context, events: Vec<egui::Event>, enabled: bool) -> FooterOut {
        let mut out = FooterOut::default();
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1200.0, 800.0))), events, ..Default::default() };
        let _ = ctx.run(input, |ctx| {
            crate::win::set_work_rect(ctx, ctx.screen_rect());
            let o = Dialog::new("Kit test", crate::glyphs::ui::INFO).subtitle("sub").help("help text").show(ctx, |ui| ui.label("body"), |ui| Footer::new("Create").enabled(enabled, "Pick one").show(ui));
            if let Some((_, f)) = o.inner {
                out = f;
            }
        });
        out
    }

    fn key(k: Key) -> egui::Event {
        egui::Event::Key { key: k, physical_key: Some(k), pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }
    }

    #[test]
    fn escape_cancels_the_top_dialog_and_nothing_else_happens_without_input() {
        let ctx = theme::test_context();
        for _ in 0..3 {
            assert_eq!(frame(&ctx, vec![], true), FooterOut::default());
        }
        assert!(frame(&ctx, vec![key(Key::Escape)], true).cancel);
    }

    #[test]
    fn a_dialog_footer_with_the_primary_disabled_never_reports_it() {
        let ctx = theme::test_context();
        for _ in 0..3 {
            frame(&ctx, vec![], false);
        }
        // Even with Enter noted from a field, a disabled primary does not fire.
        ctx.data_mut(|d| d.insert_temp(Id::new(ENTER_IN_FIELD), true));
        assert!(!frame(&ctx, vec![], false).primary);
        ctx.data_mut(|d| d.insert_temp(Id::new(ENTER_IN_FIELD), true));
        assert!(frame(&ctx, vec![], true).primary);
    }
}
