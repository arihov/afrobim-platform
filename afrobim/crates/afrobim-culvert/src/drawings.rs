//! Drawings: plan, sections A-A, D-D, B-B and C-C and the front elevation, as
//! [`afrobim_draw::Drawing`]s in millimetres.
//!
//! Dimensions are a `Line` for the extension, dimension and tick lines and a `Glyphs` primitive
//! for the text. Text is drawn with a small single-stroke font of this crate (each stroke a thin
//! contour), and the `Glyphs` primitive carries the string too, so sheet code can substitute its
//! own lettering. Long sections use a 1:1 vertical scale: a level `L` metres is drawn at
//! `(L - datum) x 1000` mm, and the datum is written on the drawing.

use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};
use serde::{Deserialize, Serialize};

use crate::layout::{EndLayout, Layout};
use crate::model::{Barrel, CulvertInput};
use crate::road::{Ogl, RoadSection, Side};
use crate::setout::SettingOutPoint;

/// The drawings of one culvert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CulvertDrawings {
    pub plan: Drawing,
    /// Long section along the culvert.
    pub section_aa: Drawing,
    /// Section across the barrel at the centreline.
    pub section_dd: Drawing,
    /// Front elevation of the left headwall and wings.
    pub front_elevation: Drawing,
    /// Section along the water face of a left wing wall.
    pub wing_section_cc: Drawing,
    /// Section through the toe beam and apron of the left end.
    pub toe_section_bb: Drawing,
}

type P = (f64, f64);

fn strokes(c: char) -> Vec<Vec<P>> {
    let ch = c.to_ascii_uppercase();
    let zero: Vec<P> = vec![
        (1., 0.),
        (3., 0.),
        (4., 1.),
        (4., 5.),
        (3., 6.),
        (1., 6.),
        (0., 5.),
        (0., 1.),
        (1., 0.),
    ];
    match ch {
        ' ' => vec![],
        '0' | 'O' => vec![zero],
        '1' => vec![vec![(1., 5.), (2., 6.), (2., 0.)], vec![(1., 0.), (3., 0.)]],
        '2' => vec![vec![
            (0., 5.),
            (1., 6.),
            (3., 6.),
            (4., 5.),
            (4., 4.),
            (0., 0.),
            (4., 0.),
        ]],
        '3' => vec![
            vec![
                (0., 5.),
                (1., 6.),
                (3., 6.),
                (4., 5.),
                (4., 4.),
                (3., 3.),
                (1.5, 3.),
            ],
            vec![(3., 3.), (4., 2.), (4., 1.), (3., 0.), (1., 0.), (0., 1.)],
        ],
        '4' => vec![vec![(3., 0.), (3., 6.), (0., 2.), (4., 2.)]],
        '5' => vec![vec![
            (4., 6.),
            (0., 6.),
            (0., 3.),
            (3., 3.),
            (4., 2.),
            (4., 1.),
            (3., 0.),
            (1., 0.),
            (0., 1.),
        ]],
        '6' => vec![vec![
            (4., 5.),
            (3., 6.),
            (1., 6.),
            (0., 5.),
            (0., 1.),
            (1., 0.),
            (3., 0.),
            (4., 1.),
            (4., 2.),
            (3., 3.),
            (0., 3.),
        ]],
        '7' => vec![vec![(0., 6.), (4., 6.), (1.5, 0.)]],
        '8' => vec![vec![
            (1., 3.),
            (0., 4.),
            (0., 5.),
            (1., 6.),
            (3., 6.),
            (4., 5.),
            (4., 4.),
            (3., 3.),
            (1., 3.),
            (0., 2.),
            (0., 1.),
            (1., 0.),
            (3., 0.),
            (4., 1.),
            (4., 2.),
            (3., 3.),
        ]],
        '9' => vec![vec![
            (0., 1.),
            (1., 0.),
            (3., 0.),
            (4., 1.),
            (4., 5.),
            (3., 6.),
            (1., 6.),
            (0., 5.),
            (0., 4.),
            (1., 3.),
            (4., 3.),
        ]],
        'A' => vec![
            vec![(0., 0.), (2., 6.), (4., 0.)],
            vec![(0.8, 2.), (3.2, 2.)],
        ],
        'B' => vec![
            vec![
                (0., 0.),
                (0., 6.),
                (3., 6.),
                (4., 5.),
                (4., 4.),
                (3., 3.),
                (0., 3.),
            ],
            vec![(3., 3.), (4., 2.), (4., 1.), (3., 0.), (0., 0.)],
        ],
        'C' => vec![vec![
            (4., 5.),
            (3., 6.),
            (1., 6.),
            (0., 5.),
            (0., 1.),
            (1., 0.),
            (3., 0.),
            (4., 1.),
        ]],
        'D' => vec![vec![
            (0., 0.),
            (0., 6.),
            (2.5, 6.),
            (4., 4.5),
            (4., 1.5),
            (2.5, 0.),
            (0., 0.),
        ]],
        'E' => vec![
            vec![(4., 6.), (0., 6.), (0., 0.), (4., 0.)],
            vec![(0., 3.), (3., 3.)],
        ],
        'F' => vec![vec![(4., 6.), (0., 6.), (0., 0.)], vec![(0., 3.), (3., 3.)]],
        'G' => vec![vec![
            (4., 5.),
            (3., 6.),
            (1., 6.),
            (0., 5.),
            (0., 1.),
            (1., 0.),
            (3., 0.),
            (4., 1.),
            (4., 3.),
            (2., 3.),
        ]],
        'H' => vec![
            vec![(0., 0.), (0., 6.)],
            vec![(4., 0.), (4., 6.)],
            vec![(0., 3.), (4., 3.)],
        ],
        'I' => vec![
            vec![(1., 6.), (3., 6.)],
            vec![(2., 6.), (2., 0.)],
            vec![(1., 0.), (3., 0.)],
        ],
        'J' => vec![vec![(4., 6.), (4., 1.), (3., 0.), (1., 0.), (0., 1.)]],
        'K' => vec![vec![(0., 0.), (0., 6.)], vec![(4., 6.), (0., 3.), (4., 0.)]],
        'L' => vec![vec![(0., 6.), (0., 0.), (4., 0.)]],
        'M' => vec![vec![(0., 0.), (0., 6.), (2., 3.), (4., 6.), (4., 0.)]],
        'N' => vec![vec![(0., 0.), (0., 6.), (4., 0.), (4., 6.)]],
        'P' => vec![vec![
            (0., 0.),
            (0., 6.),
            (3., 6.),
            (4., 5.),
            (4., 4.),
            (3., 3.),
            (0., 3.),
        ]],
        'Q' => vec![zero, vec![(2.5, 1.5), (4., 0.)]],
        'R' => vec![
            vec![
                (0., 0.),
                (0., 6.),
                (3., 6.),
                (4., 5.),
                (4., 4.),
                (3., 3.),
                (0., 3.),
            ],
            vec![(2., 3.), (4., 0.)],
        ],
        'S' => vec![vec![
            (4., 5.),
            (3., 6.),
            (1., 6.),
            (0., 5.),
            (0., 4.),
            (1., 3.),
            (3., 3.),
            (4., 2.),
            (4., 1.),
            (3., 0.),
            (1., 0.),
            (0., 1.),
        ]],
        'T' => vec![vec![(0., 6.), (4., 6.)], vec![(2., 6.), (2., 0.)]],
        'U' => vec![vec![
            (0., 6.),
            (0., 1.),
            (1., 0.),
            (3., 0.),
            (4., 1.),
            (4., 6.),
        ]],
        'V' => vec![vec![(0., 6.), (2., 0.), (4., 6.)]],
        'W' => vec![vec![(0., 6.), (1., 0.), (2., 3.), (3., 0.), (4., 6.)]],
        'X' => vec![vec![(0., 0.), (4., 6.)], vec![(0., 6.), (4., 0.)]],
        'Y' => vec![vec![(0., 6.), (2., 3.), (4., 6.)], vec![(2., 3.), (2., 0.)]],
        'Z' => vec![vec![(0., 6.), (4., 6.), (0., 0.), (4., 0.)]],
        '.' => vec![vec![
            (1.5, 0.),
            (2.5, 0.),
            (2.5, 0.8),
            (1.5, 0.8),
            (1.5, 0.),
        ]],
        ',' => vec![vec![(2.2, 0.5), (1.6, -1.)]],
        '-' => vec![vec![(0.5, 3.), (3.5, 3.)]],
        '+' => vec![vec![(0.5, 3.), (3.5, 3.)], vec![(2., 1.), (2., 5.)]],
        '/' => vec![vec![(0., 0.), (4., 6.)]],
        ':' => vec![vec![(1.8, 1.), (2.2, 1.)], vec![(1.8, 4.), (2.2, 4.)]],
        '=' => vec![vec![(0.5, 2.), (3.5, 2.)], vec![(0.5, 4.), (3.5, 4.)]],
        '%' => vec![
            vec![(0., 0.), (4., 6.)],
            vec![(0.5, 5.), (1.5, 5.), (1.5, 6.), (0.5, 6.), (0.5, 5.)],
            vec![(2.5, 0.), (3.5, 0.), (3.5, 1.), (2.5, 1.), (2.5, 0.)],
        ],
        '(' => vec![vec![(3., 6.), (1.5, 4.), (1.5, 2.), (3., 0.)]],
        ')' => vec![vec![(1., 6.), (2.5, 4.), (2.5, 2.), (1., 0.)]],
        '\u{b0}' => vec![vec![(1., 5.), (2., 6.), (3., 5.), (2., 4.), (1., 5.)]],
        _ => vec![vec![(0.5, 0.), (3.5, 0.), (3.5, 6.), (0.5, 6.), (0.5, 0.)]],
    }
}

