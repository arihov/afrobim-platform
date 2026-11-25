// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Geometry: the offset-strip relations, the typical cross-section and the 3D slabs.
//!
//! # A strip beside a curved alignment
//!
//! A point at station `s` and offset `o` (positive to the right looking up-station) is
//! `P(s) + o·n(s)`, with `n` the unit normal to the right. Where the alignment has curvature
//! `k(s)` (positive turning left, as in `afrobim-alignment`), that point moves `(1 + k·o) ds`
//! for each `ds` along the alignment: further than the centreline on the outside of a curve,
//! less on the inside. So for a strip between offsets `o1 < o2` from station `s1` to `s2`
//!
//! ```text
//!   area = ∫∫ (1 + k(s)·o) do ds = (o2 − o1) · ( L + ō·Δθ )
//!
//!   L  = s2 − s1                 length along the alignment
//!   ō  = (o1 + o2) / 2           offset of the strip's own centre
//!   Δθ = ∫ k ds                  change of heading from s1 to s2, radians, left positive
//! ```
//!
//! `L + ō·Δθ` is the length of the strip's own centre line. The result is exact for straights,
//! arcs and spirals alike, because only the integral of the curvature enters, and it needs
//! `1 + k·o > 0` everywhere (no edge beyond the centre of a curve). A strip centred on the
//! alignment (ō = 0) has area width × L whatever the curvature; a shoulder never does.
//!
//! Curvature varies linearly along every alignment element, so `Δθ` over a piece from `a` to
//! `b` of one element is `(k(a) + k(b)) / 2 · (b − a)`, with no numerical integration.

use afrobim_alignment::Alignment;
use afrobim_draw::{Drawing, Extents, Mesh3, Primitive, Pt2};

use crate::model::{PavementDefinition, Strip, StripKind};
use crate::text;

/// The pieces of each alignment element between two stations: (element index, from, to) as
/// distances into the element.
fn pieces(alignment: &Alignment, from_station_m: f64, to_station_m: f64) -> Vec<(usize, f64, f64)> {
    let mut out = Vec::new();
    let mut start = alignment.start_station_m;
    for (i, e) in alignment.elements.iter().enumerate() {
        let (a, b) = (from_station_m.max(start), to_station_m.min(start + e.length));
        if b > a {
            out.push((i, a - start, b - start));
        }
        start += e.length;
    }
    out
}

/// Change of heading along the alignment between two stations, radians, positive turning left.
/// Angle points between elements are not included.
pub fn heading_change_rad(alignment: &Alignment, from_station_m: f64, to_station_m: f64) -> f64 {
    pieces(alignment, from_station_m, to_station_m)
        .into_iter()
        .map(|(i, a, b)| {
            let e = &alignment.elements[i];
            0.5 * (e.curvature_at(a) + e.curvature_at(b)) * (b - a)
        })
        .sum()
}

/// Smallest and largest curvature between two stations, 1/m (zero included when nothing is
/// covered).
pub(crate) fn curvature_range(alignment: &Alignment, from_station_m: f64, to_station_m: f64) -> (f64, f64) {
    let mut range = (f64::INFINITY, f64::NEG_INFINITY);
    for (i, a, b) in pieces(alignment, from_station_m, to_station_m) {
        let e = &alignment.elements[i];
        for k in [e.curvature_at(a), e.curvature_at(b)] {
            range = (range.0.min(k), range.1.max(k));
        }
    }
    if range.0 > range.1 {
        (0.0, 0.0)
    } else {
        range
    }
}

/// Plan area of the strip between two offsets, m²: `(o2 − o1)·(L + ō·Δθ)` as derived in the
/// module documentation.
pub fn strip_area_m2(length_m: f64, heading_change_rad: f64, left_offset_m: f64, right_offset_m: f64) -> f64 {
    (right_offset_m - left_offset_m) * (length_m + 0.5 * (left_offset_m + right_offset_m) * heading_change_rad)
}

// ---------------------------------------------------------------------------------------------
// Cross-section
// ---------------------------------------------------------------------------------------------

