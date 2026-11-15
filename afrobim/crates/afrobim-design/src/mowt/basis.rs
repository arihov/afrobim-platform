//! The built-in `mowt_2010` design basis for `run_checks`, the list of built-in bases for the app, the checks
//! that stay "not checked" under it, and the slips found in the manual.

use serde::Serialize;

use super::classes::{self, Parameter, RoadClass, Terrain};
use super::{horizontal, refuse, sight, vertical, Source, DOCUMENT_FULL, MOWT_2010_ID, SPEEDS_KMH};
use crate::checks::{DesignBasis, StandardRow};
use crate::error::DesignError;

/// Which maximum gradient `max_grade` carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GradeLimit {
    /// The desirable maximum (Tables 4-3 to 4-9, "Max. Gradient (desirable)"). Exceeding it needs a Departure
    /// from Standard (section 6.5.1, p. 123). The default.
    Desirable,
    /// The absolute maximum ("Max. Gradient (absolute)").
    Absolute,
}

/// Options of [`DesignBasis::mowt_2010_with`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MowtOptions {
    /// Which maximum gradient to check against.
    pub grade: GradeLimit,
    /// Set `min_grade` to the class table's "Minimum Gradient in cut" (0.5 %). The manual states it for cuttings
    /// only (section 6.5.2, p. 123), and the framework applies a minimum grade to every grade line, so it is off by
    /// default: switch it on only for an alignment that is in cut throughout.
    pub minimum_grade_in_cut: bool,
}

impl Default for MowtOptions {
    fn default() -> Self {
        MowtOptions { grade: GradeLimit::Desirable, minimum_grade_in_cut: false }
    }
}

impl DesignBasis {
    /// The Uganda MoWT Road Design Manual Volume I (January 2010) as a one-row basis for `run_checks`, for a
    /// design class, terrain, design speed (km/h, a multiple of 10 from 30, not above the maximum the manual gives
    /// for the class and terrain) and maximum superelevation (percent: 4, 6, 7 or 8). The row's label is
    /// `Class II Paved, Rolling, 70 km/h, emax 7 %` (see [`mowt_2010_row_label`]); the source of every limit, with
    /// table or figure, section and page, is in the row's `limit_sources` and is printed in each check item.
    ///
    /// Refused with a plain message for escarpment terrain, for Class Ib in flat terrain, for a speed above the
    /// class maximum and for values the manual does not tabulate. See [`super`] for what is withheld and why.
    pub fn mowt_2010(class: RoadClass, terrain: Terrain, design_speed_kmh: f64, emax_percent: f64) -> Result<DesignBasis, DesignError> {
        DesignBasis::mowt_2010_with(class, terrain, design_speed_kmh, emax_percent, &MowtOptions::default())
    }

    /// As [`DesignBasis::mowt_2010`], with options.
    pub fn mowt_2010_with(class: RoadClass, terrain: Terrain, design_speed_kmh: f64, emax_percent: f64, options: &MowtOptions) -> Result<DesignBasis, DesignError> {
        let row = mowt_2010_row(class, terrain, design_speed_kmh, emax_percent, options)?;
        Ok(DesignBasis {
            name: format!("Uganda MoWT Road Design Manual Vol. I Geometric Design, January 2010 ({MOWT_2010_ID})"),
            source: format!("{DOCUMENT_FULL}. Tables and passages used are named in the source of each limit of the row."),
            notes: notes_for(class, terrain, design_speed_kmh, options),
            rows: vec![row],
        })
    }

    /// Builds the built-in basis `id` (see [`builtin_bases`]) for a selection made in the app.
    pub fn from_builtin(id: &str, selection: &BuiltinSelection) -> Result<DesignBasis, DesignError> {
        if id != MOWT_2010_ID {
            return refuse(format!("There is no built-in basis '{id}'. Available: {MOWT_2010_ID}."));
        }
        DesignBasis::mowt_2010_with(selection.class, selection.terrain, selection.design_speed_kmh, selection.emax_percent, &selection.options)
    }
}

