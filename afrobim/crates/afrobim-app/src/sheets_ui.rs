//! The windows of the drawing sheets: project information, the sheet-set manager, the title
//! block editor and the sheet preview, and the PDF and DXF exports.
//!
//! The sheet set itself is stored in the document (one `sheets` element); the windows edit a copy
//! and write it back on every change, so undo and save work as for any other element. Nothing is
//! prefilled except the crate's neutral defaults for paper, frame, pens and text heights, which
//! are shown as editable defaults. Scales, lengths, overlaps and intervals start empty.

use afrobim_annotate::DisplayUnits;
use afrobim_draw::{Colour, Drawing, Primitive, Pt2};
use afrobim_kernel::{Document, ElementId, Pt, ViewTransform};
use afrobim_sheets::{
    project_info, set_project_info, Block, Cell, CellContent, FieldKey, Margins, Notes, Orientation, Page, PaperSize, PlanRotation, PlanSeries, PlanView, ProfileAnnotations,
    ProfileView, ProjectInfo, RevColumn, RevField, Revision, Row, Scale, SectionSeries, SectionsView, SheetDrawing, SheetSet, SheetSource, SheetSpec, Table, TableSeries, TitleBlockLayout,
    TitlePlacement, UserField, ViewDef, ViewKind,
};
use egui::{Color32, FontId, RichText, Sense, Stroke, Ui, Vec2};

use crate::app::AfroBimApp;
use crate::render::{ElementMesh, Representation};
use crate::sheets_glue::{self as glue, Composed, DETAIL_KINDS, FEED_QUANTITIES, TABLE_KINDS};

#[allow(non_snake_case)]

fn WARN() -> egui::Color32 {

    crate::theme::warn()

}

// ─── state ──────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    Plan,
    Profile,
    Sections,
    Schedule,
    Detail,
    Notes,
    /// A sheet this form cannot rebuild: only its title, paper and numbering are edited.
    Props,
}

impl FormKind {
    pub const ADD: [FormKind; 6] = [FormKind::Plan, FormKind::Profile, FormKind::Sections, FormKind::Schedule, FormKind::Detail, FormKind::Notes];

    fn label(self) -> &'static str {
        match self {
            FormKind::Plan => "Plan series (alignment, station range, sheet length and overlap)",
            FormKind::Profile => "Profile sheet (station range, horizontal and vertical scales)",
            FormKind::Sections => "Cross-section sheets (corridor, interval or list)",
            FormKind::Schedule => "Schedule sheet (table from the model)",
            FormKind::Detail => "Detail sheet (sign face, Gantt chart, S-curves)",
            FormKind::Notes => "Notes block",
            FormKind::Props => "Sheet",
        }
    }

    fn short(self) -> &'static str {
        match self {
            FormKind::Plan => "Plan series",
            FormKind::Profile => "Profile",
            FormKind::Sections => "Sections",
            FormKind::Schedule => "Schedule",
            FormKind::Detail => "Detail",
            FormKind::Notes => "Notes",
            FormKind::Props => "Sheet",
        }
    }

    fn default_title(self) -> &'static str {
        match self {
            FormKind::Plan => "PLAN",
            FormKind::Profile => "PROFILE",
            FormKind::Sections => "CROSS-SECTIONS",
            FormKind::Schedule => TABLE_KINDS[0].2,
            FormKind::Detail => DETAIL_KINDS[0].2,
            FormKind::Notes => "NOTES",
            FormKind::Props => "",
        }
    }
}

/// The add and edit form of one source of the set. Every number is text, and starts empty.
#[derive(Clone)]
pub struct SourceForm {
    kind: FormKind,
    editing: Option<usize>,
    title: String,
    /// Numbering prefix for these sheets; empty follows the set's numbering.
    prefix: String,
    page: Page,
    alignment: String,
    from: String,
    to: String,
    /// Stationing region of the first and last station as picked (0: the station occurs once).
    from_region: usize,
    to_region: usize,
    /// How many stationing regions the alignment has (set for the window; 1 shows no picker).
    regions: usize,
    scale: String,
    // plan
    length: String,
    overlap: String,
    rotation: u8,
    degrees: String,
    tick: String,
    label_every: String,
    // profile
    v_scale: String,
    datum: String,
    grid_sta: String,
    grid_lvl: String,
    band: String,
    surface: String,
    ann: ProfileAnnotations,
    // sections
    corridor: String,
    /// The sections come from a sample line group (a civil object) and not from a corridor.
    from_samples: bool,
    sample_group: String,
    /// The sample line groups of the document (refreshed every frame).
    groups: Vec<String>,
    mode: u8,
    interval: String,
    list: String,
    exaggeration: String,
    columns: String,
    per_sheet: String,
    // schedule
    table_kind: usize,
    category: String,
    // detail
    detail_kind: usize,
    element: String,
    // notes
    heading: String,
    paragraphs: String,
    numbered: bool,
    error: String,
}

impl SourceForm {
    fn new(kind: FormKind, page: Page) -> SourceForm {
        SourceForm {
            kind,
            editing: None,
            title: kind.default_title().to_string(),
            prefix: String::new(),
            page,
            alignment: String::new(),
            from: String::new(),
            to: String::new(),
            from_region: 0,
            to_region: 0,
            regions: 1,
            scale: String::new(),
            length: String::new(),
            overlap: String::new(),
            rotation: 0,
            degrees: String::new(),
            tick: String::new(),
            label_every: String::new(),
            v_scale: String::new(),
            datum: String::new(),
            grid_sta: String::new(),
            grid_lvl: String::new(),
            band: String::new(),
            surface: String::new(),
            ann: ProfileAnnotations::default(),
            corridor: String::new(),
            from_samples: false,
            sample_group: String::new(),
            groups: Vec::new(),
            mode: 0,
            interval: String::new(),
            list: String::new(),
            exaggeration: String::new(),
            columns: String::new(),
            per_sheet: String::new(),
            table_kind: 0,
            category: String::new(),
            detail_kind: 0,
            element: String::new(),
            heading: String::new(),
            paragraphs: String::new(),
            numbered: false,
            error: String::new(),
        }
    }

    /// The form for an existing source, so that it can be edited. A source made some other way
    /// than by these forms gets the properties form.
    fn from_source(index: usize, source: &SheetSource, defaults: Page, u: &DisplayUnits) -> SourceForm {
        let t = source.template();
        let mut f = SourceForm::new(FormKind::Props, t.page);
        f.editing = Some(index);
        f.title = t.title.clone();
        f.prefix = t.number_prefix.clone().unwrap_or_default();
        let n = |v: f64| afrobim_sheets::trim_number(v, 6);
        let opt = |v: Option<f64>| v.map(n).unwrap_or_default();
        // Lengths and intervals are shown in the document's display unit; stations are internal
        // numbers here and `to_displayed` turns them into the stations as drawn.
        let l = |v: f64| crate::disp::len_field(u, v);
        let lopt = |v: Option<f64>| v.map(l).unwrap_or_default();
        match source {
            SheetSource::PlanSeries(p) => {
                if let Some(ViewKind::Plan(v)) = p.template.views.get(p.plan_view).map(|v| &v.kind) {
                    f.kind = FormKind::Plan;
                    f.alignment = v.alignment.clone();
                    f.from = opt(p.from_station_m);
                    f.to = opt(p.to_station_m);
                    f.scale = n(v.scale.0);
                    f.length = l(p.length_m);
                    f.overlap = l(p.overlap_m);
                    (f.rotation, f.degrees) = match v.rotation {
                        PlanRotation::AlignChord => (0, String::new()),
                        PlanRotation::NorthUp => (1, String::new()),
                        PlanRotation::Degrees(d) => (2, n(d)),
                    };
                    f.tick = l(v.tick_interval_m);
                    f.label_every = l(v.label_interval_m);
                    // The title of an expanded sheet adds the station range; the stored one does not.
                }
            }
            SheetSource::Single(s) if s.views.len() == 1 => match &s.views[0].kind {
                ViewKind::Profile(v) => {
                    f.kind = FormKind::Profile;
                    f.alignment = v.alignment.clone();
                    f.from = n(v.from_station_m);
                    f.to = n(v.to_station_m);
                    f.scale = n(v.h_scale.0);
                    f.v_scale = n(v.h_scale.0 / v.vertical_exaggeration);
                    f.datum = lopt(v.datum_level_m);
                    f.grid_sta = l(v.grid_station_interval_m);
                    f.grid_lvl = l(v.grid_level_interval_m);
                    f.band = if v.band_interval_m > 0.0 { l(v.band_interval_m) } else { String::new() };
                    f.surface = v.surface.clone().unwrap_or_default();
                    f.ann = v.annotations;
                }
                ViewKind::Notes(notes) => {
                    f.kind = FormKind::Notes;
                    f.heading = notes.heading.clone().unwrap_or_default();
                    f.paragraphs = notes.paragraphs.join("\n");
                    f.numbered = notes.numbered;
                }
                ViewKind::Detail(d) => {
                    f.kind = FormKind::Detail;
                    match (&d.source, &d.feed) {
                        (_, Some(feed)) => {
                            let (kind, rest) = feed.split_once(':').unwrap_or((feed.as_str(), ""));
                            f.detail_kind = DETAIL_KINDS.iter().position(|k| k.0 == kind).unwrap_or(1);
                            f.element = rest.to_string();
                        }
                        (afrobim_sheets::DetailSource::Element(name), None) => {
                            f.detail_kind = 0;
                            f.element = name.clone();
                            f.scale = if d.fit { String::new() } else { n(d.scale.0) };
                        }
                        _ => f.kind = FormKind::Props,
                    }
                }
                _ => {}
            },
            SheetSource::TableSeries(t) => {
                if let Some(ViewKind::Table(table)) = t.template.views.get(t.table_view).map(|v| &v.kind) {
                    if let Some(feed) = &table.feed {
                        f.kind = FormKind::Schedule;
                        f.table_kind = TABLE_KINDS.iter().position(|k| feed.starts_with(k.0)).unwrap_or(0);
                        f.category = feed.strip_prefix(FEED_QUANTITIES).map(|c| if c == "*" { String::new() } else { c.to_string() }).unwrap_or_default();
                    }
                }
            }
            SheetSource::SectionSeries(s) => {
                if let Some(ViewKind::Sections(v)) = s.template.views.get(s.sections_view).map(|v| &v.kind) {
                    f.kind = FormKind::Sections;
                    f.corridor = v.corridor.clone();
                    f.from_samples = v.sample_group.is_some();
                    f.sample_group = v.sample_group.clone().unwrap_or_default();
                    f.scale = n(v.scale.0);
                    f.exaggeration = n(v.vertical_exaggeration);
                    f.columns = v.columns.to_string();
                    f.per_sheet = s.per_sheet.to_string();
                    match &v.stations_m {
                        Some(list) => {
                            f.mode = 2;
                            f.list = list.iter().map(|s| n(*s)).collect::<Vec<_>>().join(", ");
                        }
                        None => {
                            f.mode = 0;
                            f.from = opt(v.from_station_m);
                            f.to = opt(v.to_station_m);
                        }
                    }
                }
            }
            _ => {}
        }
        let _ = defaults;
        f
    }
}

/// The internal station of a station typed as shown on the drawings (see `station_input`).
fn internal_of(a: Option<&afrobim_alignment::Alignment>, shown: f64, region: usize) -> Result<f64, String> {
    match a {
        Some(a) => crate::station_input::displayed_to_internal(a, shown, region),
        None => Ok(shown),
    }
}

/// A station text field with the region picker when the alignment has station equations.
fn station_text_row(ui: &mut Ui, label: &str, text: &mut String, region: &mut usize, hint: &str, regions: usize) {
    ui.label(label);
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(170.0));
        if regions > 1 {
            ui.label("region");
            ui.add(egui::DragValue::new(region).range(0..=regions)).on_hover_text(format!("1 to {regions}; 0 if the station occurs once"));
        } else {
            *region = 0;
        }
    });
    ui.end_row();
}

impl SourceForm {
    /// The alignment the form's stations are on: the chosen one, or for sections the document's
    /// only alignment.
    fn alignment_obj(&self, doc: &Document) -> Option<afrobim_alignment::Alignment> {
        let all = crate::alignment_glue::alignments(doc);
        let name = if self.kind == FormKind::Sections { (all.len() == 1).then(|| all[0].1.clone())? } else { self.alignment.clone() };
        let (id, _) = all.into_iter().find(|a| a.1 == name)?;
        crate::alignment_glue::alignment_of(doc, id)
    }

    /// Turns the internal stations a stored source holds into the stations shown on the drawings,
    /// with their regions, for the edit form.
    fn to_displayed(&mut self, doc: &Document) {
        let u = crate::anno_ui::units_of(doc);
        let a = self.alignment_obj(doc);
        if let Some(a) = &a {
            self.regions = a.station_equations.region_count();
        }
        // Without the alignment the stations stay as they are stored (internal), in the same form.
        let shown_of = |v: f64| a.as_ref().map(|a| crate::station_input::internal_to_displayed(a, v)).unwrap_or((v, 0));
        for (text, region) in [(&mut self.from, &mut self.from_region), (&mut self.to, &mut self.to_region)] {
            if let Ok(v) = text.trim().parse::<f64>() {
                let (shown, r) = shown_of(v);
                *text = crate::disp::station(&u, shown);
                *region = r;
            }
        }
        // A list of sections: each station as drawn (a station that occurs in two regions is
        // shown once and must be told apart when it is typed again).
        if self.mode == 2 {
            let tokens: Vec<String> = self.list.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|t| !t.is_empty()).map(|t| t.parse::<f64>().map(|v| crate::disp::station(&u, shown_of(v).0)).unwrap_or_else(|_| t.to_string())).collect();
            self.list = tokens.join(", ");
        }
    }
}

fn parse(text: &str, what: &str) -> Result<f64, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err(format!("Enter {what}."));
    }
    t.replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("'{t}' is not a number ({what})."))
}

fn parse_pos(text: &str, what: &str) -> Result<f64, String> {
    let v = parse(text, what)?;
    if v > 0.0 {
        Ok(v)
    } else {
        Err(format!("{what} must be more than 0."))
    }
}

