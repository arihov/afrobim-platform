// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The road cross-section at the culvert and the original ground line.
//!
//! Offsets are in metres from the road centreline, negative to the left looking up-station.
//! Levels are metres. Three ways in: a [`RoadTemplate`], explicit points, or a corridor section
//! supplied by the caller. All three end as a [`RoadSection`].

use serde::{Deserialize, Serialize};

use crate::error::{finite, invalid, not_negative, positive, CulvertError};

/// A side of the road, looking up-station.
pub use afrobim_drainage::Side;

/// A kerb or walkway outside the shoulder.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct KerbWalkway {
    /// Width of the walkway, m.
    pub width_m: f64,
    /// Rise of the kerb above the edge of the shoulder, m (a vertical step).
    pub height_step_m: f64,
    /// Crossfall of the walkway, % falling away from the centreline.
    pub crossfall_pct: f64,
}

/// One side of a road template, from the centreline outwards.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SideTemplate {
    pub carriageway_width_m: f64,
    /// % falling away from the centreline (2.5 means 2.5 %).
    pub carriageway_crossfall_pct: f64,
    pub shoulder_width_m: f64,
    pub shoulder_crossfall_pct: f64,
    pub kerb: Option<KerbWalkway>,
}

/// A typical road section described by widths and crossfalls. Left and right may differ.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoadTemplate {
    /// Finished road level on the centreline, m.
    pub frl_m: f64,
    pub left: SideTemplate,
    pub right: SideTemplate,
    /// Depth of the pavement below the finished surface, m.
    pub pavement_depth_m: f64,
    /// Embankment side slope 1:n (n horizontal to 1 vertical).
    pub embankment_slope_n: f64,
    /// Cut slope 1:n, used only where the ground is above the road edge.
    pub cut_slope_n: Option<f64>,
}

/// One point of a road section.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectionPoint {
    pub offset_m: f64,
    pub level_m: f64,
}

/// The finished road surface across the road at the culvert line, with the pavement depth and the
/// slopes outside the last point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoadSection {
    /// Ascending by offset. Two points may share an offset (a kerb step).
    pub points: Vec<SectionPoint>,
    pub pavement_depth_m: f64,
    pub embankment_slope_n: f64,
    pub cut_slope_n: Option<f64>,
}

/// The original ground line across the road at the culvert line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ogl {
    pub points: Vec<SectionPoint>,
}

/// Where the side slope meets the original ground.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Toe {
    pub offset_m: f64,
    pub level_m: f64,
    /// True where the road is in embankment at that side (the slope falls to the ground).
    pub fill: bool,
}

impl SideTemplate {
    fn check(&self, name: &str) -> Result<(), CulvertError> {
        not_negative(
            &format!("{name} carriageway width"),
            self.carriageway_width_m,
        )?;
        not_negative(&format!("{name} shoulder width"), self.shoulder_width_m)?;
        finite(
            &format!("{name} carriageway crossfall"),
            self.carriageway_crossfall_pct,
        )?;
        finite(
            &format!("{name} shoulder crossfall"),
            self.shoulder_crossfall_pct,
        )?;
        if let Some(k) = self.kerb {
            not_negative(&format!("{name} walkway width"), k.width_m)?;
            not_negative(&format!("{name} kerb step"), k.height_step_m)?;
            finite(&format!("{name} walkway crossfall"), k.crossfall_pct)?;
        }
        Ok(())
    }

    /// Points from the centreline outwards as (distance out, level drop from the centreline level
    /// at the start of the point list). The first point is the centreline itself.
    fn outward(&self, frl: f64) -> Vec<(f64, f64)> {
        let mut out = vec![(0.0, frl)];
        let mut x = 0.0;
        let mut y = frl;
        if self.carriageway_width_m > 0.0 {
            x += self.carriageway_width_m;
            y -= self.carriageway_width_m * self.carriageway_crossfall_pct / 100.0;
            out.push((x, y));
        }
        if self.shoulder_width_m > 0.0 {
            x += self.shoulder_width_m;
            y -= self.shoulder_width_m * self.shoulder_crossfall_pct / 100.0;
            out.push((x, y));
        }
        if let Some(k) = self.kerb {
            y += k.height_step_m;
            out.push((x, y));
            x += k.width_m;
            y -= k.width_m * k.crossfall_pct / 100.0;
            out.push((x, y));
        }
        out
    }
}

