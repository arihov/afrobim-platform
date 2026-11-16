//! Photometry for street lighting (`PHOTOMETRY`): choose an IES or LDT luminaire file for a lighting
//! installation, read the file's own checks and its polar diagram BEFORE any calculation, confirm
//! the orientation assumption against the diagram, enter the calculation settings, run, and use the
//! results in the class check.
//!
//! What this is and is not (stated in the window and in the manual): a calculation of illuminance
//! (and, with a user r-table, luminance) on grids the user defines, from the luminaire file, by
//! first-principles formulas of the `afrobim-lighting` crate. It is **not** an EN 13201-3
//! calculation, it has **not** been validated against DIALux, Relux or AGi32, luminance needs an
//! r-table file the user supplies (none ships), and threshold increment, longitudinal uniformity and
//! wet overall uniformity are not computed. Nothing is prefilled except the orientation, which is the
//! crate's per-format assumption shown as "to confirm" and not used until ticked.

use std::path::PathBuf;

use afrobim_draw::Primitive;
use afrobim_kernel::{Document, ElementId};
use afrobim_lighting::calc::{
    contour_drawing, default_carriageway_area, grid_to_csv, grid_values_drawing, luminaire_instances, nice_levels, photometry_settings_of, polar_diagram, resolve_photometry, run_photometry, set_photometry, AreaKind,
    CalcArea, CalcSettings, Division, Facing, LabelledDrawing, ObserverPosition, ObserverSpec, Orientation, PhotometryRun, RTable, VerticalSettings,
};
use afrobim_lighting::design::PhotometricResults;
use afrobim_lighting::photometry::{sha256_hex, LoadedPhotometry};
use afrobim_lighting::{lighting_of, load_photometry_file, LightingData, LightingDefinition};
use egui::{pos2, Align2, Color32, FontId, RichText, Shape, Stroke, Ui};

use crate::app::AfroBimApp;

#[allow(non_snake_case)]


/// What the photometry is and is not. Shown in the window, quoted in the manual.
pub const LIMITS: &str = "This calculates illuminance (and luminance only when you supply an r-table file) on the grids you define, from the luminaire file, using the formulas documented in afrobim-lighting. It is not an EN 13201-3 calculation (its grid and observer rules are not held). It has not been validated against DIALux, Relux or AGi32. Luminance needs an r-table file that you supply: none ships. Threshold increment, longitudinal uniformity and wet overall uniformity are not computed and stay as you enter them.";

/// Shown when no luminaire file is loaded, and in the demo.
pub const NO_FILE: &str = "AfroBIM ships no luminaire file. Choose the IES (.ies) or EULUMDAT (.ldt) file for the luminaire of this installation, from the manufacturer. Only type C photometry is read. Nothing is calculated until you have looked at the file's checks and the polar diagram and confirmed the orientation.";

/// What a photometry calculation needs, read from the form and the document on the UI thread.
pub struct PhotoInput {
    pub id: ElementId,
    pub loaded: LoadedPhotometry,
    pub settings: CalcSettings,
    pub rtable: Option<RTable>,
    pub data: LightingData,
    pub alignment: Option<afrobim_alignment::Alignment>,
    pub path: String,
    pub rt_info: Option<(String, String)>,
}

/// A finished calculation, waiting to be stored on the installation and shown.
pub struct PhotoOut {
    pub id: ElementId,
    pub run: PhotometryRun,
    pub settings: CalcSettings,
    pub loaded: LoadedPhotometry,
    pub path: String,
    pub rt_info: Option<(String, String)>,
    pub data: LightingData,
}

impl PhotoInput {
    /// The calculation itself (the part that takes the time).
    pub fn compute(self) -> Result<PhotoOut, String> {
        let run = run_photometry(&self.data, &self.loaded.photometry, &self.loaded.file_name, &self.loaded.sha256, &self.settings, self.rtable.as_ref(), self.alignment.as_ref()).map_err(|e| e.to_string())?;
        Ok(PhotoOut { id: self.id, run, settings: self.settings, loaded: self.loaded, path: self.path, rt_info: self.rt_info, data: self.data })
    }
}

/// Starts [`PhotoInput::compute`] on a worker thread, to be applied only while `doc` is unchanged.
pub fn spawn_photometry(doc: &Document, input: PhotoInput) -> crate::background::Job<Result<PhotoOut, String>> {
    crate::background::Job::spawn("Photometry", crate::background::fingerprint(doc), move || input.compute())
}

/// One calculation area as typed.
#[derive(Clone, Debug, PartialEq)]
pub struct AreaForm {
    pub name: String,
    pub kind: AreaKind,
    pub from_m: f64,
    pub to_m: f64,
    pub off_from: String,
    pub off_to: String,
    pub along_spacing: bool,
    pub along: String,
    pub across_spacing: bool,
    pub across: String,
}

impl AreaForm {
    fn blank() -> AreaForm {
        AreaForm { name: String::new(), kind: AreaKind::Carriageway, from_m: 0.0, to_m: 0.0, off_from: String::new(), off_to: String::new(), along_spacing: false, along: String::new(), across_spacing: false, across: String::new() }
    }

    pub(crate) fn of(a: &CalcArea) -> AreaForm {
        let split = |d: Division| match d {
            Division::Count(n) => (false, n.to_string()),
            Division::Spacing(s) => (true, afrobim_sheets::trim_number(s, 6)),
        };
        let (as_, a_) = split(a.along);
        let (cs, c_) = split(a.across);
        AreaForm {
            name: a.name.clone(),
            kind: a.kind,
            from_m: a.station_from_m,
            to_m: a.station_to_m,
            off_from: afrobim_sheets::trim_number(a.offset_from_m, 6),
            off_to: afrobim_sheets::trim_number(a.offset_to_m, 6),
            along_spacing: as_,
            along: a_,
            across_spacing: cs,
            across: c_,
        }
    }

    fn build(&self) -> Result<CalcArea, String> {
        let div = |label: &str, spacing: bool, t: &str| -> Result<Division, String> {
            let v = crate::check_inputs::required_positive(label, t)?;
            Ok(if spacing { Division::Spacing(v) } else { Division::Count(v as usize) })
        };
        if self.name.trim().is_empty() {
            return Err("Give each area a name.".into());
        }
        if self.to_m <= self.from_m {
            return Err(format!("Area '{}': the last station must be beyond the first.", self.name.trim()));
        }
        Ok(CalcArea {
            name: self.name.trim().to_string(),
            kind: self.kind,
            station_from_m: self.from_m,
            station_to_m: self.to_m,
            offset_from_m: crate::check_inputs::required(&format!("'{}' offset from", self.name.trim()), &self.off_from)?,
            offset_to_m: crate::check_inputs::required(&format!("'{}' offset to", self.name.trim()), &self.off_to)?,
            along: div(&format!("'{}' points along", self.name.trim()), self.along_spacing, &self.along)?,
            across: div(&format!("'{}' points across", self.name.trim()), self.across_spacing, &self.across)?,
        })
    }
}

