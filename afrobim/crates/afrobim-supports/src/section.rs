// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Post cross-sections described purely by entered dimensions. There is no section catalogue.

use std::f64::consts::PI;

use afrobim_draw::Mesh3;
use afrobim_kernel::Note;
use serde::{Deserialize, Serialize};

use crate::{error_note, positive, ROUND_SEGMENTS};

/// The cross-section of a post, in the post's plan frame: `width` dimensions run along +X
/// (parallel to the sign face), `depth` dimensions along +Y (away from the face).
///
/// Every value is entered by the engineer. Areas and perimeters are the geometry of exactly the
/// shape described here, so where a rolled or formed product has radii that are not entered
/// (the root radii of an I-section or channel, the corners of a rectangular hollow section when
/// the corner radii are left at zero) the result differs slightly from the manufacturer's
/// tabulated value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Section {
    /// Circular hollow section.
    Chs { outside_diameter_mm: f64, wall_mm: f64 },
    /// Rectangular (or square) hollow section. The corner radii default to zero, i.e. square
    /// corners; enter the product's outer and inner radii to have them taken off the area and
    /// the perimeter.
    Rhs {
        width_mm: f64,
        depth_mm: f64,
        wall_mm: f64,
        #[serde(default)]
        outer_corner_radius_mm: f64,
        #[serde(default)]
        inner_corner_radius_mm: f64,
    },
    /// A doubly symmetric I or H shape with square corners (no root radii): flanges parallel to
    /// X at the front and the back, the web along Y on the centre line.
    ISection { depth_mm: f64, flange_width_mm: f64, web_mm: f64, flange_mm: f64 },
    /// A parallel-flange channel with square corners (no root radii): flanges parallel to X at
    /// the front and the back, the web along Y on the x = 0 side.
    Channel { depth_mm: f64, flange_width_mm: f64, web_mm: f64, flange_mm: f64 },
    /// A solid round bar or pole.
    SolidRound { diameter_mm: f64 },
    /// A solid rectangular bar or post.
    SolidRect { width_mm: f64, depth_mm: f64 },
}

impl Section {
    /// Error notes for non-positive or geometrically impossible dimensions. Empty when the
    /// section can be built.
    pub fn issues(&self) -> Vec<Note> {
        let mut out = Vec::new();
        fn need(out: &mut Vec<Note>, name: &str, v: f64) -> bool {
            if positive(v) {
                true
            } else {
                out.push(error_note("SECTION_DIMENSION_NOT_POSITIVE", format!("The section's {name} must be greater than zero (it is {v}).")));
                false
            }
        }
        match *self {
            Section::Chs { outside_diameter_mm: d, wall_mm: t } => {
                let sized = need(&mut out, "outside diameter", d) & need(&mut out, "wall thickness", t);
                if sized && t >= d / 2.0 {
                    out.push(error_note(
                        "SECTION_WALL_TOO_THICK",
                        format!("A wall of {t} mm leaves no bore in a {d} mm diameter tube: it must be less than half the diameter."),
                    ));
                }
            }
            Section::Rhs { width_mm: b, depth_mm: d, wall_mm: t, outer_corner_radius_mm: ro, inner_corner_radius_mm: ri } => {
                let sized = need(&mut out, "width", b) & need(&mut out, "depth", d) & need(&mut out, "wall thickness", t);
                let mut radii_ok = true;
                for (name, r) in [("outer corner radius", ro), ("inner corner radius", ri)] {
                    if !(r.is_finite() && r >= 0.0) {
                        out.push(error_note("SECTION_CORNER_RADIUS_INVALID", format!("The section's {name} must not be negative (it is {r}).")));
                        radii_ok = false;
                    }
                }
                if sized {
                    let least = b.min(d);
                    if t >= least / 2.0 {
                        out.push(error_note(
                            "SECTION_WALL_TOO_THICK",
                            format!("A wall of {t} mm leaves no void in a {b} × {d} mm hollow section: it must be less than half the smaller side."),
                        ));
                    } else if radii_ok {
                        if ro > least / 2.0 {
                            out.push(error_note(
                                "SECTION_CORNER_RADIUS_INVALID",
                                format!("An outer corner radius of {ro} mm does not fit a {b} × {d} mm section: it can be at most half the smaller side."),
                            ));
                        } else if ri > (least - 2.0 * t) / 2.0 {
                            out.push(error_note(
                                "SECTION_CORNER_RADIUS_INVALID",
                                format!("An inner corner radius of {ri} mm does not fit the void: it can be at most half the smaller inside dimension."),
                            ));
                        } else if t * 2f64.sqrt() + (ri - ro) * (2f64.sqrt() - 1.0) <= 0.0 {
                            // Along the corner diagonal the outer face is ro(√2 − 1) from the
                            // sharp corner and the inner face is (t + ri)√2 − ri from it.
                            out.push(error_note(
                                "SECTION_CORNER_RADIUS_INVALID",
                                format!("Corner radii of {ro} mm outside and {ri} mm inside leave no wall at the corners."),
                            ));
                        }
                    }
                }
            }
            Section::ISection { depth_mm: d, flange_width_mm: b, web_mm: tw, flange_mm: tf }
            | Section::Channel { depth_mm: d, flange_width_mm: b, web_mm: tw, flange_mm: tf } => {
                let sized = need(&mut out, "depth", d) & need(&mut out, "flange width", b) & need(&mut out, "web thickness", tw) & need(&mut out, "flange thickness", tf);
                if sized {
                    if 2.0 * tf >= d {
                        out.push(error_note(
                            "SECTION_FLANGE_TOO_THICK",
                            format!("Two flanges of {tf} mm leave no web in a section {d} mm deep: the flange must be less than half the depth."),
                        ));
                    }
                    if tw >= b {
                        out.push(error_note(
                            "SECTION_WEB_TOO_THICK",
                            format!("A web of {tw} mm is not narrower than the {b} mm flange: it must be less than the flange width."),
                        ));
                    }
                }
            }
            Section::SolidRound { diameter_mm: d } => {
                need(&mut out, "diameter", d);
            }
            Section::SolidRect { width_mm: b, depth_mm: d } => {
                let _ = need(&mut out, "width", b) & need(&mut out, "depth", d);
            }
        }
        out
    }