struct Cv {
    prims: Vec<Primitive>,
    th: f64,
    lw: f64,
}

fn pt(p: P) -> Pt2 {
    Pt2::new(p.0, p.1)
}

impl Cv {
    fn new(th: f64, lw: f64) -> Cv {
        Cv {
            prims: Vec::new(),
            th,
            lw,
        }
    }

    fn line_w(&mut self, a: P, b: P, w: f64, colour: Option<Colour>) {
        if (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9 {
            return;
        }
        self.prims.push(Primitive::Line {
            from: pt(a),
            to: pt(b),
            width: w,
            colour,
        });
    }

    fn line(&mut self, a: P, b: P) {
        self.line_w(a, b, self.lw, None);
    }

    fn thin(&mut self, a: P, b: P) {
        self.line_w(a, b, self.lw * 0.4, None);
    }

    fn dashed(&mut self, a: P, b: P, dash: f64, gap: f64, colour: Option<Colour>) {
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        if len < 1e-9 {
            return;
        }
        let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
        let mut d = 0.0;
        while d < len {
            let e = (d + dash).min(len);
            self.line_w(
                (a.0 + ux * d, a.1 + uy * d),
                (a.0 + ux * e, a.1 + uy * e),
                self.lw * 0.6,
                colour,
            );
            d += dash + gap;
        }
    }

    fn poly(&mut self, pts: &[P], closed: bool) {
        for w in pts.windows(2) {
            self.line(w[0], w[1]);
        }
        if closed && pts.len() > 2 {
            self.line(pts[pts.len() - 1], pts[0]);
        }
    }

    fn rect(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        self.poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], true);
    }

    fn circle(&mut self, c: P, r: f64) {
        self.prims.push(Primitive::Arc {
            centre: pt(c),
            radius: r,
            start_deg: 0.0,
            end_deg: 360.0,
            colour: None,
        });
    }

    /// Text at `at`, rotated `rot_deg`, centred on `at` when `centre`.
    fn text_h(&mut self, at: P, s: &str, h: f64, rot_deg: f64, centre: bool) {
        let adv = 0.8 * h;
        let kw = 0.15 * h;
        let kh = h / 6.0;
        let width = adv * s.chars().count() as f64;
        let shift = if centre { -width / 2.0 } else { 0.0 };
        let (sn, cs) = rot_deg.to_radians().sin_cos();
        let place = |x: f64, y: f64| -> Pt2 {
            let (lx, ly) = (x + shift, y);
            Pt2::new(at.0 + lx * cs - ly * sn, at.1 + lx * sn + ly * cs)
        };
        let half = h / 24.0;
        let mut contours = Vec::new();
        for (i, c) in s.chars().enumerate() {
            let x0 = adv * i as f64 + adv * 0.1;
            for stroke in strokes(c) {
                for w in stroke.windows(2) {
                    let (a, b) = (
                        (x0 + w[0].0 * kw, w[0].1 * kh),
                        (x0 + w[1].0 * kw, w[1].1 * kh),
                    );
                    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                    let len = (dx * dx + dy * dy).sqrt();
                    if len < 1e-9 {
                        continue;
                    }
                    let (nx, ny) = (-dy / len * half, dx / len * half);
                    contours.push(vec![
                        place(a.0 + nx, a.1 + ny),
                        place(b.0 + nx, b.1 + ny),
                        place(b.0 - nx, b.1 - ny),
                        place(a.0 - nx, a.1 - ny),
                    ]);
                }
            }
        }
        self.prims.push(Primitive::Glyphs {
            contours,
            text: s.to_string(),
            ink: None,
        });
    }

    fn text(&mut self, at: P, s: &str, rot_deg: f64, centre: bool) {
        self.text_h(at, s, self.th, rot_deg, centre);
    }

    /// A linear dimension from `a` to `b`, the dimension line `off` mm to the left of the
    /// direction a to b. The text is the measured length in mm unless `label` is given.
    fn dim(&mut self, a: P, b: P, off: f64, label: Option<&str>) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            return;
        }
        let (ux, uy) = (dx / len, dy / len);
        let (nx, ny) = (-uy, ux);
        let (a2, b2) = (
            (a.0 + nx * off, a.1 + ny * off),
            (b.0 + nx * off, b.1 + ny * off),
        );
        let over = self.th * 0.4 * off.signum();
        self.thin(
            (
                a.0 + nx * self.th * 0.3 * off.signum(),
                a.1 + ny * self.th * 0.3 * off.signum(),
            ),
            (a2.0 + nx * over, a2.1 + ny * over),
        );
        self.thin(
            (
                b.0 + nx * self.th * 0.3 * off.signum(),
                b.1 + ny * self.th * 0.3 * off.signum(),
            ),
            (b2.0 + nx * over, b2.1 + ny * over),
        );
        self.line_w(a2, b2, self.lw * 0.5, None);
        let t = self.th * 0.35;
        for p in [a2, b2] {
            self.line_w(
                (p.0 - (ux + nx) * t, p.1 - (uy + ny) * t),
                (p.0 + (ux + nx) * t, p.1 + (uy + ny) * t),
                self.lw * 0.8,
                None,
            );
        }
        let text = label.map_or_else(|| format!("{len:.0}"), str::to_string);
        let mid = ((a2.0 + b2.0) / 2.0, (a2.1 + b2.1) / 2.0);
        let mut rot = uy.atan2(ux).to_degrees();
        // Keep the text upright: turn it half a circle when it would read upside down.
        if rot > 90.0 + 1e-9 {
            rot -= 180.0;
        } else if rot <= -90.0 + 1e-9 {
            rot += 180.0;
        }
        let side = if off >= 0.0 { 1.0 } else { -1.0 };
        let (tx, ty) = (
            mid.0 + nx * side * self.th * 0.7,
            mid.1 + ny * side * self.th * 0.7,
        );
        self.text_h((tx, ty), &text, self.th, rot, true);
    }

    fn finish(self) -> Drawing {
        let mut min = (f64::MAX, f64::MAX);
        let mut max = (f64::MIN, f64::MIN);
        let mut grow = |p: &Pt2| {
            min = (min.0.min(p.x), min.1.min(p.y));
            max = (max.0.max(p.x), max.1.max(p.y));
        };
        for prim in &self.prims {
            match prim {
                Primitive::Fill { outline, .. } => outline.iter().for_each(&mut grow),
                Primitive::Line { from, to, .. } => {
                    grow(from);
                    grow(to);
                }
                Primitive::Arc { centre, radius, .. } => {
                    grow(&Pt2::new(centre.x - radius, centre.y - radius));
                    grow(&Pt2::new(centre.x + radius, centre.y + radius));
                }
                Primitive::Glyphs { contours, .. } => contours.iter().flatten().for_each(&mut grow),
                Primitive::Symbol { origin, .. } => grow(origin),
            }
        }
        if min.0 > max.0 {
            min = (0.0, 0.0);
            max = (0.0, 0.0);
        }
        Drawing {
            primitives: self.prims,
            extents: Extents {
                min: Pt2::new(min.0, min.1),
                max: Pt2::new(max.0, max.1),
            },
        }
    }
}