/// How the cross-section is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionOptions {
    /// Levels and thicknesses of the section are multiplied by this; widths are not. 1 draws
    /// the section to one scale both ways.
    pub vertical_exaggeration: f64,
    /// Height of the lettering in drawing units (millimetres). `None` takes 1/80 of the overall
    /// width, at least 60.
    pub text_height_mm: Option<f64>,
}

impl Default for SectionOptions {
    fn default() -> Self {
        SectionOptions { vertical_exaggeration: 1.0, text_height_mm: None }
    }
}

/// A label of the cross-section as text, for a front end that sets its own type. The drawing
/// already carries the same words as stroke lettering.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionLabel {
    pub text: String,
    /// Lower-left corner of the text in the drawing, mm.
    pub at: Pt2,
    pub height_mm: f64,
}

/// The typical cross-section.
///
/// Millimetres, Y up, lower-left extent on the origin, seen looking up-station (the left
/// shoulder is on the left). It has two parts:
///
/// - The section itself: every layer of every strip as a filled outline following the entered
///   cross-fall, 1 unit = 1 mm of width; levels are multiplied by `vertical_exaggeration`
///   (1 unless asked otherwise). A layer of zero thickness is a heavy line on the surface.
///   Above each strip are its name, width and fall.
/// - To the right, one structure column per distinct layer stack (the carriageway, and any
///   shoulder with a stack of its own): the layers as boxes whose heights are their
///   thicknesses times `column_scale`, each with its thickness, name and material beside it.
///   The column exists because a 50 mm layer in a 10 m wide section is too thin to label.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossSection {
    pub drawing: Drawing,
    pub labels: Vec<SectionLabel>,
    /// Drawing units per millimetre of thickness in the structure columns.
    pub column_scale: f64,
    pub vertical_exaggeration: f64,
}

fn mm(v: f64) -> String {
    let s = format!("{v:.1}");
    s.strip_suffix(".0").map_or(s.clone(), str::to_string)
}

