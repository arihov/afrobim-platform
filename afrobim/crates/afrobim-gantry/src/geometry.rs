// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Setting out on the alignment, the sheet elevation and the 3D solids.
//!
//! Everything here is plain geometry on the entered dimensions. The members drawn are schematic
//! envelopes (boxes and cylinders), not a designed structure.

use afrobim_alignment::Alignment;
use afrobim_draw::{Colour, Drawing, Extents, Mesh3, Primitive, Pt2};
use afrobim_kernel::SitePlacement;

use crate::{Direction, Envelope, GantryDefinition, GantryError, SignMount};

const STEEL_RGB: [u8; 3] = [0x80, 0x84, 0x88];
const CONCRETE_RGB: [u8; 3] = [0xB8, 0xB4, 0xA8];
/// Sides of the prism that stands for a round envelope in the 3D view.
const ROUND_SEGMENTS: u32 = 32;
/// Line width of outlines on the elevation, mm.
const OUTLINE_MM: f64 = 20.0;

/// Where a gantry stands: the result of putting its station, skew and levels on an alignment.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingOut {
    /// Plan position of the gantry line's crossing of the centreline, mm.
    pub centre_x_mm: f64,
    pub centre_y_mm: f64,
    /// Heading of the alignment at the station, radians counter-clockwise from +X.
    pub heading_rad: f64,
    /// Direction of the gantry line towards increasing offset (the element's local +X), radians
    /// counter-clockwise from +X: heading − 90° + skew.
    pub line_rad: f64,
    /// The reference level: the profile's centreline level at the station, mm. 0 without one.
    pub reference_level_mm: f64,
    /// False when the alignment has no profile at the station (the reference level is then 0).
    pub level_known: bool,
    /// Longitudinal grade of the profile at the station, m/m, when there is a profile.
    pub grade: Option<f64>,
    skew_cos: f64,
}

/// Where a carried sign sits, ready to store on the sign element.
#[derive(Debug, Clone, PartialEq)]
pub struct SignSetting {
    /// Square offset of the centre of the face, m.
    pub centre_offset_m: f64,
    pub serves: Direction,
    /// Placement of the sign's lower-left corner. **`z` is the level of the sign's lower edge**
    /// (a gantry-hosted sign has no ground point of its own), and the rotation makes the face
    /// look towards the oncoming traffic it serves, parallel to the boom.
    pub site: SitePlacement,
}

impl SettingOut {
    /// The plan position (mm) of the point at square offset `offset_m` on the gantry line,
    /// moved `v_mm` square to the gantry line (towards local +Y).
    pub fn point_mm(&self, offset_m: f64, v_mm: f64) -> (f64, f64) {
        self.local_mm(offset_m * 1000.0 / self.skew_cos, v_mm)
    }

    /// The plan position (mm) of the point `u_mm` along the gantry line from the centreline and
    /// `v_mm` square to it.
    pub fn local_mm(&self, u_mm: f64, v_mm: f64) -> (f64, f64) {
        let (sin, cos) = self.line_rad.sin_cos();
        (self.centre_x_mm + u_mm * cos - v_mm * sin, self.centre_y_mm + u_mm * sin + v_mm * cos)
    }
}

fn normalize_deg(a: f64) -> f64 {
    let r = a % 360.0;
    if r < 0.0 {
        r + 360.0
    } else {
        r
    }
}

impl GantryDefinition {
    /// Puts the gantry on its alignment. Fails when the alignment is not the one the gantry
    /// names or the station is off it. The definition is not validated here.
    pub fn setting_out(&self, alignment: &Alignment) -> Result<SettingOut, GantryError> {
        if alignment.name != self.alignment {
            return Err(GantryError::WrongAlignment { expected: self.alignment.clone(), given: alignment.name.clone() });
        }
        let p = alignment.point_at(self.station_m).ok_or(GantryError::StationOffAlignment(self.station_m))?;
        let profile = alignment.profile.as_ref().and_then(|pr| pr.point_at(self.station_m).ok());
        Ok(SettingOut {
            centre_x_mm: p.x * 1000.0,
            centre_y_mm: p.y * 1000.0,
            heading_rad: p.heading_rad,
            line_rad: p.heading_rad - std::f64::consts::FRAC_PI_2 + self.skew_deg.to_radians(),
            reference_level_mm: profile.map_or(0.0, |q| q.elevation_m * 1000.0),
            level_known: profile.is_some(),
            grade: profile.map(|q| q.grade),
            skew_cos: self.skew_cos(),
        })
    }