/// A selection of class, terrain, speed and emax for [`DesignBasis::from_builtin`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BuiltinSelection {
    pub class: RoadClass,
    pub terrain: Terrain,
    pub design_speed_kmh: f64,
    pub emax_percent: f64,
    pub options: MowtOptions,
}

/// The label of the row [`DesignBasis::mowt_2010`] makes, for `basis.row(label)`.
pub fn mowt_2010_row_label(class: RoadClass, terrain: Terrain, design_speed_kmh: f64, emax_percent: f64) -> String {
    format!("{}, {}, {} km/h, emax {} %", class.manual_name(), terrain.manual_name(), design_speed_kmh, emax_percent)
}

fn put(row: &mut StandardRow, field: &str, text: String) {
    row.limit_sources.insert(field.to_string(), text);
}

fn cite(s: Source) -> String {
    s.citation()
}

fn mowt_2010_row(class: RoadClass, terrain: Terrain, speed: f64, emax: f64, options: &MowtOptions) -> Result<StandardRow, DesignError> {
    // Speed: a tabulated speed, not above the class maximum.
    super::speed_index(speed)?;
    let max = classes::max_design_speed(class, terrain)?;
    if speed > max.value + 1e-9 {
        return refuse(format!(
            "A design speed of {speed} km/h is above the maximum of {} km/h that the manual gives for Class {} in {} terrain ({}).",
            max.value,
            class.manual_name(),
            terrain.manual_name(),
            max.source.short()
        ));
    }
    let at_class_speed = (speed - max.value).abs() < 1e-9;

    let mut row = StandardRow { label: mowt_2010_row_label(class, terrain, speed, emax), design_speed_kmh: Some(speed), ..StandardRow::default() };

    // Horizontal: Table 6-1.
    let r = horizontal::min_radius(speed, emax)?;
    row.min_radius_m = Some(r.value.rounded_radius_m);
    row.max_superelevation = Some(emax / 100.0);
    row.max_side_friction = Some(r.value.limiting_f);
    put(&mut row, "min_radius_m", format!("{} (rounded radius; the class tables print the same value for emax 7 % or 4 %)", cite(r.source)));
    put(&mut row, "max_superelevation", format!("{} (maximum e chosen by the user: 4, 6, 7 or 8 %)", cite(r.source)));
    put(&mut row, "max_side_friction", cite(r.source));

    // Sight distance, K: the class table at the class design speed; below it Tables 6.19, 6-11 and 6-13 (low ends).
    let inputs = sight::inputs();
    if at_class_speed {
        match classes::class_parameter(class, terrain, Parameter::MinStoppingSightDistance) {
            Ok(c) => {
                row.stopping_sight_distance_m = Some(c.value);
                put(&mut row, "stopping_sight_distance_m", cite(c.source));
            }
            Err(e) => put(&mut row, "stopping_sight_distance_m", e.to_string()),
        }
        match classes::class_parameter(class, terrain, Parameter::CrestKStopping) {
            Ok(c) => {
                row.min_k_crest = Some(c.value);
                put(&mut row, "min_k_crest", format!("{} (Kmin, crest, stopping)", cite(c.source)));
            }
            Err(e) => put(&mut row, "min_k_crest", format!("{e} run_checks derives K from the stopping sight distance and the manual's eye and object heights, and says so")),
        }
        match classes::class_parameter(class, terrain, Parameter::SagKStopping) {
            Ok(c) => {
                row.min_k_sag = Some(c.value);
                put(&mut row, "min_k_sag", format!("{} (Kmin, sag, stopping)", cite(c.source)));
            }
            Err(e) => put(&mut row, "min_k_sag", e.to_string()),
        }
    } else {
        let s = sight::stopping_sight_distance(speed)?;
        row.stopping_sight_distance_m = Some(s.value.stopping_sight_distance_m.0);
        put(&mut row, "stopping_sight_distance_m", format!("{} (low end of the printed range, unrounded; the class tables round it up)", cite(s.source)));
        let kc = vertical::crest_k_stopping(speed)?;
        row.min_k_crest = Some(kc.value.k_rounded.0);
        put(&mut row, "min_k_crest", format!("{} (rounded for design, low end of the printed range, as the class tables use)", cite(kc.source)));
        let ks = vertical::sag_k(speed)?;
        row.min_k_sag = Some(ks.value.k_rounded.0);
        put(&mut row, "min_k_sag", format!("{} (rounded for design, low end of the printed range, as the class tables use)", cite(ks.source)));
    }
    row.reaction_time_s = Some(inputs.reaction_time_s.value);
    put(&mut row, "reaction_time_s", cite(inputs.reaction_time_s.source));
    row.eye_height_m = Some(inputs.eye_height_m.value);
    put(&mut row, "eye_height_m", cite(inputs.eye_height_m.source));
    row.object_height_m = Some(inputs.object_height_stopping_m.value);
    put(&mut row, "object_height_m", cite(inputs.object_height_stopping_m.source));
    row.headlight_height_m = Some(inputs.headlight_height_m.value);
    put(&mut row, "headlight_height_m", cite(inputs.headlight_height_m.source));
    row.headlight_beam_deg = Some(inputs.headlight_beam_deg.value);
    put(&mut row, "headlight_beam_deg", cite(inputs.headlight_beam_deg.source));
    put(&mut row, "longitudinal_friction", "the manual's coefficient of friction varies with the assumed speed (Table 6.19), so none is set; the stopping sight distance is a table value".to_string());

    // Gradients: class table, by class and terrain (not by speed).
    let (gp, gname) = match options.grade {
        GradeLimit::Desirable => (Parameter::MaxGradientDesirable, "desirable"),
        GradeLimit::Absolute => (Parameter::MaxGradientAbsolute, "absolute"),
    };
    match classes::class_parameter(class, terrain, gp) {
        Ok(c) => {
            row.max_grade = Some(c.value / 100.0);
            put(&mut row, "max_grade", format!("{} (maximum gradient, {gname})", cite(c.source)));
        }
        Err(e) => put(&mut row, "max_grade", e.to_string()),
    }
    if options.minimum_grade_in_cut {
        if let Ok(c) = classes::class_parameter(class, terrain, Parameter::MinGradientInCut) {
            row.min_grade = Some(c.value / 100.0);
            put(&mut row, "min_grade", format!("{} (minimum gradient in cut; applies to cuttings only, section 6.5.2, p. 123)", cite(c.source)));
        }
    } else {
        put(
            &mut row,
            "min_grade",
            "the manual's minimum gradient (0.5 % in Tables 4-3 to 4-9, 0.3 to 0.5 % in section 6.5.2, p. 123) is for cuttings only and run_checks cannot tell cut from fill; switch on MowtOptions::minimum_grade_in_cut for a design in cut".to_string(),
        );
    }

    // Not given by the manual.
    let none = "the manual gives no value (searched sections 6.2 and 6.3, Tables 4-3 to 4-9, 6-1 to 6-18)";
    put(&mut row, "min_curve_length_m", format!("{none}: no minimum length of horizontal curve"));
    put(&mut row, "min_spiral_length_m", "the manual gives a length that depends on the radius (L = 0.0702 V^3 / (R C), section 6.2.3 iv, p. 88; or the runoff length of Tables 6-4 to 6-9), not a single minimum; see mowt::horizontal".to_string());
    put(&mut row, "min_tangent_same_direction_m", format!("{none}: short tangents between curves 'should be avoided' (sections 6.2.3 i, 6.4), no length stated"));
    put(&mut row, "min_tangent_reverse_m", format!("{none}: short tangents between curves 'should be avoided' (sections 6.2.3 i, 6.4), no length stated"));
    put(&mut row, "min_vertical_curve_length_m", format!("{none}: no absolute minimum length of vertical curve (Figures 6-24 to 6-27 are graphs with no stated cut-off)"));
    Ok(row)
}