fn parse_opt(text: &str, what: &str) -> Result<Option<f64>, String> {
    if text.trim().is_empty() {
        Ok(None)
    } else {
        parse(text, what).map(Some)
    }
}

fn parse_count(text: &str, what: &str) -> Result<usize, String> {
    let v = parse_pos(text, what)?;
    if v.fract() == 0.0 {
        Ok(v as usize)
    } else {
        Err(format!("{what} must be a whole number."))
    }
}

impl SourceForm {
    /// The source this form describes, or what is wrong with it. `old` is the source being
    /// edited, whose title block and own field values carry over.
    fn build(&self, doc: &Document, old: Option<&SheetSource>) -> Result<SheetSource, String> {
        let spec = |views: Vec<ViewDef>| -> SheetSpec {
            let mut s = SheetSpec::new(self.title.trim(), self.page, views);
            s.number_prefix = Some(self.prefix.trim().to_string()).filter(|p| !p.is_empty());
            if let Some(o) = old {
                s.title_block = o.template().title_block.clone();
                s.fields = o.template().fields.clone();
            }
            s
        };
        if self.title.trim().is_empty() && self.kind != FormKind::Plan {
            return Err("Enter a title for the sheet.".into());
        }
        if self.kind == FormKind::Props {
            let Some(old) = old else { return Err("Nothing to edit.".into()) };
            let mut src = old.clone();
            let t = src.template_mut();
            t.title = self.title.trim().to_string();
            t.page = self.page;
            t.number_prefix = Some(self.prefix.trim().to_string()).filter(|p| !p.is_empty());
            return Ok(src);
        }
        let need_alignment = |name: &str| -> Result<(), String> {
            if name.is_empty() {
                Err("Choose the alignment.".into())
            } else if crate::alignment_glue::alignments(doc).iter().all(|a| a.1 != name) {
                Err(format!("There is no alignment called '{name}'."))
            } else {
                Ok(())
            }
        };
        let u = &crate::anno_ui::units_of(doc);
        let unit = crate::disp::suffix(u);
        // A length or an interval in the display unit (a number, or with m or mm), in metres.
        let len = |text: &str, what: &str| crate::disp::parse_len_or(u, text, what);
        let len_pos = |text: &str, what: &str| -> Result<f64, String> {
            let v = len(text, what)?;
            if v > 0.0 {
                Ok(v)
            } else {
                Err(format!("{what} must be more than 0."))
            }
        };
        let len_opt = |text: &str, what: &str| -> Result<Option<f64>, String> { if text.trim().is_empty() { Ok(None) } else { len(text, what).map(Some) } };
        // A station as drawn: 1+250.000 or a number of metres.
        let sta = |text: &str, what: &str| crate::disp::parse_station_or(u, text, what);
        let sta_opt = |text: &str, what: &str| -> Result<Option<f64>, String> { if text.trim().is_empty() { Ok(None) } else { sta(text, what).map(Some) } };
        match self.kind {
            FormKind::Plan => {
                need_alignment(&self.alignment)?;
                let scale = parse_pos(&self.scale, "the scale 1:n (n)")?;
                let length = len_pos(&self.length, &format!("the length of alignment on each sheet ({unit})"))?;
                let overlap = len(&self.overlap, &format!("the overlap between sheets ({unit}, 0 for none)"))?;
                if overlap < 0.0 || overlap >= length {
                    return Err("The overlap must be at least 0 and less than the sheet length.".into());
                }
                let al = self.alignment_obj(doc);
                let from = sta_opt(&self.from, "the first station")?.map(|v| internal_of(al.as_ref(), v, self.from_region)).transpose()?;
                let to = sta_opt(&self.to, "the last station")?.map(|v| internal_of(al.as_ref(), v, self.to_region)).transpose()?;
                if let (Some(a), Some(b)) = (from, to) {
                    if b <= a {
                        return Err("The last station must be beyond the first.".into());
                    }
                }
                let mut v = PlanView::new(&self.alignment, 0.0, 1.0, Scale(scale));
                v.tick_interval_m = len_pos(&self.tick, &format!("the tick interval ({unit})"))?;
                v.label_interval_m = len_pos(&self.label_every, &format!("the station label interval ({unit})"))?;
                v.rotation = match self.rotation {
                    0 => PlanRotation::AlignChord,
                    1 => PlanRotation::NorthUp,
                    _ => PlanRotation::Degrees(parse(&self.degrees, "the rotation (degrees anticlockwise)")?),
                };
                let template = spec(vec![ViewDef::full(ViewKind::Plan(v))]);
                Ok(SheetSource::PlanSeries(PlanSeries { template, plan_view: 0, from_station_m: from, to_station_m: to, length_m: length, overlap_m: overlap }))
            }
            FormKind::Profile => {
                need_alignment(&self.alignment)?;
                let h = parse_pos(&self.scale, "the horizontal scale 1:n (n)")?;
                let vs = parse_pos(&self.v_scale, "the vertical scale 1:m (m)")?;
                let al = self.alignment_obj(doc);
                let from = internal_of(al.as_ref(), sta(&self.from, "the first station")?, self.from_region)?;
                let to = internal_of(al.as_ref(), sta(&self.to, "the last station")?, self.to_region)?;
                if to <= from {
                    return Err("The last station must be beyond the first.".into());
                }
                let mut v = ProfileView::new(&self.alignment, from, to, Scale(h), h / vs);
                v.datum_level_m = len_opt(&self.datum, "the datum level")?;
                v.grid_station_interval_m = len_pos(&self.grid_sta, &format!("the grid interval along the profile ({unit})"))?;
                v.grid_level_interval_m = len_pos(&self.grid_lvl, &format!("the grid interval of level ({unit})"))?;
                v.band_interval_m = len_opt(&self.band, "the band interval")?.unwrap_or(0.0).max(0.0);
                v.surface = Some(self.surface.clone()).filter(|s| !s.is_empty());
                v.annotations = self.ann;
                Ok(SheetSource::Single(spec(vec![ViewDef::full(ViewKind::Profile(v))])))
            }
            FormKind::Sections if self.from_samples => {
                if self.sample_group.is_empty() {
                    return Err("Choose the sample line group.".into());
                }
                if !doc.elements().iter().any(|e| e.category == crate::civil::SAMPLES && e.name == self.sample_group) {
                    return Err(format!("There is no sample line group called '{}'.", self.sample_group));
                }
                let mut v = SectionsView::of_sample_group(&self.sample_group, Scale(parse_pos(&self.scale, "the scale 1:n (n)")?), parse_count(&self.columns, "the number of columns")?);
                v.vertical_exaggeration = parse_pos(&self.exaggeration, "the vertical exaggeration (1 for none)")?;
                let per_sheet = parse_count(&self.per_sheet, "the number of sections on a sheet")?;
                if self.mode == 2 {
                    let al = self.alignment_obj(doc);
                    let list = self.list.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|t| !t.is_empty()).map(|t| sta(t, "a station in the list").and_then(|x| internal_of(al.as_ref(), x, 0))).collect::<Result<Vec<f64>, String>>()?;
                    if list.is_empty() {
                        return Err("Give at least one station.".into());
                    }
                    v.stations_m = Some(list);
                } else {
                    let al = self.alignment_obj(doc);
                    v.from_station_m = sta_opt(&self.from, "the first station")?.map(|x| internal_of(al.as_ref(), x, self.from_region)).transpose()?;
                    v.to_station_m = sta_opt(&self.to, "the last station")?.map(|x| internal_of(al.as_ref(), x, self.to_region)).transpose()?;
                }
                let template = spec(vec![ViewDef::full(ViewKind::Sections(v))]);
                Ok(SheetSource::SectionSeries(SectionSeries { template, sections_view: 0, per_sheet }))
            }
            FormKind::Sections => {
                if self.corridor.is_empty() {
                    return Err("Choose the corridor.".into());
                }
                let id = doc.elements().iter().find(|e| e.category == afrobim_corridor::CORRIDOR_CATEGORY && e.name == self.corridor).map(|e| e.id).ok_or_else(|| format!("There is no corridor called '{}'.", self.corridor))?;
                let mut v = SectionsView::new(&self.corridor, Scale(parse_pos(&self.scale, "the scale 1:n (n)")?), parse_count(&self.columns, "the number of columns")?);
                v.vertical_exaggeration = parse_pos(&self.exaggeration, "the vertical exaggeration (1 for none)")?;
                let per_sheet = parse_count(&self.per_sheet, "the number of sections on a sheet")?;
                match self.mode {
                    0 => {
                        let al = self.alignment_obj(doc);
                        v.from_station_m = sta_opt(&self.from, "the first station")?.map(|x| internal_of(al.as_ref(), x, self.from_region)).transpose()?;
                        v.to_station_m = sta_opt(&self.to, "the last station")?.map(|x| internal_of(al.as_ref(), x, self.to_region)).transpose()?;
                    }
                    mode => {
                        let corridor = afrobim_corridor::corridor_of(doc, id).map_err(|e| format!("Corridor '{}' cannot be built: {e}", self.corridor))?;
                        let have: Vec<f64> = corridor.sections.iter().map(|s| s.station_m).collect();
                        let wanted: Vec<f64> = if mode == 1 {
                            let al = self.alignment_obj(doc);
                            let from = internal_of(al.as_ref(), sta(&self.from, "the first station")?, self.from_region)?;
                            let to = internal_of(al.as_ref(), sta(&self.to, "the last station")?, self.to_region)?;
                            let step = len_pos(&self.interval, &format!("the interval between sections ({unit})"))?;
                            let mut out: Vec<f64> = Vec::new();
                            for s in have.iter().copied().filter(|s| *s >= from - 1e-6 && *s <= to + 1e-6) {
                                if out.last().is_none_or(|l| s >= l + step - 1e-6) {
                                    out.push(s);
                                }
                            }
                            out
                        } else {
                            self.list.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|t| !t.is_empty()).map(|t| sta(t, "a station in the list").and_then(|v| internal_of(self.alignment_obj(doc).as_ref(), v, 0))).collect::<Result<Vec<f64>, String>>()?
                        };
                        let found: Vec<f64> = wanted.iter().copied().filter(|w| have.iter().any(|h| (h - w).abs() <= 1e-3)).collect();
                        if found.is_empty() {
                            return Err("The corridor has no section at any of those stations (sections stand where the corridor was built).".into());
                        }
                        v.stations_m = Some(found);
                    }
                }
                let template = spec(vec![ViewDef::full(ViewKind::Sections(v))]);
                Ok(SheetSource::SectionSeries(SectionSeries { template, sections_view: 0, per_sheet }))
            }
            FormKind::Schedule => {
                let (feed, _, _) = TABLE_KINDS[self.table_kind.min(TABLE_KINDS.len() - 1)];
                let feed = if feed == FEED_QUANTITIES { format!("{FEED_QUANTITIES}{}", if self.category.is_empty() { "*" } else { &self.category }) } else { feed.to_string() };
                let mut t = Table::default();
                t.title = Some(self.title.trim().to_string());
                t.feed = Some(feed);
                let template = spec(vec![ViewDef::full(ViewKind::Table(t))]);
                Ok(SheetSource::TableSeries(TableSeries { template, table_view: 0 }))
            }
            FormKind::Detail => {
                let (feed, _, _) = DETAIL_KINDS[self.detail_kind.min(DETAIL_KINDS.len() - 1)];
                let detail = if feed == "element" {
                    if self.element.is_empty() {
                        return Err("Choose the sign.".into());
                    }
                    let scale = parse_opt(&self.scale, "the scale 1:n (n)")?;
                    if scale.is_some_and(|s| s <= 0.0) {
                        return Err("The scale must be more than 0.".into());
                    }
                    glue::sign_face_detail(&self.element, scale)
                } else {
                    let empty = Drawing { primitives: vec![], extents: afrobim_draw::Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(1.0, 1.0) } };
                    let mut d = afrobim_sheets::DetailView::inline(empty, vec![], Scale(1.0), 1.0, self.title.trim());
                    d.fit = true;
                    d.feed = Some(if glue::feed_needs_element(feed) {
                        if self.element.trim().is_empty() {
                            return Err("Enter the mark of the sign or installation the drawing is of.".into());
                        }
                        format!("{feed}:{}", self.element.trim())
                    } else {
                        feed.to_string()
                    });
                    d
                };
                Ok(SheetSource::Single(spec(vec![ViewDef::full(ViewKind::Detail(detail))])))
            }
            FormKind::Notes => {
                let paragraphs: Vec<String> = self.paragraphs.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
                if paragraphs.is_empty() && self.heading.trim().is_empty() {
                    return Err("Enter the notes.".into());
                }
                let notes = Notes { heading: Some(self.heading.trim().to_string()).filter(|h| !h.is_empty()), paragraphs, numbered: self.numbered };
                Ok(SheetSource::Single(spec(vec![ViewDef::full(ViewKind::Notes(notes))])))
            }
            FormKind::Props => unreachable!("handled above"),
        }
    }
}

/// Everything the sheet windows keep between frames.
pub struct SheetsUi {
    pub manager_open: bool,
    pub titleblock_open: bool,
    pub preview_open: bool,
    pub project_open: bool,
    pub set_id: Option<ElementId>,
    loaded: serde_json::Value,
    pub set: Option<SheetSet>,
    new_name: String,
    /// The paper, margins and frame new sheets start with: the crate's neutral defaults, editable.
    defaults: Page,
    form: Option<SourceForm>,
    add_kind: usize,
    message: String,
    // title block
    tb: Option<TitleBlockLayout>,
    tb_for: Option<ElementId>,
    tb_message: String,
    // preview
    pub composed: Option<Composed>,
    pub page: usize,
    zoom: f32,
    pan: Vec2,
    fit: bool,
    cache: Option<(usize, String, Preview)>,
    // project information
    info: ProjectInfo,
    info_loaded: serde_json::Value,
    info_message: String,
}

