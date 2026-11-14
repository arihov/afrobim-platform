//! The check lines: the sweep and the sign's facts compared with the provisions of the held
//! manuals. Each line has a status and cites its source.
//!
//! Status meanings:
//!
//! - `OK`: the provision is met.
//! - `FAILS`: a minimum or requirement the source states is not met.
//! - `DIFFERS`: a recommended or normal value is departed from, or a tolerance the source allows
//!   is used or exceeded, or the sign is partly hidden; the engineer decides.
//! - `INPUT NEEDED`: the check cannot be made without an entry (or the source is not held).
//! - `INFORMATION`: a measured or sourced value shown without a verdict.

use afrobim_signs::{SignClass, SignDefinition, SignKind};
use serde::{Deserialize, Serialize};

use crate::inputs::{UkSignRule, VisibilityInputs};
use crate::sources::*;
use crate::sweep::{SweepResult, TravelDirection};

/// The result of one check line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    Ok,
    Fails,
    Differs,
    InputNeeded,
    Information,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::Fails => "FAILS",
            Status::Differs => "DIFFERS",
            Status::InputNeeded => "INPUT NEEDED",
            Status::Information => "INFORMATION",
        }
    }

    /// For choosing the worst line of a sign: FAILS, DIFFERS, INPUT NEEDED, then the rest.
    pub fn severity(self) -> u8 {
        match self {
            Status::Fails => 4,
            Status::Differs => 3,
            Status::InputNeeded => 2,
            Status::Ok => 1,
            Status::Information => 0,
        }
    }
}

/// One line of a sign's visibility report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckLine {
    /// Stable identifier, e.g. `visibility.uk`.
    pub id: String,
    pub status: Status,
    pub title: String,
    pub detail: String,
    /// The provisions the line rests on, as `Citation` text; empty for a purely geometric line.
    pub citation: String,
}

impl CheckLine {
    /// The line as one text row: `STATUS  title: detail  [citation]`.
    pub fn text(&self) -> String {
        let cite = if self.citation.is_empty() { String::new() } else { format!("  [{}]", self.citation) };
        format!("{:<12}  {}: {}{}", self.status.label(), self.title, self.detail, cite)
    }
}

fn line(id: &str, status: Status, title: &str, detail: String, cites: &[Citation]) -> CheckLine {
    CheckLine { id: id.to_string(), status, title: title.to_string(), detail, citation: cites.iter().map(|c| c.to_string()).collect::<Vec<_>>().join("; ") }
}

/// What kind of sign this is for the purpose of choosing provisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignGroup {
    Warning,
    Regulatory,
    /// Guidance or information sign with lettering.
    Lettered,
    Supplementary,
}

/// The group of a sign definition, and whether it is an advance direction sign.
pub fn classify(def: &SignDefinition) -> (SignGroup, bool) {
    let group = match (def.kind, def.class) {
        (SignKind::SupplementaryPlate, _) | (_, SignClass::Supplementary) => SignGroup::Supplementary,
        (SignKind::WarningSign, _) | (_, SignClass::Warning) => SignGroup::Warning,
        (SignKind::RegulatoryProhibitory | SignKind::RegulatoryMandatory | SignKind::Stop | SignKind::GiveWay, _) | (_, SignClass::Regulatory) => SignGroup::Regulatory,
        _ => SignGroup::Lettered,
    };
    let advance = matches!(def.kind, SignKind::StackType | SignKind::MapTypeCrossroads | SignKind::MapTypeRoundabout | SignKind::MapTypeTee);
    (group, advance)
}

/// Everything the checks need about one sign, already gathered.
#[derive(Debug, Clone)]
pub struct CheckInput<'a> {
    pub inputs: &'a VisibilityInputs,
    pub group: SignGroup,
    pub advance_direction: bool,
    pub speed_kph: Option<f64>,
    pub sweep: Option<&'a SweepResult>,
    /// Why there is no sweep (a missing input), when there is none.
    pub no_sweep_reason: Option<String>,
    pub x_height_mm: Option<f64>,
    pub words: Option<u32>,
    /// Lateral distance S from the centre of the driving lane to the centre of the sign, m.
    pub lateral_s_m: Option<f64>,
    pub direction: TravelDirection,
    pub sign_station_m: Option<f64>,
    /// Height of the face's lower edge above the road level at the sign, m.
    pub lower_edge_above_road_m: Option<f64>,
    /// Height of the lower edge above the driver's eye at the sign's station, m (overhead signs).
    pub lower_edge_above_eye_m: Option<f64>,
    /// Distances of the face's nearest edge to the centreline, m, and whether the sign is on the
    /// left of the centreline.
    pub near_edge_offset_m: Option<f64>,
    pub sign_on_left: Option<bool>,
    /// Grade of the road at the sign in the direction of travel, m/m (negative: descending).
    pub grade_in_travel_direction: Option<f64>,
    /// Angle in plan between the face normal and the direction opposite the travel direction, i.e.
    /// how far the face is turned from square to the road, degrees (signed, anticlockwise positive).
    pub face_turn_from_square_deg: Option<f64>,
    pub has_scene: bool,
}

fn m(v: f64) -> String {
    format!("{v:.1} m")
}

fn mph_note(kph: f64) -> String {
    format!("{kph:.0} km/h is {:.1} mph (1 mph = {KPH_PER_MPH:.6} km/h); the UK band is the engineer's choice", kph / KPH_PER_MPH)
}

/// The continuous clear visibility distance to compare with a requirement: the whole face must be
/// visible. Returns (distance, "at least" flag).
fn measured(sweep: &SweepResult) -> (f64, bool) {
    (sweep.full.distance_m, sweep.full.reaches_start)
}

fn compare_visibility(id: &str, title: &str, required: f64, sweep: &SweepResult, basis: String, cites: &[Citation]) -> CheckLine {
    let (d, at_least) = measured(sweep);
    let partial = sweep.partial.distance_m;
    let what = sweep
        .full
        .beyond
        .as_ref()
        .and_then(|b| b.blockers.first())
        .map(|b| format!("; the whole face is first hidden by {} '{}'{}", b.kind.label(), b.label, b.station_m.map_or(String::new(), |s| format!(" at station {s:.1}"))))
        .unwrap_or_default();
    if at_least {
        if sweep.swept_to_m + 1e-9 >= required {
            line(id, Status::Ok, title, format!("required {}; the whole face is visible from every position swept, i.e. from at least {} ({basis})", m(required), m(sweep.swept_to_m)), cites)
        } else {
            line(
                id,
                Status::InputNeeded,
                title,
                format!(
                    "required {}; the sweep only reaches {} (the alignment ends or the start distance is short), so the check cannot be completed; extend the approach ({basis})",
                    m(required),
                    m(sweep.swept_to_m)
                ),
                cites,
            )
        }
    } else if d + 1e-9 >= required {
        line(id, Status::Ok, title, format!("required {}; the whole face stays visible from {}{what} ({basis})", m(required), m(d)), cites)
    } else {
        line(
            id,
            Status::Fails,
            title,
            format!("required {}; the whole face is continuously visible only from {} (part of it from {}){what} ({basis})", m(required), m(d), m(partial)),
            cites,
        )
    }
}