/// The dialog's entries.
#[derive(Clone)]
pub struct PhotoForm {
    pub element: Option<ElementId>,
    pub orient_c: String,
    pub orient_ccw: bool,
    pub orient_confirmed: bool,
    /// The format the proposed orientation was taken for.
    pub orient_for: Option<String>,
    pub maintenance: String,
    pub contribution: String,
    pub plane: String,
    pub vertical_on: bool,
    pub vertical_height: String,
    pub eir_on: bool,
    pub eir_width: String,
    pub rtable: Option<(PathBuf, String)>,
    pub obs_on: bool,
    pub obs_station: f64,
    pub obs_offset: String,
    pub obs_height: String,
    pub areas: Vec<AreaForm>,
    pub error: String,
}

impl Default for PhotoForm {
    fn default() -> Self {
        PhotoForm {
            element: None,
            orient_c: String::new(),
            orient_ccw: true,
            orient_confirmed: false,
            orient_for: None,
            maintenance: String::new(),
            contribution: String::new(),
            plane: String::new(),
            vertical_on: false,
            vertical_height: String::new(),
            eir_on: false,
            eir_width: String::new(),
            rtable: None,
            obs_on: false,
            obs_station: 0.0,
            obs_offset: String::new(),
            obs_height: String::new(),
            areas: Vec::new(),
            error: String::new(),
        }
    }
}

fn num(v: f64) -> String {
    afrobim_sheets::trim_number(v, 6)
}

impl PhotoForm {
    /// The calculation settings, or what is missing. Refuses until the orientation is confirmed.
    pub fn settings(&self) -> Result<(CalcSettings, Option<RTable>), String> {
        if !self.orient_confirmed {
            return Err("The orientation (which C-plane faces the carriageway) is an assumption. Check it against the polar diagram and tick to confirm it.".into());
        }
        let c = crate::check_inputs::required("The C-plane that faces the carriageway", &self.orient_c)?;
        let maintenance = crate::check_inputs::required("Maintenance factor", &self.maintenance)?;
        if !(maintenance > 0.0 && maintenance <= 1.0) {
            return Err("The maintenance factor must be above 0 and at most 1.".into());
        }
        let contribution = crate::check_inputs::required_positive("Contribution distance (m)", &self.contribution)?;
        if self.areas.is_empty() {
            return Err("Add at least one calculation area.".into());
        }
        let areas = self.areas.iter().map(|a| a.build()).collect::<Result<Vec<_>, _>>()?;
        let rtable = match &self.rtable {
            Some((p, _)) => Some(RTable::load(p).map_err(|e| e.to_string())?),
            None => None,
        };
        let observers = if self.obs_on {
            if rtable.is_none() {
                return Err("An observer is only used with an r-table file: choose one, or untick the observer.".into());
            }
            vec![ObserverSpec {
                name: "Observer 1".into(),
                position: ObserverPosition::StationOffset { station_m: self.obs_station, offset_m: crate::check_inputs::required("Observer offset (m)", &self.obs_offset)? },
                height_m: crate::check_inputs::required_positive("Observer eye height (m)", &self.obs_height)?,
            }]
        } else {
            Vec::new()
        };
        Ok((
            CalcSettings {
                maintenance_factor: maintenance,
                contribution_distance_m: contribution,
                orientation: Orientation { c_toward_carriageway_deg: c, counterclockwise: self.orient_ccw },
                areas,
                plane_level_m: crate::check_inputs::number("Calculation plane level", &self.plane)?,
                vertical: if self.vertical_on { Some(VerticalSettings { height_m: crate::check_inputs::required_positive("Vertical illuminance height (m)", &self.vertical_height)?, facing: Facing::AlongAlignment }) } else { None },
                observers,
                eir_lane_width_m: if self.eir_on { Some(crate::check_inputs::required_positive("Edge strip width (m)", &self.eir_width)?) } else { None },
            },
            rtable,
        ))
    }

    /// Fills the form from settings stored on the element.
    pub fn from_settings(s: &CalcSettings) -> PhotoForm {
        let mut f = PhotoForm {
            orient_c: num(s.orientation.c_toward_carriageway_deg),
            orient_ccw: s.orientation.counterclockwise,
            maintenance: num(s.maintenance_factor),
            contribution: num(s.contribution_distance_m),
            plane: s.plane_level_m.map(num).unwrap_or_default(),
            areas: s.areas.iter().map(AreaForm::of).collect(),
            ..PhotoForm::default()
        };
        if let Some(v) = &s.vertical {
            f.vertical_on = true;
            f.vertical_height = num(v.height_m);
        }
        if let Some(w) = s.eir_lane_width_m {
            f.eir_on = true;
            f.eir_width = num(w);
        }
        f
    }
}