impl Default for SheetsUi {
    fn default() -> Self {
        SheetsUi {
            manager_open: false,
            titleblock_open: false,
            preview_open: false,
            project_open: false,
            set_id: None,
            loaded: serde_json::Value::Null,
            set: None,
            new_name: String::new(),
            defaults: Page::default(),
            form: None,
            add_kind: 0,
            message: String::new(),
            tb: None,
            tb_for: None,
            tb_message: String::new(),
            composed: None,
            page: 0,
            zoom: 1.0,
            pan: Vec2::ZERO,
            fit: true,
            cache: None,
            info: ProjectInfo::default(),
            info_loaded: serde_json::Value::Null,
            info_message: String::new(),
        }
    }
}

impl SheetsUi {
    /// Reads the selected set and the project information from the document when their data
    /// changed (open, undo, an edit made elsewhere).
    pub fn sync(&mut self, doc: &Document) {
        let sets = glue::sets(doc);
        if self.set_id.is_none_or(|id| !sets.iter().any(|s| s.0 == id)) {
            self.set_id = sets.first().map(|s| s.0);
            self.loaded = serde_json::Value::Null;
            self.set = None;
        }
        let current = self.set_id.and_then(|id| doc.find(id)).map(|e| e.data.clone()).unwrap_or(serde_json::Value::Null);
        if current != self.loaded {
            self.set = self.set_id.and_then(|id| afrobim_sheets::load_sheet_set(doc, id).ok());
            self.loaded = current;
        }
        let info_now = afrobim_sheets::project_info_id(doc).and_then(|id| doc.find(id)).map(|e| e.data.clone()).unwrap_or(serde_json::Value::Null);
        if info_now != self.info_loaded {
            self.info = project_info(doc);
            self.info_loaded = info_now;
        }
    }

    /// Opens the add form for the template chosen in the list (`add_kind`), with the first
    /// alignment, corridor and sign chosen as a convenience (no number is filled in).
    pub fn add_form(&mut self, alignments: &[String], corridors: &[String], signs: &[String]) {
        let mut f = SourceForm::new(FormKind::ADD[self.add_kind.min(FormKind::ADD.len() - 1)], self.defaults);
        f.alignment = alignments.first().cloned().unwrap_or_default();
        f.corridor = corridors.first().cloned().unwrap_or_default();
        f.element = signs.first().cloned().unwrap_or_default();
        self.form = Some(f);
    }

    /// Opens the manager with the add form of template `kind` showing, to look at it without a
    /// pointer (`--sheet-form`).
    pub fn open_form_for_screenshot(&mut self, doc: &Document, kind: usize) {
        self.sync(doc);
        self.manager_open = true;
        self.add_kind = kind;
        let names = |category: &str| -> Vec<String> { doc.elements().iter().filter(|e| e.category == category).map(|e| e.name.clone()).collect() };
        self.add_form(&names(afrobim_alignment::CATEGORY), &names(afrobim_corridor::CORRIDOR_CATEGORY), &names(crate::signs_glue::CATEGORY));
    }

    pub fn is_open(&self) -> bool {
        self.manager_open || self.titleblock_open || self.preview_open || self.project_open
    }
}

// ─── the preview picture ────────────────────────────────────────────────────────────────

/// One composed sheet turned into what is painted: polylines of the strokes, a tessellated mesh
/// of the fills and the labels, all in paper millimetres.
struct Preview {
    sheet: SheetDrawing,
    lines: Vec<(Vec<Pt2>, f64, Colour, [f64; 4])>,
    fills: ElementMesh,
    width_mm: f64,
    height_mm: f64,
}

fn build_preview(c: &Composed, page: usize) -> Option<Preview> {
    let sheet = c.composed.sheets.get(page)?;
    let d = sheet.to_drawing();
    // Chain strokes that meet end to end into one polyline: fewer shapes to paint.
    let mut lines: Vec<(Vec<Pt2>, f64, Colour)> = Vec::new();
    let mut fills = Drawing { primitives: Vec::new(), extents: d.drawing.extents };
    for p in &d.drawing.primitives {
        match p {
            Primitive::Line { from, to, width, colour } => {
                let colour = colour.unwrap_or(Colour::Black);
                match lines.last_mut() {
                    Some((pts, w, c)) if pts.last() == Some(from) && (*w - *width).abs() < 1e-9 && *c == colour => pts.push(*to),
                    _ => lines.push((vec![*from, *to], *width, colour)),
                }
            }
            Primitive::Fill { .. } | Primitive::Glyphs { .. } => fills.primitives.push(p.clone()),
            _ => {}
        }
    }
    let lines = lines
        .into_iter()
        .map(|(pts, w, c)| {
            let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
            for p in &pts {
                x0 = x0.min(p.x);
                y0 = y0.min(p.y);
                x1 = x1.max(p.x);
                y1 = y1.max(p.y);
            }
            (pts, w, c, [x0, y0, x1, y1])
        })
        .collect();
    Some(Preview { sheet: d, lines, fills: ElementMesh::build(&fills), width_mm: sheet.width_mm, height_mm: sheet.height_mm })
}

fn paint_preview(ui: &mut Ui, p: &Preview, zoom: f32, pan: Vec2) -> egui::Response {
    let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let rect = response.rect;
    // A grey desk, a white sheet, a shadow.
    painter.rect_filled(rect, 0.0, Color32::from_gray(70));
    let origin = rect.center() + pan - Vec2::new(p.width_mm as f32, -(p.height_mm as f32)) * zoom / 2.0;
    let view = ViewTransform::from_origin(zoom as f64, origin.x as f64, origin.y as f64);
    let to_screen = |x: f64, y: f64| egui::pos2(origin.x + x as f32 * zoom, origin.y - y as f32 * zoom);
    let paper = egui::Rect::from_two_pos(to_screen(0.0, 0.0), to_screen(p.width_mm, p.height_mm));
    painter.rect_filled(paper.translate(Vec2::new(3.0, 3.0)), 0.0, Color32::from_black_alpha(90));
    painter.rect_filled(paper, 0.0, Color32::WHITE);
    let painter = painter.with_clip_rect(rect.intersect(paper.expand(1.0)).intersect(rect));
    for shape in p.fills.shapes(&view, Pt::new(0.0, 0.0), Representation::Real, Color32::BLACK, 1.0) {
        painter.add(shape);
    }
    let clip = painter.clip_rect();
    for (pts, w, colour, bb) in &p.lines {
        let (a, b) = (to_screen(bb[0], bb[1]), to_screen(bb[2], bb[3]));
        if !clip.intersects(egui::Rect::from_two_pos(a, b).expand(2.0)) {
            continue;
        }
        let [r, g, bl] = colour.display_rgb();
        let screen: Vec<egui::Pos2> = pts.iter().map(|q| to_screen(q.x, q.y)).collect();
        painter.add(egui::Shape::line(screen, Stroke::new((*w as f32 * zoom).max(1.0), Color32::from_rgb(r, g, bl))));
    }
    // Texts: the crate's Helvetica metrics place them (so alignment matches the PDF); egui draws
    // them in its own face, which differs a little in width.
    for l in &p.sheet.labels {
        let px = (l.height_mm as f32 / 0.718) * zoom;
        if px < 3.0 {
            continue;
        }
        let at = to_screen(l.at.x, l.at.y);
        if !clip.expand(px * 40.0).contains(at) {
            continue;
        }
        let width = afrobim_sheets::text_width(&l.text, l.height_mm, l.bold) as f32 * zoom;
        let shift = match l.align {
            afrobim_sheets::Align::Left => 0.0,
            afrobim_sheets::Align::Centre => -width / 2.0,
            afrobim_sheets::Align::Right => -width,
        };
        let th = (l.rotation_deg as f32).to_radians();
        let galley = painter.layout_no_wrap(l.text.clone(), FontId::proportional(px), Color32::BLACK);
        // The galley's top left stands `ascent` above the baseline start; both are turned by the
        // text's angle (anticlockwise on paper, so clockwise-negative on the screen).
        let ascent = px * 0.80;
        let along = Vec2::new(th.cos(), -th.sin());
        let up = Vec2::new(-th.sin(), -th.cos());
        let top_left = at + along * shift + up * ascent;
        painter.add(egui::epaint::TextShape::new(top_left, galley, Color32::BLACK).with_angle(-th));
    }
    response
}

// ─── the app side ───────────────────────────────────────────────────────────────────────

/// What to do with a set when its composition finishes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ComposeThen {
    Preview,
    Pdf,
    Dxf { one_file: bool },
}

/// What a composition on a worker thread hands back.
pub struct ComposeOut {
    pub then: ComposeThen,
    pub result: Result<Composed, String>,
}

/// Composes the stored set `id` on a worker thread from a copy of the document, to be applied only
/// while the document is unchanged.
pub fn spawn_compose(doc: &Document, id: ElementId, engine: Option<std::sync::Arc<afrobim_signs::SignEngine>>, then: ComposeThen) -> crate::background::Job<ComposeOut> {
    let copy = doc.clone();
    crate::background::Job::spawn("Sheet set", crate::background::fingerprint(doc), move || ComposeOut { then, result: glue::compose_off_thread(&copy, id, engine) })
}

impl AfroBimApp {
    /// Composes the selected set (or the one called `name`) from the live model.
    pub(crate) fn compose_sheets(&mut self, name: Option<&str>) -> Result<Composed, String> {
        self.sheets.sync(&self.doc);
        let id = match name {
            Some(n) => glue::sets(&self.doc).into_iter().find(|s| s.1.eq_ignore_ascii_case(n)).map(|s| s.0).ok_or_else(|| format!("There is no sheet set called '{n}'."))?,
            None => self.sheets.set_id.ok_or_else(|| "There is no sheet set yet. Open Sheet sets and make one.".to_string())?,
        };
        self.sync_generated();
        let live = glue::Live::from_doc(&self.doc, &self.modules, self.engine.as_deref());
        glue::compose_stored(&self.doc, &self.modules, id, &live)
    }

    /// What to do with a set composed on a worker thread.
    fn compose_into_preview(&mut self) {
        self.start_compose(ComposeThen::Preview);
    }

    /// Composes the selected set on a worker thread, with a progress indicator and Cancel (a set
    /// with cross-section sheets builds its corridor, which can take a while). When it arrives it
    /// is previewed or exported as `then` says, if the document has not changed since.
    pub(crate) fn start_compose(&mut self, then: ComposeThen) {
        if self.bg.sheets.is_some() {
            self.status = "A sheet set is already being composed.".into();
            return;
        }
        self.sheets.sync(&self.doc);
        let Some(id) = self.sheets.set_id else {
            self.status = "There is no sheet set yet. Open Sheet sets and make one.".into();
            return;
        };
        self.bg.sheets = Some(spawn_compose(&self.doc, id, self.engine.clone(), then));
        self.status = "Composing the sheet set on a worker thread; the model stays editable, but an edit discards the result.".into();
    }

