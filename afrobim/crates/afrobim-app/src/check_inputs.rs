//! The inputs a check needs, kept on the element they were entered for.
//!
//! A check needs what only the engineer knows: the wind basis of a sign, a design flow, a speed
//! limit, traffic, the photometric results of a lighting calculation. The single-element CHECK
//! dialogs save what was entered as a **property of the element**, and the model-wide run
//! (`CHECKALL`) reads the same properties, so the whole model can be checked again with the same
//! inputs and a changed element shows its new result. Properties are saved with the document, so
//! the inputs survive save and open.
//!
//! The keys are documented here and are hidden from the generic property editor (every key that
//! starts with [`PREFIX`]). The value of each is JSON text of the type named beside it.
//!
//! | Key | On | Value |
//! |---|---|---|
//! | `check.support_design` | a sign | `supports_glue::StoredSupportDesign`: the DESIGN dialog's entries |
//! | `check.drain` | a side drain | [`DrainInputs`]: design flow (entered or from the rational method), basis, freeboard |
//! | `check.culvert` | a pipe culvert | [`CulvertInputs`]: flow, Manning's n, entrance loss, tailwater, basis, road level |
//! | `check.rational` | a drain or a culvert | [`RationalInputs`]: the catchment of the rational method |
//! | `check.restraint_speed` | a barrier run | `road_glue::SpeedInput`: the speed limit and its unit |
//! | `check.lighting_inputs` | a lighting element | `lighting_inputs::Form`: every entry of the lighting check dialog |
//! | `check.pavement_design` | a pavement | `road_glue::DesignForm`: the method and its inputs |
//!
//! Nothing here holds an engineering value. A text field that is empty is "not entered", and the
//! parsers say which field is missing.

use afrobim_drainage::design::mowt::{self, RationalInput, RationalResult, SubCatchment};
use afrobim_kernel::{Document, ElementId};
use serde::{Deserialize, Serialize};

pub const PREFIX: &str = "check.";
pub const KEY_SUPPORT: &str = "check.support_design";
pub const KEY_DRAIN: &str = "check.drain";
pub const KEY_CULVERT: &str = "check.culvert";
pub const KEY_RATIONAL: &str = "check.rational";
pub const KEY_RESTRAINT: &str = "check.restraint_speed";
pub const KEY_LIGHTING: &str = "check.lighting_inputs";
pub const KEY_PAVEMENT: &str = "check.pavement_design";

/// True for a property key this module owns (hidden from the generic editor).
pub fn is_check_key(key: &str) -> bool {
    key.starts_with(PREFIX)
}

/// The inputs stored under `key` on an element, when there are some and they can be read.
pub fn load<T: serde::de::DeserializeOwned>(doc: &Document, id: ElementId, key: &str) -> Option<T> {
    doc.find(id)?.properties.get(key).and_then(|text| serde_json::from_str(text).ok())
}

/// Stores inputs on an element. False when the element is gone.
pub fn save<T: Serialize>(doc: &mut Document, id: ElementId, key: &str, value: &T) -> bool {
    match (doc.find_mut(id), serde_json::to_string(value)) {
        (Some(e), Ok(text)) => {
            e.properties.insert(key.to_string(), text);
            true
        }
        _ => false,
    }
}

// ─── parsing text entries ───────────────────────────────────────────────────────────────

/// A number typed by the user. Empty is `Ok(None)`: not entered.
pub fn number(label: &str, text: &str) -> Result<Option<f64>, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    t.parse::<f64>().ok().filter(|v| v.is_finite()).map(Some).ok_or_else(|| format!("{label}: '{t}' is not a number"))
}

/// A number the check cannot go without.
pub fn required(label: &str, text: &str) -> Result<f64, String> {
    number(label, text)?.ok_or_else(|| format!("enter {label}"))
}

/// A number above zero the check cannot go without.
pub fn required_positive(label: &str, text: &str) -> Result<f64, String> {
    let v = required(label, text)?;
    if v > 0.0 {
        Ok(v)
    } else {
        Err(format!("{label} must be above zero (it is {v})"))
    }
}

// ─── the rational method ────────────────────────────────────────────────────────────────

/// One part of the catchment as typed: what it is, its area in hectares and its runoff coefficient.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CatchmentRow {
    pub description: String,
    pub area_ha: String,
    pub runoff_coefficient: String,
}