/// The checks of a loaded file as text lines: shown before any calculation.
pub fn file_check_lines(l: &LoadedPhotometry) -> Vec<String> {
    let p = &l.photometry;
    let c = p.flux_check(1.0);
    let peak = p.peak_intensity();
    let mut out = vec![
        format!("File {} (SHA-256 {}), format {}.", l.file_name, &l.sha256[..12.min(l.sha256.len())], p.format.label()),
        format!("Luminaire: {} {} {}. Symmetry in the file: {}.", p.manufacturer, p.luminaire, p.catalogue, if p.symmetry.is_empty() { "not stated" } else { &p.symmetry }),
        format!("Input power stated: {}. Lamp flux stated: {}.", p.input_power_w.map(|w| format!("{w} W")).unwrap_or_else(|| "not stated".into()), p.lamp_flux_lm.map(|f| format!("{f:.0} lm")).unwrap_or_else(|| "not stated".into())),
        format!("Flux by integration over the sphere (1 degree step): {:.0} lm.", c.integrated_flux_lm),
    ];
    match (c.computed_lor_percent, c.stated_lor_percent) {
        (Some(calc), Some(stated)) => out.push(format!("Light output ratio: {calc:.1} % by integration, {stated:.1} % stated in the file.")),
        (Some(calc), None) => out.push(format!("Light output ratio implied by the integration against the stated lamp flux: {calc:.1} % (the file states none).")),
        _ => out.push("Light output ratio: not compared (the file states no lamp flux).".into()),
    }
    if let (Some(d), Some(pc)) = (c.difference_lm, c.difference_percent) {
        out.push(format!("Integrated flux minus the flux the file's own figures imply: {d:+.0} lm ({pc:+.1} %). No tolerance is applied: whether this is acceptable is your decision."));
    } else {
        out.push("Integrated flux against a stated flux: no comparison is possible (the file states no flux).".into());
    }
    out.push(format!(
        "Downward flux fraction {:.1} %, upward {:.1} %{}.",
        c.downward_fraction * 100.0,
        c.upward_fraction * 100.0,
        c.stated_downward_fraction_percent.map(|d| format!(" (the file states a downward fraction of {d:.1} %)")).unwrap_or_default()
    ));
    out.push(format!("Peak intensity {:.0} cd at C {:.0} degrees, gamma {:.0} degrees.", peak.candela, peak.c_deg, peak.gamma_deg));
    let fractions: Vec<String> = [0.0, 90.0, 180.0, 270.0].iter().map(|c| format!("C{:.0}: {:.0} %", c, p.flux_fraction_towards_c(*c, 1.0) * 100.0)).collect();
    out.push(format!("Share of the flux on the half-space facing each C-plane: {}.", fractions.join(", ")));
    out.extend(p.warnings.iter().map(|w| format!("File warning: {w}")));
    out
}

fn paint_labelled(ui: &mut Ui, d: &LabelledDrawing, max_width: f32, max_height: f32) {
    let ext = d.drawing.extents;
    let (ew, eh) = (ext.width().max(1.0) as f32, ext.height().max(1.0) as f32);
    let scale = (max_width / ew).min(max_height / eh);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ew * scale + 24.0, eh * scale + 24.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(0xF6, 0xF5, 0xF0));
    let map = |x: f64, y: f64| pos2(rect.left() + 12.0 + (x - ext.min.x) as f32 * scale, rect.bottom() - 12.0 - (y - ext.min.y) as f32 * scale);
    let rgb = |c: Option<afrobim_draw::Colour>| {
        let [r, g, b] = c.map(|c| c.display_rgb()).unwrap_or([0x10, 0x10, 0x10]);
        Color32::from_rgb(r, g, b)
    };
    for prim in &d.drawing.primitives {
        match prim {
            Primitive::Fill { outline, colour } => {
                let pts: Vec<_> = outline.iter().map(|p| map(p.x, p.y)).collect();
                if pts.len() >= 3 {
                    painter.add(Shape::convex_polygon(pts, rgb(Some(*colour)), Stroke::NONE));
                }
            }
            Primitive::Line { from, to, width, colour } => {
                painter.line_segment([map(from.x, from.y), map(to.x, to.y)], Stroke::new((*width as f32 * scale).max(1.0), rgb(*colour)));
            }
            Primitive::Arc { centre, radius, start_deg, end_deg, colour } => {
                let pts: Vec<_> = (0..=48)
                    .map(|k| {
                        let a = (start_deg + (end_deg - start_deg) * k as f64 / 48.0).to_radians();
                        map(centre.x + radius * a.cos(), centre.y + radius * a.sin())
                    })
                    .collect();
                painter.add(Shape::line(pts, Stroke::new(1.0_f32, rgb(*colour))));
            }
            _ => {}
        }
    }
    // A label that would print over an earlier one is left out (a dense grid cannot show every value
    // at this size: the CSV has them all).
    let mut taken: Vec<egui::Rect> = Vec::new();
    let mut left_out = 0;
    for l in &d.labels {
        let size = (l.height as f32 * scale).clamp(9.0, 13.0);
        let at = map(l.x, l.y);
        let galley = painter.layout_no_wrap(l.text.clone(), FontId::proportional(size), Color32::from_rgb(0x22, 0x22, 0x22));
        let r = egui::Rect::from_min_size(pos2(at.x, at.y - galley.size().y / 2.0), galley.size()).expand(1.0);
        if taken.iter().any(|t| t.intersects(r)) {
            left_out += 1;
            continue;
        }
        taken.push(r);
        painter.galley(r.min + egui::vec2(1.0, 1.0), galley, Color32::from_rgb(0x22, 0x22, 0x22));
    }
    if left_out > 0 {
        painter.text(rect.right_top() + egui::vec2(-6.0, 4.0), Align2::RIGHT_TOP, format!("{left_out} labels left out (too close)"), FontId::proportional(10.0), Color32::from_rgb(0x80, 0x40, 0x20));
    }
}

/// Segments of a contour drawing in world millimetres at the plane level, for the 3D overlay.
pub fn overlay_segments(d: &LabelledDrawing, plane_m: f64) -> Vec<([f64; 3], [f64; 3], [u8; 3])> {
    let (ox, oy) = (d.origin_x_m * 1000.0, d.origin_y_m * 1000.0);
    d.drawing
        .primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Line { from, to, colour, width } if *width <= 0.0 || *width < 100.0 => Some(([ox + from.x, oy + from.y, plane_m * 1000.0], [ox + to.x, oy + to.y, plane_m * 1000.0], colour.map(|c| c.display_rgb()).unwrap_or([0x20, 0x20, 0x20]))),
            _ => None,
        })
        .collect()
}

/// The contour and grid-value drawings of a run for one area, with the levels used.
pub fn drawings_of(run: &PhotometryRun, data: &LightingData, area: &str, levels: &[f64]) -> (Option<LabelledDrawing>, Option<LabelledDrawing>) {
    let lums = luminaire_instances(data);
    (contour_drawing(&run.result, area, levels, &lums), grid_values_drawing(&run.result, area, &lums))
}

/// Levels for the contours of an area: the entered ones, else the engine's round numbers between
/// the area's minimum and maximum (a presentation aid).
pub fn levels_for(run: &PhotometryRun, area: &str, typed: &str) -> Vec<f64> {
    let entered: Vec<f64> = typed.split(|c: char| c == ',' || c.is_whitespace() || c == ';').filter(|t| !t.is_empty()).filter_map(|t| t.parse::<f64>().ok()).filter(|v| *v > 0.0).collect();
    if !entered.is_empty() {
        return entered;
    }
    run.result.area(area).map(|a| nice_levels(a.e_h.minimum, a.e_h.maximum, 6)).unwrap_or_default()
}