    /// Takes a set composed on a worker thread in.
    pub(crate) fn apply_composed(&mut self, out: ComposeOut) {
        let c = match out.result {
            Ok(c) => c,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        match out.then {
            ComposeThen::Preview => {
                let n = c.composed.sheets.len();
                self.status = format!("Composed {n} sheet(s) of '{}' at {}.", c.set_name, c.at);
                self.sheets.page = self.sheets.page.min(n.saturating_sub(1));
                self.sheets.composed = Some(c);
                self.sheets.cache = None;
                self.sheets.fit = true;
            }
            ComposeThen::Pdf => self.finish_sheet_pdf(c),
            ComposeThen::Dxf { one_file } => self.finish_sheet_dxf(c, one_file),
        }
    }

    /// The commands of the drawing sheets.
    pub(crate) fn sheets_action(&mut self, action: crate::commands::Action) {
        use crate::commands::Action;
        self.sheets.sync(&self.doc);
        match action {
            Action::SheetSets => self.sheets.manager_open = true,
            Action::ProjectInfo => self.sheets.project_open = true,
            Action::TitleBlock => {
                self.sheets.titleblock_open = true;
                self.sheets.tb_for = None;
            }
            Action::SheetPreview => {
                self.sheets.preview_open = true;
                if self.sheets.set_id.is_none() {
                    self.status = "There is no sheet set yet. Open Sheet sets and make one.".into();
                } else {
                    self.compose_into_preview();
                }
            }
            Action::SheetPdf => self.export_sheet_pdf(),
            Action::SheetDxf => self.export_sheet_dxf(false),
            _ => {}
        }
    }

    pub(crate) fn export_sheet_pdf(&mut self) {
        self.start_compose(ComposeThen::Pdf);
    }

    /// Asks where to write a composed set and writes it as a PDF.
    fn finish_sheet_pdf(&mut self, c: Composed) {
        let name = format!("{}.pdf", glue::file_stem(&c.set_name));
        let Some(mut path) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(&name).save_file() else { return };
        if path.extension().is_none() {
            path.set_extension("pdf");
        }
        self.status = match glue::write_pdf(&c, &path) {
            Ok(n) => format!("Wrote {n} page(s) of '{}' to {}. Composed {}.", c.set_name, path.display(), c.at),
            Err(e) => e,
        };
        self.sheets.composed = Some(c);
        self.sheets.cache = None;
    }

    /// `one_file`: every sheet side by side in one DXF; otherwise one DXF per sheet in a folder.
    pub(crate) fn export_sheet_dxf(&mut self, one_file: bool) {
        self.start_compose(ComposeThen::Dxf { one_file });
    }

    fn finish_sheet_dxf(&mut self, c: Composed, one_file: bool) {
        if one_file {
            let name = format!("{}.dxf", glue::file_stem(&c.set_name));
            let Some(mut path) = rfd::FileDialog::new().add_filter("AutoCAD DXF (R12)", &["dxf"]).set_file_name(&name).save_file() else { return };
            if path.extension().is_none() {
                path.set_extension("dxf");
            }
            self.status = match glue::write_dxf_one_file(&c, &path) {
                Ok(()) => format!("Wrote {} sheet(s) of '{}' side by side to {}. Composed {}.", c.composed.sheets.len(), c.set_name, path.display(), c.at),
                Err(e) => e,
            };
        } else {
            let Some(dir) = rfd::FileDialog::new().set_title("Folder for the sheet DXF files (one per sheet)").pick_folder() else { return };
            self.status = match glue::write_dxf_per_sheet(&c, &dir) {
                Ok(files) => format!("Wrote {} DXF file(s) of '{}' into {}. Composed {}.", files.len(), c.set_name, dir.display(), c.at),
                Err(e) => e,
            };
        }
        self.sheets.composed = Some(c);
        self.sheets.cache = None;
    }

    pub(crate) fn sheets_windows(&mut self, ctx: &egui::Context) {
        self.sheets.sync(&self.doc);
        if !self.sheets.is_open() {
            return;
        }
        if self.sheets.project_open {
            self.project_window(ctx);
        }
        if self.sheets.manager_open {
            self.manager_window(ctx);
        }
        if self.sheets.titleblock_open {
            self.titleblock_window(ctx);
        }
        if self.sheets.preview_open {
            self.preview_window(ctx);
        }
    }

    /// Writes the working copy of the set back into the document.
    fn sheets_store(&mut self) {
        let (Some(id), Some(set)) = (self.sheets.set_id, self.sheets.set.clone()) else { return };
        match glue::store(&mut self.doc, id, &set) {
            Ok(()) => {
                self.sheets.loaded = self.doc.find(id).map(|e| e.data.clone()).unwrap_or_default();
                self.schedule_commit_now();
            }
            Err(e) => self.sheets.message = e,
        }
    }

    // ─── project information ───────────────────────────────────────────────────────────

    fn project_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut apply = false;
        let mut s = std::mem::take(&mut self.sheets);
        crate::win::new(ctx, "Project information").open(&mut open).default_width(640.0).default_height(560.0).show(ctx, |ui| {
            ui.label(RichText::new("What every title block of the project says. Nothing is filled in for you: every field starts empty. A sheet can override the revision, date and the three names for itself.").small().weak());
            ui.add_space(4.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(430.0).show(ui, |ui| {
                egui::Grid::new("project-info").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    let info = &mut s.info;
                    for (label, field) in [
                        ("Project", &mut info.project),
                        ("Client", &mut info.client),
                        ("Contract number", &mut info.contract_number),
                        ("Drawn by", &mut info.drawn_by),
                        ("Checked by", &mut info.checked_by),
                        ("Approved by", &mut info.approved_by),
                        ("Issue date, as printed", &mut info.date),
                        ("Logo caption", &mut info.logo_text),
                    ] {
                        ui.label(label);
                        ui.add(egui::TextEdit::singleline(field).desired_width(380.0));
                        ui.end_row();
                    }
                });
                ui.add_space(8.0);
                ui.label(RichText::new("Revisions").strong());
                let mut remove: Option<usize> = None;
                egui::Grid::new("revisions").num_columns(6).spacing([6.0, 4.0]).show(ui, |ui| {
                    for h in ["Rev", "Date", "Description", "By", "Checked", ""] {
                        ui.label(RichText::new(h).small().weak());
                    }
                    ui.end_row();
                    for (i, r) in s.info.revisions.iter_mut().enumerate() {
                        edit(ui, &mut r.rev, 36.0, "");
                        edit(ui, &mut r.date, 90.0, "");
                        edit(ui, &mut r.description, 220.0, "");
                        edit(ui, &mut r.by, 44.0, "");
                        edit(ui, &mut r.checked, 44.0, "");
                        if ui.small_button("x").on_hover_text("Remove this revision").clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = remove {
                    s.info.revisions.remove(i);
                }
                if ui.button("Add a revision").clicked() {
                    s.info.revisions.push(Revision::default());
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Other fields").strong());
                ui.label(RichText::new("A caption and its text. A title block layout can show any of them (Title block editor).").small().weak());
                let mut remove: Option<usize> = None;
                egui::Grid::new("user-fields").num_columns(3).spacing([6.0, 4.0]).show(ui, |ui| {
                    for (i, f) in s.info.user_fields.iter_mut().enumerate() {
                        edit(ui, &mut f.label, 150.0, "caption");
                        edit(ui, &mut f.value, 300.0, "text");
                        if ui.small_button("x").on_hover_text("Remove this field").clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = remove {
                    s.info.user_fields.remove(i);
                }
                if ui.button("Add a field").clicked() {
                    s.info.user_fields.push(UserField::default());
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                let stored = afrobim_sheets::project_info_id(&self.doc).and_then(|id| self.doc.find(id)).map(|e| e.data.clone());
                let changed = stored.as_ref() != Some(&s.info.to_value()) && !(stored.is_none() && s.info == ProjectInfo::default());
                if ui.add_enabled(changed, egui::Button::new("Apply")).on_hover_text("Store the project information in the document").clicked() {
                    apply = true;
                }
                if changed {
                    ui.label(RichText::new("Not applied yet.").small().color(WARN()));
                } else if !s.info_message.is_empty() {
                    ui.label(RichText::new(&s.info_message).small().weak());
                }
            });
        });
        if apply {
            set_project_info(&mut self.doc, &s.info);
            s.info_loaded = afrobim_sheets::project_info_id(&self.doc).and_then(|id| self.doc.find(id)).map(|e| e.data.clone()).unwrap_or_default();
            s.info_message = "Stored in the document. Every sheet composed from now on carries it.".into();
            self.status = "Project information stored.".into();
            self.schedule_commit_now();
        }
        if !open {
            s.project_open = false;
            s.info = project_info(&self.doc);
        }
        self.sheets = s;
    }

    // ─── the sheet-set manager ─────────────────────────────────────────────────────────

    fn manager_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut s = std::mem::take(&mut self.sheets);
        let mut changed = false;
        let mut action: Option<&'static str> = None;
        let mut go: Option<crate::commands::Action> = None;
        let mut dxf_one = false;
        let sets = glue::sets(&self.doc);
        let alignments: Vec<String> = crate::alignment_glue::alignments(&self.doc).into_iter().map(|a| a.1).collect();
        let corridors: Vec<String> = self.doc.elements().iter().filter(|e| e.category == afrobim_corridor::CORRIDOR_CATEGORY).map(|e| e.name.clone()).collect();
        let surfaces: Vec<String> = self.doc.elements().iter().filter(|e| e.category == afrobim_corridor::SURFACE_CATEGORY).map(|e| e.name.clone()).collect();
        let signs: Vec<String> = self.doc.elements().iter().filter(|e| e.category == crate::signs_glue::CATEGORY).map(|e| e.name.clone()).collect();
        let categories: Vec<(String, String)> = self.module_categories().into_iter().filter(|(c, _)| c != afrobim_sheets::SHEETS_CATEGORY && c != afrobim_sheets::PROJECT_INFO_CATEGORY).collect();
        let units = crate::anno_ui::units_of(&self.doc);
        let ranges: Vec<(String, String)> = alignments
            .iter()
            .filter_map(|name| {
                let id = crate::alignment_glue::alignments(&self.doc).into_iter().find(|a| &a.1 == name)?.0;
                let a = crate::alignment_glue::alignment_of(&self.doc, id)?;
                let prof = a.profile.as_ref().and_then(|p| p.range()).map(|(lo, hi)| format!(", profile {} to {}", crate::disp::station(&units, lo), crate::disp::station(&units, hi))).unwrap_or_else(|| ", no profile".into());
                Some((name.clone(), format!("{name}: internal stations {} to {}{prof}", crate::disp::station(&units, a.start_station_m), crate::disp::station(&units, a.end_station_m()))))
            })
            .collect();

        crate::win::new(ctx, "Sheet sets").open(&mut open).default_width(900.0).default_height(640.0).show(ctx, |ui| {
            // ── choose, make, delete a set ──
            ui.horizontal(|ui| {
                ui.label("Sheet set");
                let current = sets.iter().find(|x| Some(x.0) == s.set_id).map(|x| x.1.clone()).unwrap_or_else(|| "none".into());
                egui::ComboBox::from_id_salt("sheet-set").selected_text(current).width(220.0).show_ui(ui, |ui| {
                    for (id, name) in &sets {
                        if ui.selectable_label(Some(*id) == s.set_id, name).clicked() {
                            s.set_id = Some(*id);
                            s.loaded = serde_json::Value::Null;
                            s.set = None;
                            s.form = None;
                            s.composed = None;
                            s.tb = None;
                        }
                    }
                });
                edit(ui, &mut s.new_name, 180.0, "name of a new set");
                if ui.add_enabled(!s.new_name.trim().is_empty(), egui::Button::new("New set")).clicked() {
                    action = Some("new");
                }
                if ui.add_enabled(s.set_id.is_some(), egui::Button::new("Delete set")).on_hover_text("Delete the sheet set from the document (Undo brings it back)").clicked() {
                    action = Some("delete");
                }
            });
            if s.set.is_none() {
                ui.add_space(8.0);
                ui.label("There is no sheet set yet. Enter a name and press New set; then add sheets from the templates.");
                if !s.message.is_empty() {
                    ui.label(RichText::new(&s.message).small().color(WARN()));
                }
                return;
            }
            ui.separator();
            let body_h = (ui.available_height() - 74.0).max(120.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(body_h).show(ui, |ui| {
                let Some(set) = s.set.as_mut() else { return };
                // ── name and numbering ──
                ui.horizontal(|ui| {
                    ui.label("Numbering");
                    ui.label(RichText::new("(defaults, editable)").small().weak());
                    changed |= ui.add(egui::TextEdit::singleline(&mut set.numbering.prefix).hint_text("prefix").desired_width(90.0)).changed();
                    ui.label("digits");
                    changed |= ui.add(egui::DragValue::new(&mut set.numbering.digits).range(1..=8)).changed();
                    ui.label("start");
                    changed |= ui.add(egui::DragValue::new(&mut set.numbering.start)).changed();
                    ui.label("step");
                    changed |= ui.add(egui::DragValue::new(&mut set.numbering.step).range(1..=1000)).changed();
                    changed |= ui.add(egui::TextEdit::singleline(&mut set.numbering.suffix).hint_text("suffix").desired_width(60.0)).changed();
                    ui.label(RichText::new(format!("first number {}", set.numbering.number(0))).small().weak());
                });
                // ── defaults: paper, frame, pens, text ──
                egui::CollapsingHeader::new("Defaults: paper, frame, pens and text heights (neutral starting values, editable)").default_open(false).show(ui, |ui| {
                    ui.label(RichText::new("These are the program's neutral defaults. None comes from a drafting standard or from an organisation: set them to the contract's requirement.").small().weak());
                    ui.label("New sheets start on");
                    page_editor(ui, "defaults-page", &mut s.defaults);
                    egui::Grid::new("text-heights").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                        let t = &mut set.styles.text;
                        for (label, v) in [("Title text height (mm)", &mut t.title), ("Heading (mm)", &mut t.heading), ("Normal (mm)", &mut t.normal), ("Label (mm)", &mut t.label), ("Small print (mm)", &mut t.small)] {
                            ui.label(label);
                            changed |= ui.add(egui::DragValue::new(v).speed(0.05).range(0.5..=30.0)).changed();
                            ui.end_row();
                        }
                    });
                    changed |= ui.checkbox(&mut set.styles.model_colours, "Print model geometry in the colours the modules gave it").changed();
                    ui.label(RichText::new("Pens (layer, line weight in mm, colour)").small());
                    egui::Grid::new("pens").num_columns(3).spacing([10.0, 3.0]).striped(true).show(ui, |ui| {
                        let layers: Vec<String> = set.styles.pens.pens.keys().cloned().collect();
                        for layer in layers {
                            if let Some(pen) = set.styles.pens.pens.get_mut(&layer) {
                                ui.label(RichText::new(&layer).monospace().small());
                                changed |= ui.add(egui::DragValue::new(&mut pen.weight_mm).speed(0.01).range(0.0..=3.0)).changed();
                                changed |= ui.color_edit_button_srgb(&mut pen.rgb).changed();
                                ui.end_row();
                            }
                        }
                    });
                });
                ui.add_space(4.0);
                // ── the sheets ──
                ui.label(RichText::new("Sheets").strong());
                let mut remove: Option<usize> = None;
                let mut up: Option<usize> = None;
                let mut edit: Option<usize> = None;
                egui::Grid::new("sources").num_columns(6).spacing([10.0, 4.0]).striped(true).show(ui, |ui| {
                    for h in ["#", "Kind", "Title", "Paper", "Numbering", ""] {
                        ui.label(RichText::new(h).small().weak());
                    }
                    ui.end_row();
                    for (i, source) in set.sources.iter().enumerate() {
                        let t = source.template();
                        let kind = SourceForm::from_source(i, source, s.defaults, &DisplayUnits::default()).kind;
                        ui.label((i + 1).to_string());
                        ui.label(kind.short());
                        ui.label(&t.title);
                        ui.label(format!("{} {}", t.page.size.name(), if t.page.orientation == Orientation::Landscape { "landscape" } else { "portrait" }));
                        ui.label(t.number_prefix.clone().map(|p| format!("{p}nnn")).unwrap_or_else(|| "follows the set".into()));
                        ui.horizontal(|ui| {
                            if ui.small_button("Edit").clicked() {
                                edit = Some(i);
                            }
                            if ui.add_enabled(i > 0, egui::Button::new("Up").small()).clicked() {
                                up = Some(i);
                            }
                            if ui.small_button("Delete").clicked() {
                                remove = Some(i);
                            }
                        });
                        ui.end_row();
                    }
                });
                if set.sources.is_empty() {
                    ui.label(RichText::new("No sheets yet. Add one from a template below.").small().weak());
                }
                if let Some(i) = edit {
                    let mut form = SourceForm::from_source(i, &set.sources[i], s.defaults, &crate::anno_ui::units_of(&self.doc));
                    form.to_displayed(&self.doc);
                    s.form = Some(form);
                }
                if let Some(i) = up {
                    set.sources.swap(i, i - 1);
                    changed = true;
                }
                if let Some(i) = remove {
                    set.sources.remove(i);
                    s.form = None;
                    changed = true;
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Add from template");
                    egui::ComboBox::from_id_salt("add-kind").selected_text(FormKind::ADD[s.add_kind].label()).width(430.0).show_ui(ui, |ui| {
                        for (i, k) in FormKind::ADD.iter().enumerate() {
                            ui.selectable_value(&mut s.add_kind, i, k.label());
                        }
                    });
                    if ui.button("Add…").clicked() {
                        s.add_form(&alignments, &corridors, &signs);
                    }
                });
            });
            ui.separator();
            // The actions on the whole set.
            ui.horizontal_wrapped(|ui| {
                use crate::commands::Action;
                if ui.button("Compose and preview").on_hover_text("Compose every sheet from the live model and open the preview").clicked() {
                    go = Some(Action::SheetPreview);
                }
                if ui.button("Export PDF…").on_hover_text("One PDF, a page per sheet").clicked() {
                    go = Some(Action::SheetPdf);
                }
                if ui.button("Export DXF, one per sheet…").clicked() {
                    go = Some(Action::SheetDxf);
                }
                if ui.button("Export DXF, one file…").on_hover_text("Every sheet side by side in one file").clicked() {
                    dxf_one = true;
                }
                if ui.button("Title block…").clicked() {
                    go = Some(Action::TitleBlock);
                }
                if ui.button("Project information…").clicked() {
                    go = Some(Action::ProjectInfo);
                }
            });
            match &s.composed {
                Some(c) => ui.label(RichText::new(format!("Last composed {} ({} sheet(s)); every compose reads the live model.", c.at, c.composed.sheets.len())).small().weak()),
                None => ui.label(RichText::new("Not composed yet. Every compose reads the live model.").small().weak()),
            };
            if !s.message.is_empty() {
                ui.label(RichText::new(&s.message).small().color(WARN()));
            }
        });

        // ── the source form, in its own window ──
        let mut form_done = false;
        let mut built: Option<(Option<usize>, SheetSource)> = None;
        if let Some(mut f) = s.form.take() {
            let mut keep = true;
            f.regions = f.alignment_obj(&self.doc).map(|a| a.station_equations.region_count()).unwrap_or(1);
            f.groups = self.doc.elements().iter().filter(|e| e.category == crate::civil::SAMPLES).map(|e| e.name.clone()).collect();
            let range_note = ranges.iter().find(|r| r.0 == f.alignment).map(|r| r.1.clone()).unwrap_or_default();
            let title = if f.editing.is_some() { format!("Edit sheet: {}", f.kind.short()) } else { format!("Add: {}", f.kind.short()) };
            crate::win::new(ctx, title).collapsible(false).default_width(560.0).show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([true, true]).max_height(560.0).show(ui, |ui| {
                    form_fields(ui, &mut f, &alignments, &corridors, &surfaces, &signs, &categories, &range_note, &units);
                });
                ui.add_space(6.0);
                if !f.error.is_empty() {
                    ui.label(RichText::new(&f.error).color(WARN()));
                }
                ui.horizontal(|ui| {
                    let label = if f.editing.is_some() { "Save changes" } else { "Add to the set" };
                    if ui.button(label).clicked() {
                        let old = f.editing.and_then(|i| s.set.as_ref().and_then(|set| set.sources.get(i)).cloned());
                        match f.build(&self.doc, old.as_ref()) {
                            Ok(src) => {
                                built = Some((f.editing, src));
                                form_done = true;
                            }
                            Err(e) => f.error = e,
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
            if keep && !form_done {
                s.form = Some(f);
            }
        }
        if let (Some((editing, src)), Some(set)) = (built, s.set.as_mut()) {
            match editing {
                Some(i) if i < set.sources.len() => set.sources[i] = src,
                _ => set.sources.push(src),
            }
            changed = true;
            s.message.clear();
        }

        // ── actions at the bottom of the manager ──
        match action {
            Some("new") => {
                let name = s.new_name.trim().to_string();
                let id = glue::new_set(&mut self.doc, &name);
                s.set_id = Some(id);
                s.loaded = serde_json::Value::Null;
                s.new_name.clear();
                s.composed = None;
                self.schedule_commit_now();
            }
            Some("delete") => {
                if let Some(id) = s.set_id {
                    self.doc.remove(id);
                    s.set_id = None;
                    s.set = None;
                    s.composed = None;
                    self.schedule_commit_now();
                }
            }
            _ => {}
        }
        if changed {
            self.sheets = s;
            self.sheets_store();
            s = std::mem::take(&mut self.sheets);
            // Whatever was composed no longer matches the definition.
            s.cache = None;
        }
        if !open {
            s.manager_open = false;
            s.form = None;
        }
        self.sheets = s;
        if dxf_one {
            self.export_sheet_dxf(true);
        }
        if let Some(a) = go {
            self.sheets_action(a);
        }
    }

    // ─── the title block editor ────────────────────────────────────────────────────────

    fn titleblock_window(&mut self, ctx: &egui::Context) {
        let mut s = std::mem::take(&mut self.sheets);
        // The layout being edited starts as the first sheet's of the selected set.
        if s.tb.is_none() || s.tb_for != s.set_id {
            s.tb = Some(s.set.as_ref().and_then(|set| set.sources.first()).map(|src| src.template().title_block.clone()).unwrap_or_default());
            s.tb_for = s.set_id;
        }
        let user_labels: Vec<String> = s.info.user_fields.iter().map(|f| f.label.clone()).filter(|l| !l.is_empty()).collect();
        let mut open = true;
        let mut apply = false;
        let mut save_json = false;
        let mut load_json = false;
        let has_set = s.set.is_some();
        crate::win::new(ctx, "Title block").open(&mut open).default_width(860.0).default_height(620.0).show(ctx, |ui| {
            let Some(tb) = s.tb.as_mut() else { return };
            ui.label(RichText::new("The title block is data: rows of cells, a revision table and a logo box. Nothing here reproduces any organisation's title block; build your own, or load one from a file you keep.").small().weak());
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(has_set, egui::Button::new("Apply to every sheet of the set")).clicked() {
                    apply = true;
                }
                if ui.button("Load from JSON…").clicked() {
                    load_json = true;
                }
                if ui.button("Save as JSON…").clicked() {
                    save_json = true;
                }
                if ui.button("Reload from the set").clicked() {
                    s.tb_for = None;
                }
                if ui.button("Reset to the neutral default").clicked() {
                    *tb = TitleBlockLayout::default();
                }
            });
            if !s.tb_message.is_empty() {
                ui.label(RichText::new(&s.tb_message).small().color(WARN()));
            }
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                titleblock_editor(ui, tb, &user_labels);
            });
        });
        if !open {
            s.titleblock_open = false;
        }
        if load_json {
            if let Some(path) = rfd::FileDialog::new().add_filter("Title block layout (JSON)", &["json"]).pick_file() {
                s.tb_message = match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<TitleBlockLayout>(&t).map_err(|e| format!("not a title block layout: {e}"))) {
                    Ok(l) => {
                        s.tb = Some(l);
                        format!("Loaded {}. Press Apply to use it on the set.", path.display())
                    }
                    Err(e) => format!("Could not load {}: {e}", path.display()),
                };
            }
        }
        if save_json {
            if let (Some(tb), Some(mut path)) = (s.tb.as_ref(), rfd::FileDialog::new().add_filter("Title block layout (JSON)", &["json"]).set_file_name("title-block.json").save_file()) {
                if path.extension().is_none() {
                    path.set_extension("json");
                }
                s.tb_message = match serde_json::to_string_pretty(tb).map_err(|e| e.to_string()).and_then(|t| std::fs::write(&path, t).map_err(|e| e.to_string())) {
                    Ok(()) => format!("Saved {}.", path.display()),
                    Err(e) => format!("Could not save {}: {e}", path.display()),
                };
            }
        }
        if apply {
            if let (Some(tb), Some(set)) = (s.tb.clone(), s.set.as_mut()) {
                for src in &mut set.sources {
                    src.template_mut().title_block = tb.clone();
                }
                s.tb_message = "Applied to every sheet of the set.".into();
                self.sheets = s;
                self.sheets_store();
                s = std::mem::take(&mut self.sheets);
                s.cache = None;
            }
        }
        self.sheets = s;
    }

    // ─── the preview ───────────────────────────────────────────────────────────────────

    fn preview_window(&mut self, ctx: &egui::Context) {
        let mut s = std::mem::take(&mut self.sheets);
        let mut open = true;
        let mut recompose = false;
        crate::win::new(ctx, "Sheet preview").open(&mut open).default_width(1100.0).default_height(760.0).show(ctx, |ui| {
            let Some(c) = s.composed.as_ref() else {
                ui.label("Nothing composed yet.");
                if ui.button("Compose the selected set").clicked() {
                    recompose = true;
                }
                return;
            };
            let n = c.composed.sheets.len();
            ui.horizontal_wrapped(|ui| {
                if ui.button("Compose again").on_hover_text("Read the live model again and rebuild every sheet").clicked() {
                    recompose = true;
                }
                ui.separator();
                if ui.add_enabled(s.page > 0, egui::Button::new("Previous")).clicked() {
                    s.page -= 1;
                    s.fit = true;
                }
                let label = c.composed.sheets.get(s.page).map(|sh| format!("{}  {}", sh.number, sh.title)).unwrap_or_default();
                egui::ComboBox::from_id_salt("preview-sheet").selected_text(format!("{} of {n}: {label}", s.page + 1)).width(360.0).show_ui(ui, |ui| {
                    for (i, sh) in c.composed.sheets.iter().enumerate() {
                        if ui.selectable_label(i == s.page, format!("{}  {}", sh.number, sh.title)).clicked() {
                            s.page = i;
                            s.fit = true;
                        }
                    }
                });
                if ui.add_enabled(s.page + 1 < n, egui::Button::new("Next")).clicked() {
                    s.page += 1;
                    s.fit = true;
                }
                ui.separator();
                if ui.button("Fit").clicked() {
                    s.fit = true;
                }
                if ui.button("Zoom in").clicked() {
                    s.zoom *= 1.4;
                }
                if ui.button("Zoom out").clicked() {
                    s.zoom /= 1.4;
                }
                ui.label(RichText::new(format!("{:.0} %", s.zoom * 100.0 / 3.7795)).small().weak());
            });
            ui.label(RichText::new(format!("Set '{}', composed {} from the live model. Scroll to zoom, drag to pan.", c.set_name, c.at)).small().weak());
            // Warnings of the composer for the sheet shown, and for the set.
            let sheet_number = c.composed.sheets.get(s.page).map(|sh| sh.number.clone()).unwrap_or_default();
            let mine: Vec<&String> = c.composed.sheets.get(s.page).map(|sh| sh.warnings.iter().collect()).unwrap_or_default();
            let others: Vec<&String> = c.composed.warnings.iter().filter(|w| !w.starts_with("Sheet ")).collect();
            let total = c.composed.warnings.len() + c.feed_notes.len();
            egui::CollapsingHeader::new(RichText::new(format!("Warnings: {} on this sheet, {total} in the set", mine.len())).color(if total > 0 { WARN() } else { ui.visuals().text_color() })).id_salt("preview-warnings").default_open(!mine.is_empty()).show(ui, |ui| {
                if total == 0 {
                    ui.label(RichText::new("The composer had nothing to report.").small().weak());
                }
                for w in &mine {
                    ui.label(RichText::new(format!("{sheet_number}: {w}")).small().color(WARN()));
                }
                for w in &others {
                    ui.label(RichText::new(*w).small());
                }
                for w in &c.composed.warnings {
                    if w.starts_with("Sheet ") && !w.starts_with(&format!("Sheet {sheet_number}:")) {
                        ui.label(RichText::new(w).small().weak());
                    }
                }
                for w in &c.feed_notes {
                    ui.label(RichText::new(format!("Data: {w}")).small());
                }
            });
            ui.separator();
            if s.cache.as_ref().is_none_or(|(p, at, _)| *p != s.page || at != &c.at) {
                s.cache = build_preview(c, s.page).map(|p| (s.page, c.at.clone(), p));
            }
            let Some((_, _, preview)) = s.cache.as_ref() else {
                ui.label("There is no such sheet.");
                return;
            };
            let avail = ui.available_size();
            if s.fit {
                let z = ((avail.x - 24.0) as f64 / preview.width_mm).min((avail.y - 24.0) as f64 / preview.height_mm).max(0.1) as f32;
                s.zoom = z;
                s.pan = Vec2::ZERO;
                s.fit = false;
            }
            let response = paint_preview(ui, preview, s.zoom, s.pan);
            if response.dragged() {
                s.pan += response.drag_delta();
            }
            if response.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    let f = (scroll / 200.0).exp();
                    let before = s.zoom;
                    s.zoom = (s.zoom * f).clamp(0.2, 60.0);
                    // Keep the point under the pointer fixed.
                    if let Some(pos) = response.hover_pos() {
                        let from_centre = pos - response.rect.center() - s.pan;
                        s.pan -= from_centre * (s.zoom / before - 1.0);
                    }
                }
            }
        });
        if !open {
            s.preview_open = false;
        }
        self.sheets = s;
        if recompose {
            self.compose_into_preview();
        }
    }
}

// ─── widgets ────────────────────────────────────────────────────────────────────────────

fn page_editor(ui: &mut Ui, id: &str, page: &mut Page) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| page_editor_top(ui, id, page));
        ui.horizontal(|ui| {
            let m: &mut Margins = &mut page.margins;
            ui.label("margins L R T B (mm)");
            for v in [&mut m.left, &mut m.right, &mut m.top, &mut m.bottom] {
                ui.add(egui::DragValue::new(v).speed(0.5).range(0.0..=100.0));
            }
        });
    });
}

