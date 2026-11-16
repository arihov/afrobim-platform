//! The rule set. Every rule is an input; `RuleSet::default()` has every rule off. No value is
//! built in: minimum cover, slopes, velocities and the rest come from the project's standard,
//! which the caller names in `name`.

use serde::{Deserialize, Serialize};

use crate::model::{Pipe, PipeShape};

/// Which level of two pipes is matched at a structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    Invert,
    Crown,
    Centreline,
}

impl MatchMode {
    /// The matched level of a pipe end whose invert is `invert_m`, m.
    pub fn level_m(self, shape: &PipeShape, invert_m: f64) -> f64 {
        match self {
            MatchMode::Invert => invert_m,
            MatchMode::Crown => invert_m + shape.height_m(),
            MatchMode::Centreline => invert_m + shape.height_m() / 2.0,
        }
    }

    /// The invert that gives a matched level `level_m`.
    pub fn invert_for_level_m(self, shape: &PipeShape, level_m: f64) -> f64 {
        match self {
            MatchMode::Invert => level_m,
            MatchMode::Crown => level_m - shape.height_m(),
            MatchMode::Centreline => level_m - shape.height_m() / 2.0,
        }
    }

    pub fn pipe_end_level_m(self, pipe: &Pipe, at_start: bool) -> f64 {
        self.level_m(&pipe.shape, if at_start { pipe.start_invert_m } else { pipe.end_invert_m })
    }
}

/// Design rules. `None` switches a rule off.
///
/// Cover is measured from the ground to the **outside top of the barrel** (the crown plus the
/// wall thickness; a pipe with no wall thickness set is taken as having none).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RuleSet {
    /// The project standard these inputs come from, free text.
    pub name: String,
    pub min_slope: Option<f64>,
    pub max_slope: Option<f64>,
    pub min_cover_m: Option<f64>,
    pub max_cover_m: Option<f64>,
    pub max_pipe_length_m: Option<f64>,
    /// Drop across a structure, m, in the level chosen by `match_mode`: the level of the pipe
    /// leaving is this far below the level of the pipe entering. Needs `match_mode`.
    pub structure_drop_m: Option<f64>,
    pub match_mode: Option<MatchMode>,
    pub min_velocity_m_s: Option<f64>,
    pub max_velocity_m_s: Option<f64>,
    /// Largest depth of flow over the inside height.
    pub max_depth_ratio: Option<f64>,
}