/// Position in the plan of the point `o` outwards and `t` across at an end.
fn end_xy(layout: &Layout, side: Side, o: f64, t: f64) -> P {
    match side {
        Side::Left => (-layout.left_len_mm - o, t),
        Side::Right => (layout.right_len_mm + o, t),
    }
}

fn plan(
    input: &CulvertInput,
    layout: &Layout,
    section: &RoadSection,
    points: &[SettingOutPoint],
) -> Drawing {
    let mut c = Cv::new(160.0, 20.0);
    let sin = layout.skew_deg.to_radians().sin();
    let (cos_k, sin_k) = (layout.skew_deg.to_radians().cos(), sin);
    let width = input.barrel.outer_width_mm();
    // Barrel.
    let cell_centres: Vec<f64> = {
        let n = input.barrel.cells() as f64;
        let pitch = match input.barrel {
            Barrel::Pipe(p) => p.outside_diameter_mm() + p.clear_spacing_mm,
            Barrel::Box(b) => b.clear_span_mm + b.wall_mm,
        };
        (0..input.barrel.cells())
            .map(|i| ((n - 1.0) / 2.0 - i as f64) * pitch)
            .collect()
    };
    let (x0, x1) = (-layout.left_len_mm, layout.right_len_mm);
    match input.barrel {
        Barrel::Pipe(p) => {
            for y in &cell_centres {
                let (ro, ri) = (p.outside_diameter_mm() / 2.0, p.internal_diameter_mm / 2.0);
                for r in [ro, -ro] {
                    c.line((x0, y + r), (x1, y + r));
                }
                for r in [ri, -ri] {
                    c.thin((x0, y + r), (x1, y + r));
                }
            }
        }
        Barrel::Box(b) => {
            c.rect(x0, -width / 2.0, x1, width / 2.0);
            for i in 0..=b.cells {
                let y = -width / 2.0 + b.wall_mm + i as f64 * (b.clear_span_mm + b.wall_mm);
                c.thin((x0, y), (x1, y));
                c.thin((x0, y - b.wall_mm), (x1, y - b.wall_mm));
            }
        }
    }
    // Surround and excavation lines.
    let hs = layout.surround_width_mm / 2.0;
    let he = hs + input.bedding.working_space_mm;
    c.dashed((x0, hs), (x1, hs), 120.0, 60.0, None);
    c.dashed((x0, -hs), (x1, -hs), 120.0, 60.0, None);
    c.dashed((x0, he), (x1, he), 250.0, 100.0, Some(Colour::Grey));
    c.dashed((x0, -he), (x1, -he), 250.0, 100.0, Some(Colour::Grey));
    // Centre line of the culvert and of the road.
    c.dashed((x0 - 3000.0, 0.0), (x1 + 3000.0, 0.0), 600.0, 150.0, None);
    let road_len = (section
        .edge(Side::Left)
        .offset_m
        .abs()
        .max(section.edge(Side::Right).offset_m.abs())
        * 1000.0
        + 2500.0)
        .max(he + 1500.0);
    let rd = (cos_k, sin_k);
    c.dashed(
        (-rd.0 * road_len, -rd.1 * road_len),
        (rd.0 * road_len, rd.1 * road_len),
        600.0,
        150.0,
        None,
    );
    for side in [Side::Left, Side::Right] {
        let e = section.edge(side);
        let s = e.offset_m * 1000.0 / sin;
        c.dashed(
            (s - rd.0 * road_len, -rd.1 * road_len),
            (s + rd.0 * road_len, rd.1 * road_len),
            300.0,
            120.0,
            None,
        );
    }
    // End structures.
    for e in [&layout.left, &layout.right] {
        let sd = e.side;
        let hw = e.headwall_width_mm / 2.0;
        let s = &e.spec;
        let xy = |o: f64, t: f64| end_xy(layout, sd, o, t);
        // Headwall occupies o from -thickness to 0.
        c.poly(
            &[
                xy(0.0, -hw),
                xy(0.0, hw),
                xy(-s.headwall_thickness_mm, hw),
                xy(-s.headwall_thickness_mm, -hw),
            ],
            true,
        );
        for k in [1.0, -1.0] {
            let (sn, cs) = (e.splay_rad.sin(), e.splay_rad.cos());
            let w = &s.wing;
            c.poly(
                &[
                    xy(0.0, k * hw),
                    xy(e.wing_reach_mm, k * (hw + e.wing_spread_mm)),
                    xy(
                        e.wing_reach_mm - w.thickness_mm * sn,
                        k * (hw + e.wing_spread_mm + w.thickness_mm * cs),
                    ),
                    xy(-w.thickness_mm * sn, k * (hw + w.thickness_mm * cs)),
                ],
                true,
            );
        }
        let a = s.apron.length_mm;
        c.thin(
            xy(a, -e.apron_width_at_mm(a) / 2.0),
            xy(a, e.apron_width_at_mm(a) / 2.0),
        );
        let tw = s.toe.width_mm;
        c.rect_pts(&[
            xy(a, -e.overall_width_mm / 2.0),
            xy(a + tw, -e.overall_width_mm / 2.0),
            xy(a + tw, e.overall_width_mm / 2.0),
            xy(a, e.overall_width_mm / 2.0),
        ]);
        // Rip-rap outfall channel.
        let rr = s.riprap_length_mm;
        let half = e.apron_end_width_mm / 2.0;
        for k in [1.0, -1.0] {
            c.dashed(
                xy(a + tw, k * half),
                xy(a + tw + rr, k * half),
                150.0,
                80.0,
                None,
            );
        }
        c.dashed(
            xy(a + tw + rr, -half),
            xy(a + tw + rr, half),
            150.0,
            80.0,
            None,
        );
        c.text(xy(a + tw + rr / 2.0, 0.0), "RIP-RAP", 0.0, true);
    }
    // Chainage, flow arrow, setting-out points.
    c.text(
        (0.0, he + 700.0),
        &format!("CL ROAD {}", input.chainage_label.to_uppercase()),
        0.0,
        true,
    );
    let (from, to) = if layout.flow_dir > 0.0 {
        (x0 + 1500.0, x1 - 1500.0)
    } else {
        (x1 - 1500.0, x0 + 1500.0)
    };
    let ay = -he - 600.0;
    c.line((from, ay), (to, ay));
    let dir = if to > from { 1.0 } else { -1.0 };
    c.line((to, ay), (to - dir * 400.0, ay + 160.0));
    c.line((to, ay), (to - dir * 400.0, ay - 160.0));
    c.text(((from + to) / 2.0, ay - 400.0), "FLOW", 0.0, true);
    let local = [
        (x0, 0.0),
        (x1, 0.0),
        end_xy(
            layout,
            Side::Left,
            layout.left.wing_outer_corner_o_mm,
            layout.left.wing_outer_corner_t_mm,
        ),
        end_xy(
            layout,
            Side::Left,
            layout.left.wing_outer_corner_o_mm,
            -layout.left.wing_outer_corner_t_mm,
        ),
        end_xy(
            layout,
            Side::Right,
            layout.right.wing_outer_corner_o_mm,
            -layout.right.wing_outer_corner_t_mm,
        ),
        end_xy(
            layout,
            Side::Right,
            layout.right.wing_outer_corner_o_mm,
            layout.right.wing_outer_corner_t_mm,
        ),
    ];
    for (i, p) in local.iter().enumerate() {
        c.circle(*p, 70.0);
        c.text((p.0 + 130.0, p.1 + 130.0), &(i + 1).to_string(), 0.0, false);
    }
    let _ = points;
    // Dimensions.
    let ext = layout
        .left
        .overall_width_mm
        .max(layout.right.overall_width_mm)
        / 2.0
        + 700.0;
    c.dim(
        (x0, -ext),
        (0.0, -ext),
        -450.0,
        Some(&format!("{:.0}", layout.left_len_mm)),
    );
    c.dim(
        (0.0, -ext),
        (x1, -ext),
        -450.0,
        Some(&format!("{:.0}", layout.right_len_mm)),
    );
    c.dim(
        (x0, ext),
        (x1, ext),
        450.0,
        Some(&format!("BARREL {:.0}", layout.barrel_len_mm)),
    );
    let l = &layout.left;
    c.dim(
        (x0 - l.apron_and_toe_mm, -ext + 1300.0),
        (x0, -ext + 1300.0),
        -450.0,
        None,
    );
    let r = &layout.right;
    c.dim(
        (x1, -ext + 1300.0),
        (x1 + r.apron_and_toe_mm, -ext + 1300.0),
        -450.0,
        None,
    );
    c.dim(
        (x0 - l.apron_and_toe_mm - 500.0, -l.overall_width_mm / 2.0),
        (x0 - l.apron_and_toe_mm - 500.0, l.overall_width_mm / 2.0),
        -450.0,
        None,
    );
    c.text((x0, ext + 1100.0), "PLAN", 0.0, true);
    c.finish()
}