fn page_editor_top(ui: &mut Ui, id: &str, page: &mut Page) {
    egui::ComboBox::from_id_salt((id, "size")).selected_text(page.size.name()).width(60.0).show_ui(ui, |ui| {
        for p in PaperSize::ALL {
            ui.selectable_value(&mut page.size, p, p.name());
        }
    });
    egui::ComboBox::from_id_salt((id, "orientation"))
        .selected_text(if page.orientation == Orientation::Landscape { "landscape" } else { "portrait" })
        .width(90.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut page.orientation, Orientation::Landscape, "landscape");
            ui.selectable_value(&mut page.orientation, Orientation::Portrait, "portrait");
        });
    ui.checkbox(&mut page.frame.draw_frame, "frame");
    ui.checkbox(&mut page.frame.draw_paper_edge, "paper edge");
}

/// A single-line text field of a fixed width. (`TextEdit::desired_width` alone shrinks to the grid
/// column when the column's width came from a heading label, so grids use this.)
fn edit(ui: &mut Ui, text: &mut String, width: f32, hint: &str) {
    ui.add_sized([width, ui.spacing().interact_size.y], egui::TextEdit::singleline(text).hint_text(hint));
}

fn text_row(ui: &mut Ui, label: &str, text: &mut String, hint: &str) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(260.0));
    ui.end_row();
}