fn notes_for(class: RoadClass, terrain: Terrain, speed: f64, options: &MowtOptions) -> String {
    let mut n = String::new();
    n.push_str(&format!("Class {}, {} terrain, {} km/h. ", class.manual_name(), terrain.manual_name(), speed));
    n.push_str("Every limit carries its table or figure, section and page (printed and PDF) in the row's source. ");
    n.push_str("Minimum radius is the manual's printed rounded radius of Table 6-1, computed there with 127 in R = V^2 / (127 (e + f)); the crate's physics module uses 12.96 g = 127.0966, about 0.07 % different, and the table value governs. ");
    n.push_str(&format!("Maximum gradient checked: {}. ", match options.grade {
        GradeLimit::Desirable => "desirable (exceeding it needs a Departure from Standard, section 6.5.1)",
        GradeLimit::Absolute => "absolute",
    }));
    n.push_str("Where a class table cell contradicts another table of the manual it is withheld (see mowt::known_slips); if the crest K is withheld, run_checks derives it from the stopping sight distance and the manual's eye (1.07 m) and object (0.15 m) heights and labels it derived. ");
    n.push_str("Not checked under this basis: ");
    n.push_str(&not_checked_under_mowt_2010().iter().filter(|i| i.framework_has_check).map(|i| i.check_id).collect::<Vec<_>>().join(", "));
    n.push('.');
    n
}