    /// Area of material in the cross-section, mm². Hollow sections exclude the void.
    ///
    /// * CHS: π/4 · (D² − (D − 2t)²)
    /// * RHS: (b·d − (4 − π)·ro²) − ((b − 2t)(d − 2t) − (4 − π)·ri²)
    /// * I-section and channel: 2·b·tf + (d − 2·tf)·tw
    /// * solid round: π/4 · D²; solid rectangle: b·d
    pub fn area_mm2(&self) -> f64 {
        match *self {
            Section::Chs { outside_diameter_mm: d, wall_mm: t } => PI / 4.0 * (d * d - (d - 2.0 * t).powi(2)),
            Section::Rhs { width_mm: b, depth_mm: d, wall_mm: t, outer_corner_radius_mm: ro, inner_corner_radius_mm: ri } => {
                let outer = b * d - (4.0 - PI) * ro * ro;
                let inner = (b - 2.0 * t) * (d - 2.0 * t) - (4.0 - PI) * ri * ri;
                outer - inner
            }
            Section::ISection { depth_mm: d, flange_width_mm: b, web_mm: tw, flange_mm: tf }
            | Section::Channel { depth_mm: d, flange_width_mm: b, web_mm: tw, flange_mm: tf } => 2.0 * b * tf + (d - 2.0 * tf) * tw,
            Section::SolidRound { diameter_mm: d } => PI / 4.0 * d * d,
            Section::SolidRect { width_mm: b, depth_mm: d } => b * d,
        }
    }

    /// Length of the outside outline of the cross-section, mm: the surface that a coating
    /// covers, per unit length. The bore of a hollow section is not included; for an open
    /// section (I, channel) every face is outside and is included.
    ///
    /// * CHS and solid round: π·D
    /// * RHS: 2(b + d) − (8 − 2π)·ro; solid rectangle: 2(b + d)
    /// * I-section and channel: 4·b + 2·d − 2·tw
    pub fn outer_perimeter_mm(&self) -> f64 {
        match *self {
            Section::Chs { outside_diameter_mm: d, .. } | Section::SolidRound { diameter_mm: d } => PI * d,
            Section::Rhs { width_mm: b, depth_mm: d, outer_corner_radius_mm: ro, .. } => 2.0 * (b + d) - (8.0 - 2.0 * PI) * ro,
            Section::SolidRect { width_mm: b, depth_mm: d } => 2.0 * (b + d),
            Section::ISection { depth_mm: d, flange_width_mm: b, web_mm: tw, .. }
            | Section::Channel { depth_mm: d, flange_width_mm: b, web_mm: tw, .. } => 4.0 * b + 2.0 * d - 2.0 * tw,
        }
    }