fn combo_row(ui: &mut Ui, label: &str, id: &str, value: &mut String, options: &[String], allow_none: bool) {
    ui.label(label);
    egui::ComboBox::from_id_salt(id).selected_text(if value.is_empty() { "none".to_string() } else { value.clone() }).width(260.0).show_ui(ui, |ui| {
        if allow_none {
            ui.selectable_value(value, String::new(), "none");
        }
        for o in options {
            ui.selectable_value(value, o.clone(), o);
        }
    });
    ui.end_row();
}

#[allow(clippy::too_many_arguments)]
fn form_fields(ui: &mut Ui, f: &mut SourceForm, alignments: &[String], corridors: &[String], surfaces: &[String], signs: &[String], categories: &[(String, String)], range_note: &str, u: &DisplayUnits) {
    egui::Grid::new("source-form").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label("Sheet title");
        edit(ui, &mut f.title, 260.0, "");
        ui.end_row();
        ui.label("Numbering prefix");
        edit(ui, &mut f.prefix, 260.0, "empty: follow the set's numbering");
        ui.end_row();
        ui.label("Paper");
        page_editor(ui, "form-page", &mut f.page);
        ui.end_row();
        match f.kind {
            FormKind::Plan => {
                combo_row(ui, "Alignment", "form-alignment", &mut f.alignment, alignments, false);
                station_text_row(ui, "First station (as drawn)", &mut f.from, &mut f.from_region, "empty: the alignment's start", f.regions);
                station_text_row(ui, "Last station (as drawn)", &mut f.to, &mut f.to_region, "empty: the alignment's end", f.regions);
                text_row(ui, &crate::disp::head(u, "Length on each sheet"), &mut f.length, "enter the length");
                text_row(ui, &crate::disp::head(u, "Overlap"), &mut f.overlap, "enter the overlap, 0 for none");
                text_row(ui, "Scale 1:", &mut f.scale, "enter n");
                ui.label("Rotation");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut f.rotation, 0, "along the chord");
                    ui.selectable_value(&mut f.rotation, 1, "north up");
                    ui.selectable_value(&mut f.rotation, 2, "degrees");
                    if f.rotation == 2 {
                        edit(ui, &mut f.degrees, 80.0, "anticlockwise");
                    }
                });
                ui.end_row();
                text_row(ui, &crate::disp::head(u, "Station tick every"), &mut f.tick, "enter the interval");
                text_row(ui, &crate::disp::head(u, "Station label every"), &mut f.label_every, "enter the interval");
            }
            FormKind::Profile => {
                combo_row(ui, "Alignment", "form-alignment", &mut f.alignment, alignments, false);
                station_text_row(ui, "First station (as drawn)", &mut f.from, &mut f.from_region, "enter the station", f.regions);
                station_text_row(ui, "Last station (as drawn)", &mut f.to, &mut f.to_region, "enter the station", f.regions);
                text_row(ui, "Horizontal scale 1:", &mut f.scale, "enter n");
                text_row(ui, "Vertical scale 1:", &mut f.v_scale, "enter m");
                text_row(ui, &crate::disp::head(u, "Datum level"), &mut f.datum, "empty: a grid line under the lowest level");
                text_row(ui, &crate::disp::head(u, "Grid along the profile"), &mut f.grid_sta, "enter the interval");
                text_row(ui, &crate::disp::head(u, "Grid of level"), &mut f.grid_lvl, "enter the interval");
                text_row(ui, &crate::disp::head(u, "Data band every"), &mut f.band, "empty: the grid interval");
                combo_row(ui, "Ground surface", "form-surface", &mut f.surface, surfaces, true);
                ui.label("Vertical geometry");
                ui.vertical(|ui| {
                    ui.checkbox(&mut f.ann.pvi, "PVI station and level");
                    ui.checkbox(&mut f.ann.grades, "Grade of each tangent (percent)");
                    ui.checkbox(&mut f.ann.curves, "Vertical curve length, grade change, start and end");
                    ui.checkbox(&mut f.ann.k_value, "K value (length per percent of grade change), a computed number");
                });
                ui.end_row();
            }
            FormKind::Sections => {
                if !f.groups.is_empty() || f.from_samples {
                    ui.label("Sections from");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut f.from_samples, false, "a corridor");
                        ui.selectable_value(&mut f.from_samples, true, "a sample line group");
                    });
                    ui.end_row();
                }
                if f.from_samples {
                    let groups = f.groups.clone();
                    if f.sample_group.is_empty() {
                        f.sample_group = groups.first().cloned().unwrap_or_default();
                    }
                    combo_row(ui, "Sample line group", "form-samples", &mut f.sample_group, &groups, false);
                    // The interval is a corridor option: a sample line group has its own stations.
                    if f.mode == 1 {
                        f.mode = 0;
                    }
                } else {
                    combo_row(ui, "Corridor", "form-corridor", &mut f.corridor, corridors, false);
                }
                ui.label("Sections");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut f.mode, 0, "all in a range");
                    if !f.from_samples {
                        ui.selectable_value(&mut f.mode, 1, "at an interval");
                    }
                    ui.selectable_value(&mut f.mode, 2, "a list");
                });
                ui.end_row();
                if f.mode == 2 {
                    text_row(ui, "Stations (as drawn)", &mut f.list, "for example 1+100.000, 1+150.000");
                } else {
                    station_text_row(ui, "First station (as drawn)", &mut f.from, &mut f.from_region, if f.mode == 0 { "empty: the first section" } else { "enter the station" }, f.regions);
                    station_text_row(ui, "Last station (as drawn)", &mut f.to, &mut f.to_region, if f.mode == 0 { "empty: the last section" } else { "enter the station" }, f.regions);
                    if f.mode == 1 {
                        text_row(ui, &crate::disp::head(u, "Interval"), &mut f.interval, "enter the interval");
                    }
                }
                text_row(ui, "Scale 1:", &mut f.scale, "enter n");
                text_row(ui, "Vertical exaggeration", &mut f.exaggeration, "enter it, 1 for none");
                text_row(ui, "Columns on a sheet", &mut f.columns, "enter the number");
                text_row(ui, "Sections to a sheet", &mut f.per_sheet, "enter the number");
            }
            FormKind::Schedule => {
                ui.label("Table");
                egui::ComboBox::from_id_salt("form-table").selected_text(TABLE_KINDS[f.table_kind.min(TABLE_KINDS.len() - 1)].1).width(260.0).show_ui(ui, |ui| {
                    for (i, k) in TABLE_KINDS.iter().enumerate() {
                        if ui.selectable_value(&mut f.table_kind, i, k.1).clicked() {
                            f.title = k.2.to_string();
                        }
                    }
                });
                ui.end_row();
                if TABLE_KINDS[f.table_kind.min(TABLE_KINDS.len() - 1)].0 == FEED_QUANTITIES {
                    ui.label("Module");
                    egui::ComboBox::from_id_salt("form-category").selected_text(categories.iter().find(|c| c.0 == f.category).map(|c| c.1.clone()).unwrap_or_else(|| "every module".into())).width(260.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut f.category, String::new(), "every module");
                        for (c, n) in categories {
                            ui.selectable_value(&mut f.category, c.clone(), n);
                        }
                    });
                    ui.end_row();
                }
            }
            FormKind::Detail => {
                ui.label("Detail of");
                egui::ComboBox::from_id_salt("form-detail").selected_text(DETAIL_KINDS[f.detail_kind.min(DETAIL_KINDS.len() - 1)].1).width(260.0).show_ui(ui, |ui| {
                    for (i, k) in DETAIL_KINDS.iter().enumerate() {
                        if ui.selectable_value(&mut f.detail_kind, i, k.1).clicked() {
                            f.title = k.2.to_string();
                        }
                    }
                });
                ui.end_row();
                if f.detail_kind == 0 {
                    combo_row(ui, "Sign", "form-sign", &mut f.element, signs, false);
                    text_row(ui, "Scale 1:", &mut f.scale, "empty: the scale that fits");
                } else {
                    if glue::feed_needs_element(DETAIL_KINDS[f.detail_kind.min(DETAIL_KINDS.len() - 1)].0) {
                        if DETAIL_KINDS[f.detail_kind.min(DETAIL_KINDS.len() - 1)].0 == glue::FEED_LIGHT_CONTOURS {
                            text_row(ui, "Installation mark", &mut f.element, "the mark of the lighting element");
                        } else {
                            combo_row(ui, "Sign", "form-sign-vis", &mut f.element, signs, false);
                        }
                    }
                    ui.label("");
                    ui.label(RichText::new("Fitted to the sheet; filled from the model each time the set is composed.").small().weak());
                    ui.end_row();
                }
            }
            FormKind::Notes => {
                text_row(ui, "Heading", &mut f.heading, "optional");
                ui.label("Notes");
                ui.add(egui::TextEdit::multiline(&mut f.paragraphs).hint_text("one note a line").desired_rows(6).desired_width(260.0));
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut f.numbered, "Number the notes");
                ui.end_row();
            }
            FormKind::Props => {}
        }
    });
    if !range_note.is_empty() && matches!(f.kind, FormKind::Plan | FormKind::Profile) {
        ui.label(RichText::new(range_note).small().weak());
        ui.label(RichText::new("Stations are entered as the stations shown on the drawings, as 1+250.000 or as a number of metres; lengths and intervals are in the document's display unit (a trailing m or mm overrides it). With station equations, pick the region where a station occurs more than once; AfroBIM converts to internal stations.").small().weak());
    }
    if f.kind == FormKind::Props {
        ui.label(RichText::new("This sheet was not made by these forms: its title, paper and numbering can be changed here.").small().weak());
    }
}

fn field_name(k: &FieldKey) -> String {
    match k {
        FieldKey::Project => "Project".into(),
        FieldKey::Client => "Client".into(),
        FieldKey::ContractNumber => "Contract number".into(),
        FieldKey::DrawingTitle => "Drawing title".into(),
        FieldKey::DrawingNumber => "Drawing number".into(),
        FieldKey::Revision => "Revision".into(),
        FieldKey::Date => "Date".into(),
        FieldKey::Scale => "Scale".into(),
        FieldKey::DrawnBy => "Drawn by".into(),
        FieldKey::CheckedBy => "Checked by".into(),
        FieldKey::ApprovedBy => "Approved by".into(),
        FieldKey::SheetOf => "Sheet n of m".into(),
        FieldKey::User(l) => format!("Other: {l}"),
    }
}

const STANDARD_FIELDS: [FieldKey; 12] = [
    FieldKey::Project,
    FieldKey::Client,
    FieldKey::ContractNumber,
    FieldKey::DrawingTitle,
    FieldKey::DrawingNumber,
    FieldKey::Revision,
    FieldKey::Date,
    FieldKey::Scale,
    FieldKey::DrawnBy,
    FieldKey::CheckedBy,
    FieldKey::ApprovedBy,
    FieldKey::SheetOf,
];

fn shows(tb: &TitleBlockLayout, key: &FieldKey) -> bool {
    tb.blocks.iter().any(|b| matches!(b, Block::Rows(rows) if rows.iter().any(|r| r.cells.iter().any(|c| matches!(&c.content, CellContent::Field { key: k, .. } if k == key)))))
}

/// Shows or hides a field: hiding takes its cells out (and any row left empty); showing adds a row
/// for it at the end of the last block of rows.
fn set_shown(tb: &mut TitleBlockLayout, key: &FieldKey, on: bool) {
    if on {
        let label = field_name(key).trim_start_matches("Other: ").to_string();
        let row = Row { height_mm: 10.0, cells: vec![Cell::field(tb.width_mm, key.clone(), &label)] };
        match tb.blocks.iter_mut().rev().find_map(|b| if let Block::Rows(r) = b { Some(r) } else { None }) {
            Some(rows) => rows.push(row),
            None => tb.blocks.push(Block::Rows(vec![row])),
        }
    } else {
        for b in &mut tb.blocks {
            if let Block::Rows(rows) = b {
                for r in rows.iter_mut() {
                    r.cells.retain(|c| !matches!(&c.content, CellContent::Field { key: k, .. } if k == key));
                }
                rows.retain(|r| !r.cells.is_empty());
            }
        }
        tb.blocks.retain(|b| !matches!(b, Block::Rows(r) if r.is_empty()));
    }
}

fn field_combo(ui: &mut Ui, id: impl std::hash::Hash, key: &mut FieldKey, user_labels: &[String]) {
    egui::ComboBox::from_id_salt(id).selected_text(field_name(key)).width(150.0).show_ui(ui, |ui| {
        for k in STANDARD_FIELDS {
            let name = field_name(&k);
            ui.selectable_value(key, k, name);
        }
        for l in user_labels {
            let k = FieldKey::User(l.clone());
            let name = field_name(&k);
            ui.selectable_value(key, k, name);
        }
    });
}