/// A check that stays "not checked" under `mowt_2010`, or a thing the manual limits that the framework does not check.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NotCheckedInfo {
    /// The framework's check id (`H.curve_length`), or a short name for a limit the framework has no check for.
    pub check_id: &'static str,
    pub what: &'static str,
    pub reason: String,
    /// True if `run_checks` has this check and reports it "not checked"; false if the framework has no such check.
    pub framework_has_check: bool,
}

pub fn not_checked_under_mowt_2010() -> Vec<NotCheckedInfo> {
    let c = |id: &'static str, what: &'static str, reason: &str| NotCheckedInfo { check_id: id, what, reason: reason.to_string(), framework_has_check: true };
    let l = |id: &'static str, what: &'static str, reason: &str| NotCheckedInfo { check_id: id, what, reason: reason.to_string(), framework_has_check: false };
    vec![
        c("H.curve_length", "minimum length of horizontal curve", "The manual gives no minimum length of horizontal curve."),
        c("H.spiral_length", "minimum spiral length", "The manual's spiral length depends on the radius (section 6.2.3 iv: L = 0.0702 V^3 / (R C); runoff length of Tables 6-4 to 6-9), so there is no single limit per row. Use mowt::horizontal::min_spiral_length_comfort or the runoff tables."),
        c("H.tangent_same", "minimum tangent between curves, same direction", "The manual says short tangents between curves 'should be avoided' (6.2.3 i) but states no length."),
        c("H.tangent_reverse", "minimum tangent between reverse curves", "The manual says abrupt reverse curves 'should be avoided' (6.2.3 i) but states no length."),
        c("V.min_grade", "minimum gradient", "The manual's 0.5 % (0.3 to 0.5 % in 6.5.2) is for cuttings; run_checks cannot tell cut from fill. Off unless MowtOptions::minimum_grade_in_cut is set."),
        c("V.curve_length", "minimum length of vertical curve", "The manual states no absolute minimum length of vertical curve in metres (Figures 6-24 to 6-27 are graphs)."),
        l("superelevation", "superelevation rate and runoff length (Tables 6-4 to 6-9)", "run_checks does not design or check superelevation; the lookups are in mowt::horizontal."),
        l("tangent_max", "maximum tangent length 4.0 km (6.2.2)", "The framework has no check for a maximum tangent; the value is in mowt::horizontal::rules."),
        l("widening", "widening on curves (Table 6-10)", "The framework does not check widening; the lookup is in mowt::horizontal."),
        l("horizontal_sight", "sight distance on horizontal curves (Figure 6-21)", "The framework has no lateral clearance check; the relations are in mowt::horizontal."),
        l("passing_sight", "passing sight distance (Tables 6.21, 6.22) and its crest K (Table 6-12)", "The framework checks stopping sight distance only; the tables are in mowt::sight and mowt::vertical."),
        l("critical_length", "critical length of gradient (Table 6-16)", "The framework does not check lengths of grade; the lookup is in mowt::vertical."),
    ]
}

