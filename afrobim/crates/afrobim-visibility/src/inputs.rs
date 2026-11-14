//! The inputs of a visibility check, kept on the sign they were entered for.
//!
//! They are stored as JSON text in the sign's property set under [`KEY`] (`check.visibility`).
//! The `check.` prefix is the convention of `afrobim-app` (`check_inputs::PREFIX`): every key that
//! starts with it is hidden from the generic property editor and read by the model-wide check.
//!
//! Nothing in [`VisibilityInputs`] is an engineering value of a standard except
//! [`SweepParams::eye_height_m`], whose default is the 1.05 m that two held sources state for other
//! criteria (see `sources::EYE_HEIGHT_M`); the engineer confirms it. Everything else is either an
//! analysis setting or a decision the engineer takes: the lane the driver is in, the UK speed
//! band a limit belongs to, the speed, the hazard station.

use afrobim_alignment::TrafficSide;
use afrobim_kernel::{Document, ElementId};
use serde::{Deserialize, Serialize};

use crate::sources::{SpeedBandMph, UkClearanceCase};
use crate::sweep::{LateralObstruction, SweepParams, TravelDirection};

/// The property key the inputs of one sign are stored under, as JSON text.
pub const KEY: &str = "check.visibility";

/// The property-key prefix the app hides from generic editing.
pub const CHECK_PREFIX: &str = "check.";

/// True for a key that the app hides from the generic property editor.
pub fn is_hidden_key(key: &str) -> bool {
    key.starts_with(CHECK_PREFIX)
}

/// How the driver's lateral position is entered.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EyeOffset {
    /// Signed offset from the centreline, m, positive to the right looking up-station.
    Signed(f64),
    /// Distance from the centreline to the centre of the driving lane, m (positive). The side
    /// comes from the direction of travel and the side the traffic keeps to: with traffic on the
    /// left, up-station traffic is left of the centreline (negative offset).
    LaneCentre(f64),
}

impl EyeOffset {
    /// The signed offset for a direction of travel and a traffic side.
    pub fn resolve(self, direction: TravelDirection, traffic: TrafficSide) -> f64 {
        match self {
            EyeOffset::Signed(o) => o,
            EyeOffset::LaneCentre(m) => {
                let up_is_left = traffic == TrafficSide::Left;
                let on_left = (direction == TravelDirection::UpStation) == up_is_left;
                if on_left {
                    -m.abs()
                } else {
                    m.abs()
                }
            }
        }
    }
}

/// Which provision of the UK manuals applies to the sign. `Auto` takes it from the sign's class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UkSignRule {
    #[default]
    Auto,
    /// Chapter 3, Table 1-1.
    Regulatory,
    /// Chapter 4, Appendix A.
    Warning,
    /// Chapter 3, Table 14-3 (terminal speed limit signs), by the approach speed limit and the
    /// speed limit on the sign, mph.
    SpeedLimitTerminal { approach_mph: u32, sign_mph: u32 },
    /// A lettered guidance or information sign: the x-height method of the Chapter 2 extract.
    Directional,
    /// No UK provision is applied (for example a supplementary plate read with its parent).
    NotApplicable,
}

/// Distances from the centreline to an edge of the road on each side, m (positive).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct EdgeOffsets {
    pub left_m: Option<f64>,
    pub right_m: Option<f64>,
}

impl EdgeOffsets {
    pub fn on(&self, left: bool) -> Option<f64> {
        if left {
            self.left_m
        } else {
            self.right_m
        }
    }
}

/// The edges of the road at the sign, for the clearance and set-back checks.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct RoadEdges {
    /// The edge of the carriageway (UK) or the kerb face on a kerbed road (Uganda).
    pub carriageway: EdgeOffsets,
    /// The edge of the traffic lane (Uganda).
    pub traffic_lane: EdgeOffsets,
    /// The edge of the road shoulder (Uganda).
    pub shoulder: EdgeOffsets,
    /// The road is kerbed at the sign (Uganda D.4.3.2 then asks for 0.3 m from the kerb face).
    pub kerbed: bool,
}

/// Which manuals' provisions are checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Standards {
    pub uk: bool,
    pub uganda: bool,
}

impl Default for Standards {
    fn default() -> Self {
        Standards { uk: true, uganda: true }
    }
}

