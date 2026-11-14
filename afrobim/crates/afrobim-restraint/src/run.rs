//! The definition of a barrier run: what the engineer enters.

use afrobim_kernel::{Note, Severity};
use serde::{Deserialize, Serialize};

use crate::{ContainmentLevel, ImpactSeverityLevel, RestraintError, TerminalClass, WorkingWidthClass};

/// Which side of the barrier line the traffic it restrains is on, looking up-station.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrafficFace {
    /// Traffic to the left of the barrier looking up-station (the usual case for a barrier
    /// right of the centreline).
    Left,
    /// Traffic to the right of the barrier looking up-station.
    Right,
    /// A double sided barrier, e.g. in a central reserve.
    Both,
}

impl TrafficFace {
    pub fn label(self) -> &'static str {
        match self {
            TrafficFace::Left => "left",
            TrafficFace::Right => "right",
            TrafficFace::Both => "both",
        }
    }
}

/// What a run connects to where it does not end in a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectedSystem {
    /// Another safety barrier run.
    SafetyBarrier,
    /// A vehicle parapet. A parapet is a type name only in this crate: it is not modelled or
    /// designed.
    Parapet,
    /// A structure (CD 377 7.2).
    Structure,
}

impl ConnectedSystem {
    pub fn label(self) -> &'static str {
        match self {
            ConnectedSystem::SafetyBarrier => "safety barrier",
            ConnectedSystem::Parapet => "parapet",
            ConnectedSystem::Structure => "structure",
        }
    }
}

/// A transition at the end of a run (CD 377 Section 7). All of it is entered product data.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Transition {
    /// The transition product or drawing, free text.
    #[serde(default)]
    pub description: String,
    /// Declared containment level of the transition, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub containment: Option<ContainmentLevel>,
    /// Declared impact severity level of the transition, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impact_severity: Option<ImpactSeverityLevel>,
    /// Containment level of the system the transition leads to, when known (CD 377 7.10 needs
    /// it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected_containment: Option<ContainmentLevel>,
}

/// How one end of a run is finished.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndTreatment {
    /// Not entered yet. The module raises a Warning.
    #[default]
    Unspecified,
    /// A terminal (CD 377 Section 6).
    Terminal {
        performance_class: TerminalClass,
        /// Whether the product is declared energy absorbing (CD 377 6.7, 6.8); `None` when not
        /// known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        energy_absorbing: Option<bool>,
        /// The terminal product, free text.
        #[serde(default)]
        description: String,
    },
    /// The run carries on into another system, with or without a transition.
    ///
    /// Enter a transition between two runs on **one** of them only; on the other, connect
    /// without a transition. Otherwise the take-off counts it twice.
    ConnectsTo {
        system: ConnectedSystem,
        /// The mark of the run, parapet or structure connected to.
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transition: Option<Transition>,
    },
}

impl EndTreatment {
    /// A terminal of a class CD 377 names, with nothing else stated.
    pub fn terminal(performance_class: TerminalClass) -> EndTreatment {
        EndTreatment::Terminal { performance_class, energy_absorbing: None, description: String::new() }
    }

    /// One line for schedules: `terminal T110`, `transition to parapet P1 (N2)`,
    /// `connects to safety barrier VRS-002`, `unspecified`.
    pub fn label(&self) -> String {
        match self {
            EndTreatment::Unspecified => "unspecified".into(),
            EndTreatment::Terminal { performance_class, .. } => format!("terminal {}", performance_class.label()),
            EndTreatment::ConnectsTo { system, name, transition: None } => format!("connects to {} {}", system.label(), name.trim()),
            EndTreatment::ConnectsTo { system, name, transition: Some(t) } => {
                let level = t.containment.map(|c| format!(" ({c})")).unwrap_or_default();
                format!("transition to {} {}{level}", system.label(), name.trim())
            }
        }
    }
}