fn titleblock_editor(ui: &mut Ui, tb: &mut TitleBlockLayout, user_labels: &[String]) {
    ui.horizontal(|ui| {
        ui.label("Placement");
        ui.selectable_value(&mut tb.placement, TitlePlacement::RightColumn, "column at the right");
        ui.selectable_value(&mut tb.placement, TitlePlacement::BottomRight, "block at the lower right");
        ui.label("Width (mm)");
        ui.add(egui::DragValue::new(&mut tb.width_mm).speed(0.5).range(20.0..=400.0));
        ui.label(RichText::new(format!("total height {:.1} mm", tb.height_mm())).small().weak());
    });
    ui.add_space(4.0);
    egui::CollapsingHeader::new("Fields shown").default_open(true).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for k in STANDARD_FIELDS.iter().cloned().chain(user_labels.iter().map(|l| FieldKey::User(l.clone()))) {
                let mut on = shows(tb, &k);
                if ui.checkbox(&mut on, field_name(&k)).changed() {
                    set_shown(tb, &k, on);
                }
            }
        });
        ui.label(RichText::new("Unticking removes the field's cells; ticking adds a row for it at the end. Order, sizes and captions are edited below.").small().weak());
    });
    ui.add_space(4.0);
    ui.label(RichText::new("Blocks, top down").strong());
    let n = tb.blocks.len();
    let mut remove: Option<usize> = None;
    let mut up: Option<usize> = None;
    for bi in 0..n {
        ui.separator();
        ui.horizontal(|ui| {
            let title = match &tb.blocks[bi] {
                Block::Rows(_) => "Rows of cells",
                Block::RevisionTable { .. } => "Revision table",
                Block::Logo { .. } => "Logo box",
                Block::Spacer { .. } => "Spacer",
            };
            ui.label(RichText::new(format!("{}. {title}", bi + 1)).strong());
            if ui.add_enabled(bi > 0, egui::Button::new("Up").small()).clicked() {
                up = Some(bi);
            }
            if ui.small_button("Delete block").clicked() {
                remove = Some(bi);
            }
        });
        let block_width = tb.width_mm;
        match &mut tb.blocks[bi] {
            Block::Rows(rows) => {
                let mut del_row: Option<usize> = None;
                egui::Grid::new(("tb-rows", bi)).num_columns(7).spacing([6.0, 3.0]).striped(true).show(ui, |ui| {
                    for h in ["Row height (mm)", "Cell", "Field or text", "Caption", "Width (mm, 0 = share)", "Bold", ""] {
                        ui.label(RichText::new(h).small().weak());
                    }
                    ui.end_row();
                    for (ri, row) in rows.iter_mut().enumerate() {
                        let mut del_cell: Option<usize> = None;
                        let cells = row.cells.len();
                        for ci in 0..cells {
                            if ci == 0 {
                                ui.add(egui::DragValue::new(&mut row.height_mm).speed(0.5).range(2.0..=200.0));
                            } else {
                                ui.label("");
                            }
                            let cell = &mut row.cells[ci];
                            // What the cell holds: a field, fixed text or nothing.
                            let mut kind = match &cell.content {
                                CellContent::Field { .. } => 0,
                                CellContent::Text { .. } => 1,
                                CellContent::Empty => 2,
                            };
                            let before = kind;
                            egui::ComboBox::from_id_salt(("tb-kind", bi, ri, ci)).selected_text(["Field", "Text", "Empty"][kind]).width(70.0).show_ui(ui, |ui| {
                                ui.selectable_value(&mut kind, 0, "Field");
                                ui.selectable_value(&mut kind, 1, "Text");
                                ui.selectable_value(&mut kind, 2, "Empty");
                            });
                            if kind != before {
                                cell.content = match kind {
                                    0 => CellContent::Field { key: FieldKey::Project, label: String::new() },
                                    1 => CellContent::Text { label: String::new(), text: String::new() },
                                    _ => CellContent::Empty,
                                };
                            }
                            match &mut cell.content {
                                CellContent::Field { key, label } => {
                                    field_combo(ui, ("tb-field", bi, ri, ci), key, user_labels);
                                    edit(ui, label, 130.0, "");
                                }
                                CellContent::Text { label, text } => {
                                    edit(ui, text, 150.0, "text");
                                    edit(ui, label, 130.0, "");
                                }
                                CellContent::Empty => {
                                    ui.label("");
                                    ui.label("");
                                }
                            }
                            ui.add(egui::DragValue::new(&mut cell.width_mm).speed(0.5).range(0.0..=400.0));
                            ui.checkbox(&mut cell.bold, "");
                            ui.horizontal(|ui| {
                                if ui.small_button("x").on_hover_text("Delete this cell").clicked() {
                                    del_cell = Some(ci);
                                }
                                if ci + 1 == cells {
                                    if ui.small_button("+ cell").on_hover_text("Add a cell to this row").clicked() {
                                        row.cells.push(Cell::field(0.0, FieldKey::Project, "Project"));
                                    }
                                    if ui.small_button("Delete row").clicked() {
                                        del_row = Some(ri);
                                    }
                                }
                            });
                            ui.end_row();
                        }
                        if let Some(ci) = del_cell {
                            row.cells.remove(ci);
                        }
                    }
                });
                rows.retain(|r| !r.cells.is_empty());
                if let Some(ri) = del_row {
                    if ri < rows.len() {
                        rows.remove(ri);
                    }
                }
                if ui.small_button("Add a row").clicked() {
                    rows.push(Row { height_mm: 10.0, cells: vec![Cell::field(block_width, FieldKey::Project, "Project")] });
                }
            }
            Block::RevisionTable { rows, header_height_mm, row_height_mm, columns } => {
                ui.horizontal(|ui| {
                    ui.label("Rows");
                    ui.add(egui::DragValue::new(rows).range(1..=40));
                    ui.label("header height (mm)");
                    ui.add(egui::DragValue::new(header_height_mm).speed(0.5).range(2.0..=40.0));
                    ui.label("row height (mm)");
                    ui.add(egui::DragValue::new(row_height_mm).speed(0.5).range(2.0..=40.0));
                });
                let mut del: Option<usize> = None;
                egui::Grid::new(("tb-rev", bi)).num_columns(4).spacing([6.0, 3.0]).show(ui, |ui| {
                    for h in ["Column heading", "Shows", "Width (mm)", ""] {
                        ui.label(RichText::new(h).small().weak());
                    }
                    ui.end_row();
                    for (ci, c) in columns.iter_mut().enumerate() {
                        edit(ui, &mut c.header, 110.0, "");
                        egui::ComboBox::from_id_salt(("tb-revfield", bi, ci)).selected_text(rev_name(c.field)).width(110.0).show_ui(ui, |ui| {
                            for f in [RevField::Rev, RevField::Date, RevField::Description, RevField::By, RevField::Checked] {
                                ui.selectable_value(&mut c.field, f, rev_name(f));
                            }
                        });
                        ui.add(egui::DragValue::new(&mut c.width_mm).speed(0.5).range(2.0..=200.0));
                        if ui.small_button("x").clicked() {
                            del = Some(ci);
                        }
                        ui.end_row();
                    }
                });
                if let Some(ci) = del {
                    columns.remove(ci);
                }
                if ui.small_button("Add a column").clicked() {
                    columns.push(RevColumn { header: "Rev".into(), width_mm: 10.0, field: RevField::Rev });
                }
            }
            Block::Logo { height_mm } | Block::Spacer { height_mm } => {
                ui.horizontal(|ui| {
                    ui.label("Height (mm)");
                    ui.add(egui::DragValue::new(height_mm).speed(0.5).range(1.0..=200.0));
                });
            }
        }
    }
    if let Some(i) = up {
        tb.blocks.swap(i, i - 1);
    }
    if let Some(i) = remove {
        tb.blocks.remove(i);
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Add a block");
        if ui.small_button("Rows").clicked() {
            tb.blocks.push(Block::Rows(vec![Row { height_mm: 10.0, cells: vec![Cell::field(tb.width_mm, FieldKey::Project, "Project")] }]));
        }
        if ui.small_button("Revision table").clicked() {
            tb.blocks.push(Block::RevisionTable {
                rows: 3,
                header_height_mm: 5.0,
                row_height_mm: 5.0,
                columns: vec![
                    RevColumn { header: "Rev".into(), width_mm: tb.width_mm * 0.15, field: RevField::Rev },
                    RevColumn { header: "Description".into(), width_mm: tb.width_mm * 0.55, field: RevField::Description },
                    RevColumn { header: "Date".into(), width_mm: tb.width_mm * 0.3, field: RevField::Date },
                ],
            });
        }
        if ui.small_button("Logo box").clicked() {
            tb.blocks.push(Block::Logo { height_mm: 15.0 });
        }
        if ui.small_button("Spacer").clicked() {
            tb.blocks.push(Block::Spacer { height_mm: 5.0 });
        }
    });
}