/// Puts the computed results into the lighting check's entries: the photometric results and the
/// label that says they were computed by AfroBIM. Fields the run does not compute keep what was
/// entered. Returns the names of the fields filled.
pub fn use_in_class_check(doc: &mut Document, id: ElementId, run: &PhotometryRun, area: &str) -> Result<Vec<&'static str>, String> {
    use crate::check_inputs::{load, save, KEY_LIGHTING};
    let mut form: crate::lighting_inputs::Form = load(doc, id, KEY_LIGHTING).unwrap_or_default();
    let mut ph: PhotometricResults = form.inputs()?.photometry;
    let filled = run.apply_to(&mut ph, area).map_err(|e| e.to_string())?;
    form.set_photometry(&ph);
    if !save(doc, id, KEY_LIGHTING, &form) {
        return Err("The lighting element is gone.".into());
    }
    Ok(filled)
}

// ─── the app side ───────────────────────────────────────────────────────────────────────

/// What the photometry windows keep.
#[derive(Default)]
pub struct PhotoUi {
    pub open: bool,
    pub results_open: bool,
    pub form: PhotoForm,
    pub path: Option<PathBuf>,
    pub loaded: Option<LoadedPhotometry>,
    pub file_error: String,
    pub run: Option<PhotometryRun>,
    pub run_element: Option<ElementId>,
    pub area: String,
    pub levels: String,
    pub message: String,
    /// Contour lines drawn in the 3D site view, from the last run, when switched on.
    pub overlay_on: bool,
    pub overlay: Vec<([f64; 3], [f64; 3], [u8; 3])>,
    pub polar_planes: String,
}

impl PhotoUi {
    pub fn paint_overlay(&self, painter: &egui::Painter, cam: &crate::view3d::Camera, rect: egui::Rect) {
        if !self.overlay_on {
            return;
        }
        for (a, b, [r, g, bl]) in &self.overlay {
            if let (Some((p, _)), Some((q, _))) = (cam.project(*a, rect), cam.project(*b, rect)) {
                painter.line_segment([p, q], Stroke::new(1.5_f32, Color32::from_rgb(*r, *g, *bl)));
            }
        }
    }
}

/// The lighting elements of the document: id, mark, class.
pub fn lighting_elements(doc: &Document) -> Vec<(ElementId, String)> {
    doc.elements().iter().filter(|e| e.category == afrobim_lighting::CATEGORY).map(|e| (e.id, e.name.clone())).collect()
}

impl AfroBimApp {
    fn photo_alignment(&self, data: &LightingData) -> Option<(ElementId, afrobim_alignment::Alignment)> {
        let name = match &data.definition {
            LightingDefinition::Installation(i) => i.alignment.clone(),
            LightingDefinition::Column(c) => c.alignment.clone(),
        };
        let (id, _) = crate::alignment_glue::alignments(&self.doc).into_iter().find(|a| a.1 == name)?;
        Some((id, crate::alignment_glue::alignment_of(&self.doc, id)?))
    }

    /// Opens the photometry window for the selected lighting element (or the only one).
    pub(crate) fn photo_open(&mut self) {
        let all = lighting_elements(&self.doc);
        let selected = self.doc.primary().filter(|id| all.iter().any(|(i, _)| i == id)).or_else(|| (all.len() == 1).then(|| all[0].0));
        self.photo.open = true;
        if let Some(id) = selected {
            self.photo_choose_element(id);
        } else if self.photo.form.element.is_none() {
            self.photo.form.element = all.first().map(|a| a.0);
            if let Some(id) = self.photo.form.element {
                self.photo_choose_element(id);
            }
        }
    }

    /// Selects the installation; loads the stored photometry settings and file when it has some.
    pub(crate) fn photo_choose_element(&mut self, id: ElementId) {
        if self.photo.form.element == Some(id) && (self.photo.loaded.is_some() || self.photo.run_element.is_some()) {
            return;
        }
        self.photo.form = PhotoForm { element: Some(id), ..PhotoForm::default() };
        self.photo.loaded = None;
        self.photo.path = None;
        self.photo.run = None;
        self.photo.file_error.clear();
        if let Ok(Some(stored)) = photometry_settings_of(&self.doc, id) {
            self.photo.form = PhotoForm::from_settings(&stored.settings);
            self.photo.form.element = Some(id);
            match resolve_photometry(&stored, None) {
                Ok((phot, sha, warnings)) => {
                    let name = std::path::Path::new(&stored.file_path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    self.photo.path = Some(PathBuf::from(&stored.file_path));
                    self.photo.loaded = Some(LoadedPhotometry { photometry: phot, sha256: sha, file_name: name });
                    self.photo.message = if warnings.is_empty() { "Stored photometry settings loaded. Confirm the orientation again before running.".into() } else { warnings.join(" ") };
                }
                Err(e) => self.photo.file_error = e.to_string(),
            }
            if let Some(p) = &stored.rtable_path {
                self.photo.form.rtable = Some((PathBuf::from(p), stored.rtable_sha256.clone().unwrap_or_default()));
            }
        }
        self.photo_propose_orientation();
    }

    fn photo_propose_orientation(&mut self) {
        if let Some(l) = &self.photo.loaded {
            let label = l.photometry.format.label();
            if self.photo.form.orient_for.as_deref() != Some(label.as_str()) {
                if self.photo.form.orient_c.is_empty() {
                    let o = Orientation::for_format(&l.photometry.format);
                    self.photo.form.orient_c = num(o.c_toward_carriageway_deg);
                    self.photo.form.orient_ccw = o.counterclockwise;
                }
                self.photo.form.orient_for = Some(label);
                self.photo.form.orient_confirmed = false;
            }
        }
    }

    /// Loads a luminaire file into the window (no dialog: for the dialog's button and `--photometry`).
    pub(crate) fn photo_load_file(&mut self, path: &std::path::Path) {
        match load_photometry_file(path) {
            Ok(l) => {
                self.photo.path = Some(path.to_path_buf());
                self.photo.file_error.clear();
                self.photo.loaded = Some(l);
                self.photo.run = None;
                self.photo.form.orient_for = None;
                self.photo.form.orient_c.clear();
                self.photo_propose_orientation();
            }
            Err(e) => {
                self.photo.file_error = e.to_string();
                self.photo.loaded = None;
            }
        }
    }

    /// Reads the form and the element into what a calculation needs, or says what is missing.
    fn photo_input(&self) -> Result<PhotoInput, String> {
        let id = self.photo.form.element.ok_or("Choose a lighting installation.")?;
        let loaded = self.photo.loaded.clone().ok_or("Choose a luminaire file first.")?;
        let (settings, rtable) = self.photo.form.settings()?;
        let data = lighting_of(&self.doc, id).map_err(|e| e.to_string())?;
        let alignment = self.photo_alignment(&data).map(|a| a.1);
        let path = self.photo.path.clone().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| loaded.file_name.clone());
        let rt_sha = rtable.as_ref().and_then(|_| self.photo.form.rtable.as_ref().and_then(|(p, _)| std::fs::read(p).ok().map(|b| sha256_hex(&b))));
        let rt_info = self.photo.form.rtable.as_ref().zip(rt_sha.as_ref()).map(|((p, _), h)| (p.to_string_lossy().to_string(), h.clone()));
        Ok(PhotoInput { id, loaded, settings, rtable, data, alignment, path, rt_info })
    }