/// A safety barrier run along an alignment.
///
/// Lengths are metres, as in `afrobim-alignment`. Stations are **internal** (continuous)
/// stations of the alignment; the offset is signed as the alignment crate signs it, positive to
/// the right looking up-station, and is the offset of the barrier's **traffic face** (the face
/// set-back is measured to, CD 127 2.25). For a double sided barrier it is the barrier's centre
/// line.
///
/// `start_station_m` and `end_station_m` are the two ends of the barrier itself (points A and F
/// of CD 377 Figure 3.24). Terminals are counted as items and are not part of the length.
///
/// Containment level, working width class and impact severity level are the declared classes
/// of the product. Post spacing and beam height are **entered product data**: no standard value
/// is supplied for either.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarrierRun {
    /// The mark the run is scheduled under, e.g. `VRS-001`.
    pub name: String,
    /// Name of the alignment the run follows.
    pub alignment: String,
    pub start_station_m: f64,
    pub end_station_m: f64,
    pub offset_m: f64,
    pub traffic_face: TrafficFace,
    pub containment: ContainmentLevel,
    pub working_width: WorkingWidthClass,
    pub impact_severity: ImpactSeverityLevel,
    /// System description or product, free text; never interpreted.
    #[serde(default)]
    pub system: String,
    /// Post spacing measured along the barrier, m. `None` for a system without posts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_spacing_m: Option<f64>,
    /// Height of the centre of the beam above the ground at the barrier, m. For a system
    /// without posts the schematic is drawn from the ground to this height.
    pub beam_height_m: f64,
    /// The end at `start_station_m`.
    #[serde(default)]
    pub start_end: EndTreatment,
    /// The end at `end_station_m`.
    #[serde(default)]
    pub end_end: EndTreatment,
    /// Set-back of the traffic face from the point of set-back (CD 127 2.25), m, as measured by
    /// the engineer. The crate cannot derive it: the alignment does not say where the edge of
    /// the paved carriageway is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_back_m: Option<f64>,
}

fn note(severity: Severity, code: &str, message: String) -> Note {
    Note { severity, code: code.to_string(), message }
}

impl BarrierRun {
    /// Validation notes. Errors mean the run cannot be drawn or measured. None of them is a
    /// judgement of adequacy: that is the [`crate::design`] module and, above all, the engineer.
    pub fn notes(&self) -> Vec<Note> {
        let mut out = Vec::new();
        let mut error = |code: &str, message: String| out.push(note(Severity::Error, code, message));
        if !(self.start_station_m.is_finite() && self.end_station_m.is_finite()) {
            error("RESTRAINT_STATION_NOT_A_NUMBER", "The start and end stations must be numbers.".into());
        } else if self.end_station_m <= self.start_station_m {
            error(
                "RESTRAINT_END_NOT_AFTER_START",
                format!("The end station ({} m) must be greater than the start station ({} m).", self.end_station_m, self.start_station_m),
            );
        }
        if !self.offset_m.is_finite() {
            error("RESTRAINT_OFFSET_NOT_A_NUMBER", "The offset must be a number.".into());
        }
        if let Some(s) = self.post_spacing_m {
            if !(s.is_finite() && s > 0.0) {
                error("RESTRAINT_POST_SPACING_NOT_POSITIVE", format!("The post spacing must be greater than zero (got {s} m)."));
            }
        }
        if !(self.beam_height_m.is_finite() && self.beam_height_m > 0.0) {
            error("RESTRAINT_BEAM_HEIGHT_NOT_POSITIVE", format!("The beam height must be greater than zero (got {} m).", self.beam_height_m));
        }
        if let Some(s) = self.set_back_m {
            if !(s.is_finite() && s >= 0.0) {
                error("RESTRAINT_SET_BACK_NEGATIVE", format!("The set-back must not be negative (got {s} m)."));
            }
        }
        if self.alignment.trim().is_empty() {
            error("RESTRAINT_NO_ALIGNMENT", "The run names no alignment.".into());
        }
        for (which, end) in [("start", &self.start_end), ("end", &self.end_end)] {
            match end {
                EndTreatment::Unspecified => out.push(note(
                    Severity::Warning,
                    "RESTRAINT_END_UNSPECIFIED",
                    format!("The {which} of the run has no terminal and no connection entered."),
                )),
                EndTreatment::ConnectsTo { name, .. } if name.trim().is_empty() => out.push(note(
                    Severity::Warning,
                    "RESTRAINT_CONNECTION_UNNAMED",
                    format!("The {which} of the run connects to a system that is not named."),
                )),
                EndTreatment::Terminal { performance_class: TerminalClass::Other(s), .. } if s.trim().is_empty() => out.push(note(
                    Severity::Warning,
                    "RESTRAINT_TERMINAL_CLASS_EMPTY",
                    format!("The terminal at the {which} of the run has no performance class."),
                )),
                _ => {}
            }
        }
        if self.system.trim().is_empty() {
            out.push(note(Severity::Info, "RESTRAINT_NO_SYSTEM", "No system description or product is entered.".into()));
        }
        out
    }

    /// `Err` with the Error notes joined when there are any.
    pub fn check(&self) -> Result<(), RestraintError> {
        let errors: Vec<String> = self.notes().into_iter().filter(|n| n.severity == Severity::Error).map(|n| n.message).collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(RestraintError::InvalidRun(errors.join(" ")))
        }
    }
}