    /// The gantry element's own site placement: its local origin (the leftmost extent on the
    /// gantry line, at the reference level) and the direction of the gantry line. `offset_m` is
    /// the square offset of that origin.
    pub fn site(&self, so: &SettingOut) -> SitePlacement {
        let (u0, _) = self.extent_mm();
        let (x, y) = so.local_mm(u0, 0.0);
        SitePlacement {
            x,
            y,
            z: so.reference_level_mm,
            rotation_deg: normalize_deg(so.line_rad.to_degrees()),
            alignment: Some(self.alignment.clone()),
            station_m: Some(self.station_m),
            offset_m: Some(u0 * self.skew_cos() / 1000.0),
        }
    }

    /// Where a carried sign sits.
    ///
    /// The face is parallel to the boom, on the boom's face towards the oncoming traffic it
    /// serves. For traffic travelling up-station the sign's local +X runs along the gantry line
    /// towards increasing offset (rotation = heading − 90° + skew, which without skew is the
    /// kernel's "θ − 90°" rule); for traffic travelling down-station it is turned 180°.
    pub fn sign_setting(&self, so: &SettingOut, mount: &SignMount) -> Result<SignSetting, GantryError> {
        let centre = self.sign_centre_offset_m(mount)?;
        let serves = self.sign_direction(mount)?;
        let u = self.along_mm(centre);
        let half_boom = self.boom.envelope.width_mm() / 2.0;
        // Lower-left corner of the face as the traffic it serves reads it.
        let (u_corner, v, turn) = match serves {
            Direction::UpStation => (u - mount.width_mm / 2.0, -half_boom, 0.0),
            Direction::DownStation => (u + mount.width_mm / 2.0, half_boom, 180.0),
        };
        let (x, y) = so.local_mm(u_corner, v);
        Ok(SignSetting {
            centre_offset_m: centre,
            serves,
            site: SitePlacement {
                x,
                y,
                z: so.reference_level_mm + self.boom.underside_above_reference_mm + mount.bottom_above_boom_underside_mm,
                rotation_deg: normalize_deg(so.line_rad.to_degrees() + turn),
                alignment: Some(self.alignment.clone()),
                station_m: Some(self.station_m),
                offset_m: Some(centre),
            },
        })
    }

    /// Level of the bottom of the lowest foundation or leg base above the reference level, mm
    /// (never above 0: the road surface line is always drawn).
    fn lowest_drawn_mm(&self) -> f64 {
        self.legs.iter().map(|l| l.base_above_reference_mm - l.foundation.map_or(0.0, |f| f.depth_mm)).fold(0.0, f64::min)
    }

    /// The extent drawn along the gantry line, mm from the centreline: the structure, its
    /// foundations, the signs and the paved widths.
    fn drawn_range_mm(&self) -> (f64, f64) {
        let (mut lo, mut hi) = self.extent_mm();
        let mut take = |a: f64, b: f64| {
            lo = lo.min(a);
            hi = hi.max(b);
        };
        for leg in &self.legs {
            if let Some(f) = leg.foundation {
                let u = self.along_mm(leg.offset_m);
                take(u - f.length_mm / 2.0, u + f.length_mm / 2.0);
            }
        }
        for c in &self.carriageways {
            let (l, r) = c.paved_edges_m();
            take(self.along_mm(l), self.along_mm(r));
        }
        for m in &self.signs {
            if let Ok(o) = self.sign_centre_offset_m(m) {
                let u = self.along_mm(o);
                take(u - m.width_mm / 2.0, u + m.width_mm / 2.0);
            }
        }
        (lo, hi)
    }