    /// Runs the calculation from the form on the UI thread and stores the settings on the element:
    /// for the demo run and tests.
    pub(crate) fn photo_run(&mut self) -> Result<(), String> {
        let out = self.photo_input()?.compute();
        self.apply_photo_out(out)
    }

    /// As [`AfroBimApp::photo_run`] on a worker thread, with a progress window and Cancel. The
    /// result is applied (and the settings stored) only if the document has not changed since.
    pub(crate) fn photo_run_bg(&mut self) -> Result<(), String> {
        if self.bg.photo.is_some() {
            return Err("A photometry calculation is already in progress.".into());
        }
        let input = self.photo_input()?;
        self.bg.photo = Some(spawn_photometry(&self.doc, input));
        self.status = "Photometry started on a worker thread; the model stays editable, but an edit discards the result.".into();
        Ok(())
    }

    /// Takes a finished calculation in: stores its settings on the installation and shows it.
    pub(crate) fn apply_photo_out(&mut self, out: Result<PhotoOut, String>) -> Result<(), String> {
        let PhotoOut { id, run, settings, loaded, path, rt_info, data } = out?;
        set_photometry(&mut self.doc, id, &path, &loaded, &settings, rt_info.as_ref().map(|(p, h)| (p.as_str(), h.as_str()))).map_err(|e| e.to_string())?;
        let class = data.definition.lighting_class().to_string();
        self.photo.area = run.primary_area_for_class(&class).map(|s| s.to_string()).or_else(|| run.result.areas.first().map(|a| a.name.clone())).unwrap_or_default();
        self.photo.run = Some(run);
        self.photo.run_element = Some(id);
        self.photo_refresh_overlay(&data);
        self.photo.results_open = true;
        self.sync_generated();
        self.schedule_commit_now();
        self.status = "Photometry calculated and its settings stored on the installation. Not an EN 13201-3 calculation; not validated against DIALux, Relux or AGi32.".into();
        Ok(())
    }

    fn photo_refresh_overlay(&mut self, data: &LightingData) {
        self.photo.overlay.clear();
        let Some(run) = &self.photo.run else { return };
        let levels = levels_for(run, &self.photo.area, &self.photo.levels);
        if let (Some(c), _) = drawings_of(run, data, &self.photo.area, &levels) {
            self.photo.overlay = overlay_segments(&c, run.result.plane_level_m);
        }
    }

    /// For `--photometry-run`: arbitrary demo settings so the window can be looked at without a
    /// pointer. They are not suggestions: 80 percent maintenance factor, 60 m contribution distance,
    /// cells of 3 m by 1 m over the carriageway, the orientation accepted.
    pub(crate) fn photo_demo_run(&mut self) {
        let Some(id) = self.photo.form.element else { return };
        let Ok(data) = lighting_of(&self.doc, id) else { return };
        if let Ok(a) = default_carriageway_area(&data, Division::Spacing(3.0), Division::Spacing(1.0)) {
            self.photo.form.areas = vec![AreaForm::of(&a)];
        }
        let f = &mut self.photo.form;
        f.maintenance = "0.8".into();
        f.contribution = "60".into();
        f.orient_confirmed = true;
        self.photo.message = "DEMO settings (arbitrary, for the screenshot): maintenance factor 0.8, contribution distance 60 m, 3 m by 1 m cells, orientation accepted.".into();
        if let Err(e) = self.photo_run() {
            self.photo.form.error = e;
        }
    }

    pub(crate) fn photo_use_in_class_check(&mut self) -> Result<(), String> {
        let (Some(run), Some(id)) = (self.photo.run.clone(), self.photo.run_element) else { return Err("Run the calculation first.".into()) };
        let filled = use_in_class_check(&mut self.doc, id, &run, &self.photo.area)?;
        self.schedule_commit_now();
        self.status = format!("The class check inputs of the installation now hold the computed results ({}), labelled as computed by AfroBIM. Run CHECK or CHECKALL to compare them with the class.", filled.join(", "));
        Ok(())
    }

    pub(crate) fn photometry_windows(&mut self, ctx: &egui::Context) {
        if self.photo.open {
            self.photo_window(ctx);
        }
        if self.photo.results_open {
            self.photo_results_window(ctx);
        }
    }