#[cfg(test)]
pub(crate) fn sample_run() -> BarrierRun {
    BarrierRun {
        name: "VRS-001".into(),
        alignment: "A1".into(),
        start_station_m: 100.0,
        end_station_m: 200.0,
        offset_m: 5.0,
        traffic_face: TrafficFace::Left,
        containment: ContainmentLevel::N2,
        working_width: WorkingWidthClass::W4,
        impact_severity: ImpactSeverityLevel::A,
        system: "as specified".into(),
        post_spacing_m: Some(2.0),
        beam_height_m: 0.6,
        start_end: EndTreatment::terminal(TerminalClass::T110),
        end_end: EndTreatment::terminal(TerminalClass::T80_3),
        set_back_m: Some(1.2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(run: &BarrierRun, severity: Severity) -> Vec<String> {
        run.notes().into_iter().filter(|n| n.severity == severity).map(|n| n.code).collect()
    }

    #[test]
    fn a_complete_run_has_no_notes() {
        assert!(sample_run().notes().is_empty());
        assert!(sample_run().check().is_ok());
    }

    #[test]
    fn invalid_values_are_errors() {
        let mut r = sample_run();
        r.end_station_m = 100.0;
        assert_eq!(codes(&r, Severity::Error), ["RESTRAINT_END_NOT_AFTER_START"]);
        let mut r = sample_run();
        r.start_station_m = f64::NAN;
        assert_eq!(codes(&r, Severity::Error), ["RESTRAINT_STATION_NOT_A_NUMBER"]);
        let mut r = sample_run();
        r.offset_m = f64::INFINITY;
        r.post_spacing_m = Some(0.0);
        r.beam_height_m = -0.1;
        r.set_back_m = Some(-1.0);
        r.alignment = " ".into();
        assert_eq!(
            codes(&r, Severity::Error),
            [
                "RESTRAINT_OFFSET_NOT_A_NUMBER",
                "RESTRAINT_POST_SPACING_NOT_POSITIVE",
                "RESTRAINT_BEAM_HEIGHT_NOT_POSITIVE",
                "RESTRAINT_SET_BACK_NEGATIVE",
                "RESTRAINT_NO_ALIGNMENT"
            ]
        );
        assert!(r.check().is_err());
    }

    #[test]
    fn missing_end_treatments_and_text_are_warnings_and_info() {
        let mut r = sample_run();
        r.start_end = EndTreatment::Unspecified;
        r.end_end = EndTreatment::ConnectsTo { system: ConnectedSystem::Parapet, name: "".into(), transition: None };
        r.system = String::new();
        assert_eq!(codes(&r, Severity::Warning), ["RESTRAINT_END_UNSPECIFIED", "RESTRAINT_CONNECTION_UNNAMED"]);
        assert_eq!(codes(&r, Severity::Info), ["RESTRAINT_NO_SYSTEM"]);
        assert!(r.check().is_ok());
    }

    #[test]
    fn end_labels() {
        assert_eq!(EndTreatment::Unspecified.label(), "unspecified");
        assert_eq!(EndTreatment::terminal(TerminalClass::T110_1).label(), "terminal T110/1");
        let plain = EndTreatment::ConnectsTo { system: ConnectedSystem::SafetyBarrier, name: "VRS-002".into(), transition: None };
        assert_eq!(plain.label(), "connects to safety barrier VRS-002");
        let t = Transition { containment: Some(ContainmentLevel::N2), ..Default::default() };
        let with = EndTreatment::ConnectsTo { system: ConnectedSystem::Parapet, name: "P1".into(), transition: Some(t) };
        assert_eq!(with.label(), "transition to parapet P1 (N2)");
    }

    #[test]
    fn serde_round_trip_and_defaults() {
        let mut r = sample_run();
        r.end_end = EndTreatment::ConnectsTo {
            system: ConnectedSystem::Parapet,
            name: "BR-1 north parapet".into(),
            transition: Some(Transition {
                description: "as specified".into(),
                containment: Some(ContainmentLevel::N2),
                impact_severity: Some(ImpactSeverityLevel::B),
                connected_containment: Some(ContainmentLevel::H2),
            }),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(serde_json::from_str::<BarrierRun>(&json).unwrap(), r);
        // The optional parts may be left out of a file.
        let minimal = r#"{"name":"V","alignment":"A","start_station_m":0,"end_station_m":10,"offset_m":3,
            "traffic_face":"left","containment":"N2","working_width":"W2","impact_severity":"A","beam_height_m":0.6}"#;
        let m: BarrierRun = serde_json::from_str(minimal).unwrap();
        assert_eq!(m.post_spacing_m, None);
        assert_eq!(m.start_end, EndTreatment::Unspecified);
        assert_eq!(m.set_back_m, None);
    }
}
