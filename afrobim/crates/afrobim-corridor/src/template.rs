//! The template (typical cross-section) and the corridor definition. Every number in these types
//! is entered by the engineer; none has a built-in value.

use serde::{Deserialize, Serialize};

/// A cross slope. **Positive rises away from the centreline**, negative falls, on either side
/// (the same sense as the cross-fall stored on an alignment).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slope {
    /// Percent: metres of rise per 100 m of width.
    Percent(f64),
    /// A ratio: `rise` metres vertically in `run` metres horizontally, so a 1 in 4 fall is
    /// `Ratio { run: 4.0, rise: -1.0 }`. `run` must be greater than zero.
    Ratio { run: f64, rise: f64 },
}

impl Slope {
    /// Rise in metres per metre of width away from the centreline.
    pub fn rise_per_m(&self) -> f64 {
        match *self {
            Slope::Percent(p) => p / 100.0,
            Slope::Ratio { run, rise } => rise / run,
        }
    }
}

/// One link of a template: a straight piece of the finished surface, going outward.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    /// What the link is, e.g. `lane`, `shoulder`, `ditch_foreslope`. Free text: it names the
    /// point at the link's outer end in sections and drawings and is not interpreted.
    pub code: String,
    /// Horizontal width, m. Greater than zero (a vertical face cannot be a link).
    pub width_m: f64,
    /// The entered slope.
    pub slope: Slope,
    /// When true, the link takes the cross-fall stored on the alignment for its side at the
    /// station, wherever one is stored there; elsewhere it keeps the entered slope.
    #[serde(default)]
    pub follows_superelevation: bool,
    /// When true, the formation (the surface earthworks are measured to) lies
    /// [`Template::formation_depth_m`] below this link. When false it is the link itself.
    #[serde(default)]
    pub paved: bool,
}

impl Link {
    /// A link that keeps its entered slope and has no pavement under it.
    pub fn new(code: &str, width_m: f64, slope: Slope) -> Link {
        Link { code: code.to_string(), width_m, slope, follows_superelevation: false, paved: false }
    }

    /// The same link, taking the alignment's stored cross-fall where there is one.
    pub fn following_superelevation(mut self) -> Link {
        self.follows_superelevation = true;
        self
    }

    /// The same link, with the formation below it.
    pub fn paved(mut self) -> Link {
        self.paved = true;
        self
    }
}

/// The side slopes that join the outer end of a template to the ground.
///
/// Each is the horizontal distance for one unit of height: `2.0` is 1 vertical in 2 horizontal.
/// Both must be greater than zero.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Daylight {
    /// Used where the outer end of the template is below the ground: the slope rises outward.
    pub cut_run_per_rise: f64,
    /// Used where the outer end of the template is above the ground: the slope falls outward.
    pub fill_run_per_rise: f64,
}

/// One side of a template: its links from the centreline outward, then the daylight rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateSide {
    pub links: Vec<Link>,
    pub daylight: Daylight,
}

/// A typical cross-section. The centreline point is on the alignment's profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    /// Left looking up-station.
    pub left: TemplateSide,
    pub right: TemplateSide,
    /// Depth of the formation below the finished surface under the links marked `paved`, m.
    /// Zero or more.
    pub formation_depth_m: f64,
}

impl Template {
    /// A template with the same links and daylight rule on both sides.
    pub fn symmetric(name: &str, links: Vec<Link>, daylight: Daylight, formation_depth_m: f64) -> Template {
        let side = TemplateSide { links, daylight };
        Template { name: name.to_string(), left: side.clone(), right: side, formation_depth_m }
    }

