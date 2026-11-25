//! Engineering defaults in the interface.
//!
//! The values come only from `afrobim-defaults` (the DMRB pack today). A field bound to a key that
//! the pack holds under the condition the dialog knows is pre-filled and shows the citation beside
//! it; the value stays editable, a marker says when it differs from the default, and a button resets
//! it. A field whose key the pack does not hold stays an empty required input. The project's choice
//! of basis is kept in the project settings (`defaults.basis`); `none` turns every default off.
//!
//! `DEFAULTS` opens a browsable, searchable table of the whole pack with its own statement that the
//! values were not reviewed by an engineer.

use std::cell::RefCell;

use afrobim_defaults::{fmt_num, Conditions, DefaultValue, Defaults, Resolved};
use afrobim_kernel::{project_setting, set_project_setting, Document};
use egui::{RichText, Ui};

use crate::ui_kit::{self, Check, Col, Dialog, Level, NumRule};

pub const SETTING_KEY: &str = "defaults.basis";
/// The basis id that switches every default off.
pub const NO_BASIS: &str = "none";
/// The basis of a new project: the Uganda MoWT Road Design Manual (2010).
pub const DEFAULT_BASIS: &str = afrobim_defaults::UG_MOWT_ID;

thread_local! {
    static DEFAULTS: RefCell<Defaults> = RefCell::new(Defaults::standard());
    static NONE: RefCell<bool> = const { RefCell::new(false) };
    static LAST_SYNC: RefCell<Option<std::time::Instant>> = const { RefCell::new(None) };
}

/// Runs `f` with the defaults the project works with (empty when the project turned them off).
pub fn with<R>(f: impl FnOnce(&Defaults) -> R) -> R {
    if NONE.with(|n| *n.borrow()) {
        return f(&Defaults::empty());
    }
    DEFAULTS.with(|d| f(&d.borrow()))
}

/// The default of a key under a condition.
pub fn get(key: &str, cond: &Conditions) -> Option<DefaultValue> {
    with(|d| d.get(key, cond))
}

/// The default number of a key under a condition.
pub fn number(key: &str, cond: &Conditions) -> Option<f64> {
    get(key, cond).and_then(|v| v.number())
}

/// The id of the basis in force ("none" when switched off).
pub fn basis_id() -> String {
    if NONE.with(|n| *n.borrow()) {
        NO_BASIS.into()
    } else {
        DEFAULTS.with(|d| d.borrow().basis_id().unwrap_or(NO_BASIS).to_string())
    }
}

/// The design vehicles DV-1 to DV-5 (DV-5 the largest). They come from the basis in force when it
/// holds them; otherwise from the Uganda MoWT pack, which is what the junction dialogs offer.
/// The second element names the pack they came from.
pub fn design_vehicles() -> (Vec<afrobim_defaults::DesignVehicle>, String) {
    let active = with(|d| d.active_set().map(|s| (afrobim_defaults::design_vehicles(s), s.name().to_string())));
    match active {
        Some((v, n)) if !v.is_empty() => (v, n),
        _ => {
            let p = afrobim_defaults::Pack::ug_mowt();
            let n = afrobim_defaults::DefaultsSet::name(&p).to_string();
            (afrobim_defaults::design_vehicles(&p), n)
        }
    }
}

/// The bases offered: id and name.
pub fn bases() -> Vec<(String, String)> {
    DEFAULTS.with(|d| d.borrow().bases().iter().map(|(a, b)| (a.to_string(), b.to_string())).collect())
}

/// Reads the project's basis from the document (at most once a second; `force` always reads).
pub fn sync(doc: &Document, force: bool) {
    let due = LAST_SYNC.with(|l| l.borrow().is_none_or(|t| t.elapsed().as_secs_f32() > 1.0));
    if !due && !force {
        return;
    }
    LAST_SYNC.with(|l| *l.borrow_mut() = Some(std::time::Instant::now()));
    let id = project_setting(doc, SETTING_KEY).and_then(|v| v.as_str().map(str::to_string));
    set_basis(id.as_deref().unwrap_or(DEFAULT_BASIS));
}

