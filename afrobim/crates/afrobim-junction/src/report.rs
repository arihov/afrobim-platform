// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Tables with CSV and plain drawing geometry.

use afrobim_alignment::format_station;
use afrobim_corridor::Side;
use serde::{Deserialize, Serialize};

use crate::error::Finding;
use crate::geom::{polyline_between, Pt};
use crate::intersection::{IntersectionResult, RoadId};
use crate::kerb::ReturnKind;
use crate::roundabout::RoundaboutResult;

fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn row(cells: &[String]) -> String {
    let mut s = cells.iter().map(|c| field(c)).collect::<Vec<_>>().join(",");
    s.push('\n');
    s
}

fn n(v: f64, d: usize) -> String {
    if v.is_finite() {
        format!("{v:.d$}")
    } else {
        String::new()
    }
}

/// A table: a header row and rows of cells.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub title: String,
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    /// The table as CSV (header first, quoting where a cell needs it, `\n` line ends).
    pub fn to_csv(&self) -> String {
        let mut s = row(&self.header);
        for r in &self.rows {
            s.push_str(&row(r));
        }
        s
    }
}

fn table(title: &str, header: &[&str], rows: Vec<Vec<String>>) -> Table {
    Table { title: title.to_string(), header: header.iter().map(|h| h.to_string()).collect(), rows }
}

impl IntersectionResult {
    /// Every crossing found: station on each alignment, position, angle, skew and arms.
    pub fn crossings_table(&self) -> Table {
        let used = self.crossings.iter().position(|x| x == &self.crossing);
        let rows = self
            .crossings
            .iter()
            .enumerate()
            .map(|(i, c)| {
                vec![
                    i.to_string(),
                    n(c.x, 4),
                    n(c.y, 4),
                    format_station(c.station_a_m, 3),
                    format_station(c.station_b_m, 3),
                    n(c.angle_rad.to_degrees(), 4),
                    n(c.skew_rad.to_degrees(), 4),
                    format!("{:?}", c.kind),
                    (Some(i) == used).to_string(),
                ]
            })
            .collect();
        table("Crossings", &["index", "x_m", "y_m", "station_primary", "station_secondary", "angle_deg", "skew_deg", "kind", "used"], rows)
    }

    /// Kerb returns: kind, tangent points with their stations on the roads, lengths, and the
    /// vertical figures when present.
    pub fn returns_table(&self) -> Table {
        let rows = self
            .returns
            .iter()
            .map(|r| {
                let g = &r.geometry;
                let kind = match &g.kind {
                    ReturnKind::Simple { radius_m } => format!("simple R={radius_m}"),
                    ReturnKind::Compound { radius1_m, radius2_m, radius3_m, offset1_m, offset3_m } => {
                        format!("compound R={radius1_m}/{radius2_m}/{radius3_m} o={offset1_m}/{offset3_m}")
                    }
                    ReturnKind::Chamfer { primary_leg_m, secondary_leg_m } => format!("chamfer {primary_leg_m}/{secondary_leg_m}"),
                };
                let v = r.vertical.as_ref();
                vec![
                    g.quadrant.label(),
                    kind,
                    n(g.primary_point.0, 4),
                    n(g.primary_point.1, 4),
                    format_station(r.primary_road_station_m, 3),
                    n(g.secondary_point.0, 4),
                    n(g.secondary_point.1, 4),
                    format_station(r.secondary_road_station_m, 3),
                    n(g.length_m, 4),
                    n(g.turn_rad.to_degrees(), 4),
                    n(g.primary_tangent_length_m, 4),
                    n(g.secondary_tangent_length_m, 4),
                    v.map_or(String::new(), |v| format!("{:?}", v.method)),
                    v.map_or(String::new(), |v| n(v.start_level_m, 4)),
                    v.map_or(String::new(), |v| n(v.end_level_m, 4)),
                    v.map_or(String::new(), |v| v.low_points.first().map_or(n(v.lowest_end.station_m, 3), |l| n(l.station_m, 3))),
                    v.map_or(String::new(), |v| v.low_points.first().map_or(n(v.lowest_end.level_m, 4), |l| n(l.level_m, 4))),
                    v.map_or(String::new(), |v| if v.low_points.is_empty() { "end".to_string() } else { "interior".to_string() }),
                ]
            })
            .collect();
        table(
            "Kerb returns",
            &[
                "quadrant",
                "kind",
                "t_primary_x",
                "t_primary_y",
                "t_primary_station",
                "t_secondary_x",
                "t_secondary_y",
                "t_secondary_station",
                "length_m",
                "turn_deg",
                "tangent_primary_m",
                "tangent_secondary_m",
                "profile_method",
                "start_level_m",
                "end_level_m",
                "low_station_m",
                "low_level_m",
                "low_kind",
            ],
            rows,
        )
    }