/// The entries of the rational method dialog, as typed. Every field starts empty.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RationalInputs {
    pub rows: Vec<CatchmentRow>,
    /// Design rainfall intensity for a duration equal to the time of concentration, mm/h.
    pub intensity_mm_h: String,
    /// Where the intensity comes from, quoted in the calculation.
    pub intensity_source: String,
    /// Multiplier for a higher recurrence interval; empty means none is applied.
    pub frequency_multiplier: String,
    /// Storage factor Cs of the modified rational method; empty means the basic method.
    pub storage_factor: String,
}

impl RationalInputs {
    /// The engine's input, or the first entry that is missing or cannot be read.
    pub fn input(&self) -> Result<RationalInput, String> {
        let mut parts = Vec::new();
        for (i, r) in self.rows.iter().enumerate() {
            let n = i + 1;
            if r.description.trim().is_empty() && r.area_ha.trim().is_empty() && r.runoff_coefficient.trim().is_empty() {
                continue;
            }
            parts.push(SubCatchment {
                description: if r.description.trim().is_empty() { format!("Area {n}") } else { r.description.trim().to_string() },
                area_ha: required(&format!("the area of part {n} (ha)"), &r.area_ha)?,
                runoff_coefficient: required(&format!("the runoff coefficient of part {n}"), &r.runoff_coefficient)?,
            });
        }
        if parts.is_empty() {
            return Err("enter at least one part of the catchment".into());
        }
        Ok(RationalInput {
            sub_catchments: parts,
            intensity_mm_h: required("the design rainfall intensity (mm/h)", &self.intensity_mm_h)?,
            intensity_source: if self.intensity_source.trim().is_empty() { "entered, source not stated".to_string() } else { self.intensity_source.trim().to_string() },
            // An empty entry is "no multiplier", the plain method: a stated absence, not a value.
            frequency_multiplier: number("the recurrence multiplier", &self.frequency_multiplier)?.unwrap_or(1.0),
            storage_factor: number("the storage factor", &self.storage_factor)?,
        })
    }

    /// The rational method on the entries.
    pub fn run(&self) -> Result<RationalResult, String> {
        mowt::rational_method(&self.input()?).map_err(|e| e.to_string())
    }
}

// ─── side drain ─────────────────────────────────────────────────────────────────────────

/// The entries of the side drain check, as typed.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DrainInputs {
    /// Design flow, m³/s, when it is entered directly.
    pub flow_m3_s: String,
    /// Take the design flow from the rational method inputs kept on the same element.
    pub flow_from_rational: bool,
    /// UK DMRB instead of the MoWT manual.
    pub dmrb: bool,
    /// Required freeboard, m, if the project states one. Neither basis does.
    pub required_freeboard_m: String,
}

/// What a drain check runs with.
#[derive(Debug, Clone, PartialEq)]
pub struct DrainParams {
    pub flow_m3_s: f64,
    pub dmrb: bool,
    pub freeboard_m: Option<f64>,
    /// The rational method's lines when the flow came from it.
    pub rational_lines: Vec<String>,
}

impl DrainInputs {
    pub fn params(&self, rational: Option<&RationalInputs>) -> Result<DrainParams, String> {
        let (flow, lines) = flow_of(self.flow_from_rational, &self.flow_m3_s, rational)?;
        Ok(DrainParams { flow_m3_s: flow, dmrb: self.dmrb, freeboard_m: number("the required freeboard (m)", &self.required_freeboard_m)?, rational_lines: lines })
    }
}

/// The design flow: from the rational method when asked, else as typed.
fn flow_of(from_rational: bool, typed: &str, rational: Option<&RationalInputs>) -> Result<(f64, Vec<String>), String> {
    if from_rational {
        let r = rational.ok_or("the design flow is to come from the rational method, which has no inputs on this element: open RATIONAL")?;
        let result = r.run()?;
        return Ok((result.peak_flow_m3_s, result.calculation()));
    }
    Ok((required_positive("the design flow (m3/s)", typed)?, Vec::new()))
}

// ─── pipe culvert ───────────────────────────────────────────────────────────────────────

/// The entries of the culvert checks, as typed.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CulvertInputs {
    pub flow_m3_s: String,
    pub flow_from_rational: bool,
    /// Manning's n of the barrel.
    pub manning_n: String,
    /// Entrance loss coefficient Ke, and where it comes from (the DDM Appendix 9.6 row, or another source).
    pub entrance_ke: String,
    pub entrance_note: String,
    /// Tailwater depth above the outlet invert, m.
    pub tailwater_m: String,
    /// UK DMRB layout limits instead of the MoWT ones (the hydraulics are MoWT only).
    pub dmrb: bool,
    /// Level of the road shoulder break point at the low point of the road, m, for the freeboard.
    pub shoulder_level_m: String,
    /// The road is a major road (true) or a minor road (false), to pick the freeboard of DDM 9.6.
    pub major_road: Option<bool>,
}