/// Runs every check that applies and returns the lines in a stable order.
pub fn run_checks(ci: &CheckInput) -> Vec<CheckLine> {
    let mut out = Vec::new();
    if let Some(reason) = &ci.no_sweep_reason {
        out.push(line("sweep", Status::InputNeeded, "Sight line sweep", reason.clone(), &[]));
    }
    if let Some(s) = ci.sweep {
        out.push(sweep_summary(s));
        if let Some(t) = &s.truncated {
            out.push(line("sweep.truncated", Status::Information, "Sweep length", t.clone(), &[]));
        }
        out.extend(masking(ci, s));
    }
    if !ci.has_scene {
        out.push(line(
            "scene",
            Status::Information,
            "Obstructions modelled",
            "no ground surface or solids were supplied: only the road's vertical profile and the entered lateral offsets were tested".to_string(),
            &[],
        ));
    }
    if ci.inputs.standards.uk {
        out.extend(uk_visibility(ci));
        out.extend(uk_siting(ci));
    }
    if ci.inputs.standards.uganda {
        out.extend(ug_visibility(ci));
        out.extend(ug_siting(ci));
    }
    out.extend(legibility(ci));
    out.extend(mounting(ci));
    out.extend(clearance(ci));
    out.extend(orientation(ci));
    out.extend(overhead(ci));
    out
}

fn sweep_summary(s: &SweepResult) -> CheckLine {
    let full = if s.full.reaches_start { format!("at least {}", m(s.full.distance_m)) } else { m(s.full.distance_m) };
    let part = if s.partial.reaches_start { format!("at least {}", m(s.partial.distance_m)) } else { m(s.partial.distance_m) };
    let mut d = format!(
        "driver travelling {} at offset {:+.2} m, eye {:.2} m above the road; whole face visible continuously from {full}; some of the face from {part}",
        s.direction.label(),
        s.eye_offset_m,
        s.params.eye_height_m
    );
    if let Some(at) = &s.full.at_limit {
        d.push_str(&format!("; at the limit the line of sight is {:.1} deg from the face normal ({:.1} deg in plan)", at.obliquity_deg, at.plan_obliquity_deg));
    }
    line("sweep.summary", Status::Information, "Continuous clear visibility distance", d, &[])
}

fn masking(_ci: &CheckInput, s: &SweepResult) -> Vec<CheckLine> {
    let signs = s.masking_signs();
    if signs.is_empty() {
        return Vec::new();
    }
    let mut parts = Vec::new();
    for b in &signs {
        parts.push(format!(
            "'{}' hides at least part of this sign from eye positions {} to {} before it{}",
            b.label,
            m(b.from_distance_m),
            m(b.to_distance_m),
            b.station_range_m.map_or(String::new(), |(a, c)| format!(" (at stations {a:.1} to {c:.1})"))
        ));
    }
    vec![line(
        "mask.sign",
        Status::Differs,
        "Sign hidden by another sign",
        parts.join("; "),
        &[UK_CH4_VISIBILITY, UG_SPACING],
    )]
}

fn rule_for(ci: &CheckInput) -> UkSignRule {
    match ci.inputs.uk_rule {
        UkSignRule::Auto => match ci.group {
            SignGroup::Warning => UkSignRule::Warning,
            SignGroup::Regulatory => UkSignRule::Regulatory,
            SignGroup::Lettered => UkSignRule::Directional,
            SignGroup::Supplementary => UkSignRule::NotApplicable,
        },
        r => r,
    }
}

fn speed_note(ci: &CheckInput) -> String {
    ci.speed_kph.map_or(String::new(), |k| format!(" ({})", mph_note(k)))
}

fn uk_visibility(ci: &CheckInput) -> Vec<CheckLine> {
    let title = "Clear visibility distance, UK";
    let rule = rule_for(ci);
    let lane_note = "measured from the centre of the most disadvantaged driving lane (UK para 1.16/1.23): check that the eye path entered is that lane";
    let need_band = |what: &str| line("visibility.uk", Status::InputNeeded, title, format!("choose the UK speed band of the 85th percentile speed for {what}{}", speed_note(ci)), &[UK_TABLE_1_1]);
    let Some(sweep) = ci.sweep else {
        return match rule {
            UkSignRule::NotApplicable | UkSignRule::Directional => Vec::new(),
            _ => vec![line("visibility.uk", Status::InputNeeded, title, "the sight line sweep did not run (see the sweep line)".to_string(), &[UK_TABLE_1_1])],
        };
    };
    match rule {
        UkSignRule::Regulatory => {
            let Some(band) = ci.inputs.uk_band else { return vec![need_band("Table 1-1")] };
            let (base, bracket) = uk_table_1_1(band);
            let (req, basis) = match (ci.inputs.uk_large_case, bracket) {
                (true, Some(b)) => (b, format!("Table 1-1, {}, bracketed figure for 70 mph dual carriageways and motorways with the larger sign (para 1.17); {lane_note}", band.label())),
                _ => (base, format!("Table 1-1, {}; {lane_note}", band.label())),
            };
            vec![compare_visibility("visibility.uk", title, req, sweep, basis, &[UK_TABLE_1_1, UK_CH3_MEASURED_FROM])]
        }
        UkSignRule::Warning => {
            let Some(band) = ci.inputs.uk_band else { return vec![need_band("Appendix A")] };
            let row = uk_warning_row(band);
            let (req, basis) = match (ci.inputs.uk_large_case, row.min_clear_visibility_bracketed_m) {
                (true, Some(b)) => (b, format!("Appendix A column 3, {}, largest figure for 70 mph dual carriageways and motorways (note 2); the figure should not be reduced (note 1); {lane_note}", band.label())),
                _ => (row.min_clear_visibility_m, format!("Appendix A column 3, {}; the figure should not be reduced (note 1); {lane_note}", band.label())),
            };
            vec![compare_visibility("visibility.uk", title, req, sweep, basis, &[UK_CH4_APPENDIX_A, UK_CH4_VISIBILITY])]
        }
        UkSignRule::SpeedLimitTerminal { approach_mph, sign_mph } => match uk_table_14_3(approach_mph, sign_mph) {
            Some(req) => vec![compare_visibility(
                "visibility.uk",
                title,
                req,
                sweep,
                format!("Table 14-3, approach limit {approach_mph} mph, sign {sign_mph} mph; notes 1 to 3 (side road approaches, reduced distances for lower 85th percentile speeds, both terminal signs) are not applied here"),
                &[UK_TABLE_14_3],
            )],
            None => vec![line(
                "visibility.uk",
                Status::InputNeeded,
                title,
                format!("Table 14-3 prints no figure for an approach limit of {approach_mph} mph and a sign of {sign_mph} mph; read the notes of the table and enter the distance"),
                &[UK_TABLE_14_3],
            )],
        },
        UkSignRule::Directional => vec![line(
            "visibility.uk",
            Status::Information,
            title,
            "no numeric minimum clear visibility distance for directional signs is stated in the Chapter 7 held (para 9.8 refers to one without giving it); the distance from which reading must start is in the legibility lines".to_string(),
            &[UK_CH7_OVERHEAD, UK_CH1_VISIBLE],
        )],
        UkSignRule::NotApplicable | UkSignRule::Auto => Vec::new(),
    }
}

