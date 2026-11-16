//! Common site-design helpers built on feature lines, gradings and groups.
//!
//! None of them has a default engineering value: slopes, levels, crossfall, distances and
//! template steps are all arguments.

use afrobim_corridor::Tin;
use serde::{Deserialize, Serialize};

use crate::criteria::{Criterion, Slope};
use crate::error::{finite, GradingError, Result};
use crate::featureline::FeatureLine;
use crate::geom::*;
use crate::grading::{face, Grading};
use crate::group::{GradingGroup, GroupBuild, Infill};
use crate::rays::{ray_geometry, ProjectionParams, Side};
use crate::site::Site;

/// A site and a group ready to build.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignPackage {
    pub site: Site,
    pub group: GradingGroup,
}

impl DesignPackage {
    pub fn build(&self, ground: Option<&Tin>) -> Result<GroupBuild> {
        self.group.build(&self.site, ground)
    }
}

/// The surface of a platform. Both forms are inputs; there is no default crossfall.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "platform", rename_all = "snake_case")]
pub enum PlatformSurface {
    /// Flat at `level`, m.
    Level { level: f64 },
    /// A plane: elevation `z0` at `(x0, y0)`, rising `grade_x_percent` per 100 m of easting and
    /// `grade_y_percent` per 100 m of northing.
    Plane { x0: f64, y0: f64, z0: f64, grade_x_percent: f64, grade_y_percent: f64 },
}

impl PlatformSurface {
    pub fn z_at(&self, x: f64, y: f64) -> f64 {
        match *self {
            PlatformSurface::Level { level } => level,
            PlatformSurface::Plane { x0, y0, z0, grade_x_percent, grade_y_percent } => z0 + grade_x_percent / 100.0 * (x - x0) + grade_y_percent / 100.0 * (y - y0),
        }
    }
}

/// A building pad or platform: a closed outline at the platform surface, with batter slopes
/// tying to the ground all round (cut slope where the ground is higher, fill slope where it is
/// lower) and the platform itself as infill. Returns the package; build it against the ground.
///
/// `max_distance` limits the batter search. The outline must be a simple polygon, either
/// direction; the batters always go outward.
pub fn building_pad(name: &str, outline: &[[f64; 2]], platform: &PlatformSurface, cut_slope: Slope, fill_slope: Slope, max_distance: f64, params: ProjectionParams) -> Result<DesignPackage> {
    if outline.len() < 3 {
        return Err(GradingError::Invalid("an outline needs at least three points".into()));
    }
    let pts: Vec<[f64; 3]> = outline.iter().map(|p| [p[0], p[1], platform.z_at(p[0], p[1])]).collect();
    let fl = FeatureLine::closed_from_points(format!("{name}-footprint"), &pts)?;
    let outward = if area2(outline) > 0.0 { Side::Right } else { Side::Left };
    let mut site = Site::new(name);
    site.add(fl)?;
    let mut group = GradingGroup::new(name, params);
    group.gradings.push(Grading::uniform(format!("{name}-batter"), format!("{name}-footprint"), outward, Criterion::TargetSurface { cut_slope, fill_slope, max_distance }));
    group.infills.push(Infill::Footprint { footprint: format!("{name}-footprint") });
    Ok(DesignPackage { site, group })
}

/// A pond or basin from a closed rim line (its elevations are the rim levels) with one inside
/// slope down to a flat floor at `floor_level`, and the floor as infill. The floor must be below
/// every rim elevation. Stage-storage comes from [`GroupBuild::stage_storage`] with the rim
/// polygon as region (see [`rim_region`]).
pub fn pond(name: &str, rim: &FeatureLine, floor_level: f64, inside_slope: Slope, params: ProjectionParams) -> Result<DesignPackage> {
    finite("floor_level", floor_level)?;
    if !rim.closed {
        return Err(GradingError::Invalid("a pond rim must be a closed feature line".into()));
    }
    if rim.vertices.iter().any(|v| v.z <= floor_level) {
        return Err(GradingError::Invalid("the floor level must be below every rim elevation".into()));
    }
    let ring = rim.plan_ring(params.facet_angle)?;
    let inside = if area2(&ring) > 0.0 { Side::Left } else { Side::Right };
    let mut site = Site::new(name);
    let mut rim = rim.clone();
    rim.name = format!("{name}-rim");
    site.add(rim.clone())?;
    let mut group = GradingGroup::new(name, params);
    group.gradings.push(Grading::uniform(format!("{name}-slopes"), rim.name.clone(), inside, Criterion::TargetElevation { elevation: floor_level, cut_slope: inside_slope, fill_slope: inside_slope }));
    group.infills.push(Infill::Daylight { grading: 0 });
    Ok(DesignPackage { site, group })
}