impl Cv {
    fn rect_pts(&mut self, pts: &[P]) {
        self.poly(pts, true);
    }
}

fn level_to_y(datum: f64, level_m: f64) -> f64 {
    (level_m - datum) * 1000.0
}

fn section_aa(input: &CulvertInput, layout: &Layout, section: &RoadSection, ogl: &Ogl) -> Drawing {
    let mut c = Cv::new(120.0, 16.0);
    let sin = layout.skew_deg.to_radians().sin();
    let (x0, x1) = (-layout.left_len_mm, layout.right_len_mm);
    let barrel = &input.barrel;
    let lowest = layout.invert_left_m.min(layout.invert_right_m)
        - (barrel.below_invert_mm() + input.bedding.bedding_mm + input.bedding.blinding_mm + 700.0)
            / 1000.0;
    let datum = (lowest * 10.0).floor() / 10.0;
    let y = |lv: f64| level_to_y(datum, lv);
    let inv = |s: f64| layout.invert_at_m(s);
    // Road surface and pavement.
    let mut surface = Vec::new();
    let mut base = Vec::new();
    let mut ground = Vec::new();
    let n = ((x1 - x0) / 100.0).ceil().max(1.0) as usize;
    for i in 0..=n {
        let s = x0 + (x1 - x0) * i as f64 / n as f64;
        let rho = s / 1000.0 * sin;
        surface.push((s, y(section.fill_surface_level_m(rho))));
        if section.level_at(rho).is_some() {
            base.push((
                s,
                y(section.fill_surface_level_m(rho) - section.pavement_depth_m),
            ));
        }
        ground.push((s, y(ogl.level_at(rho))));
    }
    c.poly(&surface, false);
    for w in base.windows(2) {
        c.dashed(w[0], w[1], 200.0, 100.0, None);
    }
    for w in ground.windows(2).step_by(2) {
        c.dashed(w[0], w[1], 150.0, 100.0, Some(Colour::Grey));
    }
    // Barrel.
    let below = barrel.below_invert_mm() / 1000.0;
    let crown = barrel.crown_above_invert_mm() / 1000.0;
    let bedding = input.bedding.bedding_mm / 1000.0;
    let blinding = input.bedding.blinding_mm / 1000.0;
    let sloped = |c: &mut Cv, off: f64, thin: bool| {
        let (a, b) = ((x0, y(inv(x0) + off)), (x1, y(inv(x1) + off)));
        if thin {
            c.thin(a, b);
        } else {
            c.line(a, b);
        }
    };
    sloped(&mut c, 0.0, true);
    sloped(&mut c, barrel.opening_height_mm() / 1000.0, true);
    sloped(&mut c, -below, false);
    sloped(&mut c, crown, false);
    sloped(&mut c, -below - bedding, true);
    sloped(&mut c, -below - bedding - blinding, true);
    let over = if barrel.is_pipe() {
        input.bedding.surround_over_crown_mm / 1000.0
    } else {
        0.0
    };
    if over > 0.0 {
        sloped(&mut c, crown + over, true);
    }
    // End structures.
    for e in [&layout.left, &layout.right] {
        let s = &e.spec;
        let sign = if e.side == Side::Left { -1.0 } else { 1.0 };
        let xo = |o: f64| if e.side == Side::Left { x0 - o } else { x1 + o };
        let (top, base_l, apron_top) =
            (y(e.headwall_top_m), y(e.headwall_base_m), y(e.apron_top_m));
        c.rect(
            xo(0.0).min(xo(-s.headwall_thickness_mm)),
            base_l,
            xo(0.0).max(xo(-s.headwall_thickness_mm)),
            top,
        );
        let a = s.apron.length_mm;
        c.rect(
            xo(0.0).min(xo(a)),
            apron_top - s.apron.thickness_mm,
            xo(0.0).max(xo(a)),
            apron_top,
        );
        let tw = s.toe.width_mm;
        c.rect(
            xo(a).min(xo(a + tw)),
            apron_top - e.toe_total_depth_mm,
            xo(a).max(xo(a + tw)),
            apron_top,
        );
        // Blinding under the structure.
        c.thin(
            (xo(0.0), base_l - s.blinding_mm),
            (xo(a + tw), base_l - s.blinding_mm),
        );
        // Rip-rap channel: bottom falls at the outfall slope away from the outlet and rises away
        // from the inlet.
        let outlet = (layout.flow_dir > 0.0) == (e.side == Side::Right);
        let rr = s.riprap_length_mm;
        let drop = rr * input.outfall_slope_pct / 100.0 * if outlet { -1.0 } else { 1.0 };
        c.dashed(
            (xo(a + tw), apron_top),
            (xo(a + tw + rr), apron_top + drop),
            120.0,
            60.0,
            None,
        );
        c.text(
            (xo(a + tw + rr / 2.0), apron_top + drop + 250.0),
            "RIP-RAP",
            0.0,
            true,
        );
        let _ = sign;
        // Level text.
        let label = format!("INV {:.3}", e.invert_m);
        c.text(
            (xo(-s.headwall_thickness_mm - 300.0), y(e.invert_m) - 380.0),
            &label,
            0.0,
            e.side == Side::Right,
        );
    }
    c.text(
        (0.0, y(inv(0.0)) - 500.0),
        &format!("INV CL {:.3}", layout.invert_cl_m),
        0.0,
        true,
    );
    c.text(
        (0.0, y(layout.frl_cl_m) + 250.0),
        &format!("FRL {:.3}", layout.frl_cl_m),
        0.0,
        true,
    );
    c.text(
        (
            x0 / 2.0,
            y(inv(x0 / 2.0)) + barrel.crown_above_invert_mm() + 350.0,
        ),
        &format!("GRADIENT {:.2} %", layout.gradient * 100.0),
        0.0,
        true,
    );
    c.dashed(
        (0.0, y(layout.frl_cl_m) + 1000.0),
        (0.0, base_y(layout, datum, input) - 300.0),
        300.0,
        120.0,
        None,
    );
    // Dimensions.
    let dy = base_y(layout, datum, input) - 1300.0;
    c.dim(
        (x0, dy),
        (0.0, dy),
        -450.0,
        Some(&format!("{:.0}", layout.left_len_mm)),
    );
    c.dim(
        (0.0, dy),
        (x1, dy),
        -450.0,
        Some(&format!("{:.0}", layout.right_len_mm)),
    );
    c.dim(
        (x0, dy - 1000.0),
        (x1, dy - 1000.0),
        -450.0,
        Some(&format!("BARREL {:.0}", layout.barrel_len_mm)),
    );
    c.text(
        (0.0, dy - 2200.0),
        &format!("SECTION A-A  DATUM {datum:.1} M"),
        0.0,
        true,
    );
    c.finish()
}