fn rev_name(f: RevField) -> &'static str {
    match f {
        RevField::Rev => "Revision",
        RevField::Date => "Date",
        RevField::Description => "Description",
        RevField::By => "By",
        RevField::Checked => "Checked",
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheets_glue::test_support::demo_doc;

    fn form(kind: FormKind, doc: &Document) -> SourceForm {
        let mut f = SourceForm::new(kind, Page::default());
        f.alignment = crate::alignment_glue::alignments(doc).first().map(|a| a.1.clone()).unwrap_or_default();
        f
    }

    #[test]
    fn every_number_starts_empty_and_an_empty_form_cannot_be_added() {
        let doc = demo_doc();
        for kind in FormKind::ADD {
            let f = form(kind, &doc);
            // No scale, length, overlap, interval or station is prefilled.
            for text in [&f.from, &f.to, &f.scale, &f.length, &f.overlap, &f.tick, &f.label_every, &f.v_scale, &f.datum, &f.grid_sta, &f.grid_lvl, &f.band, &f.interval, &f.exaggeration, &f.columns, &f.per_sheet] {
                assert!(text.is_empty());
            }
            if matches!(kind, FormKind::Plan | FormKind::Profile | FormKind::Sections) {
                assert!(f.build(&doc, None).is_err(), "{:?} built from nothing", kind.short());
            }
        }
        // The paper, frame and margins are the crate's neutral defaults.
        assert_eq!(SourceForm::new(FormKind::Plan, Page::default()).page, Page::default());
    }

    #[test]
    fn a_plan_series_form_checks_its_numbers_and_builds_the_series() {
        let doc = demo_doc();
        let mut f = form(FormKind::Plan, &doc);
        f.title = "PLAN".into();
        f.scale = "2500".into();
        f.length = "300".into();
        f.overlap = "300".into(); // as long as the sheet: refused
        f.tick = "25".into();
        f.label_every = "100".into();
        assert!(f.build(&doc, None).unwrap_err().contains("overlap"));
        f.overlap = "20".into();
        f.scale = "x".into();
        assert!(f.build(&doc, None).unwrap_err().contains("not a number"));
        f.scale = "2500".into();
        match f.build(&doc, None).unwrap() {
            SheetSource::PlanSeries(p) => {
                assert_eq!((p.length_m, p.overlap_m, p.from_station_m, p.to_station_m), (300.0, 20.0, None, None));
                match &p.template.views[0].kind {
                    ViewKind::Plan(v) => assert_eq!((v.scale, v.tick_interval_m, v.label_interval_m), (Scale(2500.0), 25.0, 100.0)),
                    _ => panic!("not a plan"),
                }
            }
            _ => panic!("not a plan series"),
        }
        // An alignment that is not in the document is refused.
        f.alignment = "nope".into();
        assert!(f.build(&doc, None).unwrap_err().contains("no alignment"));
    }

    #[test]
    fn the_sheet_forms_take_displayed_stations_with_a_region_and_convert_them_to_internal() {
        // The demo alignment has station equations: displayed 1550 occurs twice, at internal 1500 and 1650.
        let doc = demo_doc();
        let mut f = form(FormKind::Profile, &doc);
        f.title = "PROFILE".into();
        f.from = "1550".into();
        f.to = "1700".into();
        f.scale = "5000".into();
        f.v_scale = "250".into();
        f.grid_sta = "100".into();
        f.grid_lvl = "2".into();
        // Left as "occurs once", the ambiguous station is refused.
        assert!(f.build(&doc, None).is_err());
        f.from_region = 2;
        f.to_region = 3;
        let src = f.build(&doc, None).unwrap();
        let SheetSource::Single(spec) = &src else { panic!("not a single sheet") };
        let ViewKind::Profile(v) = &spec.views[0].kind else { panic!("not a profile") };
        assert!((v.from_station_m - 1500.0).abs() < 1e-6, "{}", v.from_station_m);
        assert!(v.to_station_m > v.from_station_m);
        // The edit form shows the stations as drawn, with their regions.
        let mut g = SourceForm::from_source(0, &src, Page::default(), &DisplayUnits::default());
        g.to_displayed(&doc);
        assert_eq!((g.from.as_str(), g.from_region), ("1+550.000", 2));
        assert!(g.build(&doc, Some(&src)).unwrap() == src);
    }

    /// The demo document with its display length unit set to millimetres.
    fn doc_in_millimetres() -> Document {
        let mut doc = demo_doc();
        let mut settings = afrobim_annotate::settings::load(&doc);
        settings.units.length = afrobim_annotate::LengthUnit::Millimetre;
        settings.units.length_decimals = 1;
        afrobim_annotate::settings::store(&mut doc, &settings);
        doc
    }

    #[test]
    fn the_sheet_forms_take_lengths_and_stations_in_the_display_units_and_store_metres() {
        let doc = doc_in_millimetres();
        // A plan series: a bare length is millimetres, a suffix overrides it, a station is k+mmm or metres.
        let mut f = form(FormKind::Plan, &doc);
        f.title = "PLAN".into();
        f.scale = "2500".into();
        f.length = "300000".into(); // millimetres
        f.overlap = "20m".into();
        f.tick = "25000".into();
        f.label_every = "100000mm".into();
        f.from = "1+000.000".into();
        f.to = "1250".into();
        let src = f.build(&doc, None).unwrap();
        let SheetSource::PlanSeries(p) = &src else { panic!("not a plan series") };
        assert_eq!((p.length_m, p.overlap_m), (300.0, 20.0), "the source holds metres");
        let ViewKind::Plan(v) = &p.template.views[0].kind else { panic!("not a plan") };
        assert_eq!((v.tick_interval_m, v.label_interval_m), (25.0, 100.0));
        // The edit form shows them again in the same units, and building it gives the same source.
        let units = crate::anno_ui::units_of(&doc);
        let mut g = SourceForm::from_source(0, &src, Page::default(), &units);
        g.to_displayed(&doc);
        assert_eq!((g.length.as_str(), g.overlap.as_str(), g.tick.as_str(), g.label_every.as_str()), ("300000", "20000", "25000", "100000"));
        assert!(g.from.contains('+') && g.to.contains('+'), "stations are shown as k+mmm: {} {}", g.from, g.to);
        assert!(g.build(&doc, Some(&src)).unwrap() == src);
        // A length that is not one is refused with its name, in the display unit.
        f.length = "x".into();
        let e = f.build(&doc, None).unwrap_err();
        assert!(e.contains("not a length") && e.contains("(mm)"), "{e}");
        f.length = "300000".into();
        f.from = "1+5".into();
        assert!(f.build(&doc, None).unwrap_err().contains("not a station"));
        // A profile: level and grid intervals are lengths, scales stay as 1:n.
        let mut p = form(FormKind::Profile, &doc);
        p.title = "PROFILE".into();
        p.from = "1+000.000".into();
        p.to = "1+250.000".into();
        p.scale = "5000".into();
        p.v_scale = "250".into();
        p.datum = "100000".into();
        p.grid_sta = "100000".into();
        p.grid_lvl = "2000".into();
        p.band = "50000".into();
        let src = p.build(&doc, None).unwrap();
        let SheetSource::Single(spec) = &src else { panic!("not a single sheet") };
        let ViewKind::Profile(v) = &spec.views[0].kind else { panic!("not a profile") };
        assert_eq!((v.datum_level_m, v.grid_station_interval_m, v.grid_level_interval_m, v.band_interval_m), (Some(100.0), 100.0, 2.0, 50.0));
        assert_eq!((v.h_scale, v.vertical_exaggeration), (Scale(5000.0), 20.0));
        let mut g = SourceForm::from_source(0, &src, Page::default(), &units);
        g.to_displayed(&doc);
        assert_eq!((g.datum.as_str(), g.grid_sta.as_str(), g.grid_lvl.as_str(), g.band.as_str(), g.scale.as_str(), g.v_scale.as_str()), ("100000", "100000", "2000", "50000", "5000", "250"));
        assert!(g.build(&doc, Some(&src)).unwrap() == src);
        // Sections: an interval and a list of stations.
        let corridor = doc.elements().iter().find(|e| e.category == afrobim_corridor::CORRIDOR_CATEGORY).unwrap().name.clone();
        let mut c = form(FormKind::Sections, &doc);
        c.title = "SECTIONS".into();
        c.corridor = corridor;
        c.mode = 1;
        c.from = "1+000.000".into();
        c.to = "1+900.000".into();
        c.interval = "100000".into();
        c.scale = "200".into();
        c.exaggeration = "1".into();
        c.columns = "2".into();
        c.per_sheet = "4".into();
        assert!(c.build(&doc, None).is_ok(), "{:?}", c.build(&doc, None).err());
        c.mode = 2;
        c.list = "1+100.000, 1150, 1+200.000".into();
        assert!(c.build(&doc, None).is_ok() || c.build(&doc, None).unwrap_err().contains("no section"));
        c.list = "1+1".into();
        assert!(c.build(&doc, None).unwrap_err().contains("not a station"));
    }

    #[test]
    fn a_profile_form_takes_two_scales_and_keeps_them_when_edited() {
        let doc = demo_doc();
        let mut f = form(FormKind::Profile, &doc);
        f.title = "PROFILE".into();
        f.from = "1000".into();
        f.to = "1250".into();
        f.scale = "5000".into();
        f.v_scale = "250".into();
        f.grid_sta = "100".into();
        f.grid_lvl = "2".into();
        f.ann.k_value = false;
        let src = f.build(&doc, None).unwrap();
        let SheetSource::Single(spec) = &src else { panic!("not a single sheet") };
        let ViewKind::Profile(v) = &spec.views[0].kind else { panic!("not a profile") };
        // 1:5000 across and 1:250 up is an exaggeration of 5000 / 250 = 20.
        assert_eq!((v.h_scale, v.vertical_exaggeration, v.datum_level_m, v.band_interval_m), (Scale(5000.0), 20.0, None, 0.0));
        assert!(!v.annotations.k_value && v.annotations.pvi);
        // The edit form shows what the source holds, and building it again gives the same source.
        let g = SourceForm::from_source(0, &src, Page::default(), &DisplayUnits::default());
        assert!(g.kind == FormKind::Profile && g.scale == "5000" && g.v_scale == "250" && g.grid_lvl == "2" && g.from == "1000");
        assert!(g.build(&doc, Some(&src)).unwrap() == src);
        // The last station must be beyond the first.
        f.to = "900".into();
        assert!(f.build(&doc, None).is_err());
    }

    #[test]
    fn schedule_detail_and_notes_forms_build_fed_sheets() {
        let doc = demo_doc();
        let mut f = form(FormKind::Schedule, &doc);
        f.title = "BILL".into();
        f.table_kind = 2;
        let SheetSource::TableSeries(t) = f.build(&doc, None).unwrap() else { panic!("not a table series") };
        let ViewKind::Table(table) = &t.template.views[0].kind else { panic!("not a table") };
        assert_eq!(table.feed.as_deref(), Some(crate::sheets_glue::FEED_BILL));
        // A module's quantities name the module.
        f.table_kind = 1;
        f.category = afrobim_drainage::DRAIN_CATEGORY.into();
        let SheetSource::TableSeries(t) = f.build(&doc, None).unwrap() else { panic!("not a table series") };
        let ViewKind::Table(table) = &t.template.views[0].kind else { panic!("not a table") };
        assert_eq!(table.feed.as_deref(), Some("quantities:drain"));
        // The Gantt chart.
        let mut d = form(FormKind::Detail, &doc);
        d.title = "GANTT".into();
        d.detail_kind = 1;
        let SheetSource::Single(spec) = d.build(&doc, None).unwrap() else { panic!("not a single sheet") };
        let ViewKind::Detail(dv) = &spec.views[0].kind else { panic!("not a detail") };
        assert!(dv.feed.as_deref() == Some(crate::sheets_glue::FEED_GANTT) && dv.fit);
        // A sign face needs a sign.
        d.detail_kind = 0;
        assert!(d.build(&doc, None).is_err());
        // Notes need text.
        let mut n = form(FormKind::Notes, &doc);
        n.title = "NOTES".into();
        assert!(n.build(&doc, None).is_err());
        n.paragraphs = "First note\n\nSecond note".into();
        n.numbered = true;
        let SheetSource::Single(spec) = n.build(&doc, None).unwrap() else { panic!("not a single sheet") };
        let ViewKind::Notes(notes) = &spec.views[0].kind else { panic!("not notes") };
        assert_eq!((notes.paragraphs.len(), notes.numbered), (2, true));
    }

    #[test]
    fn a_numbering_prefix_and_paper_per_sheet_carry_into_the_source() {
        let doc = demo_doc();
        let mut f = form(FormKind::Notes, &doc);
        f.title = "NOTES".into();
        f.paragraphs = "x".into();
        f.prefix = " ZZ-N- ".into();
        f.page = Page::new(PaperSize::A4, Orientation::Portrait);
        f.page.margins = Margins::uniform(7.0);
        let src = f.build(&doc, None).unwrap();
        assert_eq!(src.template().number_prefix.as_deref(), Some("ZZ-N-"));
        assert_eq!((src.template().page.size, src.template().page.orientation, src.template().page.margins.left), (PaperSize::A4, Orientation::Portrait, 7.0));
        // Editing keeps the sheet's title block.
        let mut old = src.clone();
        old.template_mut().title_block.width_mm = 55.0;
        assert_eq!(f.build(&doc, Some(&old)).unwrap().template().title_block.width_mm, 55.0);
    }

    #[test]
    fn title_block_fields_can_be_hidden_and_shown_and_the_layout_round_trips_as_json() {
        let mut tb = TitleBlockLayout::default();
        assert!(shows(&tb, &FieldKey::Client) && shows(&tb, &FieldKey::SheetOf));
        let height = tb.height_mm();
        // Hiding the client removes its row (10 mm).
        set_shown(&mut tb, &FieldKey::Client, false);
        assert!(!shows(&tb, &FieldKey::Client));
        assert!((tb.height_mm() - (height - 10.0)).abs() < 1e-9);
        // Hiding one cell of a row of three leaves the others.
        set_shown(&mut tb, &FieldKey::Scale, false);
        assert!(shows(&tb, &FieldKey::Revision) && shows(&tb, &FieldKey::Date) && !shows(&tb, &FieldKey::Scale));
        // Showing adds a row of 10 mm at the end.
        set_shown(&mut tb, &FieldKey::Client, true);
        assert!(shows(&tb, &FieldKey::Client));
        assert!((tb.height_mm() - (height - 10.0 + 10.0)).abs() < 1e-9);
        // A user field.
        let key = FieldKey::User("Phase".into());
        set_shown(&mut tb, &key, true);
        assert!(shows(&tb, &key));
        // JSON round trip, the way a layout is kept as a file.
        let json = serde_json::to_string_pretty(&tb).unwrap();
        let back: TitleBlockLayout = serde_json::from_str(&json).unwrap();
        assert!(back == tb);
        // A file that is not a layout is refused.
        assert!(serde_json::from_str::<TitleBlockLayout>("{\"width_mm\": 1}").is_err());
    }

    #[test]
    fn the_preview_builds_and_paints_every_sheet_of_the_demo_set() {
        let doc = demo_doc();
        let registry = crate::sheets_glue::test_support::registry();
        let (id, _) = crate::sheets_glue::sets(&doc).into_iter().next().unwrap();
        let live = crate::sheets_glue::Live::from_doc(&doc, &registry, None);
        let c = crate::sheets_glue::compose_stored(&doc, &registry, id, &live).unwrap();
        let ctx = egui::Context::default();
        for page in 0..c.composed.sheets.len() {
            let p = build_preview(&c, page).expect("a preview for each sheet");
            // Strokes, filled artwork and labels all come through `to_drawing`.
            assert!(!p.lines.is_empty() && !p.sheet.labels.is_empty(), "sheet {page}");
            assert_eq!((p.width_mm, p.height_mm), (420.0, 297.0));
            // Painting it does not panic at a fitted and at a close zoom.
            for zoom in [1.0, 6.0] {
                let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1000.0, 700.0))), ..Default::default() };
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        paint_preview(ui, &p, zoom, Vec2::new(10.0, -5.0));
                    });
                });
            }
        }
        assert!(build_preview(&c, 99).is_none());
        // Strokes that meet end to end were chained: far fewer polylines than drawing segments.
        let p = build_preview(&c, 4).unwrap();
        let segments = p.sheet.drawing.primitives.iter().filter(|q| matches!(q, Primitive::Line { .. })).count();
        assert!(p.lines.len() < segments);
    }

    #[test]
    fn every_schedule_and_chart_feed_composes_from_the_demo_model() {
        // One sheet of each kind of table and chart the forms offer. The demo has a programme and an
        // estimate, so those feeds are full; it has no clash results (CLASH has not been run) and no
        // sign engine in this test, and those two feeds say so instead.
        let doc = demo_doc();
        let registry = crate::sheets_glue::test_support::registry();
        let mut set = SheetSet::new("all feeds");
        let page = Page::default();
        let mut f = SourceForm::new(FormKind::Schedule, page);
        for (i, kind) in TABLE_KINDS.iter().enumerate() {
            f.table_kind = i;
            f.title = kind.2.to_string();
            if kind.0 == FEED_QUANTITIES {
                f.category = String::new();
            }
            set.sources.push(f.build(&doc, None).unwrap());
        }
        let mut d = SourceForm::new(FormKind::Detail, page);
        // The feeds that stand alone; the three that name an element are tested with their engines.
        for i in 1..6 {
            d.detail_kind = i;
            d.title = DETAIL_KINDS[i].2.to_string();
            set.sources.push(d.build(&doc, None).unwrap());
        }
        let model = afrobim_sheets::Model { doc: &doc, registry: &registry };
        let live = crate::sheets_glue::Live::from_doc(&doc, &registry, None);
        let notes = crate::sheets_glue::feed_set(&mut set, &model, &live);
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(notes.iter().any(|n| n.contains("There are no clash results")) && notes.iter().any(|n| n.contains("traffic signs module is not loaded")), "{notes:?}");
        let composed = afrobim_sheets::compose_set_with(&model, &set, &afrobim_sheets::project_info(&doc));
        assert!(composed.sheets.len() >= 8, "{}", composed.sheets.len());
        let all: Vec<String> = composed.sheets.iter().flat_map(|s| s.texts().map(|t| t.text.clone())).collect();
        // The priced bill is the demo estimate's (item 1.1, earthworks, cut), the check summary lists
        // the elements, the quantities come from the modules, and the charts carry their dates.
        assert!(all.iter().any(|t| t.contains("Earthworks, cut")), "bill missing");
        assert!(all.iter().any(|t| t == "Input needed" || t == "Passes"), "check summary missing");
        assert!(all.iter().any(|t| t.contains("2026-03-02")), "gantt or s-curve dates missing");
        assert!(all.iter().any(|t| t == "100%") && all.iter().any(|t| t.contains("2026-")), "s-curve labels missing");
    }

    #[test]
    fn a_layout_edited_in_the_table_composes_into_the_sheet() {
        // A layout with one row of two cells and no revision table, 90 mm wide, composes and prints its cells.
        let doc = demo_doc();
        let registry = crate::sheets_glue::test_support::registry();
        let (id, _) = crate::sheets_glue::sets(&doc).into_iter().next().unwrap();
        let mut set = afrobim_sheets::load_sheet_set(&doc, id).unwrap();
        let mut tb = TitleBlockLayout::default();
        tb.width_mm = 90.0;
        tb.blocks = vec![Block::Rows(vec![Row { height_mm: 12.0, cells: vec![Cell::field(45.0, FieldKey::Project, "Proj"), Cell::field(0.0, FieldKey::DrawingNumber, "No")] }])];
        for s in &mut set.sources {
            s.template_mut().title_block = tb.clone();
        }
        let model = afrobim_sheets::Model { doc: &doc, registry: &registry };
        let live = crate::sheets_glue::Live::from_doc(&doc, &registry, None);
        let mut fed = set.clone();
        crate::sheets_glue::feed_set(&mut fed, &model, &live);
        let composed = afrobim_sheets::compose_set_with(&model, &fed, &afrobim_sheets::project_info(&doc));
        let texts: Vec<String> = composed.sheets[0].texts().map(|t| t.text.clone()).collect();
        assert!(texts.iter().any(|t| t == "Proj") && texts.iter().any(|t| t == "No") && texts.iter().any(|t| t == "DEMO-PL-001"), "{texts:?}");
        assert!(!texts.iter().any(|t| t == "Revision"));
    }
}