/// Draws the typical cross-section. Call only on a definition without Error notes.
pub fn cross_section(def: &PavementDefinition, options: &SectionOptions) -> CrossSection {
    let ve = if options.vertical_exaggeration.is_finite() && options.vertical_exaggeration > 0.0 { options.vertical_exaggeration } else { 1.0 };
    let strips = def.strips();
    let left = strips[0].left_offset_m();
    let total_width = (strips[strips.len() - 1].right_offset_m() - left) * 1000.0;
    let h = options.text_height_mm.filter(|v| v.is_finite() && *v > 0.0).unwrap_or((total_width / 80.0).max(60.0));

    let mut prims: Vec<Primitive> = Vec::new();
    let mut labels: Vec<SectionLabel> = Vec::new();
    let mut say = |text: String, at: Pt2, prims: &mut Vec<Primitive>| {
        text::draw(&text, at, h, prims);
        labels.push(SectionLabel { text, at, height_mm: h });
    };
    let outline = |pts: &[Pt2], prims: &mut Vec<Primitive>| {
        for i in 0..pts.len() {
            prims.push(Primitive::Line { from: pts[i], to: pts[(i + 1) % pts.len()], width: 0.0, colour: None });
        }
    };

    // The section. x from the left edge, y = level × exaggeration (shifted up afterwards).
    let surface_top = strips.iter().flat_map(|s| s.profile.iter().map(|p| p.1)).fold(f64::NEG_INFINITY, f64::max) * 1000.0 * ve;
    for strip in &strips {
        let top: Vec<Pt2> = strip.profile.iter().map(|&(o, z)| Pt2::new((o - left) * 1000.0, z * 1000.0 * ve)).collect();
        let mut depth = 0.0;
        for layer in &strip.layers {
            let (d0, d1) = (depth * ve, (depth + layer.thickness_mm) * ve);
            if layer.thickness_mm > 0.0 {
                let mut poly: Vec<Pt2> = top.iter().map(|p| Pt2::new(p.x, p.y - d0)).collect();
                poly.extend(top.iter().rev().map(|p| Pt2::new(p.x, p.y - d1)));
                prims.push(Primitive::Fill { outline: poly.clone(), colour: layer.material.colour() });
                outline(&poly, &mut prims);
            } else {
                for w in top.windows(2) {
                    let lift = Pt2::new(0.0, -d0);
                    prims.push(Primitive::Line { from: w[0] + lift, to: w[1] + lift, width: h / 5.0, colour: None });
                }
            }
            depth += layer.thickness_mm;
        }
        let fall = match strip.kind {
            StripKind::Carriageway => def.crossfall_percent,
            StripKind::LeftShoulder => def.left_shoulder.as_ref().map_or(0.0, |s| s.crossfall_percent),
            StripKind::RightShoulder => def.right_shoulder.as_ref().map_or(0.0, |s| s.crossfall_percent),
        };
        let lines = [strip.kind.label().to_string(), format!("{:.3} m", strip.width_m()), format!("fall {fall}%")];
        let centre = (strip.mean_offset_m() - left) * 1000.0;
        for (row, line) in lines.into_iter().enumerate() {
            let w = text::width(&line, h);
            say(line, Pt2::new(centre - 0.5 * w, surface_top + h * (1.0 + 1.6 * (2 - row) as f64)), &mut prims);
        }
    }

    // The structure columns.
    let mut stacks: Vec<(String, &Strip)> = Vec::new();
    for strip in &strips {
        if let Some(found) = stacks.iter_mut().find(|(_, s)| s.layers == strip.layers) {
            found.0 = format!("{}, {}", found.0, strip.kind.label().to_lowercase());
        } else {
            stacks.push((strip.kind.label().to_string(), strip));
        }
    }
    let thinnest = strips.iter().flat_map(|s| s.layers.iter()).map(|l| l.thickness_mm).filter(|t| *t > 0.0).fold(f64::INFINITY, f64::min);
    let column_scale = (1.6 * h / thinnest).max(1.0);
    let column_width = 6.0 * h;
    let mut x = total_width + 4.0 * h;
    for (title, strip) in stacks {
        let mut widest = text::width(&title, h);
        say(title, Pt2::new(x, surface_top + 1.6 * h), &mut prims);
        let mut y = surface_top;
        for layer in &strip.layers {
            let box_height = layer.thickness_mm * column_scale;
            let thickness = if layer.thickness_mm > 0.0 { format!("{} mm", mm(layer.thickness_mm)) } else { "seal".to_string() };
            let line = if layer.name == layer.material.label() {
                format!("{thickness} {}", layer.name)
            } else {
                format!("{thickness} {} ({})", layer.name, layer.material.label())
            };
            if layer.thickness_mm > 0.0 {
                let poly = [Pt2::new(x, y), Pt2::new(x + column_width, y), Pt2::new(x + column_width, y - box_height), Pt2::new(x, y - box_height)];
                prims.push(Primitive::Fill { outline: poly.to_vec(), colour: layer.material.colour() });
                outline(&poly, &mut prims);
                widest = widest.max(column_width + h + text::width(&line, h));
                say(line, Pt2::new(x + column_width + h, y - 0.5 * box_height - 0.5 * h), &mut prims);
            } else {
                prims.push(Primitive::Line { from: Pt2::new(x, y), to: Pt2::new(x + column_width, y), width: h / 5.0, colour: None });
                widest = widest.max(column_width + h + text::width(&line, h));
                say(line, Pt2::new(x + column_width + h, y + 0.1 * h), &mut prims);
            }
            y -= box_height;
        }
        let scale_line = format!("thickness × {}", mm(column_scale));
        widest = widest.max(text::width(&scale_line, h));
        say(scale_line, Pt2::new(x, y - 2.0 * h), &mut prims);
        x += widest + 4.0 * h;
    }

    // Move the lower-left extent onto the origin.
    let (mut min, mut max) = (Pt2::new(f64::INFINITY, f64::INFINITY), Pt2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
    let mut grow = |p: &Pt2, pad: f64| {
        min = Pt2::new(min.x.min(p.x - pad), min.y.min(p.y - pad));
        max = Pt2::new(max.x.max(p.x + pad), max.y.max(p.y + pad));
    };
    for p in &prims {
        match p {
            Primitive::Fill { outline, .. } => outline.iter().for_each(|q| grow(q, 0.0)),
            Primitive::Line { from, to, width, .. } => {
                grow(from, 0.5 * width);
                grow(to, 0.5 * width);
            }
            _ => {}
        }
    }
    let shift = |p: &Pt2| Pt2::new(p.x - min.x, p.y - min.y);
    let primitives = prims
        .iter()
        .map(|p| match p {
            Primitive::Fill { outline, colour } => Primitive::Fill { outline: outline.iter().map(shift).collect(), colour: *colour },
            Primitive::Line { from, to, width, colour } => Primitive::Line { from: shift(from), to: shift(to), width: *width, colour: *colour },
            other => other.clone(),
        })
        .collect();
    for l in &mut labels {
        l.at = shift(&l.at);
    }
    CrossSection {
        drawing: Drawing { primitives, extents: Extents { min: Pt2::new(0.0, 0.0), max: shift(&max) } },
        labels,
        column_scale,
        vertical_exaggeration: ve,
    }
}

// ---------------------------------------------------------------------------------------------
// Slabs
// ---------------------------------------------------------------------------------------------

/// The pavement as solids for the 3D site view, with what is needed to put them on the map.
///
/// One closed mesh per layer of each strip (a layer of zero thickness has none), named
/// `"<strip>: <layer>"`. Millimetres, unrotated: +X east, +Y north, +Z up. The plan bounding
/// box of all the slabs has its lower-left corner on the origin, and that origin is the plan
/// point (`origin_x_m`, `origin_y_m`), so a vertex (x, y, z) is at
///
/// ```text
///   easting  = origin_x_m + x / 1000
///   northing = origin_y_m + y / 1000        (metres)
/// ```
///
/// **z is the level of the carriageway surface at the carriageway centre**: where the alignment
/// has a vertical profile, its level at that station in millimetres (held at the end levels
/// beyond the profile's ends); with no profile, zero at every station. The surface falls from
/// there at the entered cross-fall (superelevation is not read) and each layer lies under it at
/// its entered thickness, measured vertically.
///
/// Curves and spirals are followed by chords that stay within 5 mm of the outer edge, so a
/// slab's mesh volume is slightly below the true volume on a curve. Quantities never come from
/// the mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct PavementSolids {
    pub solids: Vec<Mesh3>,
    pub origin_x_m: f64,
    pub origin_y_m: f64,
    /// Number of cross-sections the slabs were built from.
    pub sections: usize,
}