    /// The sheet elevation, in millimetres with its lower-left extent on the origin, seen
    /// looking along local +Y (up-station for a square gantry) and developed along the gantry
    /// line: foundations, legs, boom, the carried signs in position, the road surface line at
    /// the reference level across each paved width with ticks at lane and carriageway edges, a
    /// centreline mark and the headroom dimension from the road line to the lowest point.
    ///
    /// Members are schematic envelopes. The road line is drawn level at the reference level:
    /// cross-fall is not modelled. Call only on a valid definition.
    pub fn drawing(&self) -> Drawing {
        let (x0, x1) = self.drawn_range_mm();
        let y0 = self.lowest_drawn_mm();
        let top = self.highest_point_mm();
        let p = |u: f64, z: f64| Pt2::new(u - x0, z - y0);
        let mut prims = Vec::new();
        let mut rect = |u0: f64, u1: f64, z0: f64, z1: f64, fill: Colour| {
            let c = [p(u0, z0), p(u1, z0), p(u1, z1), p(u0, z1)];
            prims.push(Primitive::Fill { outline: c.to_vec(), colour: fill });
            for i in 0..4 {
                prims.push(Primitive::Line { from: c[i], to: c[(i + 1) % 4], width: OUTLINE_MM, colour: None });
            }
        };
        for leg in &self.legs {
            let u = self.along_mm(leg.offset_m);
            if let Some(f) = leg.foundation {
                rect(u - f.length_mm / 2.0, u + f.length_mm / 2.0, leg.base_above_reference_mm - f.depth_mm, leg.base_above_reference_mm, Colour::White);
            }
            let half = leg.envelope.width_mm() / 2.0;
            rect(u - half, u + half, leg.base_above_reference_mm, self.boom_top_mm(), Colour::Grey);
        }
        let (b0, b1) = self.boom_range_mm();
        rect(b0, b1, self.boom.underside_above_reference_mm, self.boom_top_mm(), Colour::Grey);
        for m in &self.signs {
            if let Ok(o) = self.sign_centre_offset_m(m) {
                let u = self.along_mm(o);
                let z = self.boom.underside_above_reference_mm + m.bottom_above_boom_underside_mm;
                rect(u - m.width_mm / 2.0, u + m.width_mm / 2.0, z, z + m.height_mm, Colour::Blue);
            }
        }
        let mut line = |a: Pt2, b: Pt2, width: f64| prims.push(Primitive::Line { from: a, to: b, width, colour: None });
        // Road surface: one band per paved width, ticks at carriageway edges (long) and lane
        // edges (short).
        for c in &self.carriageways {
            let (l, r) = c.paved_edges_m();
            line(p(self.along_mm(l), 0.0), p(self.along_mm(r), 0.0), 2.0 * OUTLINE_MM);
            let mut edge = c.left_edge_offset_m;
            let last = c.lanes.len();
            for (i, lane) in c.lanes.iter().enumerate() {
                let tick = if i == 0 { 300.0 } else { 150.0 };
                line(p(self.along_mm(edge), 0.0), p(self.along_mm(edge), tick), OUTLINE_MM);
                edge += lane.width_m;
                if i + 1 == last {
                    line(p(self.along_mm(edge), 0.0), p(self.along_mm(edge), 300.0), OUTLINE_MM);
                }
            }
        }
        // Centreline mark.
        if x0 <= 0.0 && x1 >= 0.0 {
            line(p(0.0, -150.0_f64.max(y0)), p(0.0, 450.0), OUTLINE_MM / 2.0);
        }
        // Headroom dimension: at the lowest point, from the road line up to it.
        let (u_dim, z_low) = self.lowest_point_position_mm();
        if z_low > 0.0 {
            line(p(u_dim, 0.0), p(u_dim, z_low), OUTLINE_MM / 2.0);
            for z in [0.0, z_low] {
                line(p(u_dim - 150.0, z), p(u_dim + 150.0, z), OUTLINE_MM / 2.0);
            }
        }
        Drawing { primitives: prims, extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(x1 - x0, top - y0) } }
    }

    /// Where the headroom dimension is drawn: the position along the gantry line (mm from the
    /// centreline) and level (mm above the reference level) of the lowest point. A sign lower
    /// than the boom gives its own centre; otherwise the middle of the boom.
    pub(crate) fn lowest_point_position_mm(&self) -> (f64, f64) {
        let boom = self.boom.underside_above_reference_mm;
        let (b0, b1) = self.boom_range_mm();
        let mut best = ((b0 + b1) / 2.0, boom - self.checks.equipment_below_boom_mm.max(0.0));
        for m in &self.signs {
            let z = boom + m.bottom_above_boom_underside_mm;
            if z < best.1 {
                if let Ok(o) = self.sign_centre_offset_m(m) {
                    best = (self.along_mm(o), z);
                }
            }
        }
        best
    }

    /// The legs, boom and foundations as solids in the element's local frame (see the crate
    /// documentation): x from the leftmost extent along the gantry line, y square to it with the
    /// gantry line on y = 0, z above the reference level. Schematic envelopes: a box envelope is
    /// a cuboid, a circular one a prism of 32 sides. Call only on a valid definition.
    pub fn solids(&self) -> Vec<Mesh3> {
        let (x0, _) = self.extent_mm();
        let mut out = Vec::new();
        for (i, leg) in self.legs_sorted().into_iter().enumerate() {
            let x = self.along_mm(leg.offset_m) - x0;
            let (z0, z1) = (leg.base_above_reference_mm, self.boom_top_mm());
            let name = format!("Leg {}", i + 1);
            out.push(match leg.envelope {
                Envelope::Box { width_mm, depth_mm } => Mesh3::cuboid(&name, [x - width_mm / 2.0, -depth_mm / 2.0, z0], [x + width_mm / 2.0, depth_mm / 2.0, z1], STEEL_RGB),
                Envelope::Chs { diameter_mm } => Mesh3::cylinder(&name, x, 0.0, z0, z1, diameter_mm / 2.0, ROUND_SEGMENTS, STEEL_RGB),
            });
            if let Some(f) = leg.foundation {
                out.push(Mesh3::cuboid(
                    &format!("Foundation {}", i + 1),
                    [x - f.length_mm / 2.0, -f.width_mm / 2.0, z0 - f.depth_mm],
                    [x + f.length_mm / 2.0, f.width_mm / 2.0, z0],
                    CONCRETE_RGB,
                ));
            }
        }
        let (b0, b1) = self.boom_range_mm();
        let (z0, z1) = (self.boom.underside_above_reference_mm, self.boom_top_mm());
        out.push(match self.boom.envelope {
            Envelope::Box { width_mm, .. } => Mesh3::cuboid("Boom", [b0 - x0, -width_mm / 2.0, z0], [b1 - x0, width_mm / 2.0, z1], STEEL_RGB),
            Envelope::Chs { diameter_mm } => prism_along_x("Boom", b0 - x0, b1 - x0, 0.0, (z0 + z1) / 2.0, diameter_mm / 2.0, ROUND_SEGMENTS, STEEL_RGB),
        });
        out
    }
}

