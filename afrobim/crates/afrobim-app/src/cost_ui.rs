//! The 5D tab's windows: the bill with its mapping rules and measured quantities, the rate
//! library, currencies, the estimate (priced bill, additions, audit trail, revisions), the cash
//! flow with its S-curve, and the interim payment certificate.
//!
//! Every number on these windows is typed or loaded by the user. Items with no rate are listed as
//! unpriced; they are never shown as priced at zero.

use std::collections::{BTreeMap, HashMap};

use afrobim_cost::{
    audit_csv, cash_flow_csv, certificate_csv, comparison_csv, compare, estimate_summary_csv, lines_to_csv, priced_boq_csv, quantities_from_progress, unmapped_csv, unpriced_boq_csv, Addition, AdditionBase,
    AdditionKind, CashFlow, Certificate, ChangeStatus, Comparison, Currency, Date, Decimal, Effect, Estimate, EstimateResult, GrossBasis, ItemKind, MappingRule, Money, PaymentRule,
    PaymentRuleKind, PaymentTerms, Phasing, PhasingTarget, Rate, RateLibrary, UnmappedReason,
};
use afrobim_kernel::{ElementId, SelectMode};
use egui::{pos2, Align2, Color32, FontId, RichText, Sense, Shape, Stroke, Ui};

use crate::app::AfroBimApp;
use crate::cost_glue as glue;

const BAD: Color32 = Color32::from_rgb(0xEF, 0x6C, 0x63);
const WARN: Color32 = Color32::from_rgb(0xD9, 0x8A, 0x2B);
const GOOD: Color32 = Color32::from_rgb(0x4D, 0xC0, 0x7A);

#[derive(Default, Clone)]
struct RateEdit {
    rate: String,
    currency: String,
    note: String,
    date: String,
    error: String,
}

#[derive(Default)]
struct AdditionForm {
    id: String,
    name: String,
    lump: bool,
    percent: String,
    base: u8,
    section: String,
    earlier: Vec<String>,
    amount: String,
    currency: String,
    error: String,
}

#[derive(Default)]
struct RuleForm {
    name: String,
    kind: u8,
    a: String,
    b: String,
    c: String,
    to_date_basis: bool,
    deduct: bool,
    error: String,
}

pub struct CostUi {
    pub est: Option<Estimate>,
    pub id: Option<ElementId>,
    loaded: serde_json::Value,
    pub lines: Option<afrobim_cost::QuantitySet>,
    pub result: Option<Result<EstimateResult, String>>,
    cash: Option<Result<CashFlow, String>>,
    pub bill_open: bool,
    pub rates_open: bool,
    pub currency_open: bool,
    pub estimate_open: bool,
    pub cash_open: bool,
    pub cert_open: bool,
    bill_tab: u8,
    est_tab: u8,
    selected_item: Option<String>,
    neutral_n: String,
    new_section: (String, String),
    new_item: (String, String, String, String),
    buf: HashMap<(String, u8), String>,
    rule_rows: Vec<[String; 5]>,
    rules_synced: bool,
    rule_error: String,
    problem: String,
    rate_rows: HashMap<String, RateEdit>,
    new_estimate: String,
    sec_code: String,
    sec_decimals: String,
    primary_text: (String, String),
    xr: (String, String, String, String, String),
    as_of_text: String,
    add_form: AdditionForm,
    snapshot_name: String,
    comparison: Option<Result<Comparison, String>>,
    phase_new: (String, String, String),
    phase_section: String,
    proposals: Option<(Vec<Phasing>, Vec<String>)>,
    terms: PaymentTerms,
    terms_loaded_for: Option<ElementId>,
    rule_form: RuleForm,
    certs: Vec<Certificate>,
    prev: usize,
    cert_label: String,
    qty_text: HashMap<String, String>,
    pct_text: HashMap<String, String>,
    cert_result: Option<Result<Certificate, String>>,
    cert_note: String,
    /// The date the site records are read at for the certificate, as typed.
    site_date: String,
    /// Elements with recorded progress that no bill item measures, so they will not be paid.
    site_missing: Vec<String>,
}

impl Default for CostUi {
    fn default() -> Self {
        CostUi {
            est: None,
            id: None,
            loaded: serde_json::Value::Null,
            lines: None,
            result: None,
            cash: None,
            bill_open: false,
            rates_open: false,
            currency_open: false,
            estimate_open: false,
            cash_open: false,
            cert_open: false,
            bill_tab: 0,
            est_tab: 0,
            selected_item: None,
            neutral_n: String::new(),
            new_section: Default::default(),
            new_item: Default::default(),
            buf: HashMap::new(),
            rule_rows: Vec::new(),
            rules_synced: false,
            rule_error: String::new(),
            problem: String::new(),
            rate_rows: HashMap::new(),
            new_estimate: String::new(),
            sec_code: String::new(),
            sec_decimals: String::new(),
            primary_text: (String::new(), String::new()),
            xr: Default::default(),
            as_of_text: String::new(),
            add_form: AdditionForm::default(),
            snapshot_name: String::new(),
            comparison: None,
            phase_new: Default::default(),
            phase_section: String::new(),
            proposals: None,
            terms: PaymentTerms::default(),
            terms_loaded_for: None,
            rule_form: RuleForm::default(),
            certs: Vec::new(),
            prev: 0,
            cert_label: String::new(),
            qty_text: HashMap::new(),
            pct_text: HashMap::new(),
            cert_result: None,
            cert_note: String::new(),
            site_date: String::new(),
            site_missing: Vec::new(),
        }
    }
}

// ─── small helpers ───────────────────────────────────────────────────────────────────────

fn tf(ui: &mut Ui, text: &mut String, hint: &str, width: f32) -> egui::Response {
    ui.add_sized([width, 20.0], egui::TextEdit::singleline(text).hint_text(hint))
}

fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let (mut rows, mut row, mut field, mut quoted) = (Vec::new(), Vec::new(), String::new(), false);
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            ('\r', false) => {}
            (c, _) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

fn csv_field(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn rules_text_to_bill(rows: &[[String; 5]], bill: &afrobim_cost::Bill) -> Result<Vec<MappingRule>, String> {
    let mut text = String::from("name,item,category,factor,conditions\n");
    for r in rows {
        text.push_str(&r.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
        text.push('\n');
    }
    let mut b = bill.clone();
    b.rules_from_csv(&text).map_err(|e| e.to_string())?;
    Ok(b.rules)
}

fn rows_of_rules(bill: &afrobim_cost::Bill) -> Vec<[String; 5]> {
    parse_csv(&bill.rules_to_csv())
        .into_iter()
        .skip(1)
        .filter(|r| r.len() >= 5)
        .map(|r| [r[0].clone(), r[1].clone(), r[2].clone(), r[3].clone(), r[4].clone()])
        .collect()
}

fn money(v: Decimal, decimals: u32) -> String {
    let text = v.to_fixed(decimals);
    // Thousands separators in the integer part.
    let (int, frac) = match text.split_once('.') {
        Some((i, f)) => (i.to_string(), format!(".{f}")),
        None => (text, String::new()),
    };
    let neg = int.starts_with('-');
    let digits: Vec<char> = int.trim_start_matches('-').chars().collect();
    let mut out = String::new();
    for (i, c) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*c);
    }
    format!("{}{out}{frac}", if neg { "-" } else { "" })
}

fn date_of(text: &str) -> Option<Date> {
    let t = text.trim();
    if t.len() == 10 {
        Date::parse(t).ok()
    } else {
        None
    }
}

impl CostUi {
    pub fn set_subtab(&mut self, n: u8) {
        self.bill_tab = n.min(3);
        self.est_tab = n.min(3);
    }

    fn decimals(&self) -> u32 {
        self.est.as_ref().and_then(|e| e.currencies.currency(&e.reporting_currency).ok()).map(|c| c.decimals).unwrap_or(0)
    }

    fn live(&self) -> Option<&EstimateResult> {
        self.result.as_ref().and_then(|r| r.as_ref().ok())
    }

    fn defined_currencies(&self) -> Vec<String> {
        self.est.as_ref().map(|e| e.currencies.currencies.iter().map(|c| c.code.clone()).collect()).unwrap_or_default()
    }
}

impl AfroBimApp {
    /// Reads the document's estimate when its data changed (open, undo).
    pub(crate) fn cost_sync(&mut self) {
        let wanted = self.cost.id.filter(|id| self.doc.find(*id).is_some()).or_else(|| glue::list(&self.doc).first().map(|e| e.0));
        let current = wanted.and_then(|id| self.doc.find(id)).map(|e| e.data.clone()).unwrap_or(serde_json::Value::Null);
        if wanted != self.cost.id || current != self.cost.loaded {
            self.cost.id = wanted;
            self.cost.est = wanted.and_then(|id| glue::load(&self.doc, id));
            self.cost.loaded = current;
            self.cost.rate_rows.clear();
            self.cost.rules_synced = false;
            self.cost.buf.clear();
            self.cost.comparison = None;
            self.cost.result = None;
            self.cost.cash = None;
            self.cost.cert_result = None;
            if self.cost.terms_loaded_for != wanted {
                self.cost.terms_loaded_for = wanted;
                self.cost.terms = wanted.map(|id| glue::terms_of(&self.doc, id)).unwrap_or_default();
                self.cost.certs = wanted.and_then(|id| glue::property(&self.doc, id, glue::PROP_CERTIFICATES)).unwrap_or_default();
            }
            if self.cost.est.is_some() && self.cost.lines.is_some() {
                self.cost_recompute();
            }
        }
    }

    /// Measures the model: the sign take-off, then every collector.
    pub(crate) fn cost_collect(&mut self) {
        self.sync_generated();
        let signs = glue::sign_rows(self.engine.as_deref(), &self.doc);
        let set = glue::collect(&self.doc, &signs);
        self.cost.lines = Some(set);
        self.cost_recompute();
    }

    pub(crate) fn cost_recompute(&mut self) {
        let c = &mut self.cost;
        let (Some(est), Some(lines)) = (&c.est, &c.lines) else { return };
        c.result = Some(est.compute(lines).map_err(|e| e.to_string()));
        c.cash = c.result.as_ref().and_then(|r| r.as_ref().ok()).map(|r| est.cash_flow(r).map_err(|e| e.to_string()));
    }

    /// Stores the estimate (with the summary of the last result) in the document and prices it again.
    fn cost_commit(&mut self) {
        let Some(id) = self.cost.id else { return };
        let Some(est) = self.cost.est.clone() else { return };
        self.cost_recompute();
        let skipped = self.cost.lines.as_ref().map(|l| l.skipped.len()).unwrap_or(0);
        let result = self.cost.live().cloned();
        if let Err(e) = glue::store(&mut self.doc, id, &est, result.as_ref(), skipped) {
            self.status = format!("Estimate not stored: {e}");
            return;
        }
        self.cost.loaded = self.doc.find(id).map(|e| e.data.clone()).unwrap_or_default();
        self.schedule_commit_now();
    }

    /// Opens a cost window: makes sure there is an estimate and measured lines.
    pub(crate) fn cost_open(&mut self) -> bool {
        self.cost_sync();
        if self.cost.est.is_none() {
            match glue::create(&mut self.doc, "Estimate 1") {
                Ok((id, est)) => {
                    self.cost.id = Some(id);
                    self.cost.est = Some(est);
                    self.cost.loaded = self.doc.find(id).map(|e| e.data.clone()).unwrap_or_default();
                    self.status = "Started an estimate in UGX (a label only; no rates or exchange rates are entered).".into();
                    self.schedule_commit_now();
                }
                Err(e) => {
                    self.status = format!("Could not start an estimate: {e}");
                    return false;
                }
            }
        }
        if self.cost.lines.is_none() {
            self.cost_collect();
        }
        true
    }

    pub(crate) fn export_cost_reports(&mut self) {
        let Some(dir) = rfd::FileDialog::new().set_title("Folder for the cost reports").pick_folder() else { return };
        self.write_cost_reports(&dir);
    }