/// A terrain offered for a class, with the design speeds the built-in basis accepts.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TerrainInfo {
    pub terrain: Terrain,
    pub name: &'static str,
    /// Maximum design speed from the class tables, km/h.
    pub max_design_speed_kmh: f64,
    /// Selectable design speeds: 30 km/h and up in steps of 10, to the maximum.
    pub speeds_kmh: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassInfo {
    pub class: RoadClass,
    /// The class name as the manual prints it (`II Paved`).
    pub name: &'static str,
    /// "Capacity [pcu x 1,000/day]" of Table 4-2a as printed (none for Class C Gravel).
    pub traffic_kpcu_per_day: Option<(f64, f64)>,
    pub terrains: Vec<TerrainInfo>,
    /// Terrains left out of the choice, with the reason.
    pub refused_terrains: Vec<(Terrain, String)>,
}

/// A built-in basis the app can offer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BuiltinBasisInfo {
    pub id: &'static str,
    pub name: String,
    /// Document, edition and where the copy came from.
    pub source: String,
    pub classes: Vec<ClassInfo>,
    /// Selectable maximum superelevations, percent (Table 6-1).
    pub emax_percent_options: Vec<f64>,
    /// What stays "not checked", and what the framework cannot check.
    pub not_checked: Vec<NotCheckedInfo>,
}

/// The built-in bases, with the classes, terrains and design speeds that can be selected.
pub fn builtin_bases() -> Vec<BuiltinBasisInfo> {
    let mut classes_out = Vec::new();
    for class in RoadClass::ALL {
        let def = classes::class_definition(class);
        let mut terrains = Vec::new();
        let mut refused = Vec::new();
        for terrain in [Terrain::Flat, Terrain::Rolling, Terrain::Mountainous, Terrain::UrbanPeriUrban, Terrain::Escarpment] {
            match classes::max_design_speed(class, terrain) {
                Ok(m) => terrains.push(TerrainInfo {
                    terrain,
                    name: terrain.manual_name(),
                    max_design_speed_kmh: m.value,
                    speeds_kmh: SPEEDS_KMH.iter().copied().filter(|s| *s <= m.value + 1e-9).collect(),
                }),
                Err(e) => refused.push((terrain, e.to_string())),
            }
        }
        classes_out.push(ClassInfo { class, name: def.name, traffic_kpcu_per_day: def.traffic_kpcu_per_day, terrains, refused_terrains: refused });
    }
    vec![BuiltinBasisInfo {
        id: MOWT_2010_ID,
        name: "Uganda MoWT Road Design Manual Vol. I Geometric Design, January 2010".to_string(),
        source: DOCUMENT_FULL.to_string(),
        classes: classes_out,
        emax_percent_options: horizontal::EMAX_OPTIONS_PERCENT.to_vec(),
        not_checked: not_checked_under_mowt_2010(),
    }]
}

/// A slip found in the manual: a place where it contradicts itself or is evidently mistyped.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Slip {
    pub id: &'static str,
    /// Where, as table or section with printed page (PDF page).
    pub place: &'static str,
    pub what: &'static str,
    /// What this crate does about it: withholds the value, uses the printed value, or records it only.
    pub treatment: &'static str,
}