fn base_y(layout: &Layout, datum: f64, input: &CulvertInput) -> f64 {
    let low = layout.invert_left_m.min(layout.invert_right_m)
        - (input.barrel.below_invert_mm() + input.bedding.bedding_mm + input.bedding.blinding_mm)
            / 1000.0;
    let low = low
        .min(layout.left.headwall_base_m)
        .min(layout.right.headwall_base_m);
    level_to_y(datum, low)
}

fn section_dd(input: &CulvertInput, layout: &Layout) -> Drawing {
    let mut c = Cv::new(80.0, 12.0);
    let b = &input.bedding;
    let barrel = &input.barrel;
    let width = barrel.outer_width_mm();
    let hs = layout.surround_width_mm / 2.0;
    let below = barrel.below_invert_mm();
    // Level 0 is the top of the bedding, directly under the barrel.
    let crown = barrel.crown_above_invert_mm();
    let top_of_surround = if barrel.is_pipe() {
        crown + b.surround_over_crown_mm
    } else {
        crown
    };
    let frl = (layout.frl_cl_m - layout.invert_cl_m) * 1000.0 + below;
    // Blinding and bedding.
    c.rect(-hs, -b.bedding_mm - b.blinding_mm, hs, -b.bedding_mm);
    c.rect(-hs, -b.bedding_mm, hs, 0.0);
    match barrel {
        Barrel::Pipe(p) => {
            let n = p.cells as f64;
            let pitch = p.outside_diameter_mm() + p.clear_spacing_mm;
            for i in 0..p.cells {
                let cx = ((n - 1.0) / 2.0 - i as f64) * -pitch;
                let cy = p.outside_diameter_mm() / 2.0;
                c.circle((cx, cy), p.outside_diameter_mm() / 2.0);
                c.circle((cx, cy), p.internal_diameter_mm / 2.0);
            }
            c.rect(-hs, 0.0, hs, top_of_surround);
            // Chain of widths along the bottom of the pipe zone.
            let mut x = -hs;
            let yd = -b.bedding_mm - b.blinding_mm - 350.0;
            let seg = |c: &mut Cv, w: f64, x: &mut f64| {
                c.dim(
                    (*x, yd + 350.0),
                    (*x + w, yd + 350.0),
                    -350.0,
                    Some(&format!("{w:.0}")),
                );
                *x += w;
            };
            seg(&mut c, b.surround_side_mm, &mut x);
            for i in 0..p.cells {
                seg(&mut c, p.wall_mm, &mut x);
                seg(&mut c, p.internal_diameter_mm, &mut x);
                seg(&mut c, p.wall_mm, &mut x);
                if i + 1 < p.cells {
                    seg(&mut c, p.clear_spacing_mm, &mut x);
                }
            }
            seg(&mut c, b.surround_side_mm, &mut x);
            let r = p.outside_diameter_mm() / 2.0;
            c.dim(
                (hs + 300.0, 0.0),
                (hs + 300.0, r),
                -300.0,
                Some(&format!("{r:.0}")),
            );
            c.dim(
                (hs + 300.0, r),
                (hs + 300.0, 2.0 * r),
                -300.0,
                Some(&format!("{r:.0}")),
            );
        }
        Barrel::Box(x) => {
            let (ow, oh) = (width, x.clear_height_mm + x.top_slab_mm + x.bottom_slab_mm);
            c.rect(
                -ow / 2.0,
                -x.bottom_slab_mm + x.bottom_slab_mm,
                ow / 2.0,
                oh,
            );
            for i in 0..x.cells {
                let x0 = -ow / 2.0 + x.wall_mm + i as f64 * (x.clear_span_mm + x.wall_mm);
                c.rect(
                    x0,
                    x.bottom_slab_mm,
                    x0 + x.clear_span_mm,
                    x.bottom_slab_mm + x.clear_height_mm,
                );
            }
            c.dim(
                (-ow / 2.0, -b.bedding_mm - b.blinding_mm - 350.0),
                (ow / 2.0, -b.bedding_mm - b.blinding_mm - 350.0),
                -300.0,
                None,
            );
        }
    }
    c.dim(
        (-hs - 300.0, -b.bedding_mm),
        (-hs - 300.0, 0.0),
        300.0,
        None,
    );
    c.dim(
        (-hs - 300.0, -b.bedding_mm - b.blinding_mm),
        (-hs - 300.0, -b.bedding_mm),
        300.0,
        None,
    );
    c.line_w((-hs - 800.0, frl), (hs + 800.0, frl), 16.0, None);
    c.text(
        (hs + 850.0, frl),
        &format!("FRL {:.3}", layout.frl_cl_m),
        0.0,
        false,
    );
    c.dim(
        (hs + 700.0, crown),
        (hs + 700.0, frl),
        -300.0,
        Some(&format!("COVER {:.0}", layout.cover_cl_mm)),
    );
    c.text((0.0, frl + 500.0), "SECTION D-D", 0.0, true);
    c.finish()
}