    /// Every CSV the cost crate offers, into a folder.
    pub(crate) fn write_cost_reports(&mut self, dir: &std::path::Path) {
        self.cost_sync();
        if self.cost.est.is_none() {
            self.status = "There is no estimate to export. Open ESTIMATE first.".into();
            return;
        }
        if self.cost.lines.is_none() {
            self.cost_collect();
        }
        let c = &self.cost;
        let (Some(est), Some(lines)) = (&c.est, &c.lines) else { return };
        let mut files: Vec<(&str, String)> = vec![
            ("quantity_lines.csv", lines_to_csv(lines)),
            ("bill_structure.csv", est.bill.structure_to_csv()),
            ("mapping_rules.csv", est.bill.rules_to_csv()),
            ("rates.csv", est.rates.to_csv()),
        ];
        let mut skipped = Vec::new();
        match &c.result {
            Some(Ok(r)) => {
                files.push(("priced_boq.csv", priced_boq_csv(r)));
                files.push(("unpriced_boq.csv", unpriced_boq_csv(r)));
                files.push(("estimate_summary.csv", estimate_summary_csv(r)));
                files.push(("audit.csv", audit_csv(r)));
                files.push(("unmapped.csv", unmapped_csv(r)));
            }
            Some(Err(e)) => skipped.push(format!("priced reports skipped: {e}")),
            None => skipped.push("priced reports skipped: not computed".to_string()),
        }
        if let Some(Ok(cf)) = &c.cash {
            if !cf.months.is_empty() {
                files.push(("cash_flow.csv", cash_flow_csv(cf, c.decimals())));
            }
        }
        if let Some(Ok(cmp)) = &c.comparison {
            files.push(("comparison.csv", comparison_csv(cmp)));
        }
        if let Some(Ok(cert)) = &c.cert_result {
            files.push(("certificate.csv", certificate_csv(cert)));
        } else if let Some(cert) = c.certs.last() {
            files.push(("certificate.csv", certificate_csv(cert)));
        }
        let mut wrote = Vec::new();
        for (name, text) in files {
            match std::fs::write(dir.join(name), text) {
                Ok(()) => wrote.push(name.to_string()),
                Err(e) => skipped.push(format!("{name} not written: {e}")),
            }
        }
        self.status = format!("Cost reports in {}: {}.{}", dir.display(), wrote.join(", "), if skipped.is_empty() { String::new() } else { format!(" {}", skipped.join("; ")) });
    }

    pub(crate) fn cost_windows(&mut self, ctx: &egui::Context) {
        self.cost_sync();
        let mut changed = false;
        if self.cost.bill_open {
            changed |= self.bill_window(ctx);
        }
        if self.cost.rates_open {
            changed |= self.rates_window(ctx);
        }
        if self.cost.currency_open {
            changed |= self.currency_window(ctx);
        }
        if self.cost.estimate_open {
            changed |= self.estimate_window(ctx);
        }
        if self.cost.cash_open {
            changed |= self.cash_window(ctx);
        }
        if self.cost.cert_open {
            self.cert_window(ctx);
        }
        if changed {
            self.cost_commit();
        }
    }

    // ─── bill ────────────────────────────────────────────────────────────────────────────