    /// Overall size in plan as (along X, along Y), mm.
    pub fn bounding_mm(&self) -> (f64, f64) {
        match *self {
            Section::Chs { outside_diameter_mm: d, .. } | Section::SolidRound { diameter_mm: d } => (d, d),
            Section::Rhs { width_mm: b, depth_mm: d, .. } | Section::SolidRect { width_mm: b, depth_mm: d } => (b, d),
            Section::ISection { depth_mm: d, flange_width_mm: b, .. } | Section::Channel { depth_mm: d, flange_width_mm: b, .. } => (b, d),
        }
    }

    /// Whether the section encloses a void.
    pub fn is_hollow(&self) -> bool {
        matches!(self, Section::Chs { .. } | Section::Rhs { .. })
    }

    /// Wall thickness of a hollow section.
    pub fn wall_mm(&self) -> Option<f64> {
        match *self {
            Section::Chs { wall_mm, .. } | Section::Rhs { wall_mm, .. } => Some(wall_mm),
            _ => None,
        }
    }

    /// A plain description of the entered dimensions, e.g. `CHS D100 t10`. It is not a product
    /// designation.
    pub fn label(&self) -> String {
        match *self {
            Section::Chs { outside_diameter_mm: d, wall_mm: t } => format!("CHS D{d} t{t}"),
            Section::Rhs { width_mm: b, depth_mm: d, wall_mm: t, .. } => format!("RHS {b}x{d} t{t}"),
            Section::ISection { depth_mm: d, flange_width_mm: b, web_mm: tw, flange_mm: tf } => format!("I {b}x{d} web{tw} flange{tf}"),
            Section::Channel { depth_mm: d, flange_width_mm: b, web_mm: tw, flange_mm: tf } => {
                format!("Channel {b}x{d} web{tw} flange{tf}")
            }
            Section::SolidRound { diameter_mm: d } => format!("Round D{d}"),
            Section::SolidRect { width_mm: b, depth_mm: d } => format!("Rect {b}x{d}"),
        }
    }