fn front_elevation(input: &CulvertInput, layout: &Layout) -> Drawing {
    let mut c = Cv::new(90.0, 14.0);
    let e: &EndLayout = &layout.left;
    let s = &e.spec;
    let hw = e.headwall_width_mm / 2.0;
    // y = 0 at the base of the headwall.
    let h = s.headwall_height_mm;
    let apron_y = e.below_apron_mm;
    c.rect(-hw, 0.0, hw, h);
    // Barrel openings.
    match input.barrel {
        Barrel::Pipe(p) => {
            let n = p.cells as f64;
            let pitch = p.outside_diameter_mm() + p.clear_spacing_mm;
            for i in 0..p.cells {
                let cx = ((n - 1.0) / 2.0 - i as f64) * pitch;
                // The pipe invert is at the apron top, so its underside is the wall below it.
                let cy = apron_y - p.wall_mm + p.outside_diameter_mm() / 2.0;
                c.circle((cx, cy), p.outside_diameter_mm() / 2.0);
                c.circle((cx, cy), p.internal_diameter_mm / 2.0);
            }
        }
        Barrel::Box(b) => {
            let ow = input.barrel.outer_width_mm();
            for i in 0..b.cells {
                let x0 = -ow / 2.0 + b.wall_mm + i as f64 * (b.clear_span_mm + b.wall_mm);
                c.rect(
                    x0,
                    apron_y,
                    x0 + b.clear_span_mm,
                    apron_y + b.clear_height_mm,
                );
            }
        }
    }
    // Wings in elevation.
    let w = &s.wing;
    for k in [1.0, -1.0] {
        let (x_in, x_out) = (hw, hw + e.wing_spread_mm);
        c.poly(
            &[
                (k * x_in, 0.0),
                (k * x_out, 0.0),
                (k * x_out, w.height_at_end_mm),
                (k * x_in, w.height_at_headwall_mm),
            ],
            true,
        );
        c.thin(
            (k * x_out, w.height_at_end_mm),
            (
                k * (x_out + w.thickness_mm * e.splay_rad.cos()),
                w.height_at_end_mm,
            ),
        );
        c.dim(
            (k * x_in, -350.0),
            (k * x_out, -350.0),
            if k > 0.0 { -300.0 } else { 300.0 },
            None,
        );
    }
    // Apron top, apron underside, toe beam and blinding.
    let ow = e.overall_width_mm / 2.0;
    c.thin((-ow, apron_y), (ow, apron_y));
    c.thin(
        (-ow, apron_y - s.apron.thickness_mm),
        (ow, apron_y - s.apron.thickness_mm),
    );
    c.rect(
        -ow,
        apron_y - e.toe_total_depth_mm,
        ow,
        apron_y - s.apron.thickness_mm,
    );
    c.thin((-ow, -s.blinding_mm), (ow, -s.blinding_mm));
    // Weep holes.
    let n = s.weep.count_per_headwall;
    for i in 0..n {
        let x = -hw + (i as f64 + 1.0) * (2.0 * hw) / (n as f64 + 1.0);
        c.circle((x, apron_y + 150.0), s.weep.diameter_mm / 2.0);
    }
    // Dimensions.
    c.dim(
        (-hw, h + 100.0),
        (hw, h + 100.0),
        300.0,
        Some(&format!("HEADWALL {:.0}", e.headwall_width_mm)),
    );
    c.dim(
        (-ow, -s.blinding_mm - 700.0),
        (ow, -s.blinding_mm - 700.0),
        -300.0,
        Some(&format!("{:.0}", e.overall_width_mm)),
    );
    c.dim(
        (hw + e.wing_spread_mm + 700.0, 0.0),
        (hw + e.wing_spread_mm + 700.0, h),
        -300.0,
        None,
    );
    c.dim(
        (hw + e.wing_spread_mm + 700.0, 0.0),
        (hw + e.wing_spread_mm + 700.0, apron_y),
        700.0,
        None,
    );
    c.text((0.0, h + 800.0), "FRONT ELEVATION", 0.0, true);
    c.finish()
}