impl RoadTemplate {
    /// The section this template describes.
    pub fn to_section(&self) -> Result<RoadSection, CulvertError> {
        finite("the road centreline level (FRL)", self.frl_m)?;
        self.left.check("left")?;
        self.right.check("right")?;
        let mut points: Vec<SectionPoint> = self
            .left
            .outward(self.frl_m)
            .iter()
            .rev()
            .map(|&(d, l)| SectionPoint {
                offset_m: -d,
                level_m: l,
            })
            .collect();
        // The centreline appears once: the right side's first point repeats it.
        points.extend(
            self.right
                .outward(self.frl_m)
                .iter()
                .skip(1)
                .map(|&(d, l)| SectionPoint {
                    offset_m: d,
                    level_m: l,
                }),
        );
        RoadSection::new(
            points,
            self.pavement_depth_m,
            self.embankment_slope_n,
            self.cut_slope_n,
        )
    }
}

impl RoadSection {
    /// Validates and sorts the points. The centreline (offset 0) must lie inside the points.
    pub fn new(
        mut points: Vec<SectionPoint>,
        pavement_depth_m: f64,
        embankment_slope_n: f64,
        cut_slope_n: Option<f64>,
    ) -> Result<RoadSection, CulvertError> {
        if points.len() < 2 {
            return invalid("a road section needs at least two points");
        }
        for p in &points {
            finite("a section offset", p.offset_m)?;
            finite("a section level", p.level_m)?;
        }
        points.sort_by(|a, b| a.offset_m.total_cmp(&b.offset_m));
        let (first, last) = (points[0].offset_m, points[points.len() - 1].offset_m);
        if !(first <= 0.0 && last >= 0.0) || first == last {
            return invalid(format!("the road section runs from {first} to {last} m: it must cross the centreline (offset 0)"));
        }
        not_negative("the pavement depth", pavement_depth_m)?;
        positive("the embankment side slope (1:n)", embankment_slope_n)?;
        if let Some(c) = cut_slope_n {
            positive("the cut slope (1:n)", c)?;
        }
        Ok(RoadSection {
            points,
            pavement_depth_m,
            embankment_slope_n,
            cut_slope_n,
        })
    }

    /// From points the user typed or picked: (offset m, level m).
    pub fn from_points(
        points: &[(f64, f64)],
        pavement_depth_m: f64,
        embankment_slope_n: f64,
        cut_slope_n: Option<f64>,
    ) -> Result<RoadSection, CulvertError> {
        RoadSection::new(
            points
                .iter()
                .map(|&(o, l)| SectionPoint {
                    offset_m: o,
                    level_m: l,
                })
                .collect(),
            pavement_depth_m,
            embankment_slope_n,
            cut_slope_n,
        )
    }

    /// From a corridor or sample-line section supplied by the caller at a chainage: (offset m,
    /// level m) of the finished surface, left negative. The app passes what the corridor reports;
    /// this crate does not reach into the corridor.
    pub fn from_corridor_section(
        offsets_levels: &[(f64, f64)],
        pavement_depth_m: f64,
        embankment_slope_n: f64,
        cut_slope_n: Option<f64>,
    ) -> Result<RoadSection, CulvertError> {
        RoadSection::from_points(
            offsets_levels,
            pavement_depth_m,
            embankment_slope_n,
            cut_slope_n,
        )
    }

    /// Level of the finished surface at `offset_m`, linear between points. None outside the points.
    pub fn level_at(&self, offset_m: f64) -> Option<f64> {
        let p = &self.points;
        if offset_m < p[0].offset_m || offset_m > p[p.len() - 1].offset_m {
            return None;
        }
        for w in p.windows(2) {
            let (a, b) = (w[0], w[1]);
            if offset_m >= a.offset_m && offset_m <= b.offset_m {
                let span = b.offset_m - a.offset_m;
                return Some(if span <= 0.0 {
                    a.level_m
                } else {
                    a.level_m + (b.level_m - a.level_m) * (offset_m - a.offset_m) / span
                });
            }
        }
        None
    }

    /// Finished road level on the centreline.
    pub fn frl_m(&self) -> f64 {
        self.level_at(0.0).unwrap_or(self.points[0].level_m)
    }

    /// The outermost point on a side.
    pub fn edge(&self, side: Side) -> SectionPoint {
        match side {
            Side::Left => self.points[0],
            Side::Right => self.points[self.points.len() - 1],
        }
    }