fn uk_siting(ci: &CheckInput) -> Vec<CheckLine> {
    let title = "Siting distance before the hazard, UK";
    let rule = rule_for(ci);
    if rule != UkSignRule::Warning {
        return match rule {
            UkSignRule::Regulatory | UkSignRule::SpeedLimitTerminal { .. } => vec![line(
                "siting.uk",
                Status::Information,
                title,
                "a regulatory sign is placed at the point where the requirement begins; no distance before a hazard applies".to_string(),
                &[],
            )],
            UkSignRule::Directional => vec![line(
                "siting.uk",
                Status::Information,
                title,
                "no siting distance for advance direction signs is stated in the UK chapters held".to_string(),
                &[],
            )],
            _ => Vec::new(),
        };
    }
    let (Some(band), Some(sign_s)) = (ci.inputs.uk_band, ci.sign_station_m) else {
        return vec![line("siting.uk", Status::InputNeeded, title, format!("choose the UK speed band{}", speed_note(ci)), &[UK_CH4_APPENDIX_A])];
    };
    let Some(hazard) = ci.inputs.hazard_station_m else {
        return vec![line("siting.uk", Status::InputNeeded, title, "enter the internal station of the hazard (no sign records it)".to_string(), &[UK_CH4_APPENDIX_A, UK_CH4_SITING])];
    };
    let row = uk_warning_row(band);
    let (lo, hi) = if ci.inputs.uk_large_case && row.min_clear_visibility_bracketed_m.is_some() { (305.0, 305.0) } else { row.siting_m };
    let d = (hazard - sign_s) * ci.direction.sign();
    let range = if lo == hi { m(lo) } else { format!("{} to {}", m(lo), m(hi)) };
    let cites = [UK_CH4_APPENDIX_A, UK_CH4_SITING];
    let mut lines = Vec::new();
    let basis = format!("Appendix A column 4, {}: {range}; within about 10 % of it is acceptable (para 1.20); progressively larger figures towards the higher end of the speed range (note 5)", band.label());
    if d <= 0.0 {
        lines.push(line("siting.uk", Status::Fails, title, format!("the hazard station is {} behind the sign in the direction of travel ({basis})", m(-d)), &cites));
    } else if d + 1e-9 < lo * (1.0 - UK_SITING_TOLERANCE) {
        lines.push(line(
            "siting.uk",
            Status::Fails,
            title,
            format!("the sign is {} from the hazard, more than 10 % closer than the recommended {range}; para 1.20: unlikely to give sufficient warning, use a distance plate if unavoidable ({basis})", m(d)),
            &cites,
        ));
    } else if d > hi * (1.0 + UK_SITING_TOLERANCE) + 1e-9 {
        lines.push(line(
            "siting.uk",
            Status::Differs,
            title,
            format!("the sign is {} from the hazard, more than 10 % beyond the recommended {range}; para 1.20 allows this only where a site within 10 % is impracticable, at the nearest practicable point, and a distance plate to diagram 572 may be appropriate ({basis})", m(d)),
            &cites,
        ));
    } else if d + 1e-9 < lo || d > hi + 1e-9 {
        lines.push(line("siting.uk", Status::Differs, title, format!("the sign is {} from the hazard, outside {range} but within the 10 % tolerance ({basis})", m(d)), &cites));
    } else {
        lines.push(line("siting.uk", Status::Ok, title, format!("the sign is {} from the hazard, inside {range} ({basis})", m(d)), &cites));
    }
    if let Some(g) = ci.grade_in_travel_direction {
        if g < 0.0 {
            lines.push(line(
                "siting.uk.gradient",
                Status::Information,
                "Siting on a descent, UK",
                format!(
                    "the road falls {:.1} % in the direction of travel at the sign; Appendix A note 5 says siting distances may need to be increased on steep hills, e.g. by 50 % for a 10 % descent (an example, not a rule for other gradients)",
                    -g * 100.0
                ),
                &[UK_CH4_APPENDIX_A],
            ));
        }
    }
    lines
}

fn ug_class(ci: &CheckInput) -> Option<UgSpeedClass> {
    ci.speed_kph.map(UgSpeedClass::from_kph)
}

fn ug_visibility(ci: &CheckInput) -> Vec<CheckLine> {
    let title = "Visibility distance to the sign, Uganda";
    let Some(sweep) = ci.sweep else {
        return vec![line("visibility.ug", Status::InputNeeded, title, "the sight line sweep did not run (see the sweep line)".to_string(), &[UG_TABLE_D2])];
    };
    let Some(class) = ug_class(ci) else {
        return vec![line("visibility.ug", Status::InputNeeded, title, "enter the approach speed, km/h".to_string(), &[UG_TABLE_D2])];
    };
    vec![compare_visibility(
        "visibility.ug",
        title,
        class.visibility_m(),
        sweep,
        format!("Table D.2, {} ({:.0} km/h entered); the manual does not say from which lane", class.label(), ci.speed_kph.unwrap_or(0.0)),
        &[UG_TABLE_D2],
    )]
}