    pub(crate) fn check(&self) -> Result<(), String> {
        if !(self.formation_depth_m.is_finite() && self.formation_depth_m >= 0.0) {
            return Err(format!("template '{}': the formation depth must not be negative (got {})", self.name, self.formation_depth_m));
        }
        for (side_name, side) in [("left", &self.left), ("right", &self.right)] {
            for (i, l) in side.links.iter().enumerate() {
                let at = format!("template '{}', {side_name} link {} ('{}')", self.name, i + 1, l.code);
                if !(l.width_m.is_finite() && l.width_m > 0.0) {
                    return Err(format!("{at}: the width must be greater than zero (got {})", l.width_m));
                }
                if let Slope::Ratio { run, .. } = l.slope {
                    if !(run.is_finite() && run > 0.0) {
                        return Err(format!("{at}: the run of a slope ratio must be greater than zero (got {run})"));
                    }
                }
                if !l.slope.rise_per_m().is_finite() {
                    return Err(format!("{at}: the slope is not a finite number"));
                }
            }
            let d = side.daylight;
            for (what, v) in [("cut", d.cut_run_per_rise), ("fill", d.fill_run_per_rise)] {
                if !(v.is_finite() && v > 0.0) {
                    return Err(format!("template '{}', {side_name} side: the {what} slope must be greater than zero (got {v})", self.name));
                }
            }
        }
        Ok(())
    }
}

/// A template and the internal station it applies from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateRegion {
    /// Internal station, m. The region runs to the start of the next one. The first region is
    /// used from the start of the corridor whatever its start station says.
    pub start_station_m: f64,
    pub template: Template,
}

/// Factors and options for volumes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VolumeOptions {
    /// Multiplies every cut volume in the mass ordinate. 1.0 changes nothing.
    #[serde(default = "one")]
    pub cut_factor: f64,
    /// Multiplies every fill volume in the mass ordinate. 1.0 changes nothing.
    #[serde(default = "one")]
    pub fill_factor: f64,
    /// Applies the curvature (Pappus) correction to each end area; see `earthworks`.
    #[serde(default)]
    pub curvature_correction: bool,
}

fn one() -> f64 {
    1.0
}

impl Default for VolumeOptions {
    /// No factor (both 1.0) and no curvature correction.
    fn default() -> Self {
        VolumeOptions { cut_factor: 1.0, fill_factor: 1.0, curvature_correction: false }
    }
}

/// What a corridor is made from, apart from the alignment and the ground.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorridorDefinition {
    pub name: String,
    /// At least one, with increasing start stations. A template changes in one step at a region
    /// boundary; nothing is interpolated between regions.
    pub regions: Vec<TemplateRegion>,
    /// Spacing of the regular sections, m (internal stations on whole multiples of it).
    pub interval_m: f64,
    /// Where the corridor starts and ends (internal stations). `None`: as far as the alignment
    /// and its profile go.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_station_m: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_station_m: Option<f64>,
    #[serde(default)]
    pub volumes: VolumeOptions,
}

impl CorridorDefinition {
    /// One template for the whole length, no volume factors.
    pub fn single(name: &str, template: Template, interval_m: f64) -> CorridorDefinition {
        CorridorDefinition {
            name: name.to_string(),
            regions: vec![TemplateRegion { start_station_m: 0.0, template }],
            interval_m,
            start_station_m: None,
            end_station_m: None,
            volumes: VolumeOptions::default(),
        }
    }

    /// Checks every entered value; the message says which one is wrong.
    pub fn check(&self) -> Result<(), String> {
        if !(self.interval_m.is_finite() && self.interval_m > 0.0) {
            return Err(format!("the section interval must be greater than zero (got {})", self.interval_m));
        }
        if self.regions.is_empty() {
            return Err("a corridor needs at least one template region".to_string());
        }
        for (i, r) in self.regions.iter().enumerate() {
            r.template.check()?;
            if r.start_station_m.is_nan() || (i > 0 && r.start_station_m <= self.regions[i - 1].start_station_m) {
                return Err(format!("template region {} must start after region {}", i + 1, i));
            }
        }
        for (what, v) in [("cut", self.volumes.cut_factor), ("fill", self.volumes.fill_factor)] {
            if !(v.is_finite() && v > 0.0) {
                return Err(format!("the {what} factor must be greater than zero (got {v})"));
            }
        }
        Ok(())
    }
}