    /// Level of the road surface or its embankment slope at `offset_m`: inside the points the
    /// surface, outside them the side slope falling from the edge at 1:n.
    pub fn fill_surface_level_m(&self, offset_m: f64) -> f64 {
        if let Some(l) = self.level_at(offset_m) {
            return l;
        }
        let side = if offset_m < 0.0 {
            Side::Left
        } else {
            Side::Right
        };
        let e = self.edge(side);
        e.level_m - (offset_m - e.offset_m).abs() / self.embankment_slope_n
    }

    /// Where the side slope from the edge meets the original ground. In fill the slope falls at
    /// 1:n; where the ground is above the edge the cut slope rises, and a missing cut slope is an
    /// error.
    pub fn toe(&self, ogl: &Ogl, side: Side) -> Result<Toe, CulvertError> {
        let e = self.edge(side);
        let sgn = if side == Side::Left { -1.0 } else { 1.0 };
        let ground = |d: f64| ogl.level_at(e.offset_m + sgn * d);
        let g0 = e.level_m - ground(0.0);
        let fill = g0 >= 0.0;
        let n = if fill {
            self.embankment_slope_n
        } else {
            match self.cut_slope_n {
                Some(c) => c,
                None => {
                    return invalid(format!(
                        "the ground is {:.3} m above the {} road edge, so a cut slope is needed: enter the cut slope (1:n)",
                        -g0,
                        if side == Side::Left { "left" } else { "right" }
                    ))
                }
            }
        };
        // The slope level at distance d out, and its difference from the ground (zero at the toe).
        let slope = |d: f64| {
            if fill {
                e.level_m - d / n
            } else {
                e.level_m + d / n
            }
        };
        let diff = |d: f64| slope(d) - ground(d);
        if g0 == 0.0 {
            return Ok(Toe {
                offset_m: e.offset_m,
                level_m: e.level_m,
                fill,
            });
        }
        let mut stops: Vec<f64> = ogl
            .points
            .iter()
            .map(|p| sgn * (p.offset_m - e.offset_m))
            .filter(|&d| d > 0.0)
            .collect();
        stops.sort_by(f64::total_cmp);
        let mut a = 0.0;
        for b in stops.iter().copied().chain(std::iter::once(f64::NAN)) {
            if b.is_nan() {
                // Beyond the last ground point the ground is flat at its last level, so the slope
                // meets it at a known distance.
                let c = ground(a + 1.0);
                let d = if fill {
                    n * (e.level_m - c)
                } else {
                    n * (c - e.level_m)
                };
                if d >= a - 1e-9 {
                    let d = d.max(a);
                    return Ok(Toe {
                        offset_m: e.offset_m + sgn * d,
                        level_m: slope(d),
                        fill,
                    });
                }
                break;
            }
            let (da, db) = (diff(a), diff(b));
            if da == 0.0 || (da > 0.0) != (db > 0.0) || db == 0.0 {
                let t = if (da - db).abs() < 1e-12 {
                    0.0
                } else {
                    da / (da - db)
                };
                let d = a + (b - a) * t;
                return Ok(Toe {
                    offset_m: e.offset_m + sgn * d,
                    level_m: slope(d),
                    fill,
                });
            }
            a = b;
        }
        invalid("the side slope never meets the original ground: check the ground levels and the side slope")
    }
}

impl Ogl {
    /// From (offset m, level m) pairs, in any order. One point makes a flat ground line.
    pub fn from_points(points: &[(f64, f64)]) -> Result<Ogl, CulvertError> {
        if points.is_empty() {
            return invalid("the original ground line needs at least one point");
        }
        let mut v: Vec<SectionPoint> = Vec::with_capacity(points.len());
        for &(o, l) in points {
            finite("a ground offset", o)?;
            finite("a ground level", l)?;
            v.push(SectionPoint {
                offset_m: o,
                level_m: l,
            });
        }
        v.sort_by(|a, b| a.offset_m.total_cmp(&b.offset_m));
        Ok(Ogl { points: v })
    }

    /// Ground level at `offset_m`: linear between points, held flat beyond the first and last.
    pub fn level_at(&self, offset_m: f64) -> f64 {
        let p = &self.points;
        if p.is_empty() {
            return 0.0;
        }
        if offset_m <= p[0].offset_m {
            return p[0].level_m;
        }
        if offset_m >= p[p.len() - 1].offset_m {
            return p[p.len() - 1].level_m;
        }
        for w in p.windows(2) {
            let (a, b) = (w[0], w[1]);
            if offset_m >= a.offset_m && offset_m <= b.offset_m {
                let span = b.offset_m - a.offset_m;
                return if span <= 0.0 {
                    a.level_m
                } else {
                    a.level_m + (b.level_m - a.level_m) * (offset_m - a.offset_m) / span
                };
            }
        }
        p[p.len() - 1].level_m
    }
}