/// Chooses the basis (an unknown id leaves the basis as it is).
pub fn set_basis(id: &str) {
    if id == NO_BASIS {
        NONE.with(|n| *n.borrow_mut() = true);
        return;
    }
    NONE.with(|n| *n.borrow_mut() = false);
    DEFAULTS.with(|d| {
        let _ = d.borrow_mut().set_basis(id);
    });
}

/// Stores the choice in the document.
pub fn store_basis(doc: &mut Document, id: &str) {
    set_project_setting(doc, SETTING_KEY, serde_json::Value::String(id.to_string()));
    set_basis(id);
}

/// The name of the basis in force, for a heading.
pub fn basis_name() -> String {
    with(|d| d.active_set().map(|s| s.name().to_string())).unwrap_or_else(|| "none chosen".into())
}

/// The citation of a default as the line shown beside a field.
pub fn citation_line(v: &DefaultValue) -> String {
    let c = &v.citation;
    format!("{} {} v{} ({}), {}, p{}: {} ({})", c.pack_id.to_uppercase(), c.document_reference, c.version, c.published, c.clause, c.page, v.status.label(), c.verb)
}

/// A resolved default as text.
pub fn summary_of(r: &Resolved) -> String {
    match r {
        Resolved::Number(n) => fmt_num(*n),
        Resolved::Formula(t) | Resolved::Text(t) => t.clone(),
    }
}

/// The values of a table entry in a short text.
pub fn summary(e: &afrobim_defaults::Entry) -> String {
    use afrobim_defaults::Kind;
    match e.kind {
        Kind::Scalar => e.value.map(fmt_num).unwrap_or_default(),
        Kind::Table => e.table.as_ref().map(|t| t.rows.iter().map(|r| format!("{} = {}", r.at.show(), fmt_num(r.value))).collect::<Vec<_>>().join("; ")).unwrap_or_default(),
        Kind::Formula | Kind::Text => e.text.clone().unwrap_or_default(),
    }
}

// ─── the kit helper ─────────────────────────────────────────────────────────

/// What a field bound to a default remembers between frames.
#[derive(Clone, Default)]
struct Bound {
    /// The signature (key, condition and value) the field was last filled for.
    sig: String,
}

/// A numeric field bound to a defaults key. Pre-fills from the pack under `cond` (and again when
/// the condition changes while the user has not edited), shows the citation, marks a value changed
/// from the default and offers a reset. With no entry the field is an empty required input and the
/// note says so. `factor` converts the pack's number to the field's convention (for example -1 for
/// a cross-fall typed as negative when falling away). Returns the value of the field when valid.
pub fn field(ui: &mut Ui, salt: impl std::hash::Hash, text: &mut String, unit: &str, key: &str, cond: &Conditions, factor: f64, rule: &NumRule) -> Option<f64> {
    // Always stacked: a form cell lays out left to right, which would put the note and the citation
    // beside the field and widen the dialog.
    ui.vertical(|ui| field_inner(ui, salt, text, unit, key, cond, factor, rule)).inner
}

