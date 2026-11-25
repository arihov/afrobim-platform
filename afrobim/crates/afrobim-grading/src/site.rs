// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Sites: a named set of feature lines that share elevations where they cross.
//!
//! # Crossing rule
//!
//! Two feature lines of one site that cross in plan must have one elevation at the crossing.
//! The elevation is taken from the line that **wins**:
//!
//! 1. the line with the higher `style_priority`;
//! 2. if equal, the line edited last (the larger `last_edited` counter, kept by [`Site::edit`]);
//! 3. if still equal, the line added to the site first (reported as a tie).
//!
//! Both lines get an elevation point at the crossing with the winner's elevation (a vertex that is
//! already there just has its elevation set). Crossings are found on the lines flattened at a
//! facet angle that the caller gives, so for arcs the crossing is found on the chord polyline and
//! the elevation point is placed on the true arc at the same station (the position differs from
//! the exact crossing by at most the sagitta of one facet). Overlapping collinear parts are not
//! crossings and are ignored. Every crossing is reported with the elevations before and after.

use serde::{Deserialize, Serialize};

use crate::error::{GradingError, Result};
use crate::featureline::{FeatureLine, FlatPoint};
use crate::geom::*;

/// Which rule decided a crossing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorityRule {
    StylePriority,
    LastEdited,
    /// Equal priority and edit counter: the line added first won.
    Tie,
}

/// One resolved crossing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossingReport {
    pub line_a: String,
    pub line_b: String,
    /// Plan position of the crossing, source coordinates.
    pub x: f64,
    pub y: f64,
    pub station_a: f64,
    pub station_b: f64,
    /// Elevations of the two lines at the crossing before the resolution, m.
    pub z_a: f64,
    pub z_b: f64,
    /// The shared elevation, m.
    pub z_used: f64,
    pub winner: String,
    pub rule: PriorityRule,
    /// False when the two elevations already agreed.
    pub changed: bool,
}

/// A named set of feature lines.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Site {
    pub name: String,
    pub feature_lines: Vec<FeatureLine>,
    /// Edit counter; grows with every [`Site::add`] and [`Site::edit`].
    #[serde(default)]
    pub clock: u64,
}

impl Site {
    pub fn new(name: impl Into<String>) -> Site {
        Site { name: name.into(), feature_lines: vec![], clock: 0 }
    }

    /// Adds a line (its name must be new) and stamps it as the most recently edited.
    pub fn add(&mut self, mut line: FeatureLine) -> Result<()> {
        if self.get(&line.name).is_some() {
            return Err(GradingError::Invalid(format!("a feature line named {} exists", line.name)));
        }
        self.clock += 1;
        line.last_edited = self.clock;
        self.feature_lines.push(line);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&FeatureLine> {
        self.feature_lines.iter().find(|f| f.name == name)
    }

    pub fn remove(&mut self, name: &str) -> Option<FeatureLine> {
        let i = self.feature_lines.iter().position(|f| f.name == name)?;
        Some(self.feature_lines.remove(i))
    }

    /// Edits a line with a closure; on success the line is stamped as the most recently edited.
    pub fn edit<R>(&mut self, name: &str, f: impl FnOnce(&mut FeatureLine) -> Result<R>) -> Result<R> {
        let clock = self.clock + 1;
        let line = self.feature_lines.iter_mut().find(|l| l.name == name).ok_or_else(|| GradingError::NotFound(name.to_string()))?;
        let r = f(line)?;
        line.last_edited = clock;
        self.clock = clock;
        Ok(r)
    }

    /// Finds the crossings of all pairs of lines and gives each crossing one elevation by the
    /// rule in the module documentation. `facet_angle` (radians) sets how finely arcs are
    /// flattened for the search. Lines are modified; `last_edited` is not changed.
    pub fn resolve_crossings(&mut self, facet_angle: f64) -> Result<Vec<CrossingReport>> {
        let n = self.feature_lines.len();
        if n < 2 {
            return Ok(vec![]);
        }
        let first = &self.feature_lines[0].vertices[0];
        let origin = [first.x, first.y];
        let mut flats: Vec<Vec<FlatPoint>> = Vec::with_capacity(n);
        let mut lens = Vec::with_capacity(n);
        for l in &self.feature_lines {
            flats.push(l.flatten(facet_angle)?);
            lens.push(l.length()?);
        }
        let loc = |f: &FlatPoint| sub([f.p[0], f.p[1]], origin);
        let segs = |k: usize| -> Vec<(P2, P2, f64, f64)> {
            let f = &flats[k];
            let mut v = vec![];
            for w in f.windows(2) {
                v.push((loc(&w[0]), loc(&w[1]), w[0].station, w[1].station));
            }
            if self.feature_lines[k].closed {
                v.push((loc(&f[f.len() - 1]), loc(&f[0]), f[f.len() - 1].station, lens[k]));
            }
            v
        };
        let all: Vec<Vec<(P2, P2, f64, f64)>> = (0..n).map(segs).collect();
        let mut reports = Vec::new();
        let mut edits: Vec<Vec<(f64, f64)>> = vec![vec![]; n];
        for i in 0..n {
            for j in i + 1..n {
                let mut seen: Vec<P2> = vec![];
                for sa in &all[i] {
                    for sb in &all[j] {
                        let Some((t, u)) = seg_intersect(sa.0, sa.1, sb.0, sb.1, 1e-9) else { continue };
                        let p = add(sa.0, scale(sub(sa.1, sa.0), t));
                        if seen.iter().any(|q| len(sub(*q, p)) < 1e-6) {
                            continue;
                        }
                        seen.push(p);
                        let (st_a, st_b) = (sa.2 + t * (sa.3 - sa.2), sb.2 + u * (sb.3 - sb.2));
                        let (la, lb) = (&self.feature_lines[i], &self.feature_lines[j]);
                        let (za, zb) = (la.z_at(st_a)?, lb.z_at(st_b)?);
                        let (winner_a, rule) = if la.style_priority != lb.style_priority {
                            (la.style_priority > lb.style_priority, PriorityRule::StylePriority)
                        } else if la.last_edited != lb.last_edited {
                            (la.last_edited > lb.last_edited, PriorityRule::LastEdited)
                        } else {
                            (true, PriorityRule::Tie)
                        };
                        let z_used = if winner_a { za } else { zb };
                        reports.push(CrossingReport {
                            line_a: la.name.clone(),
                            line_b: lb.name.clone(),
                            x: origin[0] + p[0],
                            y: origin[1] + p[1],
                            station_a: st_a,
                            station_b: st_b,
                            z_a: za,
                            z_b: zb,
                            z_used,
                            winner: if winner_a { la.name.clone() } else { lb.name.clone() },
                            rule,
                            changed: (za - zb).abs() > 1e-9,
                        });
                        edits[i].push((st_a, z_used));
                        edits[j].push((st_b, z_used));
                    }
                }
            }
        }
        for (k, list) in edits.into_iter().enumerate() {
            for (s, z) in list {
                let s = s.min(lens[k]);
                self.feature_lines[k].insert_elevation_point(s, z)?;
            }
        }
        Ok(reports)
    }
}