    fn bill_window(&mut self, ctx: &egui::Context) -> bool {
        let mut open = true;
        let mut changed = false;
        let mut select: Option<ElementId> = None;
        let mut refresh = false;
        let mut status: Option<String> = None;
        let c = &mut self.cost;
        crate::win::new(ctx, "Bill of quantities").open(&mut open).default_width(980.0).default_height(560.0).show(ctx, |ui| {
            if c.est.is_none() {
                ui.label("There is no estimate yet.");
                return;
            }
            ui.horizontal(|ui| {
                for (i, t) in ["Structure", "Mapping rules", "Measured quantities", "Unmapped and ambiguous"].iter().enumerate() {
                    ui.selectable_value(&mut c.bill_tab, i as u8, *t);
                }
            });
            if let Some(Err(e)) = &c.result {
                ui.label(RichText::new(format!("Cannot measure yet: {e}")).color(BAD));
            }
            ui.separator();
            match c.bill_tab {
                0 => {
                    let est = c.est.as_mut().expect("checked above");
                    let bill = &mut est.bill;
                    ui.horizontal(|ui| {
                        if ui.button("Load structure CSV…").clicked() {
                            if let Some(path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).pick_file() {
                                match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| bill.from_structure_csv(&t).map_err(|e| e.to_string())) {
                                    Ok(()) => {
                                        status = Some(format!("Loaded the bill structure from {}.", path.display()));
                                        changed = true;
                                    }
                                    Err(e) => c.problem = format!("Not loaded: {e}"),
                                }
                            }
                        }
                        if ui.button("Save structure CSV…").clicked() {
                            if let Some(path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("bill_structure.csv").save_file() {
                                status = Some(match std::fs::write(&path, bill.structure_to_csv()) {
                                    Ok(()) => format!("Saved the bill structure to {}.", path.display()),
                                    Err(e) => format!("Could not write {}: {e}", path.display()),
                                });
                            }
                        }
                        ui.separator();
                        ui.label("Numbered sections to start from");
                        tf(ui, &mut c.neutral_n, "how many", 70.0);
                        if ui.button("Start a neutral bill").clicked() {
                            match c.neutral_n.trim().parse::<usize>() {
                                Ok(n) if n > 0 && n <= 99 => {
                                    let rules = std::mem::take(&mut bill.rules);
                                    *bill = afrobim_cost::Bill::neutral(n);
                                    bill.rules = rules;
                                    changed = true;
                                    c.problem.clear();
                                }
                                _ => c.problem = "Enter how many sections, from 1 to 99. They are numbered and titled 'Section n' for you to rename.".into(),
                            }
                        }
                    });
                    if !c.problem.is_empty() {
                        ui.label(RichText::new(&c.problem).color(BAD));
                    }
                    ui.label(RichText::new("The bill has no standard structure of its own: sections, items, units and descriptions are yours, loaded from a CSV (kind,code,section,description,unit) or typed here.").small().weak());
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("Sections ({})", bill.sections.len())).strong());
                    let mut delete_section = None;
                    egui::Grid::new("sections-grid").num_columns(4).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                        for (i, s) in bill.sections.iter_mut().enumerate() {
                            ui.label(&s.code);
                            changed |= ui.add_sized([260.0, 20.0], egui::TextEdit::singleline(&mut s.title)).changed();
                            let n = bill.items.iter().filter(|it| it.section == s.code).count();
                            ui.label(format!("{n} item(s)"));
                            if ui.add_enabled(n == 0, egui::Button::new("Delete").small()).clicked() {
                                delete_section = Some(i);
                            }
                            ui.end_row();
                        }
                    });
                    if let Some(i) = delete_section {
                        bill.sections.remove(i);
                        changed = true;
                    }
                    ui.horizontal(|ui| {
                        tf(ui, &mut c.new_section.0, "code", 60.0);
                        tf(ui, &mut c.new_section.1, "title", 220.0);
                        if ui.button("Add section").clicked() {
                            let code = c.new_section.0.trim().to_string();
                            if code.is_empty() || bill.section(&code).is_some() {
                                c.problem = "Give the section a code that is not already used.".into();
                            } else {
                                bill.sections.push(afrobim_cost::Section { code, title: c.new_section.1.trim().to_string() });
                                c.new_section = Default::default();
                                c.problem.clear();
                                changed = true;
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new(format!("Items ({})", bill.items.len())).strong());
                    let sections: Vec<String> = bill.sections.iter().map(|s| s.code.clone()).collect();
                    let mut delete_item = None;
                    egui::ScrollArea::vertical().id_salt("items-scroll").max_height(200.0).auto_shrink([false, true]).show(ui, |ui| {
                        egui::Grid::new("items-grid").num_columns(7).striped(true).spacing([8.0, 3.0]).show(ui, |ui| {
                            for h in ["Code", "Section", "Description", "Unit", "Quantity", "Reason (manual only)", ""] {
                                ui.label(RichText::new(h).strong());
                            }
                            ui.end_row();
                            for (i, it) in bill.items.iter_mut().enumerate() {
                                ui.label(&it.code);
                                egui::ComboBox::from_id_salt(("isec", i)).width(60.0).selected_text(it.section.clone()).show_ui(ui, |ui| {
                                    for s in &sections {
                                        changed |= ui.selectable_value(&mut it.section, s.clone(), s).changed();
                                    }
                                });
                                changed |= ui.add_sized([260.0, 20.0], egui::TextEdit::singleline(&mut it.description)).changed();
                                changed |= ui.add_sized([50.0, 20.0], egui::TextEdit::singleline(&mut it.unit)).changed();
                                match &mut it.kind {
                                    ItemKind::Measured => {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new("measured").weak());
                                            if ui.small_button("Make manual").on_hover_text("Enter a quantity by hand, with the reason it is not measured from the model").clicked() {
                                                it.kind = ItemKind::Manual { quantity: Decimal::ZERO, reason: String::new() };
                                                changed = true;
                                            }
                                        });
                                        ui.label("");
                                    }
                                    ItemKind::Manual { quantity, reason } => {
                                        let qt = c.buf.entry((it.code.clone(), 0)).or_insert_with(|| quantity.to_string());
                                        if tf(ui, qt, "quantity", 70.0).changed() {
                                            if let Ok(q) = Decimal::parse(qt) {
                                                *quantity = q;
                                                changed = true;
                                            }
                                        }
                                        changed |= ui.add_sized([220.0, 20.0], egui::TextEdit::singleline(reason).hint_text("why it is entered by hand")).changed();
                                    }
                                }
                                if ui.small_button("Delete").clicked() {
                                    delete_item = Some(i);
                                }
                                ui.end_row();
                            }
                        });
                    });
                    if let Some(i) = delete_item {
                        bill.items.remove(i);
                        changed = true;
                    }
                    ui.horizontal(|ui| {
                        tf(ui, &mut c.new_item.0, "code", 60.0);
                        egui::ComboBox::from_id_salt("new-item-section").width(70.0).selected_text(if c.new_item.1.is_empty() { "section".to_string() } else { c.new_item.1.clone() }).show_ui(ui, |ui| {
                            for s in &sections {
                                ui.selectable_value(&mut c.new_item.1, s.clone(), s);
                            }
                        });
                        tf(ui, &mut c.new_item.2, "description", 260.0);
                        tf(ui, &mut c.new_item.3, "unit", 50.0);
                        if ui.button("Add item").clicked() {
                            let (code, unit) = (c.new_item.0.trim().to_string(), c.new_item.3.trim().to_string());
                            if code.is_empty() || bill.item(&code).is_some() {
                                c.problem = "Give the item a code that is not already used.".into();
                            } else if c.new_item.1.is_empty() || unit.is_empty() {
                                c.problem = "Choose the item's section and give its unit.".into();
                            } else {
                                bill.items.push(afrobim_cost::Item::measured(&code, &c.new_item.1, c.new_item.2.trim(), &unit));
                                c.new_item = (String::new(), c.new_item.1.clone(), String::new(), String::new());
                                c.problem.clear();
                                changed = true;
                            }
                        }
                    });
                }
                1 => {
                    let est = c.est.as_mut().expect("checked above");
                    if !c.rules_synced {
                        c.rule_rows = rows_of_rules(&est.bill);
                        c.rules_synced = true;
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Load rules CSV…").clicked() {
                            if let Some(path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).pick_file() {
                                match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| est.bill.rules_from_csv(&t).map_err(|e| e.to_string())) {
                                    Ok(()) => {
                                        c.rules_synced = false;
                                        status = Some(format!("Loaded {} mapping rule(s) from {}.", est.bill.rules.len(), path.display()));
                                        changed = true;
                                    }
                                    Err(e) => c.rule_error = format!("Not loaded: {e}"),
                                }
                            }
                        }
                        if ui.button("Save rules CSV…").clicked() {
                            if let Some(path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("mapping_rules.csv").save_file() {
                                status = Some(match std::fs::write(&path, est.bill.rules_to_csv()) {
                                    Ok(()) => format!("Saved the mapping rules to {}.", path.display()),
                                    Err(e) => format!("Could not write {}: {e}", path.display()),
                                });
                            }
                        }
                        if ui.button("Add a rule").clicked() {
                            c.rule_rows.push([String::new(), String::new(), String::new(), String::new(), String::new()]);
                        }
                    });
                    ui.label(RichText::new("A rule sends the quantity lines of one category (a trailing * matches a prefix; * alone matches all) that satisfy its conditions to an item. Rules are tried in order and a line goes to the first that fits. Conditions: key=value, key=a|b, key~text, key>=n, key<=n, key?, separated by ;. The unit of a line must equal the item's unit unless you enter a factor (your own conversion).").small().weak());
                    let items: Vec<String> = est.bill.items.iter().map(|i| i.code.clone()).collect();
                    let mut delete = None;
                    let mut edited = false;
                    egui::ScrollArea::vertical().id_salt("rules-scroll").max_height(250.0).auto_shrink([false, true]).show(ui, |ui| {
                        egui::Grid::new("rules-grid").num_columns(6).striped(true).spacing([8.0, 3.0]).show(ui, |ui| {
                            for h in ["Name", "Item", "Category", "Factor", "Conditions", ""] {
                                ui.label(RichText::new(h).strong());
                            }
                            ui.end_row();
                            for (i, r) in c.rule_rows.iter_mut().enumerate() {
                                edited |= tf(ui, &mut r[0], "name", 190.0).changed();
                                egui::ComboBox::from_id_salt(("ritem", i)).width(70.0).selected_text(if r[1].is_empty() { "item".to_string() } else { r[1].clone() }).show_ui(ui, |ui| {
                                    for it in &items {
                                        edited |= ui.selectable_value(&mut r[1], it.clone(), it).changed();
                                    }
                                });
                                edited |= tf(ui, &mut r[2], "e.g. drain.length", 140.0).changed();
                                edited |= tf(ui, &mut r[3], "none", 60.0).changed();
                                edited |= tf(ui, &mut r[4], "key=value;key>=n", 220.0).changed();
                                if ui.small_button("Delete").clicked() {
                                    delete = Some(i);
                                }
                                ui.end_row();
                            }
                        });
                    });
                    if let Some(i) = delete {
                        c.rule_rows.remove(i);
                        edited = true;
                    }
                    if edited {
                        match rules_text_to_bill(&c.rule_rows, &est.bill) {
                            Ok(rules) => {
                                est.bill.rules = rules;
                                c.rule_error.clear();
                                changed = true;
                            }
                            Err(e) => c.rule_error = e,
                        }
                    }
                    if !c.rule_error.is_empty() {
                        ui.label(RichText::new(&c.rule_error).color(BAD));
                    }
                    ui.add_space(4.0);
                    if let Some(lines) = &c.lines {
                        egui::CollapsingHeader::new("Quantity lines the model offers (category, unit, count, attribute keys)").show(ui, |ui| {
                            let mut groups: BTreeMap<(String, String), (usize, std::collections::BTreeSet<String>)> = BTreeMap::new();
                            for l in &lines.lines {
                                let g = groups.entry((l.category.clone(), l.unit.clone())).or_default();
                                g.0 += 1;
                                g.1.extend(l.attributes.keys().cloned());
                            }
                            egui::Grid::new("offer-grid").num_columns(4).striped(true).spacing([10.0, 2.0]).show(ui, |ui| {
                                for ((cat, unit), (n, keys)) in &groups {
                                    ui.label(RichText::new(cat).monospace());
                                    ui.label(unit);
                                    ui.label(n.to_string());
                                    ui.label(RichText::new(keys.iter().cloned().collect::<Vec<_>>().join(", ")).small().weak());
                                    ui.end_row();
                                }
                            });
                        });
                    }
                }
                2 => {
                    ui.horizontal(|ui| {
                        if ui.button("Measure the model again").clicked() {
                            refresh = true;
                        }
                        if let Some(Ok(r)) = &c.result {
                            ui.label(format!("{} lines offered, {} mapped, {} unmapped.", r.measured.lines_offered, r.measured.lines_mapped, r.measured.unmapped.len()));
                        }
                    });
                    if let Some(Ok(r)) = &c.result {
                        let warnings = glue::double_count_warnings(&r.measured);
                        if !warnings.is_empty() {
                            egui::CollapsingHeader::new(RichText::new(format!("Possible double counting ({}): open to read", warnings.len())).color(WARN).strong()).default_open(false).show(ui, |ui| {
                                for w in &warnings {
                                    ui.label(RichText::new(w).small().color(WARN));
                                }
                            });
                        }
                        ui.label(RichText::new("Click an item to see the elements that make up its quantity; click an element to select it.").small().weak());
                        egui::ScrollArea::vertical().id_salt("measured-scroll").max_height(200.0).auto_shrink([false, true]).show(ui, |ui| {
                            egui::Grid::new("measured-grid").num_columns(5).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                                for h in ["Item", "Description", "Unit", "Quantity", "Lines"] {
                                    ui.label(RichText::new(h).strong());
                                }
                                ui.end_row();
                                for m in &r.measured.items {
                                    let on = c.selected_item.as_deref() == Some(m.item.code.as_str());
                                    if ui.selectable_label(on, &m.item.code).clicked() {
                                        c.selected_item = Some(m.item.code.clone());
                                    }
                                    ui.label(&m.item.description);
                                    ui.label(&m.item.unit);
                                    ui.label(m.quantity.to_string());
                                    ui.label(m.contributions.len().to_string());
                                    ui.end_row();
                                }
                            });
                        });
                        if let Some(m) = c.selected_item.as_ref().and_then(|code| r.measured.item(code)) {
                            ui.separator();
                            ui.label(RichText::new(format!("{} {}: {} {} from {} line(s)", m.item.code, m.item.description, m.quantity, m.item.unit, m.contributions.len())).strong());
                            egui::ScrollArea::vertical().id_salt("drill-scroll").max_height(160.0).show(ui, |ui| {
                                egui::Grid::new("drill-grid").num_columns(6).striped(true).spacing([10.0, 2.0]).show(ui, |ui| {
                                    for h in ["Element", "Line", "Rule", "Line quantity", "Factor", "Quantity"] {
                                        ui.label(RichText::new(h).strong());
                                    }
                                    ui.end_row();
                                    for k in &m.contributions {
                                        match k.element {
                                            Some(e) => {
                                                if ui.link(&k.mark).clicked() {
                                                    select = Some(e);
                                                }
                                            }
                                            None => {
                                                ui.label(RichText::new(format!("{} (no element)", k.mark)).weak());
                                            }
                                        }
                                        ui.label(&k.category);
                                        ui.label(&k.rule);
                                        ui.label(format!("{} {}", k.line_quantity, k.line_unit));
                                        ui.label(k.factor.to_string());
                                        ui.label(k.quantity.to_string());
                                        ui.end_row();
                                    }
                                });
                            });
                            for a in &m.adjustments {
                                ui.label(format!("Adjustment {}: {}", a.delta, a.reason));
                            }
                            if let Some(q) = m.manual {
                                ui.label(format!("Manual quantity {q}."));
                            }
                        }
                    } else {
                        ui.label(RichText::new("No result yet. Start the bill (Structure), add rules, then measure.").weak());
                    }
                    if let Some(lines) = &c.lines {
                        if !lines.skipped.is_empty() {
                            egui::CollapsingHeader::new(RichText::new(format!("Elements the model could not measure ({})", lines.skipped.len())).color(WARN)).show(ui, |ui| {
                                for s in &lines.skipped {
                                    ui.label(RichText::new(format!("{} {}: {}", s.source, s.mark, s.reason)).small());
                                }
                            });
                        }
                    }
                }
                _ => {
                    if let Some(Ok(r)) = &c.result {
                        let lines = c.lines.as_ref();
                        egui::ScrollArea::vertical().id_salt("unmapped-scroll").auto_shrink([false, false]).show(ui, |ui| {
                            ui.label(RichText::new(format!("Unmapped lines ({}): not in any item and not priced", r.measured.unmapped.len())).strong().color(if r.measured.unmapped.is_empty() { GOOD } else { WARN }));
                            if !r.measured.unmapped.is_empty() {
                                ui.label(RichText::new("By category and unit").small().weak());
                                egui::Grid::new("unmapped-totals").num_columns(4).striped(true).spacing([10.0, 2.0]).show(ui, |ui| {
                                    for (cat, unit, n, total) in r.measured.unmapped_totals() {
                                        ui.label(RichText::new(cat).monospace());
                                        ui.label(unit);
                                        ui.label(format!("{n} line(s)"));
                                        ui.label(total.to_string());
                                        ui.end_row();
                                    }
                                });
                                egui::CollapsingHeader::new("Every unmapped line").show(ui, |ui| {
                                    egui::Grid::new("unmapped-lines").num_columns(5).striped(true).spacing([10.0, 2.0]).show(ui, |ui| {
                                        for u in &r.measured.unmapped {
                                            match u.line.element {
                                                Some(e) => {
                                                    if ui.link(&u.line.mark).clicked() {
                                                        select = Some(e);
                                                    }
                                                }
                                                None => {
                                                    ui.label(&u.line.mark);
                                                }
                                            }
                                            ui.label(&u.line.category);
                                            ui.label(format!("{} {}", u.line.quantity, u.line.unit));
                                            ui.label(match &u.reason {
                                                UnmappedReason::NoRule => "no rule fits".to_string(),
                                                UnmappedReason::UnitMismatch { rule, line_unit, item_unit } => format!("rule '{rule}' sends {line_unit} to an item in {item_unit}: enter a factor"),
                                            });
                                            ui.label("");
                                            ui.end_row();
                                        }
                                    });
                                });
                            }
                            ui.add_space(8.0);
                            ui.label(RichText::new(format!("Ambiguous lines ({}): more than one rule fits; the first was used", r.measured.ambiguous.len())).strong().color(if r.measured.ambiguous.is_empty() { GOOD } else { WARN }));
                            egui::Grid::new("ambiguous-lines").num_columns(4).striped(true).spacing([10.0, 2.0]).show(ui, |ui| {
                                for a in &r.measured.ambiguous {
                                    let l = lines.and_then(|ls| ls.lines.get(a.line));
                                    ui.label(l.map(|l| l.mark.clone()).unwrap_or_default());
                                    ui.label(l.map(|l| l.category.clone()).unwrap_or_default());
                                    ui.label(format!("used '{}'", a.used));
                                    ui.label(format!("also fits {}", a.also.join(", ")));
                                    ui.end_row();
                                }
                            });
                        });
                    } else {
                        ui.label(RichText::new("No result yet.").weak());
                    }
                }
            }
        });
        self.cost.bill_open = open;
        if refresh {
            self.cost_collect();
        }
        if let Some(id) = select {
            self.doc.select(&[id], SelectMode::Replace);
        }
        if let Some(s) = status {
            self.status = s;
        }
        if changed {
            self.cost.rules_synced = self.cost.rules_synced && self.cost.rule_error.is_empty();
        }
        changed
    }

    // ─── rates ───────────────────────────────────────────────────────────────────────────

    fn rates_window(&mut self, ctx: &egui::Context) -> bool {
        let mut open = true;
        let mut changed = false;
        let mut status: Option<String> = None;
        let c = &mut self.cost;
        let currencies = c.defined_currencies();
        let unpriced: Vec<(String, String)> = c.live().map(|r| r.unpriced.iter().map(|u| (u.code.clone(), u.reason.clone())).collect()).unwrap_or_default();
        crate::win::new(ctx, "Rate library").open(&mut open).default_width(960.0).default_height(520.0).show(ctx, |ui| {
            let Some(est) = c.est.as_mut() else {
                ui.label("There is no estimate yet.");
                return;
            };
            let primary = est.reporting_currency.clone();
            ui.horizontal(|ui| {
                if ui.button("Load rates CSV…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).pick_file() {
                        match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| RateLibrary::from_csv(&t).map_err(|e| e.to_string())) {
                            Ok(lib) => {
                                for r in &lib.rates {
                                    if est.currencies.currency(&r.currency).is_err() {
                                        est.currencies.add_currency(Currency::new(&r.currency, 2));
                                        status = Some(format!("The file uses currency {}, which was added with 2 decimals; set its decimals under CURRENCY.", r.currency));
                                    }
                                }
                                let n = lib.rates.len();
                                est.rates = lib;
                                c.rate_rows.clear();
                                changed = true;
                                status.get_or_insert(format!("Loaded {n} rate(s) from {}.", path.display()));
                            }
                            Err(e) => c.problem = format!("Not loaded: {e}"),
                        }
                    }
                }
                if ui.button("Save rates CSV…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("rates.csv").save_file() {
                        status = Some(match std::fs::write(&path, est.rates.to_csv()) {
                            Ok(()) => format!("Saved {} rate(s) to {}.", est.rates.rates.len(), path.display()),
                            Err(e) => format!("Could not write {}: {e}", path.display()),
                        });
                    }
                }
            });
            if !c.problem.is_empty() {
                ui.label(RichText::new(&c.problem).color(BAD));
            }
            ui.label(RichText::new("The library is empty until you enter or load rates. Every rate carries its currency, a source note and the date it was taken. An item with no rate is listed as unpriced; it is not priced at zero.").small().weak());
            if est.bill.items.is_empty() {
                ui.label(RichText::new("The bill has no items yet (BILL).").weak());
            }
            egui::ScrollArea::both().id_salt("rates-scroll").auto_shrink([false, false]).max_height(300.0).show(ui, |ui| {
                egui::Grid::new("rates-grid").num_columns(8).striped(true).spacing([8.0, 3.0]).show(ui, |ui| {
                    for h in ["Item", "Description", "Unit", "Rate", "Currency", "Source note", "Date", ""] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    let items: Vec<(String, String, String)> = est.bill.items.iter().map(|i| (i.code.clone(), i.description.clone(), i.unit.clone())).collect();
                    for (code, desc, unit) in items {
                        let existing = est.rates.rates.iter().find(|r| r.item_code == code && r.currency.eq_ignore_ascii_case(&primary)).or_else(|| est.rates.rates.iter().find(|r| r.item_code == code));
                        let edit = c.rate_rows.entry(code.clone()).or_insert_with(|| match existing {
                            Some(r) => RateEdit { rate: r.rate.to_string(), currency: r.currency.clone(), note: r.source_note.clone(), date: r.date.map(|d| d.to_string()).unwrap_or_default(), error: String::new() },
                            None => RateEdit { currency: primary.clone(), ..RateEdit::default() },
                        });
                        let before = edit.clone();
                        ui.label(&code);
                        ui.label(desc.clone());
                        ui.label(unit.clone());
                        tf(ui, &mut edit.rate, "rate", 90.0);
                        egui::ComboBox::from_id_salt(("rcur", &code)).width(60.0).selected_text(edit.currency.clone()).show_ui(ui, |ui| {
                            for cur in &currencies {
                                ui.selectable_value(&mut edit.currency, cur.clone(), cur);
                            }
                        });
                        tf(ui, &mut edit.note, "where the rate comes from", 200.0);
                        tf(ui, &mut edit.date, "YYYY-MM-DD", 90.0);
                        if edit.rate != before.rate || edit.currency != before.currency || edit.note != before.note || edit.date != before.date {
                            let cur = edit.currency.trim().to_uppercase();
                            if edit.rate.trim().is_empty() {
                                est.rates.rates.retain(|r| !(r.item_code == code && r.currency.eq_ignore_ascii_case(&cur)));
                                edit.error.clear();
                                changed = true;
                            } else {
                                let date = if edit.date.trim().is_empty() { Ok(None) } else { Date::parse(&edit.date).map(Some).map_err(|e| e.to_string()) };
                                match (Decimal::parse(&edit.rate).map_err(|e| e.to_string()), date) {
                                    (Ok(rate), Ok(date)) => {
                                        // A changed currency replaces the rate of the old one.
                                        if before.currency != edit.currency {
                                            est.rates.rates.retain(|r| !(r.item_code == code && r.currency.eq_ignore_ascii_case(&before.currency)));
                                        }
                                        est.rates.set(Rate { item_code: code.clone(), description: desc.clone(), unit: unit.clone(), rate, currency: cur, source_note: edit.note.trim().to_string(), date });
                                        edit.error.clear();
                                        changed = true;
                                    }
                                    (Err(e), _) | (_, Err(e)) => edit.error = e,
                                }
                            }
                        }
                        if edit.error.is_empty() {
                            let priced = est.rates.rates.iter().any(|r| r.item_code == code);
                            ui.label(if priced { RichText::new("priced").small().color(GOOD) } else { RichText::new("no rate").small().color(WARN) });
                        } else {
                            ui.label(RichText::new(&edit.error).small().color(BAD));
                        }
                        ui.end_row();
                    }
                });
            });
            let known: Vec<String> = est.bill.items.iter().map(|i| i.code.clone()).collect();
            let stray = est.rates.rates.iter().filter(|r| !known.contains(&r.item_code)).count();
            if stray > 0 {
                ui.label(RichText::new(format!("{stray} rate(s) in the library are for item codes that are not in the bill.")).small().weak());
            }
            if !unpriced.is_empty() {
                ui.separator();
                ui.label(RichText::new(format!("Unpriced items ({}): shown as unpriced, never as zero", unpriced.len())).color(WARN).strong());
                for (code, reason) in &unpriced {
                    ui.label(RichText::new(format!("{code}: {reason}")).small().color(WARN));
                }
            }
        });
        self.cost.rates_open = open;
        if let Some(s) = status {
            self.status = s;
        }
        changed
    }

    // ─── currency ────────────────────────────────────────────────────────────────────────

    fn currency_window(&mut self, ctx: &egui::Context) -> bool {
        let mut open = true;
        let mut changed = false;
        let mut status: Option<String> = None;
        let c = &mut self.cost;
        crate::win::new(ctx, "Currency").open(&mut open).default_width(620.0).show(ctx, |ui| {
            let Some(est) = c.est.as_mut() else {
                ui.label("There is no estimate yet.");
                return;
            };
            if c.primary_text.0.is_empty() {
                c.primary_text = (est.reporting_currency.clone(), est.currencies.currency(&est.reporting_currency).map(|x| x.decimals.to_string()).unwrap_or_else(|_| "0".into()));
            }
            ui.label(RichText::new("The primary currency is a label for the totals. UGX with no decimals is the starting label only: it carries no exchange rate and no rate for any item.").small().weak());
            ui.horizontal(|ui| {
                ui.label("Primary");
                tf(ui, &mut c.primary_text.0, "code", 60.0);
                ui.label("decimals");
                tf(ui, &mut c.primary_text.1, "0", 40.0);
                if ui.button("Apply").clicked() {
                    match (c.primary_text.0.trim().to_uppercase(), c.primary_text.1.trim().parse::<u32>()) {
                        (code, Ok(d)) if !code.is_empty() && d <= 6 => {
                            est.currencies.add_currency(Currency::new(&code, d));
                            est.reporting_currency = code;
                            changed = true;
                            c.problem.clear();
                        }
                        _ => c.problem = "Enter a currency code and decimals from 0 to 6.".into(),
                    }
                }
            });
            ui.separator();
            ui.label(RichText::new("Secondary currency (optional)").strong());
            ui.horizontal(|ui| {
                let shown = est.secondary_currency.clone().unwrap_or_else(|| "none".to_string());
                ui.label(format!("Shown beside the primary totals: {shown}"));
                if est.secondary_currency.is_some() && ui.small_button("Remove").clicked() {
                    est.secondary_currency = None;
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                tf(ui, &mut c.sec_code, "code, e.g. a second currency", 150.0);
                tf(ui, &mut c.sec_decimals, "decimals", 70.0);
                if ui.button("Use as the secondary currency").clicked() {
                    match (c.sec_code.trim().to_uppercase(), c.sec_decimals.trim().parse::<u32>()) {
                        (code, Ok(d)) if !code.is_empty() && d <= 6 && code != est.reporting_currency => {
                            est.currencies.add_currency(Currency::new(&code, d));
                            est.secondary_currency = Some(code);
                            changed = true;
                            c.problem = "A secondary currency needs a dated exchange rate. Enter it below; until then the estimate cannot be computed.".into();
                        }
                        _ => c.problem = "Enter a code different from the primary and decimals from 0 to 6.".into(),
                    }
                }
            });
            ui.separator();
            ui.label(RichText::new("Exchange rates (entered by you, each with its date and source)").strong());
            ui.label(RichText::new("A rate reads 1 FROM = RATE TO. It converts the other way by its reciprocal. The estimate uses the latest rate on or before the as-of date.").small().weak());
            let codes: Vec<String> = est.currencies.currencies.iter().map(|x| x.code.clone()).collect();
            let mut delete = None;
            egui::Grid::new("xr-grid").num_columns(6).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                for (i, r) in est.currencies.rates.iter().enumerate() {
                    ui.label(format!("1 {} =", r.from));
                    ui.label(format!("{} {}", r.rate, r.to));
                    ui.label(r.date.to_string());
                    ui.label(RichText::new(&r.source_note).small().weak());
                    ui.label("");
                    if ui.small_button("Delete").clicked() {
                        delete = Some(i);
                    }
                    ui.end_row();
                }
            });
            if let Some(i) = delete {
                est.currencies.rates.remove(i);
                changed = true;
            }
            ui.horizontal(|ui| {
                for (salt, field) in [("xr-from", &mut c.xr.0), ("xr-to", &mut c.xr.1)] {
                    egui::ComboBox::from_id_salt(salt).width(60.0).selected_text(if field.is_empty() { "currency".to_string() } else { field.clone() }).show_ui(ui, |ui| {
                        for code in &codes {
                            ui.selectable_value(field, code.clone(), code);
                        }
                    });
                }
                tf(ui, &mut c.xr.2, "rate", 90.0);
                tf(ui, &mut c.xr.3, "YYYY-MM-DD", 90.0);
                tf(ui, &mut c.xr.4, "source note", 160.0);
                if ui.button("Add rate").clicked() {
                    let result = Decimal::parse(&c.xr.2).map_err(|e| e.to_string()).and_then(|rate| Date::parse(&c.xr.3).map_err(|e| e.to_string()).map(|d| (rate, d))).and_then(|(rate, d)| est.currencies.add_rate(&c.xr.0, &c.xr.1, rate, d, c.xr.4.trim()).map_err(|e| e.to_string()));
                    match result {
                        Ok(()) => {
                            c.xr = Default::default();
                            c.problem.clear();
                            changed = true;
                        }
                        Err(e) => c.problem = e,
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("As-of date");
                if c.as_of_text.is_empty() {
                    c.as_of_text = est.as_of.map(|d| d.to_string()).unwrap_or_default();
                }
                if tf(ui, &mut c.as_of_text, "blank uses the latest rate", 150.0).lost_focus() {
                    let new = if c.as_of_text.trim().is_empty() { None } else { date_of(&c.as_of_text) };
                    if (c.as_of_text.trim().is_empty() || new.is_some()) && new != est.as_of {
                        est.as_of = new;
                        changed = true;
                    }
                }
            });
            if !c.problem.is_empty() {
                ui.label(RichText::new(&c.problem).color(WARN));
            }
            if let Some(Err(e)) = &c.result {
                ui.label(RichText::new(e).small().color(BAD));
            }
            status = None;
        });
        self.cost.currency_open = open;
        if let Some(s) = status {
            self.status = s;
        }
        changed
    }

    // ─── estimate ────────────────────────────────────────────────────────────────────────

    fn estimate_window(&mut self, ctx: &egui::Context) -> bool {
        let mut open = true;
        let mut changed = false;
        let mut refresh = false;
        let mut new_est = false;
        let mut switch: Option<ElementId> = None;
        let mut status: Option<String> = None;
        let mut snapshot = false;
        let mut compare_snapshot: Option<String> = None;
        let mut compare_estimate: Option<ElementId> = None;
        let mut export_csv = false;
        let estimates = glue::list(&self.doc);
        let snapshots: Vec<String> = self.cost.id.and_then(|id| self.doc.find(id)).map(|e| e.properties.keys().filter_map(|k| k.strip_prefix(glue::PROP_SNAPSHOT).map(str::to_string)).collect()).unwrap_or_default();
        let current_id = self.cost.id;
        let c = &mut self.cost;
        let decimals = c.decimals();
        crate::win::new(ctx, "Estimate").open(&mut open).default_width(1020.0).default_height(600.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Estimate");
                let name = c.est.as_ref().map(|e| e.name.clone()).unwrap_or_default();
                egui::ComboBox::from_id_salt("est-pick").selected_text(name).show_ui(ui, |ui| {
                    for (id, n) in &estimates {
                        if ui.selectable_label(Some(*id) == current_id, n).clicked() {
                            switch = Some(*id);
                        }
                    }
                });
                tf(ui, &mut c.new_estimate, "name of a new estimate", 170.0);
                if ui.button("New estimate").clicked() {
                    new_est = true;
                }
                if ui.button("Measure the model again").clicked() {
                    refresh = true;
                }
            });
            let Some(est) = c.est.as_mut() else {
                ui.label("There is no estimate yet.");
                return;
            };
            ui.horizontal(|ui| {
                ui.label("Revision");
                changed |= tf(ui, &mut est.revision, "your label, e.g. Rev A", 120.0).changed();
                ui.label("Name");
                changed |= ui.add_sized([220.0, 20.0], egui::TextEdit::singleline(&mut est.name)).changed();
                if est.name.contains("DEMO") {
                    ui.label(RichText::new("demo values: not real rates").color(WARN).strong());
                }
            });
            match &c.result {
                Some(Err(e)) => {
                    ui.label(RichText::new(format!("The estimate cannot be computed: {e}")).color(BAD));
                }
                Some(Ok(r)) => {
                    if !r.unpriced.is_empty() {
                        ui.label(RichText::new(format!("{} item(s) have no usable rate and are shown as unpriced, not as zero: {}. Enter their rates under RATES.", r.unpriced.len(), r.unpriced.iter().map(|u| u.code.as_str()).collect::<Vec<_>>().join(", "))).color(WARN));
                    }
                    if !r.measured.unmapped.is_empty() {
                        ui.label(RichText::new(format!("{} quantity line(s) are not mapped to any item and are not priced (see BILL, Unmapped).", r.measured.unmapped.len())).color(WARN));
                    }
                    let warnings = glue::double_count_warnings(&r.measured);
                    if !warnings.is_empty() {
                        ui.label(RichText::new(format!("{} element(s) are measured by lines of different kinds and may be priced twice: see BILL, Measured quantities.", warnings.len())).color(WARN));
                    }
                }
                None => {
                    ui.label(RichText::new("Not computed yet.").weak());
                }
            }
            ui.horizontal(|ui| {
                for (i, t) in ["Priced bill", "Additions", "Audit trail", "Revisions"].iter().enumerate() {
                    ui.selectable_value(&mut c.est_tab, i as u8, *t);
                }
            });
            ui.separator();
            match c.est_tab {
                0 => match &c.result {
                    Some(Ok(r)) => {
                        let secondary = r.secondary.clone();
                        ui.horizontal(|ui| {
                            if ui.button("Export CSV…").clicked() {
                                export_csv = true;
                            }
                            ui.label(RichText::new(format!("Amounts in {}{}.", r.reporting.code, secondary.as_ref().map(|s| format!(", with {} beside them", s.code)).unwrap_or_default())).small().weak());
                        });
                        egui::ScrollArea::both().id_salt("priced-scroll").auto_shrink([false, false]).show(ui, |ui| {
                            egui::Grid::new("priced-grid").num_columns(if secondary.is_some() { 7 } else { 6 }).spacing([12.0, 3.0]).show(ui, |ui| {
                                let mut heads = vec!["Item".to_string(), "Description".into(), "Unit".into(), "Quantity".into(), "Rate".into(), format!("Amount {}", r.reporting.code)];
                                if let Some(s) = &secondary {
                                    heads.push(format!("Amount {}", s.code));
                                }
                                for h in heads {
                                    ui.label(RichText::new(h).strong());
                                }
                                ui.end_row();
                                for sec in &r.sections {
                                    ui.label(RichText::new(&sec.code).strong());
                                    ui.label(RichText::new(&sec.title).strong());
                                    for _ in 0..3 {
                                        ui.label("");
                                    }
                                    ui.label(RichText::new(money(sec.amount_reporting, decimals)).strong());
                                    if let Some(s) = &secondary {
                                        ui.label(RichText::new(sec.amount_secondary.map(|a| money(a, s.decimals)).unwrap_or_default()).strong());
                                    }
                                    ui.end_row();
                                    for it in r.items.iter().filter(|i| i.section == sec.code) {
                                        ui.label(&it.code);
                                        ui.label(&it.description);
                                        ui.label(&it.unit);
                                        ui.label(it.quantity.to_string());
                                        match it.rate {
                                            Some(rate) => {
                                                ui.label(format!("{} {}", rate, it.rate_currency.clone().unwrap_or_default()));
                                                ui.label(money(it.amount_reporting, decimals));
                                                if let Some(s) = &secondary {
                                                    ui.label(it.amount_secondary.map(|a| money(a, s.decimals)).unwrap_or_default());
                                                }
                                            }
                                            None => {
                                                ui.label(RichText::new("no rate").color(WARN));
                                                ui.label(RichText::new("unpriced").color(WARN));
                                                if secondary.is_some() {
                                                    ui.label(RichText::new("unpriced").color(WARN));
                                                }
                                            }
                                        }
                                        ui.end_row();
                                    }
                                }
                                ui.separator();
                                ui.end_row();
                                let total_row = |ui: &mut Ui, label: &str, a: Decimal, sa: Option<Decimal>, bold: bool| {
                                    ui.label("");
                                    let t = |s: String| if bold { RichText::new(s).strong() } else { RichText::new(s) };
                                    ui.label(t(label.to_string()));
                                    for _ in 0..3 {
                                        ui.label("");
                                    }
                                    ui.label(t(money(a, decimals)));
                                    if let Some(s) = &secondary {
                                        ui.label(t(sa.map(|v| money(v, s.decimals)).unwrap_or_default()));
                                    }
                                    ui.end_row();
                                };
                                total_row(ui, "Bill total", r.subtotal, r.subtotal_secondary, true);
                                for a in &r.additions {
                                    let how = match (a.percent, a.base) {
                                        (Some(p), Some(b)) => format!(" ({p} % of {})", money(b, decimals)),
                                        _ => String::new(),
                                    };
                                    total_row(ui, &format!("{}{how}", a.name), a.amount, a.amount_secondary, false);
                                }
                                total_row(ui, "Grand total", r.grand_total, r.grand_total_secondary, true);
                            });
                        });
                    }
                    _ => {
                        ui.label(RichText::new("Nothing to show. Start the bill and enter rates.").weak());
                    }
                },
                1 => {
                    let sections: Vec<String> = est.bill.sections.iter().map(|s| s.code.clone()).collect();
                    let currencies: Vec<String> = est.currencies.currencies.iter().map(|x| x.code.clone()).collect();
                    ui.label(RichText::new("Additions are applied in list order, each rounded to the primary currency. Preliminaries, contingency, provisional sums, dayworks and taxes are whatever you name here, with your own percentages and amounts. None are built in.").small().weak());
                    let mut delete = None;
                    let mut mv: Option<(usize, isize)> = None;
                    egui::Grid::new("additions-grid").num_columns(5).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                        for (i, a) in est.additions.iter().enumerate() {
                            ui.label(&a.id);
                            ui.label(&a.name);
                            ui.label(match &a.kind {
                                AdditionKind::Percentage { percent, base } => format!(
                                    "{percent} % of {}",
                                    match base {
                                        AdditionBase::BillTotal => "the bill total".to_string(),
                                        AdditionBase::RunningTotal => "the running total".to_string(),
                                        AdditionBase::Section(s) => format!("section {s}"),
                                        AdditionBase::Additions(l) => format!("additions {}", l.join(", ")),
                                    }
                                ),
                                AdditionKind::LumpSum(m) => format!("lump sum {} {}", m.amount, m.currency),
                            });
                            ui.horizontal(|ui| {
                                if ui.add_enabled(i > 0, egui::Button::new("Up").small()).clicked() {
                                    mv = Some((i, -1));
                                }
                                if ui.add_enabled(i + 1 < est.additions.len(), egui::Button::new("Down").small()).clicked() {
                                    mv = Some((i, 1));
                                }
                            });
                            if ui.small_button("Delete").clicked() {
                                delete = Some(i);
                            }
                            ui.end_row();
                        }
                    });
                    if let Some(i) = delete {
                        est.additions.remove(i);
                        changed = true;
                    }
                    if let Some((i, d)) = mv {
                        let j = (i as isize + d) as usize;
                        est.additions.swap(i, j);
                        changed = true;
                    }
                    ui.separator();
                    ui.label(RichText::new("Add an addition").strong());
                    let f = &mut c.add_form;
                    ui.horizontal(|ui| {
                        tf(ui, &mut f.id, "id", 60.0);
                        tf(ui, &mut f.name, "name", 180.0);
                        ui.radio_value(&mut f.lump, false, "percentage");
                        ui.radio_value(&mut f.lump, true, "lump sum");
                    });
                    ui.horizontal(|ui| {
                        if f.lump {
                            tf(ui, &mut f.amount, "amount", 100.0);
                            egui::ComboBox::from_id_salt("add-cur").width(60.0).selected_text(if f.currency.is_empty() { "currency".to_string() } else { f.currency.clone() }).show_ui(ui, |ui| {
                                for cur in &currencies {
                                    ui.selectable_value(&mut f.currency, cur.clone(), cur);
                                }
                            });
                        } else {
                            tf(ui, &mut f.percent, "percent (negative for a discount)", 200.0);
                            ui.label("of");
                            egui::ComboBox::from_id_salt("add-base").width(160.0).selected_text(["the bill total", "the running total", "a section", "earlier additions"][f.base as usize]).show_ui(ui, |ui| {
                                for (k, l) in ["the bill total", "the running total", "a section", "earlier additions"].iter().enumerate() {
                                    ui.selectable_value(&mut f.base, k as u8, *l);
                                }
                            });
                            if f.base == 2 {
                                egui::ComboBox::from_id_salt("add-section").width(70.0).selected_text(if f.section.is_empty() { "section".to_string() } else { f.section.clone() }).show_ui(ui, |ui| {
                                    for s in &sections {
                                        ui.selectable_value(&mut f.section, s.clone(), s);
                                    }
                                });
                            }
                        }
                    });
                    if !f.lump && f.base == 3 {
                        ui.horizontal(|ui| {
                            for a in &est.additions {
                                let mut on = f.earlier.contains(&a.id);
                                if ui.checkbox(&mut on, &a.id).changed() {
                                    if on {
                                        f.earlier.push(a.id.clone());
                                    } else {
                                        f.earlier.retain(|x| x != &a.id);
                                    }
                                }
                            }
                        });
                    }
                    if ui.button("Add to the end of the list").clicked() {
                        let built = (|| -> Result<Addition, String> {
                            let id = f.id.trim().to_string();
                            if id.is_empty() || est.additions.iter().any(|a| a.id == id) {
                                return Err("Give the addition an id that is not already used.".into());
                            }
                            let kind = if f.lump {
                                let amount = Decimal::parse(&f.amount).map_err(|e| e.to_string())?;
                                if f.currency.is_empty() {
                                    return Err("Choose the currency of the lump sum.".into());
                                }
                                AdditionKind::LumpSum(Money::new(amount, &f.currency))
                            } else {
                                let percent = Decimal::parse(&f.percent).map_err(|e| e.to_string())?;
                                let base = match f.base {
                                    0 => AdditionBase::BillTotal,
                                    1 => AdditionBase::RunningTotal,
                                    2 if !f.section.is_empty() => AdditionBase::Section(f.section.clone()),
                                    2 => return Err("Choose the section.".into()),
                                    _ if !f.earlier.is_empty() => AdditionBase::Additions(f.earlier.clone()),
                                    _ => return Err("Tick the earlier additions the percentage is taken of.".into()),
                                };
                                AdditionKind::Percentage { percent, base }
                            };
                            Ok(Addition { id, name: f.name.trim().to_string(), kind })
                        })();
                        match built {
                            Ok(a) => {
                                est.additions.push(a);
                                *f = AdditionForm::default();
                                changed = true;
                            }
                            Err(e) => f.error = e,
                        }
                    }
                    if !f.error.is_empty() {
                        ui.label(RichText::new(&f.error).color(BAD));
                    }
                }
                2 => match &c.result {
                    Some(Ok(r)) => {
                        egui::ScrollArea::vertical().id_salt("audit-scroll").auto_shrink([false, false]).show(ui, |ui| {
                            ui.label(RichText::new("How each quantity was made: every line, element, rule and factor, then adjustments and manual quantities.").small().weak());
                            for m in &r.measured.items {
                                egui::CollapsingHeader::new(format!("{}  {}: {} {}", m.item.code, m.item.description, m.quantity, m.item.unit)).id_salt(("audit", &m.item.code)).show(ui, |ui| {
                                    egui::Grid::new(("audit-grid", &m.item.code)).num_columns(5).striped(true).spacing([10.0, 2.0]).show(ui, |ui| {
                                        for k in &m.contributions {
                                            ui.label(&k.mark);
                                            ui.label(&k.category);
                                            ui.label(format!("{} {} x {}", k.line_quantity, k.line_unit, k.factor));
                                            ui.label(format!("rule '{}'", k.rule));
                                            ui.label(k.quantity.to_string());
                                            ui.end_row();
                                        }
                                    });
                                    for a in &m.adjustments {
                                        ui.label(format!("Adjustment {}: {}", a.delta, a.reason));
                                    }
                                    if let Some(q) = m.manual {
                                        ui.label(format!("Manual quantity {q}"));
                                    }
                                    ui.label(RichText::new(format!("From lines {}, unrounded {}, rounded {}.", m.from_lines, m.unrounded, m.quantity)).small().weak());
                                });
                            }
                        });
                    }
                    _ => {
                        ui.label(RichText::new("Nothing to show yet.").weak());
                    }
                },
                _ => {
                    ui.label(RichText::new("Compare the estimate with an earlier state of the model, or with another estimate in this document (priced against today's model).").small().weak());
                    ui.horizontal(|ui| {
                        tf(ui, &mut c.snapshot_name, "name of the snapshot", 180.0);
                        if ui.add_enabled(!c.snapshot_name.trim().is_empty() && c.lines.is_some(), egui::Button::new("Save a snapshot of the measured model")).clicked() {
                            snapshot = true;
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        if snapshots.is_empty() {
                            ui.label(RichText::new("No snapshots saved.").weak());
                        }
                        for s in &snapshots {
                            if ui.button(format!("Compare '{s}' with the model now")).clicked() {
                                compare_snapshot = Some(s.clone());
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for (id, n) in estimates.iter().filter(|(id, _)| Some(*id) != current_id) {
                            if ui.button(format!("Compare with estimate '{n}'")).clicked() {
                                compare_estimate = Some(*id);
                            }
                        }
                    });
                    match &c.comparison {
                        Some(Err(e)) => {
                            ui.label(RichText::new(e).color(BAD));
                        }
                        Some(Ok(cmp)) => {
                            ui.label(RichText::new(format!("{} against {} ({}): bill total {} to {}, grand total {} to {} ({}).", cmp.old_label, cmp.new_label, cmp.currency, money(cmp.subtotal_old, decimals), money(cmp.subtotal_new, decimals), money(cmp.grand_old, decimals), money(cmp.grand_new, decimals), money(cmp.grand_change, decimals))).strong());
                            for w in &cmp.warnings {
                                ui.label(RichText::new(w).small().color(WARN));
                            }
                            egui::ScrollArea::both().id_salt("cmp-scroll").auto_shrink([false, false]).show(ui, |ui| {
                                egui::Grid::new("cmp-grid").num_columns(9).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                                    for h in ["Item", "Description", "Status", "Qty old", "Qty new", "Amount old", "Amount new", "Change", "Change %"] {
                                        ui.label(RichText::new(h).strong());
                                    }
                                    ui.end_row();
                                    for i in &cmp.items {
                                        ui.label(&i.code);
                                        ui.label(&i.description);
                                        ui.label(match i.status {
                                            ChangeStatus::Added => "added",
                                            ChangeStatus::Removed => "removed",
                                            ChangeStatus::Changed => "changed",
                                            ChangeStatus::Unchanged => "unchanged",
                                        });
                                        ui.label(i.quantity_old.to_string());
                                        ui.label(i.quantity_new.to_string());
                                        ui.label(money(i.amount_old, decimals));
                                        ui.label(money(i.amount_new, decimals));
                                        ui.label(money(i.amount_change, decimals));
                                        ui.label(i.percent_change.map(|p| format!("{p} %")).unwrap_or_default());
                                        ui.end_row();
                                    }
                                });
                            });
                        }
                        None => {}
                    }
                }
            }
        });
        self.cost.estimate_open = open;
        if let Some(id) = switch {
            self.cost.id = Some(id);
            self.cost.loaded = serde_json::Value::Null;
            self.cost.terms_loaded_for = None;
            self.cost_sync();
            self.cost_collect();
        }
        if new_est {
            let name = if self.cost.new_estimate.trim().is_empty() { "Estimate".to_string() } else { self.cost.new_estimate.trim().to_string() };
            match glue::create(&mut self.doc, &name) {
                Ok((id, _)) => {
                    self.cost.new_estimate.clear();
                    self.cost.id = Some(id);
                    self.cost.loaded = serde_json::Value::Null;
                    self.cost.terms_loaded_for = None;
                    self.cost_sync();
                    status = Some(format!("Started '{name}' in UGX with no rates."));
                    self.schedule_commit_now();
                }
                Err(e) => status = Some(format!("Not created: {e}")),
            }
        }
        if refresh {
            self.cost_collect();
            status = Some("Measured the model again.".into());
        }
        if snapshot {
            if let (Some(id), Some(lines)) = (self.cost.id, self.cost.lines.clone()) {
                let key = format!("{}{}", glue::PROP_SNAPSHOT, self.cost.snapshot_name.trim());
                glue::set_property(&mut self.doc, id, &key, &lines);
                self.cost.snapshot_name.clear();
                status = Some("Snapshot of the measured model saved in the document.".into());
                self.schedule_commit_now();
            }
        }
        if let Some(name) = compare_snapshot {
            if let (Some(id), Some(est), Some(now)) = (self.cost.id, self.cost.est.clone(), self.cost.lines.clone()) {
                self.cost.comparison = Some(match glue::property::<afrobim_cost::QuantitySet>(&self.doc, id, &format!("{}{name}", glue::PROP_SNAPSHOT)) {
                    Some(old) => est.compare_revisions(&old, &now).map(|mut c| {
                        c.old_label = format!("snapshot '{name}'");
                        c.new_label = "the model now".into();
                        c
                    }).map_err(|e| e.to_string()),
                    None => Err("That snapshot cannot be read.".into()),
                });
            }
        }
        if let Some(other) = compare_estimate {
            if let (Some(est), Some(lines), Some(Ok(now))) = (self.cost.est.clone(), self.cost.lines.clone(), self.cost.result.clone()) {
                let _ = est;
                self.cost.comparison = Some(match glue::load(&self.doc, other) {
                    Some(o) => o.compute(&lines).map(|old| compare(&old, &now)).map_err(|e| e.to_string()),
                    None => Err("That estimate cannot be read.".into()),
                });
            }
        }
        if export_csv {
            if let Some(Ok(r)) = &self.cost.result {
                if let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("priced_boq.csv").save_file() {
                    if path.extension().is_none() {
                        path.set_extension("csv");
                    }
                    status = Some(match std::fs::write(&path, priced_boq_csv(r)) {
                        Ok(()) => format!("Wrote the priced bill to {}.", path.display()),
                        Err(e) => format!("Could not write {}: {e}", path.display()),
                    });
                }
            }
        }
        if let Some(s) = status {
            self.status = s;
        }
        changed
    }

    // ─── cash flow ───────────────────────────────────────────────────────────────────────

    fn cash_window(&mut self, ctx: &egui::Context) -> bool {
        let mut open = true;
        let mut changed = false;
        let mut propose = false;
        let mut apply = false;
        let mut export = false;
        let mut status: Option<String> = None;
        let sched_ready = matches!(self.sched.result, Some(Ok(_))) && self.sched.states.is_some();
        let c = &mut self.cost;
        let decimals = c.decimals();
        crate::win::new(ctx, "Cash flow").open(&mut open).default_width(900.0).default_height(640.0).show(ctx, |ui| {
            let Some(est) = c.est.as_mut() else {
                ui.label("There is no estimate yet.");
                return;
            };
            ui.label(RichText::new("Each window spreads an item's whole amount evenly by calendar day, start and finish both counted. Whatever has no window is reported as unphased, never guessed.").small().weak());
            let items: Vec<String> = est.bill.items.iter().map(|i| i.code.clone()).collect();
            let sections: Vec<String> = est.bill.sections.iter().map(|s| s.code.clone()).collect();
            let mut delete = None;
            egui::ScrollArea::vertical().id_salt("phasing-scroll").max_height(150.0).show(ui, |ui| {
                egui::Grid::new("phasing-grid").num_columns(4).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                    for h in ["Item or addition", "Start", "Finish", ""] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for (i, p) in est.phasing.iter().enumerate() {
                        ui.label(match &p.target {
                            PhasingTarget::Item(code) => format!("item {code}"),
                            PhasingTarget::Element(id) => format!("element {}", id.0),
                            PhasingTarget::Addition(a) => format!("addition {a}"),
                        });
                        ui.label(p.start.to_string());
                        ui.label(p.finish.to_string());
                        if ui.small_button("Delete").clicked() {
                            delete = Some(i);
                        }
                        ui.end_row();
                    }
                });
            });
            if let Some(i) = delete {
                est.phasing.remove(i);
                changed = true;
            }
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("ph-item").width(90.0).selected_text(if c.phase_new.0.is_empty() { "item".to_string() } else { c.phase_new.0.clone() }).show_ui(ui, |ui| {
                    for i in &items {
                        ui.selectable_value(&mut c.phase_new.0, i.clone(), i);
                    }
                });
                tf(ui, &mut c.phase_new.1, "start YYYY-MM-DD", 120.0);
                tf(ui, &mut c.phase_new.2, "finish YYYY-MM-DD", 120.0);
                if ui.button("Add window for the item").clicked() {
                    match (date_of(&c.phase_new.1), date_of(&c.phase_new.2)) {
                        (Some(s), Some(f)) if !c.phase_new.0.is_empty() && f >= s => {
                            est.phasing.retain(|p| p.target != PhasingTarget::Item(c.phase_new.0.clone()));
                            est.phasing.push(Phasing { target: PhasingTarget::Item(c.phase_new.0.clone()), start: s, finish: f, distribution: Default::default() });
                            c.problem.clear();
                            changed = true;
                        }
                        _ => c.problem = "Choose an item and enter a start and a finish date (YYYY-MM-DD), finish not before start.".into(),
                    }
                }
                egui::ComboBox::from_id_salt("ph-section").width(80.0).selected_text(if c.phase_section.is_empty() { "section".to_string() } else { c.phase_section.clone() }).show_ui(ui, |ui| {
                    for s in &sections {
                        ui.selectable_value(&mut c.phase_section, s.clone(), s);
                    }
                });
                if ui.button("Same dates for the whole section").clicked() {
                    match (date_of(&c.phase_new.1), date_of(&c.phase_new.2)) {
                        (Some(s), Some(f)) if !c.phase_section.is_empty() && f >= s => {
                            for it in est.bill.items.iter().filter(|i| i.section == c.phase_section) {
                                est.phasing.retain(|p| p.target != PhasingTarget::Item(it.code.clone()));
                                est.phasing.push(Phasing { target: PhasingTarget::Item(it.code.clone()), start: s, finish: f, distribution: Default::default() });
                            }
                            c.problem.clear();
                            changed = true;
                        }
                        _ => c.problem = "Choose a section and enter the dates above.".into(),
                    }
                }
            });
            if !c.problem.is_empty() {
                ui.label(RichText::new(&c.problem).color(BAD));
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.add_enabled(sched_ready, egui::Button::new("Take the dates from the linked activities")).on_disabled_hover_text("Needs a scheduled programme with elements linked to activities (4D tab).").clicked() {
                    propose = true;
                }
                if !sched_ready {
                    ui.label(RichText::new("No scheduled programme: enter the windows by hand.").small().weak());
                }
                if ui.button("Export cash flow CSV…").clicked() {
                    export = true;
                }
            });
            if let Some((list, notes)) = &c.proposals {
                ui.label(RichText::new(format!("From the programme: {} item window(s) proposed. An item takes the earliest start and latest finish of the activities its elements are linked to.", list.len())).small());
                egui::CollapsingHeader::new("Proposed windows and what was left out").default_open(true).show(ui, |ui| {
                    for p in list {
                        if let PhasingTarget::Item(code) = &p.target {
                            ui.label(RichText::new(format!("item {code}: {} to {}", p.start, p.finish)).small());
                        }
                    }
                    for n in notes {
                        ui.label(RichText::new(n).small().color(WARN));
                    }
                });
                if ui.button("Use these windows (they replace the windows of the same items)").clicked() {
                    apply = true;
                }
            }
            ui.separator();
            match &c.cash {
                Some(Ok(cf)) if !cf.months.is_empty() => {
                    ui.label(format!("{} {}: phased {}, unphased {}, of a grand total of {}.", cf.currency, "cash flow", money(cf.phased_total, decimals), money(cf.unphased_total, decimals), money(cf.grand_total, decimals)));
                    for w in &cf.warnings {
                        ui.label(RichText::new(w).small().color(WARN));
                    }
                    if !cf.unphased_total.is_zero() {
                        ui.label(RichText::new("Part of the estimate has no window and is not in the months below.").small().color(WARN));
                    }
                    ui.horizontal_top(|ui| {
                        egui::ScrollArea::vertical().id_salt("months-scroll").max_height(280.0).show(ui, |ui| {
                            egui::Grid::new("months-grid").num_columns(4).striped(true).spacing([14.0, 3.0]).show(ui, |ui| {
                                for h in ["Month", "Amount", "Cumulative", "Cumulative %"] {
                                    ui.label(RichText::new(h).strong());
                                }
                                ui.end_row();
                                for m in &cf.months {
                                    ui.label(format!("{}-{:02}", m.year, m.month));
                                    ui.label(money(m.amount, decimals));
                                    ui.label(money(m.cumulative, decimals));
                                    ui.label(format!("{} %", m.cumulative_percent));
                                    ui.end_row();
                                }
                            });
                        });
                        paint_s_curve(ui, cf, decimals);
                    });
                }
                Some(Ok(_)) => {
                    ui.label(RichText::new("No windows entered yet: there is nothing to phase.").weak());
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(format!("Cash flow not computed: {e}")).color(BAD));
                }
                None => {
                    ui.label(RichText::new("The estimate has no result yet.").weak());
                }
            }
        });
        self.cost.cash_open = open;
        if propose {
            if let (Some(Ok(r)), Some(Ok(s)), Some(p), Some(st)) = (self.cost.result.clone(), self.sched.result.clone(), self.sched.prog.clone(), self.sched.states.clone()) {
                self.cost.proposals = Some(glue::phasing_from_programme(&r.measured, &p, &s, &st));
                status = Some("Proposed cash-flow windows from the programme. Read them, then accept or ignore.".into());
            }
        }
        if apply {
            if let (Some(est), Some((list, _))) = (self.cost.est.as_mut(), self.cost.proposals.take()) {
                for p in list {
                    est.phasing.retain(|q| q.target != p.target);
                    est.phasing.push(p);
                }
                changed = true;
                status = Some("Cash-flow windows taken from the programme.".into());
            }
        }
        if export {
            if let Some(Ok(cf)) = &self.cost.cash {
                if let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("cash_flow.csv").save_file() {
                    if path.extension().is_none() {
                        path.set_extension("csv");
                    }
                    status = Some(match std::fs::write(&path, cash_flow_csv(cf, self.cost.decimals())) {
                        Ok(()) => format!("Wrote the cash flow to {}.", path.display()),
                        Err(e) => format!("Could not write {}: {e}", path.display()),
                    });
                }
            }
        }
        if let Some(s) = status {
            self.status = s;
        }
        changed
    }

    // ─── interim payment certificate ─────────────────────────────────────────────────────

    fn cert_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut run = false;
        let mut percent = false;
        let mut site = false;
        let mut save = false;
        let mut export = false;
        let mut terms_changed = false;
        let sched_percent = self.sched.prog.is_some();
        let c = &mut self.cost;
        let decimals = c.decimals();
        crate::win::new(ctx, "Interim payment certificate").open(&mut open).default_width(960.0).default_height(700.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(620.0).auto_shrink([false, true]).show(ui, |ui| {
            ui.label(RichText::new("This module encodes no FIDIC clause and no other contract's terms. Every percentage, cap, threshold and amount below is your own contract data, entered by you: AfroBIM only does the arithmetic on it and does not decide what a contract entitles anyone to.").color(WARN));
            let (Some(est), Some(Ok(result))) = (c.est.as_ref(), c.result.as_ref()) else {
                ui.label("The estimate has no result yet. Start the bill, enter rates and open ESTIMATE.");
                return;
            };
            ui.separator();
            ui.label(RichText::new("Payment terms (rules, shown and applied in this order)").strong());
            let mut delete = None;
            egui::Grid::new("terms-grid").num_columns(3).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                for (i, r) in c.terms.rules.iter().enumerate() {
                    ui.label(&r.name);
                    ui.label(match &r.kind {
                        PaymentRuleKind::Retention { percent, cap } => format!("retention {percent} % of gross to date{}", cap.map(|c| format!(", up to {c}")).unwrap_or_default()),
                        PaymentRuleKind::AdvanceRecovery { advance, percent, start_threshold } => format!("recover an advance of {advance} at {percent} % of gross above {start_threshold}"),
                        PaymentRuleKind::PercentOfGross { percent, basis, effect } => format!("{} {percent} % of gross {}", if *effect == Effect::Add { "add" } else { "deduct" }, if *basis == GrossBasis::ToDate { "to date" } else { "this period" }),
                        PaymentRuleKind::Lump { amount, effect } => format!("{} a lump of {amount}", if *effect == Effect::Add { "add" } else { "deduct" }),
                    });
                    if ui.small_button("Delete").clicked() {
                        delete = Some(i);
                    }
                    ui.end_row();
                }
            });
            if let Some(i) = delete {
                c.terms.rules.remove(i);
                terms_changed = true;
            }
            let f = &mut c.rule_form;
            ui.horizontal(|ui| {
                tf(ui, &mut f.name, "rule name", 130.0);
                egui::ComboBox::from_id_salt("rule-kind").width(170.0).selected_text(["Retention", "Advance recovery", "Percent of gross", "Lump amount"][f.kind as usize]).show_ui(ui, |ui| {
                    for (k, l) in ["Retention", "Advance recovery", "Percent of gross", "Lump amount"].iter().enumerate() {
                        ui.selectable_value(&mut f.kind, k as u8, *l);
                    }
                });
                match f.kind {
                    0 => {
                        tf(ui, &mut f.a, "percent", 70.0);
                        tf(ui, &mut f.b, "cap (optional)", 100.0);
                    }
                    1 => {
                        tf(ui, &mut f.a, "advance", 90.0);
                        tf(ui, &mut f.b, "percent", 70.0);
                        tf(ui, &mut f.c, "start threshold", 100.0);
                    }
                    2 => {
                        tf(ui, &mut f.a, "percent", 70.0);
                        ui.radio_value(&mut f.to_date_basis, true, "to date");
                        ui.radio_value(&mut f.to_date_basis, false, "this period");
                        ui.radio_value(&mut f.deduct, false, "adds");
                        ui.radio_value(&mut f.deduct, true, "deducts");
                    }
                    _ => {
                        tf(ui, &mut f.a, "amount", 100.0);
                        ui.radio_value(&mut f.deduct, false, "adds");
                        ui.radio_value(&mut f.deduct, true, "deducts");
                    }
                }
                if ui.button("Add rule").clicked() {
                    let p = |s: &str| Decimal::parse(s).map_err(|e| e.to_string());
                    let effect = if f.deduct { Effect::Deduct } else { Effect::Add };
                    let kind = match f.kind {
                        0 => p(&f.a).and_then(|percent| glue::decimal_of(&f.b).map(|cap| PaymentRuleKind::Retention { percent, cap })),
                        1 => p(&f.a).and_then(|advance| p(&f.b).and_then(|percent| p(&f.c).map(|start_threshold| PaymentRuleKind::AdvanceRecovery { advance, percent, start_threshold }))),
                        2 => p(&f.a).map(|percent| PaymentRuleKind::PercentOfGross { percent, basis: if f.to_date_basis { GrossBasis::ToDate } else { GrossBasis::ThisPeriod }, effect }),
                        _ => p(&f.a).map(|amount| PaymentRuleKind::Lump { amount, effect }),
                    };
                    match kind {
                        _ if f.name.trim().is_empty() || c.terms.rules.iter().any(|r| r.name == f.name.trim()) => f.error = "Give the rule a name that is not already used.".into(),
                        Ok(kind) => {
                            c.terms.rules.push(PaymentRule { name: f.name.trim().to_string(), kind });
                            *f = RuleForm::default();
                            terms_changed = true;
                        }
                        Err(e) => f.error = e,
                    }
                }
            });
            if !f.error.is_empty() {
                ui.label(RichText::new(&f.error).color(BAD));
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Certificate");
                tf(ui, &mut c.cert_label, "your label, e.g. No. 3", 140.0);
                ui.label("Previous certificate");
                let shown = if c.prev == 0 { "none (first)".to_string() } else { c.certs.get(c.prev - 1).map(|x| x.label.clone()).unwrap_or_default() };
                egui::ComboBox::from_id_salt("prev-cert").selected_text(shown).show_ui(ui, |ui| {
                    ui.selectable_value(&mut c.prev, 0, "none (first)");
                    for (i, x) in c.certs.iter().enumerate() {
                        ui.selectable_value(&mut c.prev, i + 1, &x.label);
                    }
                });
            });
            ui.label(RichText::new("Quantities to date: type them, or give a percent complete per item and press the button. Percent complete is taken from the linked activities where the programme has them, and from the percent you enter for an item otherwise; an element with neither is 0 %.").small().weak());
            egui::ScrollArea::vertical().id_salt("cert-qty").max_height(210.0).auto_shrink([false, true]).show(ui, |ui| {
                egui::Grid::new("cert-qty-grid").num_columns(6).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                    for h in ["Item", "Description", "Unit", "Bill quantity", "Quantity to date", "Percent complete"] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for it in result.items.iter().filter(|i| i.rate_reporting.is_some()) {
                        ui.label(&it.code);
                        ui.label(&it.description);
                        ui.label(&it.unit);
                        ui.label(it.quantity.to_string());
                        let q = c.qty_text.entry(it.code.clone()).or_default();
                        tf(ui, q, "to date", 90.0);
                        let pc = c.pct_text.entry(it.code.clone()).or_default();
                        tf(ui, pc, "0 to 100", 70.0);
                        ui.end_row();
                    }
                });
            });
            let unpriced = result.items.iter().filter(|i| i.rate_reporting.is_none()).count();
            if unpriced > 0 {
                ui.label(RichText::new(format!("{unpriced} unpriced item(s) cannot be certified and are not listed.")).small().color(WARN));
            }
            ui.horizontal(|ui| {
                if ui.button("Fill the quantities from percent complete").clicked() {
                    percent = true;
                }
                if ui.button("Work out the certificate").clicked() {
                    run = true;
                }
                if ui.add_enabled(matches!(c.cert_result, Some(Ok(_))), egui::Button::new("Keep it as a certificate (for the next one)")).clicked() {
                    save = true;
                }
                if ui.add_enabled(matches!(c.cert_result, Some(Ok(_))), egui::Button::new("Export CSV…")).clicked() {
                    export = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Site records at");
                tf(ui, &mut c.site_date, "YYYY-MM-DD", 90.0);
                if ui.button("Take quantities from site records at date").on_hover_text("Each element's quantity times its recorded percent complete at the date (Construction tab, SITEREC); the percents typed above are used for adjustments and manual items").clicked() {
                    site = true;
                }
            });
            if !c.site_missing.is_empty() {
                ui.label(RichText::new(format!("Recorded elements that no bill item measures, so they are not paid: {}.", c.site_missing.join(", "))).small().color(WARN));
            }
            if !c.cert_note.is_empty() {
                ui.label(RichText::new(&c.cert_note).small().color(WARN));
            }
            match &c.cert_result {
                Some(Err(e)) => {
                    ui.label(RichText::new(e).color(BAD));
                }
                Some(Ok(cert)) => {
                    ui.separator();
                    ui.label(RichText::new(format!("Certificate {} ({})", cert.label, cert.currency)).strong());
                    egui::Grid::new("cert-out").num_columns(4).spacing([16.0, 3.0]).show(ui, |ui| {
                        for h in ["", "To date", "Previous", "This period"] {
                            ui.label(RichText::new(h).strong());
                        }
                        ui.end_row();
                        ui.label("Gross value of work");
                        ui.label(money(cert.gross_to_date, decimals));
                        ui.label(money(cert.gross_previous, decimals));
                        ui.label(money(cert.gross_this_period, decimals));
                        ui.end_row();
                        for r in &cert.rules {
                            ui.label(format!("{} {}", if r.effect == Effect::Add { "plus" } else { "less" }, r.name));
                            ui.label(money(r.to_date, decimals));
                            ui.label(money(r.previous, decimals));
                            ui.label(money(r.this_period, decimals));
                            ui.end_row();
                        }
                        ui.label(RichText::new("Net").strong());
                        ui.label(RichText::new(money(cert.net_to_date, decimals)).strong());
                        ui.label("");
                        ui.label(RichText::new(money(cert.net_this_period, decimals)).strong());
                        ui.end_row();
                    });
                    for w in &cert.warnings {
                        ui.label(RichText::new(w).small().color(WARN));
                    }
                    egui::CollapsingHeader::new("Lines").show(ui, |ui| {
                        egui::Grid::new("cert-lines").num_columns(6).striped(true).spacing([10.0, 2.0]).show(ui, |ui| {
                            for l in cert.lines.iter().filter(|l| !l.amount_to_date.is_zero() || !l.amount_previous.is_zero()) {
                                ui.label(&l.code);
                                ui.label(&l.description);
                                ui.label(format!("{} {}", l.quantity_to_date, l.unit));
                                ui.label(format!("rate {}", l.rate));
                                ui.label(money(l.amount_to_date, decimals));
                                ui.label(money(l.amount_this_period, decimals));
                                ui.end_row();
                            }
                        });
                    });
                }
                None => {}
            }
            let _ = est;
            });
        });
        self.cost.cert_open = open;
        if terms_changed {
            if let Some(id) = self.cost.id {
                glue::set_property(&mut self.doc, id, glue::PROP_TERMS, &self.cost.terms.clone());
                self.schedule_commit_now();
            }
        }
        if percent {
            self.cert_fill_from_percent(sched_percent);
        }
        if site {
            self.cert_from_site();
        }
        if run {
            self.cert_run();
        }
        if save {
            if let (Some(Ok(cert)), Some(id)) = (self.cost.cert_result.clone(), self.cost.id) {
                self.cost.certs.push(cert);
                self.cost.prev = self.cost.certs.len();
                glue::set_property(&mut self.doc, id, glue::PROP_CERTIFICATES, &self.cost.certs.clone());
                self.cost.qty_text.clear();
                self.cost.pct_text.clear();
                self.cost.cert_label.clear();
                self.status = "Certificate kept. The next one takes its state as the previous certificate.".into();
                self.schedule_commit_now();
            }
        }
        if export {
            if let Some(Ok(cert)) = &self.cost.cert_result {
                if let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("certificate.csv").save_file() {
                    if path.extension().is_none() {
                        path.set_extension("csv");
                    }
                    self.status = match std::fs::write(&path, certificate_csv(cert)) {
                        Ok(()) => format!("Wrote the certificate to {}.", path.display()),
                        Err(e) => format!("Could not write {}: {e}", path.display()),
                    };
                }
            }
        }
    }

    /// For `--subtab 1` with `--demo`: fills 50 percent complete (an arbitrary demo value) on every
    /// priced item and works the certificate out, so the result can be looked at without a pointer.
    pub(crate) fn cert_demo_run(&mut self) {
        let codes: Vec<String> = match &self.cost.result {
            Some(Ok(r)) => r.items.iter().filter(|i| i.rate_reporting.is_some()).map(|i| i.code.clone()).collect(),
            _ => return,
        };
        for c in codes {
            self.cost.pct_text.insert(c, "50".into());
        }
        self.cost.cert_label = "Demo certificate".into();
        self.cert_fill_from_percent(false);
        self.cert_run();
    }

    /// Fills the quantities to date from the site records at a date (the progress crate's
    /// `qty_to_date`) and lists the recorded elements no bill item measures.
    fn cert_from_site(&mut self) {
        self.construction.sync(&self.doc);
        let (Some(est), Some(Ok(result))) = (&self.cost.est, &self.cost.result) else { return };
        let text = self.cost.site_date.trim();
        let date = if text.len() == 10 { afrobim_schedule::Date::parse(text) } else { None };
        let Some(date) = date else {
            self.cost.cert_result = Some(Err(if text.is_empty() { "Enter the date of the site records (YYYY-MM-DD).".to_string() } else { format!("'{text}' is not a date (YYYY-MM-DD).") }));
            return;
        };
        let mut item_percent: BTreeMap<String, Decimal> = BTreeMap::new();
        for (code, t) in &self.cost.pct_text {
            match glue::decimal_of(t) {
                Ok(Some(v)) => {
                    item_percent.insert(code.clone(), v);
                }
                Ok(None) => {}
                Err(e) => {
                    self.cost.cert_result = Some(Err(format!("Percent complete of {code}: {e}")));
                    return;
                }
            }
        }
        match crate::construction_glue::site_quantities(result, &self.construction.book.log, date, &item_percent, &est.bill.rounding) {
            Ok(q) => {
                for (code, v) in &q.quantities {
                    if result.item(code).is_some_and(|i| i.rate_reporting.is_some()) {
                        self.cost.qty_text.insert(code.clone(), v.to_string());
                    }
                }
                self.cost.site_missing = q.elements_without_item.iter().map(|id| self.doc.find(*id).map(|e| e.name.clone()).unwrap_or_else(|| format!("element {}", id.0))).collect();
                self.cost.cert_note = format!(
                    "Quantities filled from the site records at {date}: each element's quantity times its recorded percent complete.{}",
                    if self.construction.book.log.records().is_empty() { " There are no site records yet, so every quantity is 0." } else { "" }
                );
                self.cost.cert_result = None;
            }
            Err(e) => self.cost.cert_result = Some(Err(e)),
        }
    }

    fn cert_fill_from_percent(&mut self, programme: bool) {
        let (Some(est), Some(Ok(result))) = (&self.cost.est, &self.cost.result) else { return };
        let mut element_percent: BTreeMap<ElementId, Decimal> = BTreeMap::new();
        if programme {
            if let (Some(p), Some(st)) = (&self.sched.prog, &self.sched.states) {
                for e in &st.states {
                    let best = e.links.iter().filter_map(|l| p.activity(&l.activity)).map(|a| a.progress.percent_complete).fold(None::<f64>, |m, v| Some(m.map_or(v, |x| x.max(v))));
                    if let Some(v) = best.and_then(|v| Decimal::from_f64(v.clamp(0.0, 100.0), 2).ok()) {
                        element_percent.insert(e.element, v);
                    }
                }
            }
        }
        let mut item_percent: BTreeMap<String, Decimal> = BTreeMap::new();
        for (code, text) in &self.cost.pct_text {
            match glue::decimal_of(text) {
                Ok(Some(v)) => {
                    item_percent.insert(code.clone(), v);
                    if let Some(m) = result.measured.item(code) {
                        for e in m.elements() {
                            element_percent.insert(e, v);
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    self.cost.cert_result = Some(Err(format!("Percent complete of {code}: {e}")));
                    return;
                }
            }
        }
        match quantities_from_progress(result, &element_percent, &item_percent, &est.bill.rounding) {
            Ok(q) => {
                for (code, v) in q {
                    if result.item(&code).is_some_and(|i| i.rate_reporting.is_some()) {
                        self.cost.qty_text.insert(code, v.to_string());
                    }
                }
                self.cost.cert_note = if programme { "Quantities filled from percent complete: linked activities first, then the percentages you entered.".into() } else { "Quantities filled from the percentages you entered.".into() };
                self.cost.cert_result = None;
            }
            Err(e) => self.cost.cert_result = Some(Err(e.to_string())),
        }
    }

    fn cert_run(&mut self) {
        let c = &mut self.cost;
        let (Some(est), Some(Ok(result))) = (&c.est, &c.result) else { return };
        let mut quantities = BTreeMap::new();
        for (code, text) in &c.qty_text {
            match glue::decimal_of(text) {
                Ok(Some(v)) => {
                    quantities.insert(code.clone(), v);
                }
                Ok(None) => {}
                Err(e) => {
                    c.cert_result = Some(Err(format!("Quantity to date of {code}: {e}")));
                    return;
                }
            }
        }
        let label = if c.cert_label.trim().is_empty() { "Certificate".to_string() } else { c.cert_label.trim().to_string() };
        let previous = c.prev.checked_sub(1).and_then(|i| c.certs.get(i)).map(|x| x.state.clone());
        let rounding = est.bill.rounding.amount_for(c.decimals());
        c.cert_result = Some(afrobim_cost::certify(result, &c.terms, &label, &quantities, previous.as_ref(), rounding).map_err(|e| e.to_string()));
        c.cert_note.clear();
    }
}

/// The S-curve of cumulative cost against time.
fn paint_s_curve(ui: &mut Ui, cf: &CashFlow, decimals: u32) {
    let pts = cf.s_curve();
    let size = egui::vec2(340.0, 240.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(0xF6, 0xF5, 0xF0));
    let inner = rect.shrink2(egui::vec2(46.0, 24.0));
    let ink = Color32::from_rgb(0x22, 0x22, 0x22);
    if pts.len() < 2 {
        return;
    }
    let max = pts.last().map(|p| p.cumulative.to_f64()).unwrap_or(0.0).max(1e-9);
    for q in 0..=4 {
        let y = inner.bottom() - inner.height() * q as f32 / 4.0;
        painter.line_segment([pos2(inner.left(), y), pos2(inner.right(), y)], Stroke::new(1.0_f32, Color32::from_rgb(0xD0, 0xD0, 0xCC)));
        painter.text(pos2(inner.left() - 4.0, y), Align2::RIGHT_CENTER, format!("{}%", q * 25), FontId::proportional(10.0), ink);
    }
    let n = (pts.len() - 1) as f32;
    let map = |i: usize, v: f64| pos2(inner.left() + inner.width() * i as f32 / n, inner.bottom() - inner.height() * (v / max) as f32);
    let line: Vec<_> = pts.iter().enumerate().map(|(i, p)| map(i, p.cumulative.to_f64())).collect();
    painter.add(Shape::line(line, Stroke::new(2.0_f32, Color32::from_rgb(0x00, 0x4F, 0x9E))));
    painter.text(pos2(inner.left(), rect.bottom() - 4.0), Align2::LEFT_BOTTOM, pts[0].date.to_string(), FontId::proportional(10.0), ink);
    painter.text(pos2(inner.right(), rect.bottom() - 4.0), Align2::RIGHT_BOTTOM, pts[pts.len() - 1].date.to_string(), FontId::proportional(10.0), ink);
    painter.text(pos2(rect.center().x, rect.top() + 4.0), Align2::CENTER_TOP, format!("Cumulative cost, 100 % = {} {}", money(pts[pts.len() - 1].cumulative, decimals), cf.currency), FontId::proportional(10.0), ink);
}