fn ug_siting(ci: &CheckInput) -> Vec<CheckLine> {
    let title = "Siting distance before the hazard, Uganda";
    let (table, req_of): (Citation, fn(UgSpeedClass) -> f64) = match (ci.group, ci.advance_direction) {
        (SignGroup::Warning, _) => (UG_TABLE_D3, UgSpeedClass::warning_siting_m),
        (SignGroup::Lettered, true) => (UG_TABLE_D5, UgSpeedClass::advance_direction_siting_m),
        (SignGroup::Regulatory, _) => {
            return vec![line("siting.ug", Status::Information, title, "regulatory signs are normally sited at or near the point where the instruction applies".to_string(), &[UG_TABLE_D2])];
        }
        _ => return Vec::new(),
    };
    let (Some(class), Some(sign_s)) = (ug_class(ci), ci.sign_station_m) else {
        return vec![line("siting.ug", Status::InputNeeded, title, "enter the approach speed, km/h".to_string(), &[table])];
    };
    let Some(hazard) = ci.inputs.hazard_station_m else {
        return vec![line("siting.ug", Status::InputNeeded, title, "enter the internal station of the hazard or intersection (no sign records it)".to_string(), &[table])];
    };
    let req = req_of(class);
    let d = (hazard - sign_s) * ci.direction.sign();
    let basis = format!("{}: {} ({:.0} km/h); the manual prefers increasing the distance to decreasing it, with a supplementary plate where the sign is far from its normal position", class.label(), m(req), ci.speed_kph.unwrap_or(0.0));
    let (status, text) = if d <= 0.0 {
        (Status::Fails, format!("the hazard station is {} behind the sign in the direction of travel ({basis})", m(-d)))
    } else if d + 1e-9 >= req {
        (Status::Ok, format!("the sign is {} from the hazard, not less than the normal distance ({basis})", m(d)))
    } else {
        (Status::Differs, format!("the sign is {} from the hazard, less than the normal distance ({basis})", m(d)))
    };
    vec![line("siting.ug", status, title, text, &[table])]
}

/// The distance from the sign at which the observation angle reaches 10 degrees, m.
fn reading_stop_m(ci: &CheckInput) -> Option<f64> {
    ci.lateral_s_m.map(|s| reading_stop_distance_m(s, cot_reading_stop_angle()))
}

fn legibility(ci: &CheckInput) -> Vec<CheckLine> {
    if ci.group != SignGroup::Lettered {
        return match ci.group {
            SignGroup::Supplementary => Vec::new(),
            _ => vec![line(
                "legibility",
                Status::Information,
                "Legibility",
                "the x-height relation is for lettered directional and informatory signs; for symbolic warning and regulatory signs the size is set by speed (Chapter 1 para 1.32) and the visibility lines above apply".to_string(),
                &[UK_CH1_READING],
            )],
        };
    }
    let mut out = Vec::new();
    let Some(x) = ci.x_height_mm else {
        out.push(line("legibility.x", Status::InputNeeded, "Legend legibility", "the x-height of the legend is not known: enter it (the sign module derives it when the layout runs)".to_string(), &[UK_X_HEIGHT]));
        return out;
    };
    let legible = legibility_distance_m(x);
    let visible = ci.sweep.map(|s| (s.full.distance_m, s.full.reaches_start));
    let (vis_text, status) = match visible {
        Some((d, at_least)) if d + 1e-9 >= legible => (format!("the whole face is visible from {}{}, so visibility does not limit legibility", if at_least { "at least " } else { "" }, m(d)), Status::Ok),
        Some((d, _)) => (format!("the whole face is only continuously visible from {}, shorter than the legibility distance: reading starts later than the legend size allows", m(d)), Status::Differs),
        None => ("the sight line sweep did not run".to_string(), Status::InputNeeded),
    };
    out.push(line(
        "legibility.x",
        status,
        "Legend legibility distance",
        format!("x-height {x:.1} mm becomes readable at {} (the extract: 100 mm at 60 m, proportional); {vis_text}", m(legible)),
        &[UK_X_HEIGHT],
    ));

    // Reading time and the 10 degree rule.
    let (Some(kph), Some(s_lat)) = (ci.speed_kph, ci.lateral_s_m) else {
        out.push(line("legibility.reading", Status::InputNeeded, "Reading distance and time", "enter the approach speed, km/h, and place the sign so the lateral distance can be taken".to_string(), &[UK_X_HEIGHT]));
        return out;
    };
    let words = ci.words;
    let rt = ci.inputs.reading_time_s.or(words.map(reading_time_s));
    let Some(rt) = rt else {
        out.push(line("legibility.reading", Status::InputNeeded, "Reading distance and time", "enter the number of words or destinations (N of the reading time 2 + N/3 s), or a reading time".to_string(), &[UK_X_HEIGHT]));
        return out;
    };
    let v = kph / 3.6;
    let r = reading_distance_m(rt, v);
    let c = reading_stop_m(ci).unwrap_or(0.0);
    let total = r + c;
    let need_x = required_x_height_mm(total);
    let mut basis = format!(
        "reading time {rt:.2} s{}, speed {v:.2} m/s ({kph:.0} km/h), R = {}, S = {s_lat:.2} m, C = S cot 10 = {}, reading must start {} from the sign",
        words.map_or(String::new(), |n| format!(" (N = {n})")),
        m(r),
        m(c),
        m(total)
    );
    if ci.inputs.reading_time_s.is_some() {
        basis.push_str(" (reading time entered, not from the source's formula)");
    }
    let (st, txt) = if x + 1e-9 >= need_x {
        (Status::Ok, format!("x-height {x:.1} mm is not less than the {need_x:.1} mm needed ({basis})"))
    } else {
        (Status::Fails, format!("x-height {x:.1} mm is less than the {need_x:.1} mm needed ({basis})"))
    };
    out.push(line("legibility.x_required", st, "x-height needed for the reading distance", txt, &[UK_X_HEIGHT]));

    let eff = visible.map_or(legible, |(d, _)| d.min(legible));
    let t_avail = ((eff - c) / v).max(0.0);
    let (st, txt) = if t_avail + 1e-9 >= rt {
        (Status::Ok, format!("time available to read {t_avail:.1} s (from {} to {} before the sign at {v:.1} m/s) is not less than the {rt:.1} s needed", m(eff), m(c)))
    } else {
        (Status::Fails, format!("time available to read {t_avail:.1} s (from {} to {} before the sign at {v:.1} m/s) is less than the {rt:.1} s needed", m(eff), m(c)))
    };
    out.push(line("legibility.time", st, "Time available for reading", txt, &[UK_X_HEIGHT, UK_CH1_READING]));

    if let Some(n) = words {
        if n > MAX_WORDS_DESIRABLE {
            out.push(line(
                "legibility.words",
                Status::Differs,
                "Number of destinations",
                format!("{n} words or destinations; the extract takes 4 s (N = 6) as the maximum desirable reading time and says the number should not exceed six"),
                &[UK_X_HEIGHT],
            ));
        }
    }
    if ci.inputs.standards.uganda {
        if let Some(class) = ug_class(ci) {
            let req = class.guidance_x_height_mm();
            let (st, txt) = if x + 1e-9 >= req {
                (Status::Ok, format!("x-height {x:.1} mm is not less than the {req:.0} mm of Table D.6 for {}", class.label()))
            } else {
                (Status::Fails, format!("x-height {x:.1} mm is less than the {req:.0} mm of Table D.6 for {}", class.label()))
            };
            out.push(line("legibility.ug", st, "Lettering size, Uganda", txt, &[UG_TABLE_D6]));
        }
    }
    out
}