/// A horizontal cylinder (a prism of `segments` sides) along X from `x0` to `x1`, its axis at
/// (`cy`, `cz`). Closed and wound outwards.
fn prism_along_x(name: &str, x0: f64, x1: f64, cy: f64, cz: f64, radius: f64, segments: u32, colour: [u8; 3]) -> Mesh3 {
    let n = segments.max(3);
    let mut vertices = Vec::with_capacity(2 * n as usize + 2);
    for x in [x0, x1] {
        for i in 0..n {
            let a = i as f64 / n as f64 * std::f64::consts::TAU;
            vertices.push([x, cy + radius * a.cos(), cz + radius * a.sin()]);
        }
    }
    let (start, end) = (2 * n, 2 * n + 1);
    vertices.push([x0, cy, cz]);
    vertices.push([x1, cy, cz]);
    let mut triangles = Vec::with_capacity(4 * n as usize);
    for i in 0..n {
        let j = (i + 1) % n;
        triangles.push([i, j, n + j]);
        triangles.push([i, n + j, n + i]);
        triangles.push([start, j, i]);
        triangles.push([end, n + i, n + j]);
    }
    Mesh3 { name: name.to_string(), vertices, triangles, colour }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;
    use crate::SignPosition;
    use afrobim_alignment::{AlignmentBuilder, Profile, Pvi};

    /// A straight alignment "A1" from (1000, 2000) m heading due north (+Y), stations 0 to 500,
    /// with a profile rising at 2 % from level 50 m at station 0.
    fn north() -> Alignment {
        let mut a = AlignmentBuilder::new("A1", 0.0, 1000.0, 2000.0, std::f64::consts::FRAC_PI_2).line(500.0).build().unwrap();
        a.profile = Some(Profile::new("P", vec![Pvi::point(0.0, 50.0), Pvi::point(500.0, 60.0)]).unwrap());
        a
    }

    #[test]
    fn square_gantry_setting_out_and_levels_from_the_profile() {
        let d = portal();
        let so = d.setting_out(&north()).unwrap();
        // Station 100 heading north: centre (1000, 2100) m; level 50 + 0.02 × 100 = 52 m.
        assert!(close(so.centre_x_mm, 1_000_000.0) && close(so.centre_y_mm, 2_100_000.0));
        assert!(close(so.reference_level_mm, 52_000.0) && so.level_known);
        assert!(close(so.grade.unwrap(), 0.02));
        // Heading north, right is east: the gantry line points along +X (0 rad).
        assert!(close(so.line_rad, 0.0));
        // Legs at −9 m (west) and +9 m (east), on the station line.
        let (lx, ly) = so.point_mm(-9.0, 0.0);
        let (rx, ry) = so.point_mm(9.0, 0.0);
        assert!(close(lx, 991_000.0) && close(ly, 2_100_000.0));
        assert!(close(rx, 1_009_000.0) && close(ry, 2_100_000.0));
        // The element's origin is the outer face of the left leg: −9.3 m.
        let site = d.site(&so);
        assert!(close(site.x, 990_700.0) && close(site.y, 2_100_000.0) && close(site.z, 52_000.0));
        assert!(close(site.rotation_deg, 0.0));
        assert_eq!((site.alignment.as_deref(), site.station_m), (Some("A1"), Some(100.0)));
        assert!(close(site.offset_m.unwrap(), -9.3));
    }

    #[test]
    fn skewed_gantry_leg_positions() {
        let mut d = portal();
        d.skew_deg = 30.0;
        let so = d.setting_out(&north()).unwrap();
        // Counter-clockwise skew: a point at square offset o is o·tan 30° up-station (north).
        let t = 30f64.to_radians().tan();
        let (rx, ry) = so.point_mm(9.0, 0.0);
        assert!(close(rx, 1_009_000.0) && close(ry, 2_100_000.0 + 9000.0 * t));
        let (lx, ly) = so.point_mm(-9.0, 0.0);
        assert!(close(lx, 991_000.0) && close(ly, 2_100_000.0 - 9000.0 * t));
        // Legs are 18 / cos 30° apart along the gantry line.
        let apart = ((rx - lx).powi(2) + (ry - ly).powi(2)).sqrt();
        assert!(close(apart, 18_000.0 / 30f64.to_radians().cos()));
        assert!(close(d.clear_span_mm().unwrap(), apart - 600.0));
        assert!(close(d.site(&so).rotation_deg, 30.0));
    }

    #[test]
    fn signs_over_lanes_face_oncoming_traffic_keep_left() {
        let mut d = portal();
        d.signs.push(mount(1, SignPosition::Lane { index: 2 }, -150.0)); // up-station lane, centre −1.825 m
        d.signs.push(mount(2, SignPosition::Lane { index: 3 }, 200.0)); // down-station lane, centre +1.825 m
        let so = d.setting_out(&north()).unwrap();

        let up = d.sign_setting(&so, &d.signs[0]).unwrap();
        assert_eq!(up.serves, Direction::UpStation);
        assert!(close(up.centre_offset_m, -1.825));
        // Northbound traffic: rotation = heading 90° − 90° = 0°; the face looks south (local −Y).
        // Lower-left corner 1500 mm west of the sign centre, on the boom's south face (boom 500
        // wide: 250 mm south of the gantry line).
        assert!(close(up.site.rotation_deg, 0.0));
        assert!(close(up.site.x, 1_000_000.0 - 1825.0 - 1500.0) && close(up.site.y, 2_100_000.0 - 250.0));
        // Level of the lower edge: 52 000 + 6000 − 150.
        assert!(close(up.site.z, 57_850.0));
        assert_eq!((up.site.station_m, up.site.alignment.as_deref()), (Some(100.0), Some("A1")));

        let down = d.sign_setting(&so, &d.signs[1]).unwrap();
        assert_eq!(down.serves, Direction::DownStation);
        // Southbound traffic: rotation 180°; lower-left corner is the east end of the face, on
        // the boom's north face.
        assert!(close(down.site.rotation_deg, 180.0));
        assert!(close(down.site.x, 1_000_000.0 + 1825.0 + 1500.0) && close(down.site.y, 2_100_000.0 + 250.0));
        assert!(close(down.site.z, 58_200.0));
    }

    #[test]
    fn without_a_profile_levels_are_relative_to_zero() {
        let mut a = north();
        a.profile = None;
        let so = portal().setting_out(&a).unwrap();
        assert!(!so.level_known && so.reference_level_mm == 0.0 && so.grade.is_none());
        let mut d = portal();
        d.station_m = 900.0;
        assert_eq!(d.setting_out(&a), Err(GantryError::StationOffAlignment(900.0)));
        d.alignment = "B".into();
        assert!(matches!(d.setting_out(&a), Err(GantryError::WrongAlignment { .. })));
    }

    #[test]
    fn elevation_extents_and_content() {
        let mut d = portal();
        d.signs.push(mount(1, SignPosition::Lane { index: 2 }, -150.0));
        let g = d.drawing();
        // Width: foundations 3000 long centred on legs at ±9000: −10 500..10 500 = 21 000.
        // Height: lowest is leg 1's foundation, −200 − 1500 = −1700; highest the sign top,
        // 6000 − 150 + 2000 = 7850: 9550.
        assert_eq!(g.extents.min, Pt2::new(0.0, 0.0));
        assert!(close(g.extents.width(), 21_000.0) && close(g.extents.height(), 9550.0));
        let fills: Vec<&Vec<Pt2>> = g.primitives.iter().filter_map(|p| if let Primitive::Fill { outline, .. } = p { Some(outline) } else { None }).collect();
        // 2 foundations + 2 legs + boom + 1 sign.
        assert_eq!(fills.len(), 6);
        // The boom: x from −8700 to 8700 (shifted by 10 500), y from 6000 to 6800 (shifted by 1700).
        assert!(fills.iter().any(|o| o[0] == Pt2::new(1800.0, 7700.0) && o[2] == Pt2::new(19_200.0, 8500.0)));
        // The sign: centre −1825, 3000 wide, lower edge at 5850.
        assert!(fills.iter().any(|o| close(o[0].x, 10_500.0 - 1825.0 - 1500.0) && close(o[0].y, 5850.0 + 1700.0) && close(o[2].y, 7850.0 + 1700.0)));
        // The headroom dimension runs from the road line (y = 1700) to the sign's lower edge.
        assert!(g.primitives.iter().any(|p| matches!(p, Primitive::Line { from, to, .. } if close(from.x, 10_500.0 - 1825.0) && close(from.y, 1700.0) && close(to.y, 7550.0))));
        // The paved width band: −8.3 m to 8.3 m at the reference level.
        assert!(g.primitives.iter().any(|p| matches!(p, Primitive::Line { from, to, width, .. } if close(from.x, 2200.0) && close(to.x, 18_800.0) && close(from.y, 1700.0) && *width == 40.0)));
    }

    #[test]
    fn solids_are_the_entered_envelopes() {
        let d = portal();
        let s = d.solids();
        let names: Vec<&str> = s.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["Leg 1", "Foundation 1", "Leg 2", "Foundation 2", "Boom"]);
        let vol = |name: &str| s.iter().find(|m| m.name == name).unwrap().volume();
        // Leg 1: 600 × 400 × 7000; leg 2: × 6500; foundations 3000 × 2000 × 1500; boom 17 400 × 500 × 800.
        assert!(close(vol("Leg 1"), 600.0 * 400.0 * 7000.0));
        assert!(close(vol("Leg 2"), 600.0 * 400.0 * 6500.0));
        assert!(close(vol("Foundation 1"), 9.0e9));
        assert!(close(vol("Boom"), 17_400.0 * 500.0 * 800.0));
        // Frame: leg 1's outer face on x = 0, its base at z = −200, gantry line on y = 0.
        let leg1 = &s[0];
        let min = |k: usize| leg1.vertices.iter().map(|v| v[k]).fold(f64::INFINITY, f64::min);
        assert_eq!((min(0), min(1), min(2)), (0.0, -200.0, -200.0));
        // Round envelopes: a 32-sided prism is within 1 % of the cylinder and never larger.
        let mut c = cantilever();
        c.boom.envelope = Envelope::Chs { diameter_mm: 600.0 };
        let s = c.solids();
        assert_eq!(s.len(), 2);
        let exact_leg = std::f64::consts::PI * 250.0 * 250.0 * (6000.0 + 600.0);
        let exact_boom = std::f64::consts::PI * 300.0 * 300.0 * 7750.0;
        assert!(s[0].volume() < exact_leg && s[0].volume() > 0.99 * exact_leg);
        assert!(s[1].volume() < exact_boom && s[1].volume() > 0.99 * exact_boom);
    }
}