/// The plan polygon of the rim of a pond package, for stage-storage.
pub fn rim_region(pkg: &DesignPackage) -> Result<Vec<[f64; 2]>> {
    let g = pkg.group.gradings.first().ok_or_else(|| GradingError::NotFound("grading".into()))?;
    let rim = pkg.site.get(&g.footprint).ok_or_else(|| GradingError::NotFound(g.footprint.clone()))?;
    rim.plan_ring(pkg.group.params.facet_angle)
}

/// A lay-by or access platform tie-in: the platform edge is a feature line carrying the design
/// grade (an open line, or a closed one for a whole platform, which is then also infilled); slopes
/// project from it on `side`, over `range` if given, to the ground.
pub fn layby_tie_in(name: &str, edge: &FeatureLine, side: Side, range: Option<(f64, f64)>, cut_slope: Slope, fill_slope: Slope, max_distance: f64, params: ProjectionParams) -> Result<DesignPackage> {
    let mut site = Site::new(name);
    let mut edge = edge.clone();
    edge.name = format!("{name}-edge");
    site.add(edge.clone())?;
    let mut group = GradingGroup::new(name, params);
    let mut g = Grading::uniform(format!("{name}-tie-in"), edge.name.clone(), side, Criterion::TargetSurface { cut_slope, fill_slope, max_distance });
    g.range = range;
    group.gradings.push(g);
    if edge.closed && range.is_none() {
        group.infills.push(Infill::Footprint { footprint: edge.name.clone() });
    }
    Ok(DesignPackage { site, group })
}

/// One step of a stepped offset template: move `offset` metres further from the line in plan
/// (not negative) and `dz` metres up (positive) or down (negative) from the previous point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TemplateStep {
    pub offset: f64,
    pub dz: f64,
}

/// The result of sweeping a template along a feature line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateSweep {
    /// One polyline per template point (the first is the line itself), each following the rays.
    pub lines: Vec<Vec<[f64; 3]>>,
    /// Surface triangles between consecutive lines, counter-clockwise in plan.
    pub triangles: Vec<[[f64; 3]; 3]>,
    /// True when the lines close on themselves.
    pub ring: bool,
}

/// A ditch or berm along a feature line by a stepped offset template.
///
/// The template points are at cumulative offsets and elevation differences from the line; a
/// ditch is a template that goes down and back up, a berm one that goes up and back down. The
/// same rays as a grading are used (see the `rays` module), so interior corners are mitered
/// (the cross-section stays square to both spans) and exterior corners are fanned by the facet
/// angle. Heights are relative to the feature line elevation at each ray.
pub fn sweep_template(line: &FeatureLine, range: Option<(f64, f64)>, side: Side, steps: &[TemplateStep], params: &ProjectionParams) -> Result<TemplateSweep> {
    if steps.is_empty() {
        return Err(GradingError::Invalid("a template needs at least one step".into()));
    }
    for s in steps {
        finite("dz", s.dz)?;
        if !(s.offset >= 0.0 && s.offset.is_finite()) {
            return Err(GradingError::Invalid("a template offset must not be negative".into()));
        }
    }
    let set = ray_geometry(line, range, side, params, &[])?;
    let mut cum = vec![(0.0, 0.0)];
    for s in steps {
        let l = *cum.last().unwrap();
        cum.push((l.0 + s.offset, l.1 + s.dz));
    }
    let o = [set.rays[0].foot[0], set.rays[0].foot[1]];
    let lines: Vec<Vec<[f64; 3]>> = cum
        .iter()
        .map(|&(off, dz)| {
            set.rays
                .iter()
                .map(|r| {
                    let f = [r.foot[0] - o[0], r.foot[1] - o[1]];
                    let p = add(f, scale(r.dir, r.miter * off));
                    [o[0] + p[0], o[1] + p[1], r.foot[2] + dz]
                })
                .collect()
        })
        .collect();
    let n = set.rays.len();
    let pairs = if set.ring { n } else { n - 1 };
    let mut triangles = vec![];
    for k in 0..cum.len() - 1 {
        for i in 0..pairs {
            let j = (i + 1) % n;
            face(lines[k][i], lines[k][j], lines[k + 1][i], lines[k + 1][j], &mut triangles);
        }
    }
    Ok(TemplateSweep { lines, triangles, ring: set.ring })
}