    /// The section extruded from `z0` to `z1` for display, with its plan bounding box on
    /// x ∈ [0, width], y ∈ [0, depth].
    ///
    /// Hollow sections are drawn as their outer solid, round sections as a prism of
    /// [`ROUND_SEGMENTS`] sides and rounded RHS corners as square, so the mesh volume is not the
    /// material volume. Quantities use [`Section::area_mm2`].
    pub fn solids(&self, name: &str, z0: f64, z1: f64, colour: [u8; 3]) -> Vec<Mesh3> {
        match *self {
            Section::Chs { outside_diameter_mm: d, .. } | Section::SolidRound { diameter_mm: d } => {
                vec![Mesh3::cylinder(name, d / 2.0, d / 2.0, z0, z1, d / 2.0, ROUND_SEGMENTS, colour)]
            }
            Section::Rhs { width_mm: b, depth_mm: d, .. } | Section::SolidRect { width_mm: b, depth_mm: d } => {
                vec![Mesh3::cuboid(name, [0.0, 0.0, z0], [b, d, z1], colour)]
            }
            Section::ISection { depth_mm: d, flange_width_mm: b, web_mm: tw, flange_mm: tf } => vec![
                Mesh3::cuboid(name, [0.0, 0.0, z0], [b, tf, z1], colour),
                Mesh3::cuboid(name, [(b - tw) / 2.0, tf, z0], [(b + tw) / 2.0, d - tf, z1], colour),
                Mesh3::cuboid(name, [0.0, d - tf, z0], [b, d, z1], colour),
            ],
            Section::Channel { depth_mm: d, flange_width_mm: b, web_mm: tw, flange_mm: tf } => vec![
                Mesh3::cuboid(name, [0.0, 0.0, z0], [b, tf, z1], colour),
                Mesh3::cuboid(name, [0.0, tf, z0], [tw, d - tf, z1], colour),
                Mesh3::cuboid(name, [0.0, d - tf, z0], [b, d, z1], colour),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-6 * b.abs().max(1.0)
    }

    fn codes(s: &Section) -> Vec<String> {
        s.issues().into_iter().map(|n| n.code).collect()
    }

    #[test]
    fn chs_area_and_perimeter_by_hand() {
        // D = 100, t = 10: bore = 80.
        // A = π/4 · (100² − 80²) = π/4 · 3600 = 900π = 2827.4334 mm².
        // P = π · 100 = 314.1593 mm.
        let s = Section::Chs { outside_diameter_mm: 100.0, wall_mm: 10.0 };
        assert!(close(s.area_mm2(), 2827.4333882));
        assert!(close(s.area_mm2(), 900.0 * PI));
        assert!(close(s.outer_perimeter_mm(), 314.1592654));
        assert_eq!(s.bounding_mm(), (100.0, 100.0));
        assert!(s.is_hollow());
    }

    #[test]
    fn rhs_area_and_perimeter_by_hand() {
        // 100 × 50, t = 5, square corners: outer 5000, inner 90 × 40 = 3600, A = 1400 mm².
        // P = 2 · (100 + 50) = 300 mm.
        let s = Section::Rhs { width_mm: 100.0, depth_mm: 50.0, wall_mm: 5.0, outer_corner_radius_mm: 0.0, inner_corner_radius_mm: 0.0 };
        assert!(close(s.area_mm2(), 1400.0));
        assert!(close(s.outer_perimeter_mm(), 300.0));
        // With ro = 10 and ri = 5. Each rounded corner removes (1 − π/4)·r², four of them
        // (4 − π)·r² = 0.8584073·r².
        //   outer: 5000 − 0.8584073 · 100 = 4914.15927
        //   inner: 3600 − 0.8584073 · 25  = 3578.53982
        //   A = 1335.61945 mm²
        // Each corner shortens the outline by (2 − π/2)·ro, four of them (8 − 2π)·ro = 1.7168147·ro:
        //   P = 300 − 17.168147 = 282.831853 mm
        let r = Section::Rhs { width_mm: 100.0, depth_mm: 50.0, wall_mm: 5.0, outer_corner_radius_mm: 10.0, inner_corner_radius_mm: 5.0 };
        assert!(close(r.area_mm2(), 1335.61945));
        assert!(close(r.outer_perimeter_mm(), 282.831853));
        // A square section whose corner radii are half the side and half the bore is a tube.
        let tube = Section::Rhs { width_mm: 100.0, depth_mm: 100.0, wall_mm: 10.0, outer_corner_radius_mm: 50.0, inner_corner_radius_mm: 40.0 };
        let chs = Section::Chs { outside_diameter_mm: 100.0, wall_mm: 10.0 };
        assert!(tube.issues().is_empty());
        assert!(close(tube.area_mm2(), chs.area_mm2()));
        assert!(close(tube.outer_perimeter_mm(), chs.outer_perimeter_mm()));
    }

    #[test]
    fn open_and_solid_sections_by_hand() {
        // I: depth 200, flange 100 × 10, web 6.
        //   A = 2 · 100 · 10 + (200 − 20) · 6 = 2000 + 1080 = 3080 mm².
        //   Outline: 2 flange faces (2 · 100) + 4 flange tips (4 · 10) + 4 inner flange faces
        //   (4 · 47) + 2 web faces (2 · 180) = 200 + 40 + 188 + 360 = 788 mm
        //   = 4 · 100 + 2 · 200 − 2 · 6.
        let i = Section::ISection { depth_mm: 200.0, flange_width_mm: 100.0, web_mm: 6.0, flange_mm: 10.0 };
        assert!(close(i.area_mm2(), 3080.0));
        assert!(close(i.outer_perimeter_mm(), 788.0));
        assert_eq!(i.bounding_mm(), (100.0, 200.0));
        // Channel: depth 150, flange 75 × 10, web 6.
        //   A = 2 · 75 · 10 + 130 · 6 = 1500 + 780 = 2280 mm².
        //   Outline: back of web 150 + 2 outer flange faces 150 + 2 tips 20 + 2 inner flange
        //   faces 2 · 69 = 138 + inner web 130 = 588 mm = 4 · 75 + 2 · 150 − 2 · 6.
        let c = Section::Channel { depth_mm: 150.0, flange_width_mm: 75.0, web_mm: 6.0, flange_mm: 10.0 };
        assert!(close(c.area_mm2(), 2280.0));
        assert!(close(c.outer_perimeter_mm(), 588.0));
        // Solid round D = 50: A = π/4 · 2500 = 1963.4954 mm², P = 157.0796 mm.
        let r = Section::SolidRound { diameter_mm: 50.0 };
        assert!(close(r.area_mm2(), 1963.4954085));
        assert!(close(r.outer_perimeter_mm(), 157.0796327));
        // Solid rectangle 80 × 40: A = 3200 mm², P = 240 mm.
        let q = Section::SolidRect { width_mm: 80.0, depth_mm: 40.0 };
        assert!(close(q.area_mm2(), 3200.0));
        assert!(close(q.outer_perimeter_mm(), 240.0));
        assert!(!i.is_hollow() && !c.is_hollow() && !r.is_hollow() && !q.is_hollow());
    }

    #[test]
    fn display_meshes_match_the_formulae_within_the_stated_tolerance() {
        let (z0, z1) = (-1000.0, 3000.0);
        let total = |s: &Section| s.solids("Post", z0, z1, [0, 0, 0]).iter().map(Mesh3::volume).sum::<f64>();
        // Exact for flat-sided shapes: the mesh is the shape.
        let i = Section::ISection { depth_mm: 200.0, flange_width_mm: 100.0, web_mm: 6.0, flange_mm: 10.0 };
        assert!(close(total(&i), 3080.0 * 4000.0));
        let c = Section::Channel { depth_mm: 150.0, flange_width_mm: 75.0, web_mm: 6.0, flange_mm: 10.0 };
        assert!(close(total(&c), 2280.0 * 4000.0));
        let q = Section::SolidRect { width_mm: 80.0, depth_mm: 40.0 };
        assert!(close(total(&q), 3200.0 * 4000.0));
        // A round is a 48-sided prism. An inscribed n-gon has n/2 · sin(2π/n) / π of the
        // circle's area: for n = 48 that is 24 · sin(7.5°) / π = 24 · 0.1305262 / 3.1415927
        // = 0.99715, so the mesh is 0.29 % under the cylinder and never over it.
        let r = Section::SolidRound { diameter_mm: 50.0 };
        let exact = r.area_mm2() * 4000.0;
        assert!(total(&r) < exact && total(&r) > exact * 0.997);
        assert!((total(&r) / exact - 0.99715).abs() < 1e-5);
        // A tube is displayed as its outer solid, so its mesh is the outer cylinder, far more
        // than the steel: the outer disc is 2500π against 900π of material.
        let t = Section::Chs { outside_diameter_mm: 100.0, wall_mm: 10.0 };
        let outer = PI / 4.0 * 100.0 * 100.0 * 4000.0;
        assert!(total(&t) < outer && total(&t) > outer * 0.997);
        assert!(total(&t) > 2.5 * t.area_mm2() * 4000.0);
        // Every mesh stays inside the plan bounding box and between the two levels.
        for s in [&i, &c, &q, &r, &t] {
            let (w, d) = s.bounding_mm();
            for m in s.solids("Post", z0, z1, [0, 0, 0]) {
                assert!(m.volume() > 0.0, "outward winding");
                for v in &m.vertices {
                    assert!(v[0] >= -1e-9 && v[0] <= w + 1e-9 && v[1] >= -1e-9 && v[1] <= d + 1e-9);
                    assert!(v[2] == z0 || v[2] == z1);
                }
            }
        }
    }

    #[test]
    fn every_impossible_section_is_reported() {
        let nd = "SECTION_DIMENSION_NOT_POSITIVE";
        assert_eq!(codes(&Section::Chs { outside_diameter_mm: 0.0, wall_mm: 5.0 }), [nd]);
        assert_eq!(codes(&Section::Chs { outside_diameter_mm: 100.0, wall_mm: -1.0 }), [nd]);
        assert_eq!(codes(&Section::Chs { outside_diameter_mm: f64::NAN, wall_mm: 0.0 }), [nd, nd]);
        // Wall equal to half the diameter: no bore left.
        assert_eq!(codes(&Section::Chs { outside_diameter_mm: 100.0, wall_mm: 50.0 }), ["SECTION_WALL_TOO_THICK"]);
        assert_eq!(codes(&Section::Chs { outside_diameter_mm: 100.0, wall_mm: 60.0 }), ["SECTION_WALL_TOO_THICK"]);
        assert!(codes(&Section::Chs { outside_diameter_mm: 100.0, wall_mm: 49.9 }).is_empty());

        let rhs = |b, d, t, ro, ri| Section::Rhs { width_mm: b, depth_mm: d, wall_mm: t, outer_corner_radius_mm: ro, inner_corner_radius_mm: ri };
        assert_eq!(codes(&rhs(100.0, 0.0, 5.0, 0.0, 0.0)), [nd]);
        assert_eq!(codes(&rhs(100.0, 50.0, 25.0, 0.0, 0.0)), ["SECTION_WALL_TOO_THICK"]);
        assert_eq!(codes(&rhs(100.0, 50.0, 5.0, -1.0, 0.0)), ["SECTION_CORNER_RADIUS_INVALID"]);
        assert_eq!(codes(&rhs(100.0, 50.0, 5.0, 26.0, 0.0)), ["SECTION_CORNER_RADIUS_INVALID"]);
        assert_eq!(codes(&rhs(100.0, 50.0, 5.0, 0.0, 21.0)), ["SECTION_CORNER_RADIUS_INVALID"]);
        // ro = 25, ri = 0, t = 5: along the diagonal the outer face is 25 · 0.4142 = 10.36 mm in
        // and the inner sharp corner 5 · 1.4142 = 7.07 mm in, so the void breaks through.
        assert_eq!(codes(&rhs(100.0, 50.0, 5.0, 25.0, 0.0)), ["SECTION_CORNER_RADIUS_INVALID"]);
        assert!(codes(&rhs(100.0, 50.0, 5.0, 10.0, 5.0)).is_empty());

        let i = |d, b, tw, tf| Section::ISection { depth_mm: d, flange_width_mm: b, web_mm: tw, flange_mm: tf };
        assert_eq!(codes(&i(200.0, 100.0, 6.0, 0.0)), [nd]);
        assert_eq!(codes(&i(200.0, 100.0, 6.0, 100.0)), ["SECTION_FLANGE_TOO_THICK"]);
        assert_eq!(codes(&i(200.0, 100.0, 100.0, 10.0)), ["SECTION_WEB_TOO_THICK"]);
        let c = Section::Channel { depth_mm: 20.0, flange_width_mm: 5.0, web_mm: 6.0, flange_mm: 10.0 };
        assert_eq!(codes(&c), ["SECTION_FLANGE_TOO_THICK", "SECTION_WEB_TOO_THICK"]);

        assert_eq!(codes(&Section::SolidRound { diameter_mm: -5.0 }), [nd]);
        assert_eq!(codes(&Section::SolidRect { width_mm: 0.0, depth_mm: f64::INFINITY }), [nd, nd]);
        assert!(codes(&Section::SolidRect { width_mm: 1.0, depth_mm: 1.0 }).is_empty());
    }

    #[test]
    fn serialises_with_a_type_tag_and_reads_rhs_without_radii() {
        let s = Section::Chs { outside_diameter_mm: 88.9, wall_mm: 4.0 };
        assert_eq!(serde_json::to_string(&s).unwrap(), r#"{"type":"Chs","outside_diameter_mm":88.9,"wall_mm":4.0}"#);
        let r: Section = serde_json::from_str(r#"{"type":"Rhs","width_mm":100,"depth_mm":50,"wall_mm":5}"#).unwrap();
        assert_eq!(r, Section::Rhs { width_mm: 100.0, depth_mm: 50.0, wall_mm: 5.0, outer_corner_radius_mm: 0.0, inner_corner_radius_mm: 0.0 });
    }
}