    fn photo_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let (mut choose_file, mut choose_rtable, mut run) = (false, false, false);
        let mut pick: Option<ElementId> = None;
        let mut add_blank = false;
        let mut add_carriageway = false;
        let elements = lighting_elements(&self.doc);
        let data = self.photo.form.element.and_then(|id| lighting_of(&self.doc, id).ok());
        let alignment_id = data.as_ref().and_then(|d| self.photo_alignment(d)).map(|a| a.0);
        let class = data.as_ref().map(|d| d.definition.lighting_class().to_string()).unwrap_or_default();
        let polar: Option<LabelledDrawing> = self.photo.loaded.as_ref().map(|l| {
            let planes: Vec<(f64, f64)> = if self.photo.polar_planes.trim().is_empty() {
                vec![(0.0, 180.0), (90.0, 270.0)]
            } else {
                self.photo.polar_planes.split(|c: char| c == ';').filter_map(|p| p.split_once(',').and_then(|(a, b)| Some((a.trim().parse().ok()?, b.trim().parse().ok()?)))).collect()
            };
            polar_diagram(&l.photometry, &planes)
        });
        let check_lines = self.photo.loaded.as_ref().map(file_check_lines);
        let can_calc = self.photo.form.element.is_some() && self.photo.loaded.is_some();
        let dlg = crate::ui_kit::Dialog::titled("Photometry").subtitle("Luminaire data files and the lighting calculation").width(900.0).show(ctx, |ui| {
            crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, LIMITS);
            crate::ui_kit::heading(ui, "Installation and luminaire file");
            ui.horizontal(|ui| {
                ui.label("Installation");
                let shown = self.photo.form.element.and_then(|id| elements.iter().find(|e| e.0 == id)).map(|e| e.1.clone()).unwrap_or_else(|| "none: add one with LIGHTING".into());
                egui::ComboBox::from_id_salt("photo-element").selected_text(shown).show_ui(ui, |ui| {
                    for (id, name) in &elements {
                        if ui.selectable_label(self.photo.form.element == Some(*id), name).clicked() {
                            pick = Some(*id);
                        }
                    }
                });
                if !class.is_empty() {
                    crate::ui_kit::note(ui, format!("lighting class entered: {class}"));
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Choose an IES or LDT file...").clicked() {
                    choose_file = true;
                }
                if let Some(p) = &self.photo.path {
                    ui.label(RichText::new(p.display().to_string()).small().weak());
                }
            });
            if !self.photo.file_error.is_empty() {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, &self.photo.file_error);
            }
            if !self.photo.message.is_empty() {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, &self.photo.message);
            }
            if self.photo.loaded.is_none() {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, NO_FILE);
                if self.options.demo {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, "Demo: the demo model has a lighting installation but no luminaire file, because none ships. Nothing here is calculated until you supply one.");
                }
                return;
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                crate::ui_kit::heading(ui, "1. The file's own checks (before any calculation)");
                if let Some(lines) = &check_lines {
                    for l in lines {
                        ui.label(RichText::new(l).small());
                    }
                }
                ui.add_space(4.0);
                crate::ui_kit::heading(ui, "2. Polar diagram: right half plane C, left half the plane 180 degrees on, nadir at the bottom");
                ui.horizontal(|ui| {
                    ui.label("Plane pairs");
                    ui.add(egui::TextEdit::singleline(&mut self.photo.polar_planes).desired_width(180.0).hint_text("0,180; 90,270"));
                });
                if let Some(p) = &polar {
                    paint_labelled(ui, p, 420.0, 320.0);
                }
                ui.add_space(4.0);
                crate::ui_kit::heading(ui, "3. Orientation: an assumption you confirm");
                let f = &mut self.photo.form;
                ui.horizontal(|ui| {
                    ui.label("C-plane facing the carriageway (degrees)");
                    ui.add(egui::TextEdit::singleline(&mut f.orient_c).desired_width(60.0));
                    ui.checkbox(&mut f.orient_ccw, "C increases counter-clockwise seen from above");
                });
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, "to confirm");
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "The file does not say which C-plane faces the road. The value above is the crate's assumption for this format (IES: C0; EULUMDAT: C270; counter-clockwise). Judge it from the polar diagram and the shares of flux in the checks above: a road luminaire throws most of its light to the street side.");
                ui.checkbox(&mut f.orient_confirmed, "I have checked this orientation against the polar diagram");
                ui.add_space(4.0);
                crate::ui_kit::heading(ui, "4. Calculation settings (none is prefilled)");
                crate::ui_kit::grid(ui, "photo-settings", |ui| {
                    for (label, field, hint) in [
                        ("Maintenance factor (0 to 1)", &mut f.maintenance, "yours, from BS 5489-1 or the supplier"),
                        ("Contribution distance (m)", &mut f.contribution, "luminaires further away add nothing"),
                        ("Calculation plane level (m)", &mut f.plane, "blank: the mean column level"),
                    ] {
                        ui.label(label);
                        ui.add(egui::TextEdit::singleline(field).desired_width(220.0).hint_text(hint));
                        ui.end_row();
                    }
                    ui.label("Vertical illuminance");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut f.vertical_on, "also vertical and semi-cylindrical, facing along the alignment, at height (m)");
                        ui.add_enabled(f.vertical_on, egui::TextEdit::singleline(&mut f.vertical_height).desired_width(60.0));
                    });
                    ui.end_row();
                    ui.label("Edge illuminance ratio");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut f.eir_on, "calculate it, with edge strips of width (m)");
                        ui.add_enabled(f.eir_on, egui::TextEdit::singleline(&mut f.eir_width).desired_width(60.0));
                    });
                    ui.end_row();
                    ui.label("r-table file (for luminance)");
                    ui.horizontal(|ui| {
                        if ui.button("Choose...").clicked() {
                            choose_rtable = true;
                        }
                        match &f.rtable {
                            Some((p, _)) => {
                                ui.label(RichText::new(p.display().to_string()).small().weak());
                                if ui.small_button("Clear").clicked() {
                                    f.rtable = None;
                                    f.obs_on = false;
                                }
                            }
                            None => {
                                crate::ui_kit::note(ui, "none: luminance is not calculated (no r-table values ship)");
                            }
                        }
                    });
                    ui.end_row();
                    if f.rtable.is_some() {
                        ui.label("Observer");
                        ui.vertical(|ui| {
                            ui.checkbox(&mut f.obs_on, "one observer, entered by you (the standard's positions are not held)");
                            if f.obs_on {
                                crate::station_input::station_field(ui, &self.doc, alignment_id, "photo-observer", &mut f.obs_station);
                                ui.horizontal(|ui| {
                                    ui.add(egui::TextEdit::singleline(&mut f.obs_offset).desired_width(70.0).hint_text("offset m"));
                                    ui.add(egui::TextEdit::singleline(&mut f.obs_height).desired_width(70.0).hint_text("eye height m"));
                                });
                            }
                        });
                        ui.end_row();
                    }
                });
                ui.add_space(4.0);
                crate::ui_kit::heading(ui, "Calculation areas: rectangles in stations and offsets (right of the alignment positive); points are cell centres");
                let mut remove = None;
                for (k, a) in f.areas.iter_mut().enumerate() {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(120.0).hint_text("area name"));
                            egui::ComboBox::from_id_salt(("photo-kind", k)).selected_text(a.kind.label()).show_ui(ui, |ui| {
                                for kind in [AreaKind::Carriageway, AreaKind::Footway, AreaKind::Conflict, AreaKind::Other] {
                                    ui.selectable_value(&mut a.kind, kind, kind.label());
                                }
                            });
                            if ui.small_button("Remove").clicked() {
                                remove = Some(k);
                            }
                        });
                        egui::Grid::new(("photo-area", k)).num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                            crate::station_input::station_row(ui, &self.doc, alignment_id, &format!("photo-from-{k}"), "From station", &mut a.from_m);
                            crate::station_input::station_row(ui, &self.doc, alignment_id, &format!("photo-to-{k}"), "To station", &mut a.to_m);
                            ui.label("Offset from, to (m)");
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut a.off_from).desired_width(70.0));
                                ui.add(egui::TextEdit::singleline(&mut a.off_to).desired_width(70.0));
                            });
                            ui.end_row();
                            ui.label("Points along");
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut a.along).desired_width(70.0));
                                ui.selectable_value(&mut a.along_spacing, false, "points");
                                ui.selectable_value(&mut a.along_spacing, true, "m spacing");
                            });
                            ui.end_row();
                            ui.label("Points across");
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut a.across).desired_width(70.0));
                                ui.selectable_value(&mut a.across_spacing, false, "points");
                                ui.selectable_value(&mut a.across_spacing, true, "m spacing");
                            });
                            ui.end_row();
                        });
                    });
                }
                if let Some(k) = remove {
                    f.areas.remove(k);
                }
                ui.horizontal(|ui| {
                    if ui.button("Add an area").clicked() {
                        add_blank = true;
                    }
                    if ui.button("Add the carriageway between the first and last column").on_hover_text("Fills the stations and offsets; the number of points stays yours to enter").clicked() {
                        add_carriageway = true;
                    }
                });
                if !f.error.is_empty() {
                    crate::ui_kit::callout(ui, crate::ui_kit::Level::Error, &f.error);
                }
            });
        }, |ui| crate::ui_kit::Footer::new("Calculate").enabled(can_calc, "Choose an installation and a luminaire file").cancel(Some("Close")).show(ui));
        if dlg.closed {
            open = false;
        }
        if let Some((_, out)) = &dlg.inner {
            if out.cancel {
                open = false;
            }
            if out.primary {
                run = true;
            }
        }
        self.photo.open = open;
        if let Some(id) = pick {
            self.photo.form.element = None;
            self.photo_choose_element(id);
        }
        if choose_file {
            if let Some(p) = rfd::FileDialog::new().add_filter("Luminaire files", &["ies", "ldt", "IES", "LDT"]).pick_file() {
                self.photo_load_file(&p);
            }
        }
        if choose_rtable {
            if let Some(p) = rfd::FileDialog::new().add_filter("r-table", &["txt", "rtable", "r"]).pick_file() {
                let sha = std::fs::read(&p).map(|b| sha256_hex(&b)).unwrap_or_default();
                self.photo.form.rtable = Some((p, sha));
            }
        }
        if add_blank {
            self.photo.form.areas.push(AreaForm::blank());
        }
        if add_carriageway {
            match data.as_ref().map(|d| default_carriageway_area(d, Division::Count(1), Division::Count(1))) {
                Some(Ok(a)) => {
                    let mut f = AreaForm::of(&a);
                    f.along.clear();
                    f.across.clear();
                    self.photo.form.areas.push(f);
                }
                Some(Err(e)) => self.photo.form.error = e.to_string(),
                None => self.photo.form.error = "Choose an installation first.".into(),
            }
        }
        if run {
            match self.photo_run_bg() {
                Ok(()) => self.photo.form.error.clear(),
                Err(e) => self.photo.form.error = e,
            }
        }
    }

    fn photo_results_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let (mut csv, mut use_it) = (false, false);
        let mut area_changed = false;
        let data = self.photo.run_element.and_then(|id| lighting_of(&self.doc, id).ok());
        let mark = self.photo.run_element.and_then(|id| self.doc.find(id)).map(|e| e.name.clone()).unwrap_or_default();
        let class = data.as_ref().map(|d| d.definition.lighting_class().to_string()).unwrap_or_default();
        let mut message = None;
        let drawings = match (&self.photo.run, &data) {
            (Some(run), Some(d)) => {
                let levels = levels_for(run, &self.photo.area, &self.photo.levels);
                Some((drawings_of(run, d, &self.photo.area, &levels), levels))
            }
            _ => None,
        };
        let dlg = crate::ui_kit::Dialog::titled(format!("Photometry results: {mark}")).width(920.0).show_body(ctx, |ui| {
            crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, LIMITS);
            let Some(run) = &self.photo.run else {
                ui.label(RichText::new("No run yet.").weak());
                return;
            };
            ui.label(RichText::new(format!("{}; {} luminaire(s); maintenance factor {}; contribution distance {} m; plane level {:.2} m. Values are maintained (initial times the maintenance factor).", run.file_name, run.result.luminaire_count, run.result.maintenance_factor, run.result.contribution_distance_m, run.result.plane_level_m)).small());
            for w in &run.warnings {
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Warning, format!("Warning: {w}"));
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("photo-areas").num_columns(8).striped(true).spacing([12.0, 3.0]).show(ui, |ui| {
                    for h in ["Area", "Points", "Average (lx)", "Minimum (lx)", "Maximum (lx)", "U_o (min/avg)", "min/max", "Class use"] {
                        crate::ui_kit::th(ui, &h);
                    }
                    ui.end_row();
                    for a in &run.result.areas {
                        ui.label(&a.name);
                        ui.label(format!("{} x {}", a.n_along, a.n_across));
                        ui.label(format!("{:.2}", a.e_h.average));
                        ui.label(format!("{:.2}", a.e_h.minimum));
                        ui.label(format!("{:.2}", a.e_h.maximum));
                        ui.label(format!("{:.3}", a.e_h.uniformity_min_avg));
                        ui.label(format!("{:.3}", a.e_h.uniformity_min_max));
                        ui.label(if a.name == self.photo.area { "primary for the class" } else { "" });
                        ui.end_row();
                    }
                });
                for a in &run.result.areas {
                    for d in &a.vertical {
                        ui.label(RichText::new(format!("{}: {} E_v min {:.2} lx, avg {:.2} lx; E_sc min {:.2} lx, avg {:.2} lx", a.name, d.label, d.ev.minimum, d.ev.average, d.esc.minimum, d.esc.average)).small());
                    }
                    for l in &a.luminance {
                        ui.label(RichText::new(format!("{}: luminance for {}: L_av {:.3} cd/m2, U_o {:.3}, L_min {:.3}", a.name, l.observer, l.stats.average, l.stats.uniformity_min_avg, l.stats.minimum)).small());
                    }
                }
                if let Some(e) = &run.result.eir {
                    ui.label(RichText::new(format!("Edge illuminance ratio (strip width {} m): left {:.3}, right {:.3}, value {:.3}", e.lane_width_m, e.left, e.right, e.value)).small());
                }
                crate::ui_kit::callout(ui, crate::ui_kit::Level::Info, "Not computed: threshold increment TI, longitudinal uniformity U_l, wet overall uniformity. Luminance appears only with your r-table and observer.");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Area for the drawings and the class");
                    egui::ComboBox::from_id_salt("photo-area-pick").selected_text(self.photo.area.clone()).show_ui(ui, |ui| {
                        for a in &run.result.areas {
                            if ui.selectable_value(&mut self.photo.area, a.name.clone(), &a.name).clicked() {
                                area_changed = true;
                            }
                        }
                    });
                    ui.label("Contour levels (lx)");
                    if ui.add(egui::TextEdit::singleline(&mut self.photo.levels).desired_width(160.0).hint_text("blank: round numbers")).changed() {
                        area_changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Export grid CSV").clicked() {
                        csv = true;
                    }
                    if ui.button(format!("Use these results in the class check ({})", if class.is_empty() { "no class entered" } else { &class })).clicked() {
                        use_it = true;
                    }
                    ui.checkbox(&mut self.photo.overlay_on, "Contours in the site view (plan overlay)");
                });
                if let Some(((contours, values), levels)) = &drawings {
                    crate::ui_kit::heading(ui, &format!("Contours of horizontal illuminance, levels {} lx (a sheet detail feed: add a Detail sheet 'Lighting, iso-illuminance contours' in SHEETSET)", levels.iter().map(|l| format!("{l}")).collect::<Vec<_>>().join(", ")));
                    match contours {
                        Some(c) => paint_labelled(ui, c, 820.0, 300.0),
                        None => {
                            ui.label("No contours for this area.");
                        }
                    }
                    crate::ui_kit::heading(ui, "Grid values");
                    if let Some(v) = values {
                        paint_labelled(ui, v, 820.0, 300.0);
                    }
                }
            });
        });
        if dlg.closed {
            open = false;
        }
        self.photo.results_open = open;
        if csv {
            if let Some(run) = &self.photo.run {
                let text = grid_to_csv(&run.result);
                if let Some(mut path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("photometry-grid.csv").save_file() {
                    if path.extension().is_none() {
                        path.set_extension("csv");
                    }
                    self.status = match std::fs::write(&path, text) {
                        Ok(()) => format!("Wrote the grid values to {}.", path.display()),
                        Err(e) => format!("Could not write {}: {e}", path.display()),
                    };
                }
            }
        }
        if use_it {
            if let Err(e) = self.photo_use_in_class_check() {
                message = Some(e);
            }
        }
        if area_changed {
            if let Some(d) = &data {
                self.photo_refresh_overlay(d);
            }
        }
        if let Some(m) = message {
            self.status = format!("Not used: {m}");
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::sheets_glue::test_support::{demo_doc, scratch};

    /// A small synthetic type C IES file written for the tests. Its numbers are made up.
    pub fn synthetic_ies() -> String {
        "IESNA:LM-63-2002\n[TEST] synthetic file written by the AfroBIM tests\n[LUMINAIRE] Synthetic roadway luminaire\nTILT=NONE\n\
         1 8000 1 7 3 1 2 0.3 0.5 0.1\n1 1 60\n\
         0 15 30 45 60 75 90\n0 90 180\n\
         2000 2400 3000 2600 1500 600 100\n\
         2000 2200 2400 2000 1000 300 50\n\
         2000 1800 1500 1000 400 100 10\n"
            .to_string()
    }

    #[test]
    fn a_run_on_a_synthetic_ies_file_fills_the_class_check_with_the_computed_source_label() {
        let mut doc = demo_doc();
        let id = lighting_elements(&doc).first().expect("the demo has street lighting").0;
        let path = scratch("synthetic.ies");
        std::fs::write(&path, synthetic_ies()).unwrap();
        let loaded = load_photometry_file(&path).unwrap();
        // The file's own checks come first and mention the integration against the stated flux.
        let lines = file_check_lines(&loaded);
        assert!(lines.iter().any(|l| l.contains("Flux by integration")) && lines.iter().any(|l| l.contains("Peak intensity")));
        // The form refuses to run until the orientation is confirmed, and has no maintenance factor.
        let data = lighting_of(&doc, id).unwrap();
        let mut f = PhotoForm::default();
        f.orient_c = num(Orientation::for_format(&loaded.photometry.format).c_toward_carriageway_deg);
        assert!(f.settings().unwrap_err().contains("orientation"));
        f.orient_confirmed = true;
        assert!(f.settings().unwrap_err().contains("Maintenance"));
        f.maintenance = "0.8".into();
        f.contribution = "60".into();
        let mut area = AreaForm::of(&default_carriageway_area(&data, Division::Count(1), Division::Count(1)).unwrap());
        area.along = "3".into();
        area.along_spacing = true;
        area.across = "1".into();
        area.across_spacing = true;
        f.areas.push(area);
        let (settings, rtable) = f.settings().unwrap();
        assert!(rtable.is_none());
        let alignment = crate::alignment_glue::alignments(&doc).into_iter().next().and_then(|a| crate::alignment_glue::alignment_of(&doc, a.0));
        let run = run_photometry(&data, &loaded.photometry, &loaded.file_name, &loaded.sha256, &settings, None, alignment.as_ref()).unwrap();
        set_photometry(&mut doc, id, &path.to_string_lossy(), &loaded, &settings, None).unwrap();
        let class = data.definition.lighting_class().to_string();
        let area_name = run.primary_area_for_class(&class).unwrap().to_string();
        let filled = use_in_class_check(&mut doc, id, &run, &area_name).unwrap();
        assert!(filled.contains(&"e_av_lx"));
        // The class check inputs hold the numbers and the computed-source label, and the check quotes it.
        let form: crate::lighting_inputs::Form = crate::check_inputs::load(&doc, id, crate::check_inputs::KEY_LIGHTING).unwrap();
        let inputs = form.inputs().unwrap();
        assert!(inputs.photometry.source.starts_with(afrobim_lighting::calc::COMPUTED_SOURCE_PREFIX), "{}", inputs.photometry.source);
        assert!((inputs.photometry.e_av_lx.unwrap() - run.result.area(&area_name).unwrap().e_h.average).abs() < 1e-3);
        // The same record is what the model-wide check reads.
        let _ = afrobim_lighting::design::check_lighting(&lighting_of(&doc, id).unwrap(), &inputs);
        // The drawings the sheets and the window use.
        let levels = levels_for(&run, &area_name, "");
        let (contours, values) = drawings_of(&run, &data, &area_name, &levels);
        assert!(contours.is_some() && values.is_some());
        assert!(grid_to_csv(&run.result).starts_with("area,kind,"));
    }
}