fn wing_section(layout: &Layout) -> Drawing {
    let mut c = Cv::new(80.0, 14.0);
    let e = &layout.left;
    let w = &e.spec.wing;
    c.poly(
        &[
            (0.0, 0.0),
            (w.length_mm, 0.0),
            (w.length_mm, w.height_at_end_mm),
            (0.0, w.height_at_headwall_mm),
        ],
        true,
    );
    c.dim((0.0, 0.0), (w.length_mm, 0.0), -300.0, None);
    c.dim((0.0, 0.0), (0.0, w.height_at_headwall_mm), 300.0, None);
    c.dim(
        (w.length_mm, 0.0),
        (w.length_mm, w.height_at_end_mm),
        -300.0,
        None,
    );
    c.dim(
        (0.0, w.height_at_headwall_mm),
        (w.length_mm, w.height_at_end_mm),
        350.0,
        None,
    );
    c.text(
        (w.length_mm / 2.0, w.height_at_headwall_mm + 900.0),
        "WING WALL SECTION C-C",
        0.0,
        true,
    );
    c.text(
        (w.length_mm / 2.0, w.height_at_headwall_mm / 2.0),
        &format!("{:.0} THICK", w.thickness_mm),
        0.0,
        true,
    );
    c.finish()
}

fn toe_section(layout: &Layout) -> Drawing {
    let mut c = Cv::new(60.0, 10.0);
    let e = &layout.left;
    let s = &e.spec;
    let (a, tw, ta) = (s.apron.length_mm, s.toe.width_mm, s.apron.thickness_mm);
    // x along the axis from the headwall face, y up from the underside of the apron.
    c.poly(&[(0.0, 0.0), (0.0, ta), (a, ta), (a, 0.0)], false);
    c.rect(a, -s.toe.depth_mm, a + tw, ta);
    c.thin(
        (-100.0, -s.toe.depth_mm - s.blinding_mm),
        (a + tw + 100.0, -s.toe.depth_mm - s.blinding_mm),
    );
    c.thin((-100.0, -s.blinding_mm), (a, -s.blinding_mm));
    c.dim(
        (a, -s.toe.depth_mm),
        (a + tw, -s.toe.depth_mm),
        -250.0,
        None,
    );
    c.dim((a + tw, -s.toe.depth_mm), (a + tw, 0.0), -250.0, None);
    c.dim(
        (a + tw, -s.toe.depth_mm),
        (a + tw, ta),
        -550.0,
        Some(&format!("{:.0}", e.toe_total_depth_mm)),
    );
    c.dim((0.0, ta), (a, ta), 250.0, None);
    c.text((a / 2.0, ta + 700.0), "TOE BEAM SECTION B-B", 0.0, true);
    c.finish()
}

/// Builds every drawing. `points` are the setting-out points (empty when none were given).
pub fn draw_all(
    input: &CulvertInput,
    layout: &Layout,
    points: &[SettingOutPoint],
) -> CulvertDrawings {
    let section = input.road.section().ok();
    let ogl = Ogl::from_points(&input.ogl).ok();
    match (section, ogl) {
        (Some(section), Some(ogl)) => CulvertDrawings {
            plan: plan(input, layout, &section, points),
            section_aa: section_aa(input, layout, &section, &ogl),
            section_dd: section_dd(input, layout),
            front_elevation: front_elevation(input, layout),
            wing_section_cc: wing_section(layout),
            toe_section_bb: toe_section(layout),
        },
        _ => {
            let empty = Drawing::default();
            CulvertDrawings {
                plan: empty.clone(),
                section_aa: empty.clone(),
                section_dd: empty.clone(),
                front_elevation: empty.clone(),
                wing_section_cc: empty.clone(),
                toe_section_bb: empty,
            }
        }
    }
}