/// Greatest distance allowed between a chord and the true edge, m.
const CHORD_SAG_M: f64 = 0.005;

/// The stations at which the slabs are cut: both ends, every element joint between them, and
/// enough points on curves and spirals to hold the chord tolerance at the widest edge.
fn section_stations(def: &PavementDefinition, widest_offset_m: f64) -> Vec<f64> {
    let (s0, s1) = (def.start_station_m, def.end_station_m);
    let mut out = vec![s0];
    let mut start = def.alignment.start_station_m;
    for e in &def.alignment.elements {
        let (a, b) = (s0.max(start), s1.min(start + e.length));
        if b > a {
            let k = e.curvature_at(a - start).abs().max(e.curvature_at(b - start).abs());
            let n = if k > 0.0 {
                // Sagitta of a chord c on radius R is c²/8R; the outer edge is on radius 1/k + offset.
                let chord = (8.0 * (1.0 / k + widest_offset_m) * CHORD_SAG_M).sqrt().clamp(0.25, 20.0);
                (((b - a) / chord).ceil() as usize).clamp(1, 100_000)
            } else {
                1
            };
            for i in 1..=n {
                out.push(a + (b - a) * i as f64 / n as f64);
            }
        }
        start += e.length;
    }
    out.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    out
}

/// The level of the alignment's vertical profile at a station, m above datum, held at the end
/// levels beyond the profile's ends. `None` when the alignment has no profile.
fn profile_level_m(alignment: &Alignment, station_m: f64) -> Option<f64> {
    let profile = alignment.profile.as_ref()?;
    let (lo, hi) = profile.range()?;
    profile.elevation_at(station_m.clamp(lo, hi)).ok()
}

