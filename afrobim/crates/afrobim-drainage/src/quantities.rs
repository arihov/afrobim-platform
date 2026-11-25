// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Take-off for side drains and pipe culverts, by geometry only.
//!
//! Every figure is the entered dimensions put through the formula stated on its field. No figure
//! comes from a display mesh or a rate, and nothing is added for working space, overbreak,
//! bulking, joints or waste.

use afrobim_kernel::Element;

use crate::{fixed, DrainageError, PipeCulvertDefinition, SideDrainDefinition, CULVERT_CATEGORY, DRAIN_CATEGORY};

/// One side drain measured.
#[derive(Debug, Clone, PartialEq)]
pub struct DrainQuantities {
    pub mark: String,
    pub alignment: String,
    pub start_station_m: f64,
    pub end_station_m: f64,
    pub offset_m: f64,
    /// [`crate::ChannelSection::label`].
    pub section: String,
    /// [`crate::Lining::label`].
    pub lining: String,
    /// Zero for an unlined drain.
    pub lining_thickness_mm: f64,
    /// Length of the alignment between the stations.
    pub centreline_length_m: f64,
    /// Length of the drain's own centreline ([`crate::DrainRoute::length_m`]): longer than the
    /// alignment on the outside of a curve, shorter on the inside.
    pub length_m: f64,
    /// [`SideDrainDefinition::finished_area_m2`].
    pub finished_area_m2: f64,
    /// [`SideDrainDefinition::excavation_area_m2`].
    pub excavation_area_m2: f64,
    /// Excavated section area × drain length. On a curve this takes the centroid of the section
    /// to travel the drain's centreline; for a section that is not symmetrical, or far wider
    /// than it is offset, the true figure differs by the centroid's sideways eccentricity over
    /// the radius.
    pub excavation_m3: f64,
    /// Length of the finished faces across the section
    /// ([`SideDrainDefinition::lined_perimeter_m`]) × drain length. Zero for an unlined drain.
    pub lining_area_m2: f64,
    /// (Excavated area − finished area) × drain length. Zero for an unlined drain.
    pub lining_volume_m3: f64,
}

impl DrainQuantities {
    /// Measures one drain. Fails when the definition has Error notes or has not been routed.
    pub fn measure(mark: &str, def: &SideDrainDefinition) -> Result<DrainQuantities, DrainageError> {
        def.check()?;
        let Some(route) = &def.route else {
            return Err(DrainageError::Route(format!("the drain has not been routed along alignment '{}'", def.alignment)));
        };
        let lined = def.lining_mm() > 0.0;
        let (finished, excavated) = (def.finished_area_m2(), def.excavation_area_m2());
        Ok(DrainQuantities {
            mark: mark.to_string(),
            alignment: def.alignment.clone(),
            start_station_m: route.start_station_m,
            end_station_m: route.end_station_m,
            offset_m: route.offset_m,
            section: def.section.label(),
            lining: def.lining.label(),
            lining_thickness_mm: def.lining_mm(),
            centreline_length_m: route.centreline_length_m,
            length_m: route.length_m,
            finished_area_m2: finished,
            excavation_area_m2: excavated,
            excavation_m3: excavated * route.length_m,
            lining_area_m2: if lined { def.lined_perimeter_m() * route.length_m } else { 0.0 },
            lining_volume_m3: if lined { (excavated - finished) * route.length_m } else { 0.0 },
        })
    }
}

/// One pipe culvert measured.
#[derive(Debug, Clone, PartialEq)]
pub struct CulvertQuantities {
    pub mark: String,
    pub alignment: String,
    pub station_m: f64,
    pub skew_deg: f64,
    pub barrels: u32,
    pub internal_diameter_mm: f64,
    pub wall_thickness_mm: f64,
    pub material: String,
    /// Length of one barrel along the pipe, as entered.
    pub length_per_barrel_m: f64,
    /// Barrels × length per barrel.
    pub total_length_m: f64,
}

impl CulvertQuantities {
    /// Measures one culvert. Fails when the definition has Error notes.
    pub fn measure(mark: &str, def: &PipeCulvertDefinition) -> Result<CulvertQuantities, DrainageError> {
        def.check()?;
        let length_per_barrel_m = def.length_mm / 1000.0;
        Ok(CulvertQuantities {
            mark: mark.to_string(),
            alignment: def.alignment.clone(),
            station_m: def.station_m,
            skew_deg: def.skew_deg,
            barrels: def.barrels,
            internal_diameter_mm: def.internal_diameter_mm,
            wall_thickness_mm: def.wall_thickness_mm,
            material: def.material.clone(),
            length_per_barrel_m,
            total_length_m: def.barrels as f64 * length_per_barrel_m,
        })
    }
}

/// An element that could not be measured, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct SkippedElement {
    pub mark: String,
    pub reason: String,
}

/// The drains and culverts of a set of elements, in the order given.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DrainageQuantities {
    pub drains: Vec<DrainQuantities>,
    pub culverts: Vec<CulvertQuantities>,
    /// Drainage elements left out, with the reason. Never silently dropped.
    pub skipped: Vec<SkippedElement>,
}