#[allow(clippy::too_many_arguments)]
fn field_inner(ui: &mut Ui, salt: impl std::hash::Hash, text: &mut String, unit: &str, key: &str, cond: &Conditions, factor: f64, rule: &NumRule) -> Option<f64> {
    let dv = get(key, cond).and_then(|v| v.number().map(|n| (v.clone(), n * factor)));
    let id = ui.id().with(("bound", key, salt));
    let sig = dv.as_ref().map(|(v, n)| format!("{key}|{n}|{}", v.citation.clause)).unwrap_or_else(|| format!("{key}|none"));
    let mut st: Bound = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
    if st.sig != sig {
        // First sight, or the condition changed: take the default unless the user typed a value of
        // their own (a value that is not the default last applied).
        let was_default = ui.ctx().data(|d| d.get_temp::<String>(id.with("applied"))).is_some_and(|prev| prev == *text);
        if let Some((_, n)) = &dv {
            if text.trim().is_empty() || was_default {
                *text = fmt_num(*n);
            }
        } else if was_default {
            text.clear();
        }
        st.sig = sig;
        ui.ctx().data_mut(|d| {
            d.insert_temp(id, st.clone());
            d.insert_temp(id.with("applied"), dv.as_ref().map(|(_, n)| fmt_num(*n)).unwrap_or_default());
        });
    }
    let out = ui_kit::num_field(ui, text, unit, rule);
    match &dv {
        Some((v, n)) => {
            let default_text = fmt_num(*n);
            let same = match &out.check {
                Check::Ok(x) | Check::Warn(x, _) => (x - n).abs() <= 1e-9 * n.abs().max(1.0),
                _ => text.trim() == default_text,
            };
            let t = crate::theme::tokens_ui(ui);
            ui.horizontal_wrapped(|ui| {
                if same {
                    ui.label(RichText::new("default").size(crate::theme::FONT_SMALL).color(t.accent_text)).on_hover_text("The value is the default of the basis. Edit it to use your own.");
                } else {
                    ui.label(RichText::new(format!("changed from the default {default_text}{}", if unit.is_empty() { String::new() } else { format!(" {unit}") })).size(crate::theme::FONT_SMALL).color(t.warning));
                    if ui.small_button("Reset").on_hover_text("Put the default back").clicked() {
                        *text = default_text.clone();
                    }
                }
            });
            // A grid cell does not wrap a label, so the citation is given a width of its own: a long
            // clause must not widen the column (and with it the dialog) it stands in.
            ui.scope(|ui| {
                ui.set_max_width(CITATION_WIDTH);
                ui.add(egui::Label::new(RichText::new(citation_line(v)).size(crate::theme::FONT_SMALL - 1.0).color(t.text_muted)).wrap()).on_hover_text(format!("{}\nCondition: {}\n{}", v.title, v.citation.condition, project_note()));
                if v.interpolated {
                    ui.add(egui::Label::new(RichText::new("interpolated between two values of the document").size(crate::theme::FONT_SMALL - 1.0).color(t.warning)).wrap());
                }
            });
        }
        None => {
            let why = with(|d| d.active_set().is_none()).then_some("No defaults basis is chosen for the project.").unwrap_or("No default under this condition in the basis: enter a value.");
            ui.scope(|ui| {
                ui.set_max_width(CITATION_WIDTH);
                ui.add(egui::Label::new(RichText::new(why).size(crate::theme::FONT_SMALL - 1.0).color(crate::theme::tokens_ui(ui).text_muted)).wrap());
            });
        }
    }
    out.check.value()
}

/// The width a citation line wraps at, in points.
const CITATION_WIDTH: f32 = 340.0;

fn project_note() -> &'static str {
    "A default is a starting value, not a requirement of the project. The pack was not reviewed by an engineer."
}

/// The default of a scalar entry shown as a reference row: label, value, unit, citation.
pub fn reference_rows(items: &[(&str, &str, Conditions)]) -> Vec<Vec<String>> {
    items
        .iter()
        .map(|(label, key, cond)| match get(key, cond) {
            Some(v) => vec![label.to_string(), match &v.value {
                Resolved::Number(n) => fmt_num(*n),
                Resolved::Formula(t) | Resolved::Text(t) => t.clone(),
            }, v.unit.symbol().to_string(), citation_line(&v)],
            None => vec![label.to_string(), "no default".into(), String::new(), "Not in the basis under this condition.".into()],
        })
        .collect()
}

/// The design speeds of a table entry, ascending.
pub fn speeds_of(key: &str) -> Vec<f64> {
    with(|d| {
        d.all().iter().find(|e| e.key == key).and_then(|e| e.table.as_ref()).map(|t| t.rows.iter().filter_map(|r| if let afrobim_defaults::At::Num(n) = r.at { Some(n) } else { None }).collect()).unwrap_or_default()
    })
}

/// The labels (rows) of a table entry.
pub fn labels_of(key: &str) -> Vec<String> {
    with(|d| d.all().iter().find(|e| e.key == key).and_then(|e| e.table.as_ref()).map(|t| t.rows.iter().map(|r| r.at.show()).collect()).unwrap_or_default())
}

// ─── road type ─────────────────────────────────────────────────────────────

/// The road type groups of the cross-section defaults (CD 127 figures): label and key prefix.
pub const ROAD_GROUPS: [(&str, &str); 4] = [("Rural motorway", "xs.rural_motorway"), ("Urban motorway", "xs.urban_motorway"), ("Rural all-purpose", "xs.rural_all_purpose"), ("Urban all-purpose", "xs.urban_all_purpose")];