pub fn known_slips() -> Vec<Slip> {
    let s = |id, place, what, treatment| Slip { id, place, what, treatment };
    vec![
        s("S01", "Table 4-4 (p. 45, PDF 54) against Table 4-2a (p. 44, PDF 53)", "Class Ib flat column prints design speed 120 km/h with the figures of Class Ia; Table 4-2a gives 110 km/h. The PDF page also holds a hidden second set (110 km/h, SSD 180, passing 730, radius 530, rolling crest K 62) that is not visible when the page is rendered.", "Class Ib in flat terrain is refused (flat column withheld)."),
        s("S02", "Tables 4-3 and 4-4 (p. 45, PDF 54)", "Rolling terrain, 100 km/h: crest K (stopping) printed 80; Table 6-11 and Figure 6-25 give 62 for 100 km/h and S^2/404 = 63.4 for S = 160 m.", "Cell withheld; run_checks derives K from the SSD."),
        s("S03", "Tables 4-5 and 4-7 (p. 46, PDF 55)", "Flat terrain, 90 km/h: passing K printed 307; Table 6-12 gives 387 (605^2/946 = 386.9).", "Cell withheld."),
        s("S04", "Table 4-5 (p. 46, PDF 55) against Tables 6-14, 6-15 (p. 122, PDF 131)", "Class II mountainous (60 km/h): gradients 6 % and 8 % printed; Tables 6-14 and 6-15 give 8 % and 10 % at 60 km/h in mountainous terrain. The 6 and 8 are those of Class Ia at 80 km/h.", "Both gradient cells withheld (maximum gradient not checked)."),
        s("S05", "Table 4-9 (p. 47, PDF 56)", "Class C flat (60 km/h) and rolling (50 km/h): crest K stopping 22 and 14 and passing K 246 and 176 are the values of 70 and 60 km/h (Tables 6-11, 6-12 give 14 and 9, 176 and 126), while the sight distances in the same columns are those of 60 and 50 km/h.", "Four cells withheld."),
        s("S06", "Table 4-5 (p. 46, PDF 55)", "Urban/peri-urban stopping sight distance printed 58; every other class table prints 60 for 50 km/h (Table 6.19 gives 57.5).", "Printed value used (consistent with 57.5)."),
        s("S07", "Tables 6-6, 6-7 (7 % emax; p. 100 to 101, PDF 109 to 110) against Table 6-1 (p. 80)", "Printed Rmin at 30 to 120 km/h are 30, 50, 90, 140, 200, 250, 300, 400, 500, 700 m; Table 6-1 (7 %) prints 30, 55, 85, 130, 185, 240, 320, 415, 530, 710 m. Tables 4-3 to 4-9 use Table 6-1.", "Table 6-1 values used for the limit; the printed Rmin of the superelevation tables are available from table_rmin."),
        s("S08", "Tables 6-8, 6-9 (8 % emax; p. 102 to 103, PDF 111 to 112) against Table 6-1", "Printed Rmin at 100 and 120 km/h are 395 and 655 m; Table 6-1 (8 %) prints 400 and 670 m.", "Table 6-1 values used for the limit."),
        s("S09", "Tables 6-8, 6-9 (p. 102 to 103, PDF 111 to 112)", "At 100 km/h, R = 500 m (e 7.6 %) and R = 400 m (e 8.0 %): runoff L printed 59 and 62; the manual's formula with the Table 6-2 ratio 210:1 and a 3.5 m lane gives 55.9 and 58.8 (the printed lengths correspond to a ratio of about 222:1, which Table 6-2 does not print). Every other L cell of Tables 6-4 to 6-9 reproduces from the formula.", "Printed values kept in the table."),
        s("S10", "Section 6.2.3 vii (p. 93, PDF 102) against Tables 6-4 to 6-9", "The text says the tables are for 3.65 m lanes; the notes under each table say lane width 3.5 m, and the printed L values reproduce with 3.5 m.", "3.5 m recorded as the lane width of the tables."),
        s("S11", "Worked example 2 (p. 95, PDF 104)", "Takes the superelevation from 'table 6-7' with normal crown 2.5 %; Table 6-7 is for a normal crown of 4 %, Table 6-6 for 2.5 %. The cell used (V 80, R 250: 7.0 %, L 49) is the same in Tables 6-6 and 6-7.", "Recorded."),
        s("S12", "Table 6-2 (p. 92, PDF 101)", "The column heading prints the unit of the relative gradient as [%]; the cells are ratios (133:1, ...).", "Ratios returned."),
        s("S13", "Section 6.2.3 v (p. 88, PDF 97)", "The clothoid relation is printed A^2 = R - L; Figure 6-13 (p. 89) prints A^2 = R x L.", "R x L used."),
        s("S14", "Worked example 1 on degree of curve (p. 83, PDF 92)", "The deflection is given as 20 deg 18' 02\" but the working uses 23 deg 18' 02\" (T = 59.07, L = 116.50), and E is worked with half of 20 deg 18' 02\" (10 deg 09' 01\"): E printed 4.55 (4.555 for that angle); with 23 deg 18' 02\" it is 6.03.", "Recorded; the tests reproduce T, L and the stations with 23 deg 18' 02\"."),
        s("S15", "Worked example 2 (p. 95, PDF 104)", "L = 20 (23.3 - 2 x 5.615) / 4.5386 = 53.188 uses D = 4.5386; D = 1145.916 / 250 = 4.5837, which gives 52.66 m. The manual's total tangent T = R tan(D/2) + Ls/2 = 76.045 is an approximation; the exact spiral tangent (R + p) tan(D/2) + k is larger, by 0.08 m here (76.12 m against 76.045 m).", "Recorded; the crate's exact geometry is used elsewhere."),
        s("S16", "Worked example 1 on superelevation (p. 95, PDF 104)", "'End of equal superelevation = 10+8219.765 - 16.67 = 10+813.095' prints 8219.765 for 829.765.", "Recorded."),
        s("S17", "Figure 6-21 and example (p. 105, PDF 114)", "The text says the ordinate of [b] 'should not be less than that of [a]'; the example concludes '11.85 >> 3.94 design is OK' (available [a] above required [b]). The example works [b] with 28.55 where the relation has 28.65: 500 (1 - cos(28.65 x 126 / 500)) = 3.96, not 3.94.", "The example's direction is used; 3.96 recorded."),
        s("S18", "Table 6-12 (p. 110, PDF 119)", "Titled 'based on stopping sight distance'; it is the passing sight distance table.", "Recorded."),
        s("S19", "Table 6.20 (p. 129, PDF 138)", "The 5 % downgrade column equals the manual's own formula for a 4 % downgrade in all ten rows.", "Column withheld."),
        s("S20", "Table 6.20 (p. 129, PDF 138)", "Upgrade 3 %, 60 km/h (assumed 55): printed 71.6; the formula gives 71.3.", "Printed value kept."),
        s("S21", "Tables 6-17, 6-18 (p. 125, PDF 134)", "Titled 'Maximum Gradients (Desirable)' and 'Absolute Maximum Gradients' (copied from Tables 6-14, 6-15); they hold climbing lane thresholds and tapers.", "Recorded."),
        s("S22", "Section 6.2.3, the second item numbered vii, Shoulder Superelevation (p. 96, PDF 105)", "'Figure 6-8 depicts shoulder superelevation rates'; the figure is 6-20 (Figure 6-8 is the elements of a circular curve).", "Recorded."),
        s("S23", "Section 6.7.3 (p. 132, PDF 141)", "The relation for the opposing vehicle's distance d4 is printed 'd3 = 2/3 d2'.", "d4 = 2/3 d2 used."),
        s("S24", "Worked example on minimum radius (p. 79, PDF 88)", "States V 80 km/h, emax 7 %, f 0.14 and asks for the minimum radius but prints no answer; the formula gives 239.97 m, the Table 6-1 row (240 m).", "Recorded."),
        s("S25", "Tables 6-5 and 6-9 (p. 99 and 103, PDF 108 and 112)", "For a normal crown of 4 % the tables print RC wherever the rate is not above 4 %, but the cells V 40 R 60 (Table 6-5) and V 90 R 1000 (Table 6-9) print 4.0 % (with L 22 and 50, equal to the RC lengths).", "Printed cells kept."),
    ]
}