impl DrainageQuantities {
    /// Measures every drain and culvert among `elements`; other categories are ignored.
    pub fn measure<'a>(elements: impl IntoIterator<Item = &'a Element>) -> DrainageQuantities {
        let mut out = DrainageQuantities::default();
        for e in elements {
            let skipped = |err: DrainageError| SkippedElement { mark: e.name.clone(), reason: err.to_string() };
            if e.category == DRAIN_CATEGORY {
                match SideDrainDefinition::from_value(&e.data).and_then(|d| DrainQuantities::measure(&e.name, &d)) {
                    Ok(q) => out.drains.push(q),
                    Err(err) => out.skipped.push(skipped(err)),
                }
            } else if e.category == CULVERT_CATEGORY {
                match PipeCulvertDefinition::from_value(&e.data).and_then(|d| CulvertQuantities::measure(&e.name, &d)) {
                    Ok(q) => out.culverts.push(q),
                    Err(err) => out.skipped.push(skipped(err)),
                }
            }
        }
        out
    }

    /// Sum of the drain lengths, m.
    pub fn total_drain_length_m(&self) -> f64 {
        self.drains.iter().map(|d| d.length_m).sum()
    }

    /// Sum of the drain excavation, m³.
    pub fn total_excavation_m3(&self) -> f64 {
        self.drains.iter().map(|d| d.excavation_m3).sum()
    }

    /// Sum of the pipe lengths of all culverts, m.
    pub fn total_pipe_length_m(&self) -> f64 {
        self.culverts.iter().map(|c| c.total_length_m).sum()
    }
}

/// Header of [`drains_to_csv`].
pub const DRAINS_CSV_HEADER: &str = "mark,alignment,start_station_m,end_station_m,offset_m,section,lining,lining_thickness_mm,centreline_length_m,length_m,finished_area_m2,excavation_area_m2,excavation_m3,lining_area_m2,lining_volume_m3";

/// Header of [`culverts_to_csv`].
pub const CULVERTS_CSV_HEADER: &str = "mark,alignment,station_m,skew_deg,barrels,internal_diameter_mm,wall_thickness_mm,material,length_per_barrel_m,total_length_m";