/// Builds the slabs. `None` when a station is off the alignment; call only on a definition
/// without Error notes.
pub fn solids(def: &PavementDefinition) -> Option<PavementSolids> {
    let strips = def.strips();
    let widest = strips[0].left_offset_m().abs().max(strips[strips.len() - 1].right_offset_m().abs());
    let stations = section_stations(def, widest);
    let frames: Vec<_> = stations.iter().map(|s| def.alignment.point_at(*s)).collect::<Option<_>>()?;
    // The level of the carriageway centre at each section, m: the alignment's profile where it has
    // one (held at its end levels beyond its ends), else zero.
    let levels: Vec<f64> = stations.iter().map(|s| profile_level_m(&def.alignment, *s).unwrap_or(0.0)).collect();

    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    for f in &frames {
        for o in [strips[0].left_offset_m(), strips[strips.len() - 1].right_offset_m()] {
            let (x, y) = f.offset(o);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
        }
    }

    let mut meshes = Vec::new();
    for strip in &strips {
        let m = strip.profile.len();
        let mut depth_mm = 0.0;
        for layer in &strip.layers {
            let (top, bottom) = (depth_mm, depth_mm + layer.thickness_mm);
            depth_mm = bottom;
            if !(layer.thickness_mm > 0.0) {
                continue;
            }
            // Per section: the top points left to right, then the bottom points.
            let mut vertices = Vec::with_capacity(frames.len() * 2 * m);
            for (f, level) in frames.iter().zip(&levels) {
                for d in [top, bottom] {
                    for &(o, z) in &strip.profile {
                        let (x, y) = f.offset(o);
                        // Subtract in metres before scaling, so large map coordinates lose nothing.
                        // The cross-fall (z) is relative to the centre; the profile level lifts it.
                        vertices.push([(x - min_x) * 1000.0, (y - min_y) * 1000.0, (level + z) * 1000.0 - d]);
                    }
                }
            }
            let at = |section: usize, bottom: bool, j: usize| (section * 2 * m + usize::from(bottom) * m + j) as u32;
            let mut triangles = Vec::new();
            for i in 0..frames.len() - 1 {
                for j in 0..m - 1 {
                    let (a, b, c, d) = (at(i, false, j), at(i, false, j + 1), at(i + 1, false, j + 1), at(i + 1, false, j));
                    triangles.extend([[a, b, c], [a, c, d]]);
                    let (a, b, c, d) = (at(i, true, j), at(i, true, j + 1), at(i + 1, true, j + 1), at(i + 1, true, j));
                    triangles.extend([[a, c, b], [a, d, c]]);
                }
                let (tl0, tl1, bl0, bl1) = (at(i, false, 0), at(i + 1, false, 0), at(i, true, 0), at(i + 1, true, 0));
                triangles.extend([[tl0, tl1, bl1], [tl0, bl1, bl0]]);
                let (tr0, tr1, br0, br1) = (at(i, false, m - 1), at(i + 1, false, m - 1), at(i, true, m - 1), at(i + 1, true, m - 1));
                triangles.extend([[tr0, br1, tr1], [tr0, br0, br1]]);
            }
            let last = frames.len() - 1;
            for j in 0..m - 1 {
                let (tl, tr, bl, br) = (at(0, false, j), at(0, false, j + 1), at(0, true, j), at(0, true, j + 1));
                triangles.extend([[tl, bl, br], [tl, br, tr]]);
                let (tl, tr, bl, br) = (at(last, false, j), at(last, false, j + 1), at(last, true, j), at(last, true, j + 1));
                triangles.extend([[tl, br, bl], [tl, tr, br]]);
            }
            meshes.push(Mesh3 { name: format!("{}: {}", strip.kind.label(), layer.name), vertices, triangles, colour: layer.material.rgb() });
        }
    }
    Some(PavementSolids { solids: meshes, origin_x_m: min_x, origin_y_m: min_y, sections: frames.len() })
}