/// The three ways of describing the road at the culvert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RoadSource {
    /// Widths and crossfalls.
    Template(RoadTemplate),
    /// Typed or picked points.
    Points(RoadSection),
    /// A section from the corridor or a sample line at a chainage (same data as `Points`; kept
    /// apart so the app can record where it came from).
    Corridor(RoadSection),
}

impl RoadSource {
    pub fn section(&self) -> Result<RoadSection, CulvertError> {
        match self {
            RoadSource::Template(t) => t.to_section(),
            RoadSource::Points(s) | RoadSource::Corridor(s) => RoadSection::new(
                s.points.clone(),
                s.pavement_depth_m,
                s.embankment_slope_n,
                s.cut_slope_n,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(kerb: Option<KerbWalkway>) -> SideTemplate {
        SideTemplate {
            carriageway_width_m: 3.5,
            carriageway_crossfall_pct: 2.5,
            shoulder_width_m: 1.5,
            shoulder_crossfall_pct: 4.0,
            kerb,
        }
    }

    #[test]
    fn template_builds_points_and_the_centreline_once() {
        let t = RoadTemplate {
            frl_m: 100.0,
            left: side(None),
            right: side(None),
            pavement_depth_m: 0.5,
            embankment_slope_n: 2.0,
            cut_slope_n: None,
        };
        let s = t.to_section().expect("section");
        assert_eq!(s.points.len(), 5);
        assert!((s.level_at(3.5).unwrap() - (100.0 - 0.0875)).abs() < 1e-9);
        assert!((s.level_at(-5.0).unwrap() - (100.0 - 0.0875 - 0.06)).abs() < 1e-9);
        assert_eq!(s.frl_m(), 100.0);
    }

    #[test]
    fn asymmetric_walkway_adds_a_step_on_one_side_only() {
        let k = KerbWalkway {
            width_m: 1.5,
            height_step_m: 0.15,
            crossfall_pct: 2.0,
        };
        let t = RoadTemplate {
            frl_m: 100.0,
            left: side(None),
            right: side(Some(k)),
            pavement_depth_m: 0.5,
            embankment_slope_n: 2.0,
            cut_slope_n: None,
        };
        let s = t.to_section().expect("section");
        assert_eq!(s.points.len(), 7);
        assert!((s.edge(Side::Right).offset_m - 6.5).abs() < 1e-9);
        assert!((s.edge(Side::Left).offset_m + 5.0).abs() < 1e-9);
    }

    #[test]
    fn toe_in_fill_meets_the_ground_and_cut_needs_a_slope() {
        let s = RoadSection::from_points(&[(-5.0, 10.0), (0.0, 10.0), (5.0, 10.0)], 0.3, 2.0, None)
            .expect("section");
        let flat = Ogl::from_points(&[(0.0, 8.0)]).expect("ogl");
        let toe = s.toe(&flat, Side::Right).expect("toe");
        assert!((toe.offset_m - 9.0).abs() < 1e-9 && (toe.level_m - 8.0).abs() < 1e-9 && toe.fill);
        // Rising ground on the right: 1 in 10 up from 8.0 at 5 m.
        let rising = Ogl::from_points(&[(0.0, 8.0), (5.0, 8.0), (25.0, 10.0)]).expect("ogl");
        let t2 = s.toe(&rising, Side::Right).expect("toe");
        assert!((s.fill_surface_level_m(t2.offset_m) - rising.level_at(t2.offset_m)).abs() < 1e-9);
        let high = Ogl::from_points(&[(0.0, 12.0)]).expect("ogl");
        assert!(s.toe(&high, Side::Right).is_err());
        let s2 = RoadSection::from_points(&[(-5.0, 10.0), (5.0, 10.0)], 0.3, 2.0, Some(1.5))
            .expect("section");
        let t3 = s2.toe(&high, Side::Right).expect("cut toe");
        assert!((t3.offset_m - 8.0).abs() < 1e-9 && !t3.fill);
    }

    #[test]
    fn a_section_that_misses_the_centreline_is_refused() {
        assert!(RoadSection::from_points(&[(1.0, 10.0), (5.0, 10.0)], 0.3, 2.0, None).is_err());
        assert!(RoadSection::from_points(&[(-1.0, 10.0)], 0.3, 2.0, None).is_err());
    }
}