/// The road type the dialog's defaults are looked up for. `group` 0 is not chosen.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoadChoice {
    pub group: usize,
    pub road: String,
}

impl RoadChoice {
    pub fn prefix(&self) -> Option<&'static str> {
        (self.group > 0 && !self.road.is_empty()).then(|| ROAD_GROUPS[self.group - 1].1)
    }
    pub fn key(&self, what: &str) -> Option<String> {
        self.prefix().map(|p| format!("{p}.{what}"))
    }
    pub fn cond(&self) -> Conditions {
        Conditions::none().label("road_type", &self.road)
    }
}

/// The selector the cross-section defaults need: a group and a road type of it.
pub fn road_selector(ui: &mut Ui, id: &str, c: &mut RoadChoice) {
    ui.horizontal(|ui| {
        let mut labels: Vec<String> = vec!["Not chosen".into()];
        labels.extend(ROAD_GROUPS.iter().map(|g| g.0.to_string()));
        let before = c.group;
        ui_kit::combo_owned(ui, &format!("{id}-grp"), &mut c.group, &labels);
        if before != c.group {
            c.road.clear();
        }
        if c.group > 0 {
            let types = labels_of(&format!("{}.carriageway_width", ROAD_GROUPS[c.group - 1].1));
            let mut i = types.iter().position(|t| *t == c.road).map(|i| i + 1).unwrap_or(0);
            let mut shown: Vec<String> = vec!["Choose".into()];
            shown.extend(types.iter().cloned());
            if ui_kit::combo_owned(ui, &format!("{id}-road"), &mut i, &shown) {
                c.road = if i == 0 { String::new() } else { types[i - 1].clone() };
            }
        }
    });
}

// ─── the DEFAULTS window ────────────────────────────────────────────────────

#[derive(Default)]
pub struct DefaultsUi {
    pub open: bool,
    pub search: String,
    pub group: usize,
    pub tab: usize,
}

impl crate::app::AfroBimApp {
    pub(crate) fn defaults_cmd(&mut self) {
        self.defaults_ui.open = true;
    }