fn csv(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// The side drain schedule as CSV: one row per drain in the order given; lengths and stations
/// to 0.001 m, areas to 0.0001 m², volumes to 0.001 m³; lines end with a line feed.
pub fn drains_to_csv(drains: &[DrainQuantities]) -> String {
    let mut out = String::from(DRAINS_CSV_HEADER);
    out.push('\n');
    for d in drains {
        let row = [
            csv(&d.mark),
            csv(&d.alignment),
            fixed(d.start_station_m, 3, 3),
            fixed(d.end_station_m, 3, 3),
            fixed(d.offset_m, 3, 3),
            csv(&d.section),
            csv(&d.lining),
            fixed(d.lining_thickness_mm, 0, 1),
            fixed(d.centreline_length_m, 3, 3),
            fixed(d.length_m, 3, 3),
            fixed(d.finished_area_m2, 4, 4),
            fixed(d.excavation_area_m2, 4, 4),
            fixed(d.excavation_m3, 3, 3),
            fixed(d.lining_area_m2, 3, 3),
            fixed(d.lining_volume_m3, 3, 3),
        ];
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out
}

/// The pipe culvert schedule as CSV: one row per culvert in the order given; lengths and
/// stations to 0.001 m; lines end with a line feed.
pub fn culverts_to_csv(culverts: &[CulvertQuantities]) -> String {
    let mut out = String::from(CULVERTS_CSV_HEADER);
    out.push('\n');
    for c in culverts {
        let row = [
            csv(&c.mark),
            csv(&c.alignment),
            fixed(c.station_m, 3, 3),
            fixed(c.skew_deg, 0, 2),
            c.barrels.to_string(),
            fixed(c.internal_diameter_mm, 0, 1),
            fixed(c.wall_thickness_mm, 0, 1),
            csv(&c.material),
            fixed(c.length_per_barrel_m, 3, 3),
            fixed(c.total_length_m, 3, 3),
        ];
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::mowt::ChannelMaterial;
    use crate::{ChannelSection, Lining, Side};
    use afrobim_alignment::{Alignment, AlignmentBuilder, Turn};

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!((actual - expected).abs() <= tolerance, "expected {expected} ± {tolerance}, got {actual}");
    }

    /// 100 m straight, then a 200 m radius left-hand arc 100 m long.
    fn alignment() -> Alignment {
        AlignmentBuilder::new("MC10", 0.0, 0.0, 0.0, 0.0).line(100.0).arc(200.0, 100.0, Turn::Left).build().unwrap()
    }

    fn drain(start: f64, end: f64, offset: f64, lining_mm: Option<f64>) -> SideDrainDefinition {
        let mut d = SideDrainDefinition {
            alignment: "MC10".into(),
            start_station_m: start,
            end_station_m: end,
            offset_m: offset,
            section: ChannelSection::Trapezoidal { bottom_width_mm: 500.0, depth_mm: 600.0, left_side_slope: 1.5, right_side_slope: 1.5 },
            lining: Lining::Table(ChannelMaterial::Concrete),
            lining_thickness_mm: lining_mm,
            gradient_m_per_m: 0.01,
            manning_n: None,
            route: None,
            placeholder: false,
        };
        d.route_along(&alignment()).unwrap();
        d
    }

    #[test]
    fn drain_on_a_straight() {
        // Section (see drain.rs): finished 0.84 m², faces 2.663331 m, excavated 1.127389 m².
        let q = DrainQuantities::measure("SD-01", &drain(0.0, 100.0, 5.0, Some(100.0))).unwrap();
        close(q.length_m, 100.0, 1e-12);
        close(q.centreline_length_m, 100.0, 1e-12);
        close(q.excavation_m3, 112.7389, 1e-4);
        close(q.lining_area_m2, 266.3331, 1e-4);
        close(q.lining_volume_m3, 28.7389, 1e-4);
        // Unlined: the excavation is the finished section, 0.84 × 100 = 84 m³, and no lining.
        let u = DrainQuantities::measure("SD-02", &drain(0.0, 100.0, 5.0, None)).unwrap();
        close(u.excavation_m3, 84.0, 1e-9);
        assert_eq!((u.lining_area_m2, u.lining_volume_m3, u.lining_thickness_mm), (0.0, 0.0, 0.0));
    }

    #[test]
    fn drain_on_a_curve_is_measured_along_its_own_line() {
        // Left-hand curve, R = 200 m, 100 m of alignment. Right of it (outside) the drain is
        // 100 × 205/200 = 102.5 m long; left of it (inside) 100 × 195/200 = 97.5 m.
        let outside = DrainQuantities::measure("SD-R", &drain(100.0, 200.0, 5.0, None)).unwrap();
        let inside = DrainQuantities::measure("SD-L", &drain(100.0, 200.0, -5.0, None)).unwrap();
        close(outside.length_m, 102.5, 1e-9);
        close(inside.length_m, 97.5, 1e-9);
        close(outside.centreline_length_m, 100.0, 1e-12);
        // 0.84 × 102.5 = 86.1 m³; 0.84 × 97.5 = 81.9 m³.
        close(outside.excavation_m3, 86.1, 1e-8);
        close(inside.excavation_m3, 81.9, 1e-8);
    }

    #[test]
    fn culvert_lengths() {
        let c = PipeCulvertDefinition {
            alignment: "MC10".into(),
            station_m: 50.0,
            skew_deg: 15.0,
            barrels: 3,
            internal_diameter_mm: 900.0,
            wall_thickness_mm: 100.0,
            length_mm: 12_500.0,
            barrel_spacing_mm: Some(1500.0),
            invert_level_inlet_m: 10.25,
            invert_level_outlet_m: 10.0,
            inlet_side: Side::Left,
            centre_offset_m: 0.0,
            road_level_m: None,
            material: "Concrete, class as specified".into(),
            placeholder: false,
        };
        let q = CulvertQuantities::measure("C-01", &c).unwrap();
        assert_eq!((q.barrels, q.length_per_barrel_m, q.total_length_m), (3, 12.5, 37.5));
        let text = culverts_to_csv(&[q]);
        assert_eq!(
            text,
            format!("{CULVERTS_CSV_HEADER}\nC-01,MC10,50.000,15,3,900,100,\"Concrete, class as specified\",12.500,37.500\n")
        );
        assert!(CulvertQuantities::measure("C-02", &PipeCulvertDefinition { barrels: 0, ..c }).is_err());
    }

    #[test]
    fn drain_csv_and_unmeasurable_drains() {
        let q = DrainQuantities::measure("SD-01", &drain(100.0, 200.0, 5.0, Some(100.0))).unwrap();
        let text = drains_to_csv(&[q]);
        let mut lines = text.lines();
        assert_eq!(lines.next().unwrap(), DRAINS_CSV_HEADER);
        // 1.127389 × 102.5 = 115.557; 2.663331 × 102.5 = 272.991; 0.287389 × 102.5 = 29.457.
        assert_eq!(
            lines.next().unwrap(),
            "SD-01,MC10,100.000,200.000,5.000,Trapezoidal b500 d600 1.5:1/1.5:1,Concrete,100,100.000,102.500,0.8400,1.1274,115.557,272.991,29.457"
        );
        assert_eq!(DRAINS_CSV_HEADER.split(',').count(), 15);
        assert_eq!(CULVERTS_CSV_HEADER.split(',').count(), 10);

        let mut unrouted = drain(0.0, 100.0, 5.0, None);
        unrouted.route = None;
        assert!(matches!(DrainQuantities::measure("SD-X", &unrouted), Err(DrainageError::Route(_))));
    }
}