    /// The edge alignments: lengths, road station ranges and low points.
    pub fn edges_table(&self) -> Table {
        let rows = self
            .edges
            .iter()
            .map(|e| {
                vec![
                    e.alignment.name.clone(),
                    if e.road == RoadId::Primary { "primary" } else { "secondary" }.to_string(),
                    if e.side == Side::Left { "left" } else { "right" }.to_string(),
                    format_station(e.road_station_start_m, 3),
                    format_station(e.road_station_end_m, 3),
                    n(e.alignment.length(), 4),
                    e.low_points.len().to_string(),
                    e.low_points.iter().map(|l| format!("{:.3}@{:.4}", l.station_m, l.level_m)).collect::<Vec<_>>().join(" "),
                ]
            })
            .collect();
        table("Edges", &["name", "road", "side", "road_station_start", "road_station_end", "length_m", "low_points", "low_station@level"], rows)
    }

    pub fn findings_table(&self) -> Table {
        findings_table(&self.findings)
    }

    /// Plan geometry for drawing, by layer: every alignment as a polyline with chords no longer than
    /// `max_chord_m`, islands as closed outlines.
    pub fn drawing(&self, max_chord_m: f64) -> Drawing {
        let line = |a: &afrobim_alignment::Alignment| polyline_between(a, a.start_station_m, a.end_station_m(), max_chord_m);
        Drawing {
            layers: vec![
                DrawingLayer { name: "edges".into(), polylines: self.edges.iter().map(|e| line(&e.alignment)).collect() },
                DrawingLayer { name: "returns".into(), polylines: self.returns.iter().map(|r| line(&r.geometry.alignment)).collect() },
                DrawingLayer { name: "kerb lines".into(), polylines: self.kerb_lines.iter().map(|k| line(&k.alignment)).collect() },
                DrawingLayer { name: "islands".into(), polylines: self.islands.iter().map(|i| i.outline.clone()).collect() },
                DrawingLayer { name: "crossing".into(), polylines: vec![vec![(self.crossing.x, self.crossing.y)]] },
            ],
        }
    }
}

fn findings_table(f: &[Finding]) -> Table {
    table(
        "Findings",
        &["code", "severity", "subject", "message", "values"],
        f.iter()
            .map(|x| {
                vec![
                    x.code.clone(),
                    format!("{:?}", x.severity).to_lowercase(),
                    x.subject.clone(),
                    x.message.clone(),
                    x.values.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("; "),
                ]
            })
            .collect(),
    )
}

impl RoundaboutResult {
    /// The figures of each arm.
    pub fn arms_table(&self) -> Table {
        let rows = self
            .arms
            .iter()
            .map(|a| {
                let m = &a.measures;
                vec![
                    a.name.clone(),
                    n(a.azimuth_rad.to_degrees(), 3),
                    n(m.entry_angle_rad.to_degrees(), 3),
                    n(m.entry_width_m, 3),
                    n(m.exit_width_m, 3),
                    n(m.entry_radius_m, 3),
                    n(m.exit_radius_m, 3),
                    n(m.flare_length_m, 3),
                    n(m.approach_half_width_m, 3),
                    n(m.flare_ratio, 4),
                    n(m.angle_to_next_arm_rad.to_degrees(), 3),
                    m.entry_path_radius_m.map_or(String::new(), |r| n(r, 3)),
                ]
            })
            .collect();
        table(
            "Roundabout arms",
            &[
                "arm",
                "azimuth_deg",
                "entry_angle_deg",
                "entry_width_m",
                "exit_width_m",
                "entry_radius_m",
                "exit_radius_m",
                "flare_length_m",
                "approach_half_width_m",
                "flare_ratio",
                "angle_to_next_arm_deg",
                "entry_path_radius_m",
            ],
            rows,
        )
    }

    pub fn findings_table(&self) -> Table {
        findings_table(&self.findings)
    }

    /// Plan geometry for drawing, by layer.
    pub fn drawing(&self, max_chord_m: f64) -> Drawing {
        let line = |a: &afrobim_alignment::Alignment| polyline_between(a, a.start_station_m, a.end_station_m(), max_chord_m);
        Drawing {
            layers: vec![
                DrawingLayer {
                    name: "circles".into(),
                    polylines: vec![line(&self.inscribed_circle), line(&self.carriageway_inner_edge), line(&self.central_island_edge), line(&self.circulatory_centreline)],
                },
                DrawingLayer { name: "entry kerbs".into(), polylines: self.arms.iter().map(|a| line(&a.entry_kerb)).collect() },
                DrawingLayer { name: "exit kerbs".into(), polylines: self.arms.iter().map(|a| line(&a.exit_kerb)).collect() },
                DrawingLayer { name: "splitters".into(), polylines: self.arms.iter().map(|a| a.splitter.outline.clone()).collect() },
            ],
        }
    }
}

/// A layer of polylines in plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingLayer {
    pub name: String,
    pub polylines: Vec<Vec<Pt>>,
}

/// Plain plan geometry for drawing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Drawing {
    pub layers: Vec<DrawingLayer>,
}