    pub(crate) fn defaults_window(&mut self, ctx: &egui::Context) {
        if !self.defaults_ui.open {
            return;
        }
        let mut st = std::mem::take(&mut self.defaults_ui);
        let (meta, entries, missing, name) = with(|d| match d.active_set() {
            Some(s) => (Some(s.meta().clone()), d.all().into_iter().cloned().collect::<Vec<_>>(), d.missing(), s.name().to_string()),
            None => (None, Vec::new(), d.missing(), "none".to_string()),
        });
        let mut groups: Vec<String> = entries.iter().map(|e| e.group.clone()).collect();
        groups.sort();
        groups.dedup();
        let mut labels: Vec<String> = vec!["All groups".into()];
        labels.extend(groups.iter().cloned());
        let out = Dialog::new("Engineering defaults", crate::glyphs::by_name("DEFAULTS")).width(1000.0).resizable(true).max_body_height(640.0).help("The default values that pre-fill the fields of the dialogs, each with the document, version, table or clause it was read from. A default is a starting value for the engineer to confirm, never a requirement of the project.").show_body(
            ctx,
            |ui| {
                match &meta {
                    Some(m) => {
                        ui_kit::callout(ui, Level::Warning, format!("{}  {}", m.review_status, "Treat every value as unreviewed until an engineer has checked it against the cited clause."));
                        ui_kit::callout(ui, Level::Info, m.uganda_caveat.clone());
                    }
                    None => {
                        ui_kit::callout(ui, Level::Info, "The project has no defaults basis: every field is an empty input. Choose a basis in the project settings.");
                    }
                }
                ui.label(RichText::new(format!("Basis: {name}")).strong());
                ui_kit::tabs(ui, &mut st.tab, &["Values", "Not in the pack"]);
                if st.tab == 1 {
                    let rows: Vec<Vec<String>> = missing.iter().map(|m| vec![m.key.clone(), m.reason.clone()]).collect();
                    ui_kit::note(ui, "Keys the application asks for that the basis does not hold. Those fields stay empty required inputs.");
                    egui::ScrollArea::both().max_height(460.0).show(ui, |ui| {
                        ui_kit::results_table(ui, "def-missing", &["Key", "Why it is not held"], &[Col::Text, Col::Text], &rows);
                    });
                    return;
                }
                ui.horizontal(|ui| {
                    ui.label("Search");
                    ui_kit::text_field(ui, &mut st.search, "key, title, document, clause, condition");
                    ui_kit::combo_owned(ui, "def-group", &mut st.group, &labels);
                });
                let q = st.search.trim().to_ascii_lowercase();
                let want = if st.group == 0 { None } else { labels.get(st.group).cloned() };
                let docs = with(|d| d.active_set().map(|_| ()).is_some());
                let _ = docs;
                let shown: Vec<&afrobim_defaults::Entry> = entries
                    .iter()
                    .filter(|e| want.as_ref().is_none_or(|g| &e.group == g))
                    .filter(|e| q.is_empty() || [&e.key, &e.title, &e.document, &e.clause, &e.condition, &e.group].iter().any(|s| s.to_ascii_lowercase().contains(&q)))
                    .collect();
                ui.label(RichText::new(format!("{} of {} values", shown.len(), entries.len())).small().weak());
                let t = crate::theme::tokens_ui(ui);
                egui::ScrollArea::vertical().max_height(430.0).auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("def-values").num_columns(5).spacing([12.0, 6.0]).striped(true).show(ui, |ui| {
                        for h in ["Default (key below)", "Value, or value by condition", "Unit", "Source", "Status"] {
                            ui_kit::th(ui, h);
                        }
                        ui.end_row();
                        for e in shown {
                            ui.vertical(|ui| {
                                ui.set_width(250.0);
                                ui.add(egui::Label::new(RichText::new(&e.title).size(crate::theme::FONT_SMALL + 1.0)).wrap()).on_hover_text(format!("Condition: {}", e.condition));
                                ui.label(RichText::new(&e.key).size(crate::theme::FONT_SMALL - 1.0).color(t.text_muted));
                            });
                            ui.vertical(|ui| {
                                ui.set_width(220.0);
                                ui.add(egui::Label::new(RichText::new(summary(e)).size(crate::theme::FONT_SMALL)).wrap());
                            });
                            ui.label(RichText::new(e.unit.symbol()).size(crate::theme::FONT_SMALL));
                            ui.vertical(|ui| {
                                ui.set_width(170.0);
                                ui.add(egui::Label::new(RichText::new(format!("{}, {}, p{}", e.document, e.clause, e.page)).size(crate::theme::FONT_SMALL)).wrap());
                            });
                            ui.vertical(|ui| {
                                ui.set_width(110.0);
                                ui.add(egui::Label::new(RichText::new(format!("{} ({})", e.status.label(), e.verb)).size(crate::theme::FONT_SMALL)).wrap());
                            });
                            ui.end_row();
                        }
                    });
                });
                if let Some(m) = &meta {
                    ui.label(RichText::new(&m.licence.attribution).small().weak());
                }
            },
        );
        if out.closed {
            st.open = false;
        }
        self.defaults_ui = st;
    }
}

/// The project settings tab: the choice of basis.
pub fn basis_section(ui: &mut Ui, chosen: &mut Option<String>) {
    ui_kit::section(ui, "Defaults basis", |ui| {
        let now = basis_id();
        let mut options: Vec<(String, String)> = bases();
        options.push((NO_BASIS.into(), "No defaults (every field is an empty input)".into()));
        let mut i = options.iter().position(|(id, _)| *id == now).unwrap_or(0);
        let labels: Vec<&str> = options.iter().map(|(_, n)| n.as_str()).collect();
        if ui_kit::radio_group(ui, &mut i, &labels) {
            *chosen = Some(options[i].0.clone());
        }
        ui.add_space(4.0);
        ui_kit::callout(ui, Level::Info, "The basis fills the fields of the dialogs with the values of one standard, each shown with its citation. It is stored in the document. A default is a starting value: the engineer confirms or changes it. See DEFAULTS for the whole table.");
        let _ = with(|d| d.active_set().map(|s| s.meta().review_status.clone())).map(|r| ui_kit::callout(ui, Level::Warning, r));
    });
}