/// Everything entered for the visibility check of one sign. Every field has a default, so a
/// partly filled record loads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VisibilityInputs {
    /// Settings of the sweep, including the driver's eye height.
    pub sweep: SweepParams,
    /// The driver's lateral position. `None` is "not entered": the sweep cannot run.
    pub eye_offset: Option<EyeOffset>,
    /// Direction of travel; `None` takes it from the face's rotation (the traffic it serves).
    pub direction: Option<TravelDirection>,
    /// Which side the traffic keeps to.
    pub traffic_side: TrafficSide,
    /// Approach speed, km/h (the 85th percentile speed in the Uganda manual's sense); `None`
    /// takes the speed class stored on the sign, if any.
    pub approach_speed_kph: Option<f64>,
    /// The UK speed band the speed belongs to. The engineer's decision; never derived here.
    pub uk_band: Option<SpeedBandMph>,
    /// The larger sign/visibility figures of Table 1-1 and Appendix A for 70 mph dual carriageways
    /// and motorways (para 1.17, Appendix A note 2).
    pub uk_large_case: bool,
    pub uk_rule: UkSignRule,
    pub uk_clearance_case: UkClearanceCase,
    /// Height of the lower edge above the carriageway, m; `None` takes the mounting height
    /// recorded on the sign's first post.
    pub mounting_height_m: Option<f64>,
    /// Is the sign over or alongside a footway (changes the headroom provisions)?
    pub over_footway: bool,
    /// An overhead sign (gantry or mast arm).
    pub overhead: bool,
    /// x-height of the legend, mm; `None` takes it from the sign module's derived value if the
    /// caller provides one.
    pub x_height_mm: Option<f64>,
    /// Words or destinations on the sign (N of the reading time); `None` counts the destinations
    /// of the sign definition.
    pub words: Option<u32>,
    /// Reading time, s, overriding the source's 2 + N / 3.
    pub reading_time_s: Option<f64>,
    /// Internal station of the hazard or junction the sign refers to. No sign records it today.
    pub hazard_station_m: Option<f64>,
    /// Walls or banks beside the road beyond which nothing can be seen (the inside of a curve).
    pub lateral: Vec<LateralObstruction>,
    pub road_edges: RoadEdges,
    pub standards: Standards,
}

impl Default for VisibilityInputs {
    fn default() -> Self {
        VisibilityInputs {
            sweep: SweepParams::default(),
            eye_offset: None,
            direction: None,
            traffic_side: TrafficSide::Left,
            approach_speed_kph: None,
            uk_band: None,
            uk_large_case: false,
            uk_rule: UkSignRule::Auto,
            uk_clearance_case: UkClearanceCase::Normal,
            mounting_height_m: None,
            over_footway: false,
            overhead: false,
            x_height_mm: None,
            words: None,
            reading_time_s: None,
            hazard_station_m: None,
            lateral: Vec::new(),
            road_edges: RoadEdges::default(),
            standards: Standards::default(),
        }
    }
}

/// The inputs stored on an element, when there are some and they can be read.
pub fn load(doc: &Document, id: ElementId) -> Option<VisibilityInputs> {
    doc.find(id)?.properties.get(KEY).and_then(|text| serde_json::from_str(text).ok())
}

/// Stores the inputs on an element under [`KEY`]. False when the element is gone.
pub fn save(doc: &mut Document, id: ElementId, inputs: &VisibilityInputs) -> bool {
    match (doc.find_mut(id), serde_json::to_string(inputs)) {
        (Some(e), Ok(text)) => {
            e.properties.insert(KEY.to_string(), text);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_side_follows_the_traffic_side_and_direction() {
        let lane = EyeOffset::LaneCentre(1.75);
        assert_eq!(lane.resolve(TravelDirection::UpStation, TrafficSide::Left), -1.75);
        assert_eq!(lane.resolve(TravelDirection::DownStation, TrafficSide::Left), 1.75);
        assert_eq!(lane.resolve(TravelDirection::UpStation, TrafficSide::Right), 1.75);
        assert_eq!(EyeOffset::Signed(-3.0).resolve(TravelDirection::DownStation, TrafficSide::Left), -3.0);
    }

    #[test]
    fn a_partial_record_loads_with_defaults_and_the_key_is_hidden() {
        let i: VisibilityInputs = serde_json::from_str(r#"{"approach_speed_kph":80.0,"uk_band":"From41To50"}"#).unwrap();
        assert_eq!(i.approach_speed_kph, Some(80.0));
        assert_eq!(i.uk_band, Some(SpeedBandMph::From41To50));
        assert_eq!(i.sweep.step_m, SweepParams::default().step_m);
        assert!(is_hidden_key(KEY));
        assert_eq!(KEY, "check.visibility");
    }
}