/// What the outlet control calculation runs with.
#[derive(Debug, Clone, PartialEq)]
pub struct CulvertParams {
    pub flow_m3_s: f64,
    pub manning_n: f64,
    pub entrance_ke: f64,
    pub tailwater_m: f64,
    pub shoulder_level_m: Option<f64>,
    pub major_road: Option<bool>,
    pub rational_lines: Vec<String>,
}

impl CulvertInputs {
    pub fn params(&self, rational: Option<&RationalInputs>) -> Result<CulvertParams, String> {
        let (flow, lines) = flow_of(self.flow_from_rational, &self.flow_m3_s, rational)?;
        let ke = required("the entrance loss coefficient Ke", &self.entrance_ke)?;
        if ke < 0.0 {
            return Err(format!("the entrance loss coefficient Ke must not be negative (it is {ke})"));
        }
        let tailwater = required("the tailwater depth (m)", &self.tailwater_m)?;
        if tailwater < 0.0 {
            return Err(format!("the tailwater depth must not be negative (it is {tailwater})"));
        }
        let shoulder_level_m = number("the shoulder break level (m)", &self.shoulder_level_m)?;
        if shoulder_level_m.is_some() && self.major_road.is_none() {
            return Err("say whether the road is a major or a minor road, for the freeboard".into());
        }
        Ok(CulvertParams {
            flow_m3_s: flow,
            manning_n: required_positive("Manning's n of the barrel", &self.manning_n)?,
            entrance_ke: ke,
            tailwater_m: tailwater,
            shoulder_level_m,
            major_road: self.major_road,
            rational_lines: lines,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_kernel::Pt;

    #[test]
    fn an_empty_entry_is_not_entered_and_the_parsers_say_which() {
        assert_eq!(number("x", "  ").unwrap(), None);
        assert_eq!(number("x", "2.5").unwrap(), Some(2.5));
        assert!(number("the flow", "abc").unwrap_err().contains("the flow"));
        assert!(required("the flow", "").unwrap_err().contains("enter the flow"));
        assert!(required_positive("the flow", "0").is_err());
        assert!(DrainInputs::default().params(None).unwrap_err().contains("design flow"));
        let culvert = CulvertInputs { flow_m3_s: "1".into(), ..Default::default() };
        assert!(culvert.params(None).unwrap_err().contains("Ke"));
    }

    #[test]
    fn inputs_round_trip_on_an_element_and_through_save_and_open() {
        let mut doc = Document::default();
        let id = doc.add("drain", "SD-1", Pt::new(0.0, 0.0), 10.0, 10.0, "Drains", serde_json::Value::Null);
        let inputs = DrainInputs { flow_m3_s: "0.35".into(), flow_from_rational: false, dmrb: true, required_freeboard_m: "0.1".into() };
        assert!(load::<DrainInputs>(&doc, id, KEY_DRAIN).is_none());
        assert!(save(&mut doc, id, KEY_DRAIN, &inputs));
        let reopened = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(load::<DrainInputs>(&reopened, id, KEY_DRAIN), Some(inputs));
        assert!(is_check_key(KEY_DRAIN) && !is_check_key("Material"));
    }

    #[test]
    fn the_rational_method_runs_on_entered_text() {
        let mut r = RationalInputs::default();
        assert!(r.run().unwrap_err().contains("at least one part"));
        // Arbitrary test values: 10 ha at C 0.5, 50 mm/h. Q = C i A / 360 = 0.5 * 50 * 10 / 360.
        r.rows.push(CatchmentRow { description: "test".into(), area_ha: "10".into(), runoff_coefficient: "0.5".into() });
        assert!(r.run().unwrap_err().contains("intensity"));
        r.intensity_mm_h = "50".into();
        let result = r.run().unwrap();
        assert!((result.peak_flow_m3_s - 0.5 * 50.0 * 10.0 / 360.0).abs() < 1e-12);
        // A drain takes its flow from the rational inputs when asked.
        let drain = DrainInputs { flow_from_rational: true, ..Default::default() };
        let p = drain.params(Some(&r)).unwrap();
        assert!((p.flow_m3_s - result.peak_flow_m3_s).abs() < 1e-12 && !p.rational_lines.is_empty());
        assert!(drain.params(None).unwrap_err().contains("RATIONAL"));
    }
}