fn mounting(ci: &CheckInput) -> Vec<CheckLine> {
    let mut out = Vec::new();
    let Some(h) = ci.lower_edge_above_road_m else {
        out.push(line("mounting", Status::InputNeeded, "Mounting height", "no mounting height is recorded on the sign's post and none was entered".to_string(), &[UK_CH3_MOUNTING]));
        return out;
    };
    let mm = h * 1000.0;
    if ci.inputs.standards.uk && !ci.inputs.overhead {
        let (lo, hi) = UK_MOUNTING_NORMAL_MM;
        let cites = [UK_CH3_MOUNTING, UK_CH4_MOUNTING, UK_CH1_MOUNTING];
        let (st, txt) = if ci.inputs.over_footway {
            let (rec, min) = UK_HEADROOM_MM;
            if mm + 1e-9 >= rec {
                (Status::Ok, format!("lower edge {mm:.0} mm over a footway or pedestrian area; headroom {rec:.0} mm recommended, {min:.0} mm absolute minimum"))
            } else if mm + 1e-9 >= min {
                (Status::Differs, format!("lower edge {mm:.0} mm over a footway or pedestrian area: above the {min:.0} mm absolute minimum but below the {rec:.0} mm recommended"))
            } else {
                (Status::Fails, format!("lower edge {mm:.0} mm over a footway or pedestrian area: below the {min:.0} mm absolute minimum"))
            }
        } else if mm + 1e-9 >= lo && mm <= hi + 1e-9 {
            (Status::Ok, format!("lower edge {mm:.0} mm above the carriageway alongside; normal range {lo:.0} to {hi:.0} mm"))
        } else {
            (Status::Differs, format!("lower edge {mm:.0} mm above the carriageway alongside; normal range {lo:.0} to {hi:.0} mm (the greater height where spray would soil the sign)"))
        };
        out.push(line("mounting.uk", st, "Mounting height, UK", format!("{txt} (measured here from the profile level at the sign, not from the highest point of the carriageway)"), &cites));
    }
    if ci.inputs.standards.uganda {
        let cites = [UG_MOUNTING];
        let (st, txt) = if ci.inputs.overhead {
            if h + 1e-9 >= UG_MOUNTING_OVERHEAD_M {
                (Status::Ok, format!("overhead sign lower edge {h:.2} m; the manual's mounting height for gantries and mast arms is {UG_MOUNTING_OVERHEAD_M} m with {UG_OVERHEAD_CLEARANCE_M} m always kept clear"))
            } else if h + 1e-9 >= UG_OVERHEAD_CLEARANCE_M {
                (Status::Differs, format!("overhead sign lower edge {h:.2} m is below the {UG_MOUNTING_OVERHEAD_M} m of the manual but not below the {UG_OVERHEAD_CLEARANCE_M} m clearance"))
            } else {
                (Status::Fails, format!("overhead sign lower edge {h:.2} m is below the {UG_OVERHEAD_CLEARANCE_M} m clearance the manual requires"))
            }
        } else if ci.inputs.over_footway {
            if h + 1e-9 >= UG_MOUNTING_FOOTWAY_M {
                (Status::Ok, format!("lower edge {h:.2} m over or alongside a footway; the manual asks for {UG_MOUNTING_FOOTWAY_M} m above footway level"))
            } else {
                (Status::Fails, format!("lower edge {h:.2} m over or alongside a footway is below the {UG_MOUNTING_FOOTWAY_M} m the manual asks for"))
            }
        } else if h + 1e-9 < UG_MOUNTING_MIN_M {
            (Status::Fails, format!("lower edge {h:.2} m: the manual says signs should never be mounted less than {UG_MOUNTING_MIN_M} m above carriageway level"))
        } else if (h - UG_MOUNTING_NORMAL_M).abs() <= 0.005 {
            (Status::Ok, format!("lower edge {h:.2} m, the normal {UG_MOUNTING_NORMAL_M} m above the highest point of the carriageway"))
        } else {
            (Status::Differs, format!("lower edge {h:.2} m; the normal height is {UG_MOUNTING_NORMAL_M} m above the highest point of the carriageway (not less than {UG_MOUNTING_MIN_M} m)"))
        };
        out.push(line("mounting.ug", st, "Mounting height, Uganda", format!("{txt} (measured here from the profile level at the sign)"), &cites));
    }
    out
}

fn clearance(ci: &CheckInput) -> Vec<CheckLine> {
    let mut out = Vec::new();
    let (Some(near), Some(left)) = (ci.near_edge_offset_m, ci.sign_on_left) else {
        return out;
    };
    let edges = &ci.inputs.road_edges;
    let near = near.abs();
    if ci.inputs.standards.uk {
        let case = ci.inputs.uk_clearance_case;
        match edges.carriageway.on(left) {
            Some(edge) => {
                let c = (near - edge.abs()) * 1000.0;
                let need = case.min_mm();
                let (st, txt) = if c + 1e-9 >= need {
                    (Status::Ok, format!("the nearest edge of the sign is {c:.0} mm from the edge of the carriageway; at least {need:.0} mm ({case:?})"))
                } else {
                    (Status::Fails, format!("the nearest edge of the sign is {c:.0} mm from the edge of the carriageway; at least {need:.0} mm is required ({case:?})"))
                };
                out.push(line("clearance.uk", st, "Lateral clearance, UK", txt, &[UK_CH1_PLACEMENT]));
            }
            None => out.push(line("clearance.uk", Status::InputNeeded, "Lateral clearance, UK", "enter the offset of the edge of the carriageway on the sign's side".to_string(), &[UK_CH1_PLACEMENT])),
        }
    }
    if ci.inputs.standards.uganda {
        let cites = [UG_CLEARANCE];
        if edges.kerbed {
            match edges.carriageway.on(left) {
                Some(kerb) => {
                    let c = near - kerb.abs();
                    let (st, txt) = if c + 1e-9 >= UG_FROM_KERB_M {
                        (Status::Ok, format!("the nearest edge of the sign is {c:.2} m from the kerb face; at least {UG_FROM_KERB_M} m"))
                    } else {
                        (Status::Fails, format!("the nearest edge of the sign is {c:.2} m from the kerb face; at least {UG_FROM_KERB_M} m is required"))
                    };
                    out.push(line("clearance.ug", st, "Lateral clearance, Uganda", txt, &cites));
                }
                None => out.push(line("clearance.ug", Status::InputNeeded, "Lateral clearance, Uganda", "enter the offset of the kerb face on the sign's side".to_string(), &cites)),
            }
        } else {
            match (edges.shoulder.on(left), edges.traffic_lane.on(left)) {
                (Some(sh), Some(lane)) => {
                    let (from_sh, from_lane) = (near - sh.abs(), near - lane.abs());
                    let ok = from_sh + 1e-9 >= UG_CLEAR_OF_SHOULDER_M && from_lane + 1e-9 >= UG_FROM_LANE_EDGE_M;
                    out.push(line(
                        "clearance.ug",
                        if ok { Status::Ok } else { Status::Fails },
                        "Lateral clearance, Uganda",
                        format!(
                            "the nearest edge of the sign is {from_sh:.2} m clear of the road shoulder (at least {UG_CLEAR_OF_SHOULDER_M} m) and {from_lane:.2} m from the edge of the traffic lane (at least {UG_FROM_LANE_EDGE_M} m)"
                        ),
                        &cites,
                    ));
                }
                _ => out.push(line("clearance.ug", Status::InputNeeded, "Lateral clearance, Uganda", "enter the offsets of the shoulder edge and of the traffic lane edge on the sign's side".to_string(), &cites)),
            }
        }
    }
    out
}

fn orientation(ci: &CheckInput) -> Vec<CheckLine> {
    let Some(turn) = ci.face_turn_from_square_deg.map(|t| if t.abs() < 0.05 { 0.0 } else { t }) else { return Vec::new() };
    let mut d = format!("the face is turned {turn:+.1} deg from square to the direction of travel (positive anticlockwise seen from above)");
    if let Some(s) = ci.sweep {
        let obl: Vec<f64> = s.samples.iter().filter(|e| e.state != crate::sweep::SampleState::Blocked).map(|e| e.obliquity_deg).collect();
        if let (Some(lo), Some(hi)) = (obl.iter().cloned().reduce(f64::min), obl.iter().cloned().reduce(f64::max)) {
            d.push_str(&format!("; the line of sight is {lo:.1} to {hi:.1} deg from the face normal at the positions that see it"));
        }
    }
    d.push_str(&format!(
        "; UK Chapter 1 (1982) para 1.51 sets the horizontal axis at {UK_ORIENTATION_DEG:.0} deg to the left edge of carriageway on a straight rural road to avoid specular reflection (the sense of the 5 deg is in a figure that was not read)"
    ));
    vec![line("orientation", Status::Information, "Sign orientation", d, &[UK_CH1_ORIENTATION])]
}

fn overhead(ci: &CheckInput) -> Vec<CheckLine> {
    if !ci.inputs.overhead {
        return Vec::new();
    }
    let Some(h) = ci.lower_edge_above_eye_m else {
        return vec![line("overhead", Status::InputNeeded, "Overhead sign sight line", "no mounting height".to_string(), &[UK_CH7_OVERHEAD])];
    };
    if h <= 0.0 {
        return vec![line("overhead", Status::Information, "Overhead sign sight line", "the lower edge is not above the driver's eye".to_string(), &[UK_CH7_OVERHEAD])];
    }
    let d = h / OVERHEAD_SIGHT_LINE_DEG.to_radians().tan();
    vec![line(
        "overhead",
        Status::Information,
        "Overhead sign sight line",
        format!(
            "the last point at which a driver can read the lower part of the sign is {} before it (a sight line from the lower edge inclined {OVERHEAD_SIGHT_LINE_DEG:.0} deg below the horizontal reaches the eye at {h:.2} m below the edge); no part of the gantry or its luminaires should obstruct that line",
            m(d)
        ),
        &[UK_CH7_OVERHEAD],
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::SpeedBandMph;
    use crate::sweep::*;
    use afrobim_alignment::AlignmentBuilder;

    fn base<'a>(inputs: &'a VisibilityInputs, sweep: Option<&'a SweepResult>) -> CheckInput<'a> {
        CheckInput {
            inputs,
            group: SignGroup::Lettered,
            advance_direction: true,
            speed_kph: Some(80.467),
            sweep,
            no_sweep_reason: None,
            x_height_mm: None,
            words: None,
            lateral_s_m: None,
            direction: TravelDirection::UpStation,
            sign_station_m: Some(500.0),
            lower_edge_above_road_m: None,
            lower_edge_above_eye_m: None,
            near_edge_offset_m: None,
            sign_on_left: None,
            grade_in_travel_direction: None,
            face_turn_from_square_deg: None,
            has_scene: false,
        }
    }

    fn ids(lines: &[CheckLine]) -> Vec<(&str, Status)> {
        lines.iter().map(|l| (l.id.as_str(), l.status)).collect()
    }

    fn status_of(lines: &[CheckLine], id: &str) -> Status {
        lines.iter().find(|l| l.id == id).unwrap_or_else(|| panic!("no line {id} in {:?}", ids(lines))).status
    }

    /// An unobstructed sweep on a straight road: the whole face is visible from the whole sweep.
    fn clear_sweep(start: f64) -> SweepResult {
        let a = AlignmentBuilder::new("A", 0.0, 0.0, 0.0, 0.0).line(2000.0).build().unwrap();
        let approach = Approach { alignment: a, sign_station_m: 1000.0, direction: TravelDirection::UpStation, eye_offset_m: -1.75, fallback_road_z_m: 0.0 };
        let face = FaceGeometry { origin: [1000.0, 5.0, 1.8], rotation_deg: -90.0, width_m: 1.0, height_m: 1.0 };
        Sweeper::new(face, &approach, SweepParams { start_distance_m: start, step_m: 10.0, ..SweepParams::default() }).run()
    }

    /// The worked example of the official Chapter 2 extract, reproduced.
    ///
    /// Single carriageway 8 m wide (lane 4 m), 85th percentile 50 mph, sign 2 m wide, 1 m from
    /// the carriageway edge: S = 2 + 1 + 1 = 4 m; C = S x 5.7 = 22.8 m ("say 23"); R = 4 s x 22 m/s
    /// = 88 m; total 111 m; required x-height = (100/60) x 111 = 185 mm.
    ///
    /// Slips in the source, recorded: 50 mph is 22.35 m/s and the extract rounds it to 22, which
    /// makes R 1.6 % short (89.4 m, not 88 m); 5.7 is a rounding of cot 10 = 5.6713; and "22.8, say
    /// 23" rounds up while the speed was rounded down. With exact values (22.352 m/s,
    /// cot 10 = 5.6713) the same example gives 186.82 mm (see the assertions); the extract's 185 mm
    /// is 1 % lower, from those roundings.
    #[test]
    fn the_official_x_height_example_is_reproduced() {
        let s = 2.0 + 1.0 + 1.0;
        let c = reading_stop_distance_m(s, COT_10_AS_PRINTED_OFFICIAL);
        assert!((c - 22.8).abs() < 1e-9);
        let rt = reading_time_s(6);
        assert!((rt - 4.0).abs() < 1e-12);
        let r = reading_distance_m(rt, 22.0);
        assert!((r - 88.0).abs() < 1e-12);
        // The extract rounds C to 23 before adding.
        assert!((required_x_height_mm(r + 23.0) - 185.0).abs() < 1e-9);
        // Unrounded: 88 + 22.8 = 110.8 m gives 184.67 mm.
        assert!((required_x_height_mm(r + c) - 184.666_666).abs() < 1e-3);
        // With exact speed (50 mph) and exact cot 10:
        let v = 50.0 * KPH_PER_MPH / 3.6;
        assert!((v - 22.352).abs() < 1e-9);
        let exact = required_x_height_mm(reading_distance_m(rt, v) + reading_stop_distance_m(s, cot_reading_stop_angle()));
        assert!((exact - 186.82).abs() < 0.01, "{exact}");
        // The other relation of the extract: 100 mm at 60 m, 50 mm at 30 m.
        assert!((legibility_distance_m(100.0) - 60.0).abs() < 1e-12 && (legibility_distance_m(50.0) - 30.0).abs() < 1e-12);
    }

    /// The iterative worked example of the variant copy (Chapter2-x-height.pdf).
    ///
    /// 2 m sign at x = 100 mm, 1 m from the edge, S = 2 + 1 + half width; width scales with x
    /// (20 mm of width per mm of x-height); C = S x 5.67; R = 4 x 22.352 = 89.41; x = (R + C) x 0.6^-1.
    /// The fixed point is x = (R + 5.67 x 3) / (0.6 - 5.67 x 0.01) = 195.88 mm, which is the
    /// document's final value. Iteration 1 gives 186.81 (document 186.82: it adds 89.41 and 22.69
    /// to give 112.09, which is 112.10).
    ///
    /// Slips recorded in the document: iteration 1 prints a total of 112.09 where 89.41 + 22.69 is
    /// 112.10; iteration 4 prints S = 4.96 but its next line multiplies 4.95, and prints a total
    /// of 117.52 where 89.41 + 28.12 is 117.53, so the 195.88 beside it belongs to 117.53 (117.52
    /// gives 195.87); and the final "sensibly rounded to 195 mm" rounds a requirement of 195.88
    /// down, where 196 mm would not be short of it.
        #[test]
    fn the_iterative_x_height_example_converges_to_the_documents_value() {
        let r = 4.0 * 50.0 * KPH_PER_MPH / 3.6;
        assert!((r - 89.408).abs() < 1e-9);
        let step = |x: f64| (r + 5.67 * (3.0 + x / 100.0)) / 0.6;
        let x1 = step(100.0);
        assert!((x1 - 186.81).abs() < 0.01, "{x1}");
        let mut x = 100.0;
        for _ in 0..40 {
            x = step(x);
        }
        assert!((x - 195.88).abs() < 0.01, "{x}");
        let fixed = (r + 5.67 * 3.0) / (0.6 - 5.67 * 0.01);
        assert!((fixed - x).abs() < 1e-6);
        // The document's own intermediate figures, recomputed.
        assert!((89.41f64 + 22.69 - 112.10).abs() < 1e-9); // printed 112.09
        assert!((4.96f64 * 5.67 - 28.12).abs() < 0.005); // printed with 4.95
        assert!((117.53f64 / 0.6 - 195.883).abs() < 0.001);
        assert!((117.52f64 / 0.6 - 195.867).abs() < 0.001); // printed 195.88
    }

    #[test]
    fn legibility_lines_use_the_source_relation() {
        // 80 km/h, N = 4 destinations: Rt = 3.33 s, R = 74.07 m; S = 5 m, C = 28.36 m; total 102.43 m, so 170.7 mm.
        let inputs = VisibilityInputs { standards: crate::inputs::Standards { uk: true, uganda: false }, ..VisibilityInputs::default() };
        let sweep = clear_sweep(300.0);
        let mut ci = base(&inputs, Some(&sweep));
        ci.speed_kph = Some(80.0);
        ci.words = Some(4);
        ci.lateral_s_m = Some(5.0);
        ci.x_height_mm = Some(200.0);
        let lines = run_checks(&ci);
        let rt = 2.0 + 4.0 / 3.0;
        let total = rt * 80.0 / 3.6 + 5.0 * cot_reading_stop_angle();
        let need = total / 0.6;
        assert!((need - 170.7).abs() < 0.1, "{need}");
        assert_eq!(status_of(&lines, "legibility.x_required"), Status::Ok);
        assert_eq!(status_of(&lines, "legibility.time"), Status::Ok);
        // 120 mm is readable from 72 m: time available (72 - 28.4) / 22.2 = 1.96 s < 3.33 s.
        ci.x_height_mm = Some(120.0);
        let lines = run_checks(&ci);
        assert_eq!(status_of(&lines, "legibility.x_required"), Status::Fails);
        assert_eq!(status_of(&lines, "legibility.time"), Status::Fails);
        let t = lines.iter().find(|l| l.id == "legibility.time").unwrap();
        assert!(t.detail.contains("1.9 s") || t.detail.contains("2.0 s"), "{}", t.detail);
        // Missing words: input needed.
        ci.words = None;
        assert_eq!(status_of(&run_checks(&ci), "legibility.reading"), Status::InputNeeded);
        // Seven destinations exceed the desirable six.
        ci.words = Some(7);
        assert_eq!(status_of(&run_checks(&ci), "legibility.words"), Status::Differs);
    }

    #[test]
    fn uk_tables_need_the_band_and_compare_with_the_sweep() {
        let sweep = clear_sweep(300.0);
        let mut inputs = VisibilityInputs { standards: crate::inputs::Standards { uk: true, uganda: false }, ..VisibilityInputs::default() };
        let mut ci = base(&inputs, Some(&sweep));
        ci.group = SignGroup::Regulatory;
        assert_eq!(status_of(&run_checks(&ci), "visibility.uk"), Status::InputNeeded);
        inputs.uk_band = Some(SpeedBandMph::From41To50);
        let mut ci = base(&inputs, Some(&sweep));
        ci.group = SignGroup::Regulatory;
        let lines = run_checks(&ci);
        assert_eq!(status_of(&lines, "visibility.uk"), Status::Ok);
        assert!(lines.iter().find(|l| l.id == "visibility.uk").unwrap().citation.contains("Table 1-1"));
        // A requirement longer than the sweep cannot be completed.
        let short = clear_sweep(50.0);
        let ci = CheckInput { sweep: Some(&short), group: SignGroup::Regulatory, ..base(&inputs, None) };
        assert_eq!(status_of(&run_checks(&ci), "visibility.uk"), Status::InputNeeded);
    }

    #[test]
    fn siting_of_a_warning_sign_follows_appendix_a() {
        let sweep = clear_sweep(300.0);
        let inputs = VisibilityInputs {
            uk_band: Some(SpeedBandMph::From41To50),
            hazard_station_m: Some(500.0 + 150.0),
            standards: crate::inputs::Standards { uk: true, uganda: true },
            ..VisibilityInputs::default()
        };
        let mut ci = base(&inputs, Some(&sweep));
        ci.group = SignGroup::Warning;
        ci.speed_kph = Some(80.0);
        // 150 m is inside 110 to 180 (UK) and above 100 m (Uganda, 80 km/h).
        let lines = run_checks(&ci);
        assert_eq!(status_of(&lines, "siting.uk"), Status::Ok);
        assert_eq!(status_of(&lines, "siting.ug"), Status::Ok);
        // 95 m is 14 % closer than 110: FAILS; 105 m is within the 10 % tolerance: DIFFERS.
        for (hazard, want) in [(595.0, Status::Fails), (605.0, Status::Differs), (800.0, Status::Differs)] {
            let inputs = VisibilityInputs { hazard_station_m: Some(hazard), ..inputs.clone() };
            let mut ci = base(&inputs, Some(&sweep));
            ci.group = SignGroup::Warning;
            ci.speed_kph = Some(80.0);
            assert_eq!(status_of(&run_checks(&ci), "siting.uk"), want, "hazard at {hazard}");
        }
        // Without the hazard station the line asks for it.
        let inputs = VisibilityInputs { hazard_station_m: None, ..inputs };
        let mut ci = base(&inputs, Some(&sweep));
        ci.group = SignGroup::Warning;
        assert_eq!(status_of(&run_checks(&ci), "siting.uk"), Status::InputNeeded);
        // A descent adds the information line of note 5.
        let inputs2 = VisibilityInputs { hazard_station_m: Some(650.0), ..inputs.clone() };
        let mut ci2 = base(&inputs2, Some(&sweep));
        ci2.group = SignGroup::Warning;
        ci2.grade_in_travel_direction = Some(-0.08);
        assert_eq!(status_of(&run_checks(&ci2), "siting.uk.gradient"), Status::Information);
    }

    #[test]
    fn mounting_and_clearance_lines() {
        let inputs = VisibilityInputs {
            road_edges: crate::inputs::RoadEdges { carriageway: crate::inputs::EdgeOffsets { left_m: Some(3.65), right_m: Some(3.65) }, ..Default::default() },
            ..VisibilityInputs::default()
        };
        let mut ci = base(&inputs, None);
        ci.lower_edge_above_road_m = Some(1.2);
        ci.near_edge_offset_m = Some(-4.2);
        ci.sign_on_left = Some(true);
        let lines = run_checks(&ci);
        assert_eq!(status_of(&lines, "mounting.uk"), Status::Ok);
        assert_eq!(status_of(&lines, "mounting.ug"), Status::Differs); // 1.2 m is not the normal 1.8 m
        // 4.2 - 3.65 = 0.55 m = 550 mm >= 450.
        assert_eq!(status_of(&lines, "clearance.uk"), Status::Ok);
        ci.near_edge_offset_m = Some(-3.9);
        assert_eq!(status_of(&run_checks(&ci), "clearance.uk"), Status::Fails); // 250 mm
        ci.lower_edge_above_road_m = Some(0.8);
        let lines = run_checks(&ci);
        assert_eq!(status_of(&lines, "mounting.ug"), Status::Fails); // below 1 m
        assert_eq!(status_of(&lines, "mounting.uk"), Status::Differs);
        // The Uganda clearance needs the shoulder and lane edges.
        assert_eq!(status_of(&lines, "clearance.ug"), Status::InputNeeded);
    }

    #[test]
    fn overhead_sight_line() {
        let inputs = VisibilityInputs { overhead: true, ..VisibilityInputs::default() };
        let mut ci = base(&inputs, None);
        ci.lower_edge_above_road_m = Some(5.2);
        ci.lower_edge_above_eye_m = Some(5.2 - 1.05);
        let l = run_checks(&ci);
        let o = l.iter().find(|l| l.id == "overhead").unwrap();
        // 4.15 / tan 10 deg = 23.5 m.
        assert!(o.detail.contains("23.5 m"), "{}", o.detail);
        assert_eq!(status_of(&l, "mounting.ug"), Status::Ok);
    }

    #[test]
    fn terminal_speed_limit_signs_use_table_14_3() {
        let inputs = VisibilityInputs {
            uk_rule: UkSignRule::SpeedLimitTerminal { approach_mph: 60, sign_mph: 30 },
            standards: crate::inputs::Standards { uk: true, uganda: false },
            ..VisibilityInputs::default()
        };
        // Table 14-3: 60 mph approach, 30 mph sign: 115 m. A sweep of 100 m reaching its start cannot decide.
        let short = clear_sweep(100.0);
        let ci = CheckInput { group: SignGroup::Regulatory, ..base(&inputs, Some(&short)) };
        let lines = run_checks(&ci);
        assert_eq!(status_of(&lines, "visibility.uk"), Status::InputNeeded);
        assert!(lines.iter().find(|l| l.id == "visibility.uk").unwrap().detail.contains("115.0 m"));
        let long = clear_sweep(300.0);
        let ci = CheckInput { group: SignGroup::Regulatory, ..base(&inputs, Some(&long)) };
        assert_eq!(status_of(&run_checks(&ci), "visibility.uk"), Status::Ok);
        // A pair the table leaves blank asks for the distance.
        let blank = VisibilityInputs { uk_rule: UkSignRule::SpeedLimitTerminal { approach_mph: 40, sign_mph: 40 }, ..inputs };
        let ci = CheckInput { group: SignGroup::Regulatory, ..base(&blank, Some(&long)) };
        assert_eq!(status_of(&run_checks(&ci), "visibility.uk"), Status::InputNeeded);
    }
}
