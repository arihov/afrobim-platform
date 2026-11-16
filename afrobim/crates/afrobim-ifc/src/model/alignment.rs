//! The complete alignment export, switched on by [`AlignmentExport::Geometry`] and
//! [`AlignmentExport::Linear`]. The README, "Alignments, complete", gives every entity and rule
//! with the page of the IFC 4.3 documentation it comes from.
//!
//! Everything here is written in the file's length unit, the millimetre, and in radians.
//!
//! * The business logic: `IfcAlignmentHorizontal` and `IfcAlignmentVertical`, nested by the
//!   alignment, each nesting `IfcAlignmentSegment`s with their design parameters and ending in a
//!   segment of length 0.
//! * The geometry: an `IfcCompositeCurve` of `IfcCurveSegment`s for the horizontal layout and,
//!   when there is a profile, an `IfcGradientCurve` on it for the vertical one, with the
//!   parent curves `IfcLine`, `IfcCircle`, `IfcClothoid` and `IfcPolynomialCurve`.
//! * The referents: the start station, one per station equation, one per superelevation event.
//! * [`AlignGeo`]: what a linear placement needs to know about an alignment, and the arithmetic
//!   that turns a position in the world into distance along, lateral offset and vertical offset.

use std::f64::consts::FRAC_PI_2;

use afrobim_alignment::{format_station, wrap_pi, Alignment, AlignmentElement, ElementKind, StationDirection, VerticalCurve};
use afrobim_kernel::Element;

use super::{valid_site, Model, Pose};
use crate::rot::{self, Rot, V3};
use crate::step::{list, reals, refs, step_fine, step_real, step_string};
use crate::{clip, element_global_id, AlignmentExport, Value};

/// A joint is continuous in position when the gap is at most 1 mm (IfcOpenShell's default for the
/// same decision), in direction when the headings differ by at most 1e-7 rad, in curvature when
/// the two curvatures differ by at most 1e-9 /m.
const GAP_M: f64 = 1e-3;
const HEADING_RAD: f64 = 1e-7;
const CURVATURE_PER_M: f64 = 1e-9;
/// Vertical pieces shorter than this (m) are dropped.
const MIN_PIECE_M: f64 = 1e-6;

/// A real with every digit a double has, for the few numbers that are far smaller than the
/// micro-unit `step_real` rounds to (a polynomial coefficient per millimetre).
pub(crate) fn step_exact(value: f64) -> String {
    if !value.is_finite() {
        return "0.".to_string();
    }
    if value == 0.0 {
        return "0.".to_string();
    }
    let text = format!("{value:E}");
    let (mantissa, exponent) = text.split_once('E').unwrap_or((&text, "0"));
    let mantissa = if mantissa.contains('.') { mantissa.to_string() } else { format!("{mantissa}.") };
    if exponent == "0" {
        mantissa
    } else {
        format!("{mantissa}E{exponent}")
    }
}

// ───────────────────────────────────────────────────────── the horizontal curve segments ──

/// The parent curve of a horizontal segment, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum HParent {
    Line,
    Circle { radius_mm: f64 },
    Clothoid { constant_mm: f64 },
}

/// One `IfcCurveSegment` of the horizontal layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HSeg {
    pub x_mm: f64,
    pub y_mm: f64,
    pub heading: f64,
    pub parent: HParent,
    pub start_mm: f64,
    pub length_mm: f64,
}

/// How an alignment element is written as a curve segment (IFC 4.3, "Alignment Geometry -
/// Segments", and the partial templates "Linear Segment", "Arc Segment", "Clothoid Transition
/// Segment").
///
/// * A line: an `IfcLine` through the origin along +X, from 0 to the length.
/// * An arc: an `IfcCircle` of the radius at the origin; the segment length is negative for a
///   clockwise (right-hand) arc, as the sense of the segment is the sign of `SegmentLength`.
/// * A clothoid: `IfcClothoid` with the constant A = sign(dk) sqrt(L / |dk|) where dk = k1 - k0 is
///   the change of curvature over the length L. The curvature on the parent is s / A|A|, so the
///   segment starts at s0 = k0 L / dk on it and runs L from there. The tangent end of a spiral
///   from a curve starts at s0 = -L.
pub(crate) fn h_segment(el: &AlignmentElement) -> HSeg {
    let k0 = el.curvature_start / 1000.0;
    let k1 = el.curvature_end / 1000.0;
    let length_mm = el.length * 1000.0;
    let (parent, start_mm, length_mm) = match el.kind() {
        ElementKind::Line => (HParent::Line, 0.0, length_mm),
        ElementKind::Arc => (HParent::Circle { radius_mm: 1.0 / k0.abs() }, 0.0, length_mm * k0.signum()),
        ElementKind::Spiral => {
            let d = k1 - k0;
            (HParent::Clothoid { constant_mm: d.signum() * (length_mm / d.abs()).sqrt() }, k0 * length_mm / d, length_mm)
        }
    };
    HSeg { x_mm: el.x * 1000.0, y_mm: el.y * 1000.0, heading: el.heading_rad, parent, start_mm, length_mm }
}

/// The transition code at the end of `a` into the start of `b`: the continuity the documentation
/// asks `IfcCurveSegment.Transition` to state (IfcTransitionCode).
fn h_transition(a: &AlignmentElement, b: &AlignmentElement) -> &'static str {
    let (x, y, h) = a.end();
    if (b.x - x).hypot(b.y - y) > GAP_M {
        return "DISCONTINUOUS";
    }
    if wrap_pi(b.heading_rad - h).abs() > HEADING_RAD {
        return "CONTINUOUS";
    }
    if (b.curvature_start - a.curvature_end).abs() > CURVATURE_PER_M {
        "CONTSAMEGRADIENT"
    } else {
        "CONTSAMEGRADIENTSAMECURVATURE"
    }
}

// ─────────────────────────────────────────────────────────── the vertical curve segments ──

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum VKind {
    Line,
    Parabola,
    Circle,
}

/// One segment of the vertical layout, in metres (internal stations) and ratios.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct VSeg {
    pub kind: VKind,
    /// Distance along the alignment at the start, m.
    pub d0: f64,
    /// Horizontal length, m.
    pub length: f64,
    /// Level at the start, m.
    pub z0: f64,
    pub g0: f64,
    pub g1: f64,
    /// Signed radius (m) for the business logic: positive for a sag. `None` on a line.
    pub radius: Option<f64>,
    /// Level at the end, m.
    pub z1: f64,
}

impl VSeg {
    fn curvature_start(&self) -> f64 {
        match self.kind {
            VKind::Line => 0.0,
            VKind::Parabola => (self.g1 - self.g0) / self.length / (1.0 + self.g0 * self.g0).powf(1.5),
            VKind::Circle => 1.0 / self.radius.unwrap_or(f64::INFINITY),
        }
    }
    fn curvature_end(&self) -> f64 {
        match self.kind {
            VKind::Parabola => (self.g1 - self.g0) / self.length / (1.0 + self.g1 * self.g1).powf(1.5),
            _ => self.curvature_start(),
        }
    }
}

/// The vertical layout of an alignment: the profile cut into its straights and curves and
/// clipped to the alignment's length. The straights are `CONSTANTGRADIENT`; a parabola, or each
/// half of an asymmetric parabola, is a `PARABOLICARC`; a circular vertical curve is a
/// `CIRCULARARC`.
pub(crate) fn vertical_segments(a: &Alignment) -> Option<(Vec<VSeg>, (f64, f64))> {
    let profile = a.profile.as_ref()?;
    profile.validate().ok()?;
    let (s0, s1) = profile.range()?;
    let (lo, hi) = (a.start_station_m.max(s0), a.end_station_m().min(s1));
    if hi - lo < MIN_PIECE_M {
        return None;
    }
    // Raw pieces in internal stations.
    let mut pieces: Vec<(VKind, f64, f64)> = Vec::new();
    let extents = profile.curve_extents().ok()?;
    let mut pos = s0;
    for ex in &extents {
        if ex.start_station_m > pos + 1e-9 {
            pieces.push((VKind::Line, pos, ex.start_station_m));
        }
        let pvi = &profile.pvis[ex.pvi];
        match pvi.curve {
            VerticalCurve::Parabola { .. } => pieces.push((VKind::Parabola, ex.start_station_m, ex.end_station_m)),
            VerticalCurve::AsymmetricParabola { .. } => {
                pieces.push((VKind::Parabola, ex.start_station_m, pvi.station_m));
                pieces.push((VKind::Parabola, pvi.station_m, ex.end_station_m));
            }
            VerticalCurve::Circular { .. } => pieces.push((VKind::Circle, ex.start_station_m, ex.end_station_m)),
            VerticalCurve::None => {}
        }
        pos = ex.end_station_m;
    }
    if s1 > pos + 1e-9 {
        pieces.push((VKind::Line, pos, s1));
    }
    let mut out = Vec::new();
    for (kind, from, to) in pieces {
        let (b0, b1) = (from.max(lo), to.min(hi));
        if b1 - b0 < MIN_PIECE_M {
            continue;
        }
        let p0 = profile.point_at(b0).ok()?;
        let p1 = profile.point_at(b1).ok()?;
        let length = b1 - b0;
        let (g0, g1) = (p0.grade, p1.grade);
        let kind = if kind != VKind::Line && (g1 - g0).abs() < 1e-12 { VKind::Line } else { kind };
        let g1 = if kind == VKind::Line { g0 } else { g1 };
        let radius = match kind {
            VKind::Line => None,
            // The radius at the vertex of a parabola, positive for a sag (IfcAlignmentVerticalSegment).
            VKind::Parabola => Some(length / (g1 - g0)),
            VKind::Circle => {
                let (a0, a1) = (g0.atan(), g1.atan());
                let r = length / (a1.sin() - a0.sin()).abs();
                Some(if a1 > a0 { r } else { -r })
            }
        };
        out.push(VSeg { kind, d0: b0 - a.start_station_m, length, z0: p0.elevation_m, g0, g1, radius, z1: p1.elevation_m });
    }
    if out.is_empty() {
        return None;
    }
    Some((out, (lo, hi)))
}

/// The transition code between two vertical segments.
fn v_transition(a: &VSeg, b: &VSeg) -> &'static str {
    let end = a.d0 + a.length;
    if (b.d0 - end).abs() > GAP_M || (b.z0 - a.z1).abs() > GAP_M {
        return "DISCONTINUOUS";
    }
    if (b.g0 - a.g1).abs() > 1e-9 {
        return "CONTINUOUS";
    }
    let (ka, kb) = (a.curvature_end(), b.curvature_start());
    if (ka - kb).abs() > CURVATURE_PER_M.max(1e-6 * ka.abs()) {
        "CONTSAMEGRADIENT"
    } else {
        "CONTSAMEGRADIENTSAMECURVATURE"
    }
}

/// Arc length (mm) of the parabola `y = g0 x + c x^2` over a horizontal length `h` (mm) ending at
/// gradient `g1`. `SegmentLength` of a vertical curve segment is this length along the curve, as
/// IfcOpenShell reads it.
fn parabola_arc_length(h: f64, g0: f64, g1: f64) -> f64 {
    if (g1 - g0).abs() < 1e-12 {
        return h * (1.0 + g0 * g0).sqrt();
    }
    let f = |u: f64| u * (1.0 + u * u).sqrt() + u.asinh();
    h * (f(g1) - f(g0)) / (2.0 * (g1 - g0))
}

// ───────────────────────────────────────────────────────────────── linear placement maths ──

/// What a linear placement needs of one written alignment.
#[derive(Debug, Clone)]
pub(crate) struct AlignGeo {
    pub alignment: Alignment,
    /// The `IfcAlignment` and its placement (the placement of whatever is placed on it).
    pub ifc: u32,
    pub placement: u32,
    /// The horizontal `IfcCompositeCurve` and, with a profile, the `IfcGradientCurve`.
    pub composite: u32,
    pub gradient: Option<u32>,
    /// The internal stations the gradient curve covers.
    pub vertical: Option<(f64, f64)>,
}

/// The frame of the alignment's curve at a station: the point (mm) and the unit tangent, left
/// normal and "normal" (perpendicular to the tangent in the vertical plane, positive Z), which
/// are the axes an `IfcAxis2PlacementLinear` without `Axis` and `RefDirection` has, and the
/// directions its offsets are measured in (IfcPointByDistanceExpression, IfcAxis2PlacementLinear).
#[derive(Debug, Clone, Copy)]
pub(crate) struct CurveFrame {
    pub origin_mm: V3,
    pub t: V3,
    pub left: V3,
    pub n: V3,
}

impl CurveFrame {
    pub fn rotation(&self) -> Rot {
        rot::from_columns(self.t, self.left, self.n)
    }
}

/// What the arithmetic of a linear placement found.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Solved {
    pub distance_mm: f64,
    pub lateral_mm: f64,
    pub vertical_mm: f64,
    pub longitudinal_mm: f64,
    /// Placed on the gradient curve (the profile covers the station); otherwise on the 2D curve.
    pub gradient: bool,
    /// The frame of the linear placement: the axes of the curve at the point.
    pub frame: Rot,
    /// The turn from the frame of the linear placement to the frame the element had.
    pub q: Rot,
}

impl AlignGeo {
    pub fn covers_vertical(&self, station_m: f64) -> bool {
        self.gradient.is_some() && self.vertical.is_some_and(|(a, b)| station_m >= a - 1e-9 && station_m <= b + 1e-9)
    }

    /// The curve frame at an internal station, on the gradient curve or on the 2D curve.
    pub fn frame_at(&self, station_m: f64, gradient: bool) -> Option<CurveFrame> {
        let pt = self.alignment.point_at(station_m)?;
        let (sin_h, cos_h) = pt.heading_rad.sin_cos();
        let (mut theta, mut z) = (0.0, 0.0);
        if gradient {
            let profile = self.alignment.profile.as_ref()?;
            theta = profile.grade_at(station_m).ok()?.atan();
            z = profile.elevation_at(station_m).ok()? * 1000.0;
        }
        let (sin_t, cos_t) = theta.sin_cos();
        Some(CurveFrame {
            origin_mm: [pt.x * 1000.0, pt.y * 1000.0, z],
            t: [cos_t * cos_h, cos_t * sin_h, sin_t],
            left: [-sin_h, cos_h, 0.0],
            n: [-sin_t * cos_h, -sin_t * sin_h, cos_t],
        })
    }

    /// Distance along, lateral offset (positive to the left), vertical offset and the turn for an
    /// element standing at `pose` (mm, degrees about +Z) that was placed at `station_m`,
    /// `offset_m` (positive to the right) on this alignment.
    ///
    /// The station and offset are taken as they are when they reproduce the position to a
    /// micrometre; otherwise the nearest point of the alignment decides them. Either way the
    /// three offsets are the components of the position along the three axes of the curve frame,
    /// so the placement reaches the position exactly.
    pub fn solve(&self, pose: Pose, station_m: f64, offset_m: f64) -> Option<Solved> {
        let here = self.alignment.point_at(station_m)?;
        let (px, py) = here.offset(offset_m);
        let station = if (px * 1000.0 - pose.x).hypot(py * 1000.0 - pose.y) <= 1e-3 {
            station_m
        } else {
            self.alignment.station_offset(pose.x / 1000.0, pose.y / 1000.0)?.0
        };
        let gradient = self.covers_vertical(station);
        let frame = self.frame_at(station, gradient)?;
        let r = [pose.x - frame.origin_mm[0], pose.y - frame.origin_mm[1], pose.z - frame.origin_mm[2]];
        let q = rot::mul_t(&frame.rotation(), &rot::about_z(pose.rotation_deg));
        let frame_rotation = frame.rotation();
        Some(Solved {
            distance_mm: (station - self.alignment.start_station_m) * 1000.0,
            lateral_mm: rot::dot(r, frame.left),
            vertical_mm: rot::dot(r, frame.n),
            longitudinal_mm: rot::dot(r, frame.t),
            gradient,
            frame: frame_rotation,
            q,
        })
    }
}

/// The alignment, station and offset a site placement names.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Link {
    pub alignment: String,
    pub station_m: f64,
    pub offset_m: f64,
}

/// The link of an element whose site placement carries an alignment, a station and an offset.
pub(crate) fn site_link(e: &Element) -> Option<Link> {
    let s = valid_site(e)?;
    let alignment = s.alignment.clone().filter(|a| !a.is_empty())?;
    let station_m = s.station_m.filter(|v| v.is_finite())?;
    let offset_m = s.offset_m.filter(|v| v.is_finite())?;
    Some(Link { alignment, station_m, offset_m })
}

// ──────────────────────────────────────────────────────────────────────────── the writer ──

/// A property that is an enumeration: its name, the allowed values and the value.
struct Enumerated<'a> {
    name: &'a str,
    enumeration: &'a str,
    allowed: &'a [&'a str],
    value: &'a str,
}

impl Model<'_> {
    fn point2(&mut self, x: f64, y: f64) -> u32 {
        self.w.data.add("IFCCARTESIANPOINT", &reals(&[x, y]))
    }

    fn direction2(&mut self, x: f64, y: f64) -> u32 {
        self.w.data.add("IFCDIRECTION", &format!("({},{})", step_fine(x), step_fine(y)))
    }

    /// `IfcAxis2Placement2D` at (x, y) with its X axis along `heading`.
    fn axes2(&mut self, x: f64, y: f64, heading: f64) -> u32 {
        let point = self.point2(x, y);
        let direction = self.direction2(heading.cos(), heading.sin());
        self.w.data.add("IFCAXIS2PLACEMENT2D", &format!("#{point},#{direction}"))
    }

    /// `IfcLine` through the origin along +X (the parent of every straight).
    fn unit_line(&mut self) -> u32 {
        let origin = self.point2(0.0, 0.0);
        let direction = self.direction2(1.0, 0.0);
        let vector = self.w.data.add("IFCVECTOR", &format!("#{direction},1."));
        self.w.data.add("IFCLINE", &format!("#{origin},#{vector}"))
    }

    fn curve_segment(&mut self, code: &str, placement: u32, start: f64, length: f64, parent: u32) -> u32 {
        self.w.data.add(
            "IFCCURVESEGMENT",
            &format!(".{code}.,#{placement},IFCLENGTHMEASURE({}),IFCLENGTHMEASURE({}),#{parent}", step_real(start), step_real(length)),
        )
    }

    /// The curve segment of one horizontal element.
    fn horizontal_curve_segment(&mut self, el: &AlignmentElement, code: &str) -> u32 {
        let seg = h_segment(el);
        let placement = self.axes2(seg.x_mm, seg.y_mm, seg.heading);
        let origin_axes = |m: &mut Self| m.axes2(0.0, 0.0, 0.0);
        let parent = match seg.parent {
            HParent::Line => self.unit_line(),
            HParent::Circle { radius_mm } => {
                let position = origin_axes(self);
                self.w.data.add("IFCCIRCLE", &format!("#{position},{}", step_real(radius_mm)))
            }
            HParent::Clothoid { constant_mm } => {
                let position = origin_axes(self);
                self.w.data.add("IFCCLOTHOID", &format!("#{position},{}", step_real(constant_mm)))
            }
        };
        self.curve_segment(code, placement, seg.start_mm, seg.length_mm, parent)
    }

    /// The curve segment of one vertical segment: in (distance along, level) coordinates.
    fn vertical_curve_segment(&mut self, s: &VSeg, code: &str) -> u32 {
        let (d0, z0, h) = (s.d0 * 1000.0, s.z0 * 1000.0, s.length * 1000.0);
        let theta = s.g0.atan();
        let placement = self.axes2(d0, z0, theta);
        match s.kind {
            VKind::Line => {
                let parent = self.unit_line();
                self.curve_segment(code, placement, 0.0, h * (1.0 + s.g0 * s.g0).sqrt(), parent)
            }
            VKind::Parabola => {
                // y = g0 x + c x^2 with x the distance along from the segment's start.
                let c = (s.g1 - s.g0) / (2.0 * h);
                let position = self.axes2(0.0, 0.0, 0.0);
                let parent = self.w.data.add("IFCPOLYNOMIALCURVE", &format!("#{position},(0.,1.),(0.,{},{}),$", step_exact(s.g0), step_exact(c)));
                self.curve_segment(code, placement, 0.0, parabola_arc_length(h, s.g0, s.g1), parent)
            }
            VKind::Circle => {
                // The circle is placed so that its point at the start of the segment is the origin
                // of the parent's coordinates, as in the buildingSMART alignment samples.
                let (a0, a1) = (s.g0.atan(), s.g1.atan());
                let (r, centre, start_angle, end_angle) = if a0 < a1 {
                    let r = h / (a1.sin() - a0.sin());
                    (r, (-r * a0.sin(), r * a0.cos()), a0 + 3.0 * FRAC_PI_2, a1 + 3.0 * FRAC_PI_2)
                } else {
                    let r = h / (a0.sin() - a1.sin());
                    (r, (r * a0.sin(), -r * a0.cos()), a0 + FRAC_PI_2, a1 + FRAC_PI_2)
                };
                let position = self.axes2(centre.0, centre.1, 0.0);
                let parent = self.w.data.add("IFCCIRCLE", &format!("#{position},{}", step_real(r)));
                self.curve_segment(code, placement, r * start_angle, r * (end_angle - start_angle), parent)
            }
        }
    }

    /// The zero-length segment that ends a curve: a line placed at the end point and direction.
    fn zero_length_curve_segment(&mut self, x: f64, y: f64, heading: f64) -> u32 {
        let placement = self.axes2(x, y, heading);
        let parent = self.unit_line();
        self.curve_segment("DISCONTINUOUS", placement, 0.0, 0.0, parent)
    }

    /// An `IfcAlignmentSegment` with its design parameters and its 'Axis' / 'Segment'
    /// representation of the matching curve segment ("Alignment Geometry - Segments").
    fn alignment_segment(&mut self, owner: &str, name: &str, index: usize, placement: u32, curve_segment: u32, design: u32) -> u32 {
        let context = self.axis_context.expect("the Axis sub-context exists");
        let representation = self.w.data.add("IFCSHAPEREPRESENTATION", &format!("#{context},'Axis','Segment',(#{curve_segment})"));
        let shape = self.w.data.add("IFCPRODUCTDEFINITIONSHAPE", &format!("$,$,(#{representation})"));
        let guid = step_string(&self.w.guid(&format!("{owner}/{name}/segment/{index}")));
        self.w.data.add("IFCALIGNMENTSEGMENT", &format!("{guid},$,{},$,$,#{placement},#{shape},#{design}", step_string(&format!("{}{}", name[..1].to_uppercase(), index + 1))))
    }

    /// The 'Axis' sub-context, and the 'FootPrint' one, created with the first alignment.
    fn alignment_contexts(&mut self) -> (u32, u32) {
        let axis = match self.axis_context {
            Some(c) => c,
            None => {
                let c = self.w.data.add("IFCGEOMETRICREPRESENTATIONSUBCONTEXT", &format!("'Axis','Model',*,*,*,*,#{},$,.MODEL_VIEW.,$", self.w.context));
                self.axis_context = Some(c);
                c
            }
        };
        let footprint = match self.footprint_context {
            Some(c) => c,
            None => {
                let c = self.w.data.add("IFCGEOMETRICREPRESENTATIONSUBCONTEXT", &format!("'FootPrint','Model',*,*,*,*,#{},$,.MODEL_VIEW.,$", self.w.context));
                self.footprint_context = Some(c);
                c
            }
        };
        (axis, footprint)
    }

    /// A property set of an object with single values and enumerated values.
    fn mixed_property_set(&mut self, object: u32, owner: &str, name: &str, singles: &[(String, Value)], enums: &[Enumerated]) {
        let mut ids: Vec<u32> = singles
            .iter()
            .map(|(key, value)| self.w.data.add("IFCPROPERTYSINGLEVALUE", &format!("{},$,{},$", step_string(&clip(key)), value.encode())))
            .collect();
        for e in enums {
            let enumeration = match self.enumerations.get(e.enumeration) {
                Some(id) => *id,
                None => {
                    let values = list(e.allowed.iter().map(|v| format!("IFCLABEL({})", step_string(v))));
                    let id = self.w.data.add("IFCPROPERTYENUMERATION", &format!("{},{values},$", step_string(e.enumeration)));
                    self.enumerations.insert(e.enumeration.to_string(), id);
                    id
                }
            };
            ids.push(self.w.data.add(
                "IFCPROPERTYENUMERATEDVALUE",
                &format!("{},$,(IFCLABEL({})),#{enumeration}", step_string(e.name), step_string(e.value)),
            ));
        }
        let guid = step_string(&self.w.guid(&format!("{owner}/pset/{name}")));
        let set = self.w.data.add("IFCPROPERTYSET", &format!("{guid},$,{},$,{}", step_string(name), refs(&ids)));
        self.w.define_of(object, owner, name, set);
    }

    /// An `IfcLinearPlacement` for a referent: `PlacementRelTo` the alignment's own placement,
    /// distance along on the basis curve, no offsets, and the point and frame of the curve there
    /// as the fallback position.
    fn referent_placement(&mut self, geo: &AlignGeo, station_m: f64, gradient: bool) -> u32 {
        let basis = if gradient { geo.gradient.unwrap_or(geo.composite) } else { geo.composite };
        let distance = (station_m - geo.alignment.start_station_m) * 1000.0;
        let point = self.w.data.add(
            "IFCPOINTBYDISTANCEEXPRESSION",
            &format!("IFCLENGTHMEASURE({}),0.,0.,0.,#{basis}", step_real(distance)),
        );
        let axes = self.w.data.add("IFCAXIS2PLACEMENTLINEAR", &format!("#{point},$,$"));
        let fallback = match geo.frame_at(station_m, gradient && geo.gradient.is_some()) {
            Some(f) => Some(self.w.axes_of_turn(&f.rotation(), f.origin_mm)),
            None => None,
        };
        self.w.data.add("IFCLINEARPLACEMENT", &format!("#{},#{axes},{}", geo.placement, fallback.map_or("$".to_string(), |f| format!("#{f}"))))
    }

    /// One alignment, complete. Returns its index in `self.geo`.
    pub(super) fn alignment_full(&mut self, e: &Element, a: &Alignment, notes: &mut Vec<String>) -> usize {
        let owner = format!("element/{}", e.id.0);
        let (axis_context, footprint_context) = self.alignment_contexts();
        // Alignment coordinates are plan metres in the same system as the site placements; the
        // placement of the alignment is the file's origin.
        let origin = self.w.data.add("IFCCARTESIANPOINT", "(0.,0.,0.)");
        let axes = self.w.data.add("IFCAXIS2PLACEMENT3D", &format!("#{origin},$,$"));
        let placement = self.w.data.add("IFCLOCALPLACEMENT", &format!("$,#{axes}"));

        // ── horizontal geometry
        let count = a.elements.len();
        let mut h_curve_segments = Vec::with_capacity(count + 1);
        let last = a.elements[count - 1];
        let (end_x, end_y, end_heading) = last.end();
        for (i, el) in a.elements.iter().enumerate() {
            // The zero-length segment lies at the end of the last element, with no curvature.
            let code = if i + 1 < count {
                h_transition(el, &a.elements[i + 1])
            } else if el.curvature_end.abs() <= CURVATURE_PER_M {
                "CONTSAMEGRADIENTSAMECURVATURE"
            } else {
                "CONTSAMEGRADIENT"
            };
            h_curve_segments.push(self.horizontal_curve_segment(el, code));
        }
        h_curve_segments.push(self.zero_length_curve_segment(end_x * 1000.0, end_y * 1000.0, end_heading));
        let composite = self.w.data.add("IFCCOMPOSITECURVE", &format!("{},.F.", refs(&h_curve_segments)));

        // ── vertical geometry
        let vertical = vertical_segments(a);
        if a.profile.is_some() && vertical.is_none() {
            notes.push(format!("{}: the profile does not cover the alignment (or cannot be read); no vertical layout exported", e.name));
        }
        let mut v_curve_segments = Vec::new();
        let mut gradient = None;
        if let Some((segs, (lo, hi))) = &vertical {
            for (i, s) in segs.iter().enumerate() {
                let code = if i + 1 < segs.len() {
                    v_transition(s, &segs[i + 1])
                } else if s.curvature_end().abs() <= CURVATURE_PER_M {
                    "CONTSAMEGRADIENTSAMECURVATURE"
                } else {
                    "CONTSAMEGRADIENT"
                };
                v_curve_segments.push(self.vertical_curve_segment(s, code));
            }
            let last = segs[segs.len() - 1];
            let placement2 = self.axes2((last.d0 + last.length) * 1000.0, last.z1 * 1000.0, last.g1.atan());
            let line = self.unit_line();
            v_curve_segments.push(self.curve_segment("DISCONTINUOUS", placement2, 0.0, 0.0, line));
            gradient = Some(self.w.data.add("IFCGRADIENTCURVE", &format!("{},.F.,#{composite},$", refs(&v_curve_segments))));
            if *lo > a.start_station_m + 1e-6 || *hi < a.end_station_m() - 1e-6 {
                notes.push(format!(
                    "{}: the profile covers stations {} to {} of the alignment's {} to {}; the gradient curve covers that part only, and elements outside it are placed on the 2D curve",
                    e.name,
                    format_station(*lo, 3),
                    format_station(*hi, 3),
                    format_station(a.start_station_m, 3),
                    format_station(a.end_station_m(), 3)
                ));
            }
        }

        // ── representations
        let mut representations = Vec::new();
        match gradient {
            Some(g) => {
                representations.push(self.w.data.add("IFCSHAPEREPRESENTATION", &format!("#{axis_context},'Axis','Curve3D',(#{g})")));
                representations.push(self.w.data.add("IFCSHAPEREPRESENTATION", &format!("#{footprint_context},'FootPrint','Curve2D',(#{composite})")));
            }
            None => representations.push(self.w.data.add("IFCSHAPEREPRESENTATION", &format!("#{axis_context},'Axis','Curve2D',(#{composite})"))),
        }
        let shape = self.w.data.add("IFCPRODUCTDEFINITIONSHAPE", &format!("$,$,{}", refs(&representations)));
        let alignment = self.w.data.add(
            "IFCALIGNMENT",
            &format!("{},$,{},$,$,#{placement},#{shape},$", step_string(&element_global_id(&self.w.options.project_name, e.id)), step_string(&clip(&e.name))),
        );

        // ── horizontal business logic
        let horizontal = self.w.data.add("IFCALIGNMENTHORIZONTAL", &format!("{},$,'Horizontal',$,$,$,$", step_string(&self.w.guid(&format!("{owner}/horizontal")))));
        let radius = |curvature: f64| if curvature == 0.0 { 0.0 } else { 1000.0 / curvature };
        let mut h_segments = Vec::new();
        let design_h = |m: &mut Self, x: f64, y: f64, heading: f64, r0: f64, r1: f64, length: f64, kind: &str| {
            let start = m.w.data.add("IFCCARTESIANPOINT", &reals(&[x * 1000.0, y * 1000.0]));
            m.w.data.add(
                "IFCALIGNMENTHORIZONTALSEGMENT",
                &format!("$,$,#{start},{},{},{},{},$,.{kind}.", step_fine(wrap_pi(heading)), step_real(r0), step_real(r1), step_real(length * 1000.0)),
            )
        };
        for (i, el) in a.elements.iter().enumerate() {
            let (r0, r1, kind) = match el.kind() {
                ElementKind::Line => (0.0, 0.0, "LINE"),
                ElementKind::Arc => (radius(el.curvature_start), radius(el.curvature_start), "CIRCULARARC"),
                ElementKind::Spiral => (radius(el.curvature_start), radius(el.curvature_end), "CLOTHOID"),
            };
            let design = design_h(self, el.x, el.y, el.heading_rad, r0, r1, el.length, kind);
            h_segments.push(self.alignment_segment(&owner, "horizontal", i, placement, h_curve_segments[i], design));
        }
        let design = design_h(self, end_x, end_y, end_heading, 0.0, 0.0, 0.0, "LINE");
        h_segments.push(self.alignment_segment(&owner, "horizontal", count, placement, h_curve_segments[count], design));
        let guid = step_string(&self.w.guid(&format!("{owner}/rel-nests-segments")));
        self.w.data.add("IFCRELNESTS", &format!("{guid},$,$,$,#{horizontal},{}", refs(&h_segments)));

        // ── vertical business logic
        let mut layouts = vec![horizontal];
        if let Some((segs, _)) = &vertical {
            let vertical_layout = self.w.data.add("IFCALIGNMENTVERTICAL", &format!("{},$,'Vertical',$,$,$,$", step_string(&self.w.guid(&format!("{owner}/vertical")))));
            let mut v_segments = Vec::new();
            for (i, s) in segs.iter().enumerate() {
                let (kind, radius) = match s.kind {
                    VKind::Line => ("CONSTANTGRADIENT", None),
                    // The radius of a parabola is the horizontal length over the change of gradient
                    // (buildingSMART rule ALB012, tolerance 1e-3 in the length unit): worked out
                    // from the numbers as they are written, not from the unrounded ones.
                    VKind::Parabola => {
                        let round = |v: f64, unit: f64| (v * unit).round() / unit;
                        ("PARABOLICARC", Some(round(s.length * 1000.0, 1e6) / (round(s.g1, 1e12) - round(s.g0, 1e12)) / 1000.0))
                    }
                    VKind::Circle => ("CIRCULARARC", s.radius),
                };
                let design = self.w.data.add(
                    "IFCALIGNMENTVERTICALSEGMENT",
                    &format!(
                        "$,$,{},{},{},{},{},{},.{kind}.",
                        step_real(s.d0 * 1000.0),
                        step_real(s.length * 1000.0),
                        step_real(s.z0 * 1000.0),
                        step_fine(s.g0),
                        step_fine(s.g1),
                        radius.map_or("$".to_string(), |r| step_real(r * 1000.0))
                    ),
                );
                v_segments.push(self.alignment_segment(&owner, "vertical", i, placement, v_curve_segments[i], design));
            }
            let last = segs[segs.len() - 1];
            let design = self.w.data.add(
                "IFCALIGNMENTVERTICALSEGMENT",
                &format!("$,$,{},0.,{},{},{},$,.CONSTANTGRADIENT.", step_real((last.d0 + last.length) * 1000.0), step_real(last.z1 * 1000.0), step_fine(last.g1), step_fine(last.g1)),
            );
            v_segments.push(self.alignment_segment(&owner, "vertical", segs.len(), placement, v_curve_segments[segs.len()], design));
            let guid = step_string(&self.w.guid(&format!("{owner}/rel-nests-vertical-segments")));
            self.w.data.add("IFCRELNESTS", &format!("{guid},$,$,$,#{vertical_layout},{}", refs(&v_segments)));
            layouts.push(vertical_layout);
        }
        let guid = step_string(&self.w.guid(&format!("{owner}/rel-nests-layouts")));
        self.w.data.add("IFCRELNESTS", &format!("{guid},$,$,$,#{alignment},{}", refs(&layouts)));

        let geo = AlignGeo {
            alignment: a.clone(),
            ifc: alignment,
            placement,
            composite,
            gradient,
            vertical: vertical.as_ref().map(|(_, range)| *range),
        };

        // ── stationing referents: the start station first, then every station equation
        let mut stationing = Vec::new();
        let start_placement = self.referent_placement(&geo, a.start_station_m, false);
        let start_owner = format!("{owner}/start-station");
        let start = self.w.data.add(
            "IFCREFERENT",
            &format!("{},$,{},$,$,#{start_placement},$,.STATION.", step_string(&self.w.guid(&start_owner)), step_string(&format_station(a.start_station_m, 3))),
        );
        self.w.property_set_of(start, &start_owner, "Pset_Stationing", &[("Station".to_string(), Value::Length(a.start_station_m * 1000.0))]);
        stationing.push(start);
        let mut increasing = true;
        for (i, eq) in a.station_equations.equations.iter().enumerate() {
            if eq.internal_m < a.start_station_m - 1e-9 || eq.internal_m > a.end_station_m() + 1e-9 {
                notes.push(format!("{}: station equation at {} is outside the alignment; not exported", e.name, format_station(eq.internal_m, 3)));
                continue;
            }
            let eq_owner = format!("{owner}/station-equation/{}", i + 1);
            let placement = self.referent_placement(&geo, eq.internal_m, false);
            let referent = self.w.data.add(
                "IFCREFERENT",
                &format!("{},$,{},$,$,#{placement},$,.STATION.", step_string(&self.w.guid(&eq_owner)), step_string(&format_station(eq.ahead_m, 3))),
            );
            let mut props = vec![("Station".to_string(), Value::Length(eq.ahead_m * 1000.0))];
            if let Some(back) = a.station_equations.back_station(i) {
                props.push(("IncomingStation".to_string(), Value::Length(back * 1000.0)));
            }
            let now = eq.direction == StationDirection::Increasing;
            if now != increasing {
                props.push(("HasIncreasingStation".to_string(), Value::Boolean(now)));
                increasing = now;
            }
            self.w.property_set_of(referent, &eq_owner, "Pset_Stationing", &props);
            stationing.push(referent);
        }
        let guid = step_string(&self.w.guid(&format!("{owner}/rel-nests-referents")));
        self.w.data.add("IFCRELNESTS", &format!("{guid},$,$,$,#{alignment},{}", refs(&stationing)));

        // ── superelevation events
        let mut events = Vec::new();
        if let Some(sup) = &a.superelevation {
            for (si, section) in sup.sections.iter().enumerate() {
                for (pi, p) in section.points.iter().enumerate() {
                    if p.station_m < a.start_station_m - 1e-9 || p.station_m > a.end_station_m() + 1e-9 {
                        continue;
                    }
                    // The sign of the standard property: positive for a curve to the right, the
                    // slope in the profile frame whose X axis points to the left of the curve.
                    let left = p.left_pct.map(|v| v / 100.0);
                    let right = p.right_pct.map(|v| -v / 100.0);
                    let sides: Vec<(&str, f64)> = match (left, right) {
                        (Some(l), Some(r)) if (l - r).abs() < 1e-12 => vec![("BOTH", l)],
                        (l, r) => l.map(|v| ("LEFT", v)).into_iter().chain(r.map(|v| ("RIGHT", v))).collect(),
                    };
                    for (side, value) in sides {
                        let ev_owner = format!("{owner}/superelevation/{}/{}/{}", si + 1, pi + 1, side.to_lowercase());
                        let gradient = geo.covers_vertical(p.station_m);
                        let placement = self.referent_placement(&geo, p.station_m, gradient);
                        let name = format!("Superelevation {} {}", format_station(p.station_m, 3), side.to_lowercase());
                        let referent = self.w.data.add(
                            "IFCREFERENT",
                            &format!("{},$,{},$,$,#{placement},$,.SUPERELEVATIONEVENT.", step_string(&self.w.guid(&ev_owner)), step_string(&name)),
                        );
                        let mut enums = vec![Enumerated { name: "Side", enumeration: "PEnum_SideType", allowed: &["BOTH", "LEFT", "RIGHT"], value: side }];
                        // The first stated station of a section has nothing before it to transition from.
                        if pi > 0 {
                            enums.push(Enumerated { name: "TransitionSuperelevation", enumeration: "PEnum_TransitionSuperelevationType", allowed: &["LINEAR"], value: "LINEAR" });
                        }
                        self.mixed_property_set(referent, &ev_owner, "Pset_Superelevation", &[("Superelevation".to_string(), Value::Measure("IFCRATIOMEASURE", value))], &enums);
                        let mut own = vec![("StationM".to_string(), Value::Real(p.station_m)), ("Section".to_string(), Value::Integer(si as i64 + 1))];
                        if let Some(v) = p.left_pct {
                            own.push(("LeftCrossFallPct".to_string(), Value::Real(v)));
                        }
                        if let Some(v) = p.right_pct {
                            own.push(("RightCrossFallPct".to_string(), Value::Real(v)));
                        }
                        if let Some(k) = p.kind {
                            own.push(("CriticalStation".to_string(), Value::Text(format!("{k:?}"))));
                        }
                        self.w.property_set_of(referent, &ev_owner, "AfroBIM_Superelevation", &own);
                        events.push(referent);
                    }
                }
            }
        }
        if !events.is_empty() {
            let guid = step_string(&self.w.guid(&format!("{owner}/rel-nests-superelevation")));
            self.w.data.add("IFCRELNESTS", &format!("{guid},$,$,$,#{alignment},{}", refs(&events)));
            notes.push(format!(
                "{}: {} superelevation event(s) written as IfcReferent SUPERELEVATIONEVENT with Pset_Superelevation (IfcAlignmentCant is for rail cant and is not used); the stored cross-fall is also in AfroBIM_Superelevation",
                e.name,
                events.len()
            ));
        }

        // ── property sets
        let mut identity = vec![
            ("Mark".to_string(), Value::Text(e.name.clone())),
            ("Category".to_string(), Value::Text(e.category.clone())),
            ("Layer".to_string(), Value::Text(e.layer.clone())),
            ("StartStationM".to_string(), Value::Real(a.start_station_m)),
            ("EndStationM".to_string(), Value::Real(a.end_station_m())),
            ("LengthM".to_string(), Value::Real(a.length())),
            ("Elements".to_string(), Value::Integer(count as i64)),
        ];
        if let Some((segs, (lo, hi))) = &vertical {
            identity.push(("VerticalSegments".to_string(), Value::Integer(segs.len() as i64)));
            identity.push(("VerticalFromStationM".to_string(), Value::Real(*lo)));
            identity.push(("VerticalToStationM".to_string(), Value::Real(*hi)));
        }
        if !a.station_equations.is_empty() {
            identity.push(("StationEquations".to_string(), Value::Integer(a.station_equations.equations.len() as i64)));
        }
        self.w.property_set(alignment, e.id, "AfroBIM_Alignment", &identity);
        let text: Vec<(String, Value)> = e.properties.iter().map(|(k, v)| (k.clone(), Value::Text(v.clone()))).collect();
        self.w.property_set(alignment, e.id, "AfroBIM_Properties", &text);

        let joints = a.discontinuities(1e-3, 1e-6);
        if !joints.is_empty() {
            notes.push(format!(
                "{}: {} joint(s) between elements are not continuous (gap over 1 mm or a change of direction); exported as given, with the transition code the joint has",
                e.name,
                joints.len()
            ));
        }
        let index = self.geo.len();
        for name in [e.name.clone(), a.name.clone()] {
            self.geo_by_name.entry(name).or_insert(index);
        }
        self.geo.push(geo);
        index
    }

    /// The placement of a root product at `pose`, relative to the road, or, when the alignment
    /// export is [`AlignmentExport::Linear`] and `link` names an exported alignment, by an
    /// `IfcLinearPlacement` on it.
    pub(super) fn root_placement_on(&mut self, link: Option<Link>, pose: Pose) -> u32 {
        self.w.frame = None;
        if self.w.options.alignment == AlignmentExport::Linear {
            if let Some(l) = link {
                if let Some(id) = self.linear_placement(&l, pose) {
                    return id;
                }
            }
        }
        self.w.placement(self.w.container_placement, pose)
    }

    /// [`Model::root_placement_on`] for an element's own site placement.
    pub(super) fn root_placement(&mut self, e: &Element, pose: Pose) -> u32 {
        // A site pose is moved by the local origin of a georeferenced file; a sheet pose is not.
        let pose = if super::valid_site(e).is_some() { self.shifted(pose) } else { pose };
        self.root_placement_on(site_link(e), pose)
    }

    /// `pose` (a site position, mm) relative to the local origin of the file.
    pub(super) fn shifted(&self, pose: Pose) -> Pose {
        let s = self.w.shift_mm;
        Pose { x: pose.x - s[0], y: pose.y - s[1], z: pose.z - s[2], rotation_deg: pose.rotation_deg }
    }

    fn linear_placement(&mut self, link: &Link, pose: Pose) -> Option<u32> {
        let Some(index) = self.geo_by_name.get(&link.alignment).copied() else {
            self.extra_notes.push(format!("alignment '{}' is not among the exported alignments; an element placed on it keeps its absolute placement", link.alignment));
            return None;
        };
        let Some(solved) = self.geo[index].solve(pose, link.station_m, link.offset_m) else {
            self.extra_notes.push(format!(
                "station {} is not on alignment '{}' (it runs from {} to {}); an element placed there keeps its absolute placement",
                format_station(link.station_m, 3),
                link.alignment,
                format_station(self.geo[index].alignment.start_station_m, 3),
                format_station(self.geo[index].alignment.end_station_m(), 3)
            ));
            return None;
        };
        let geo = &self.geo[index];
        let basis = if solved.gradient { geo.gradient.unwrap_or(geo.composite) } else { geo.composite };
        let alignment_placement = geo.placement;
        let point = self.w.data.add(
            "IFCPOINTBYDISTANCEEXPRESSION",
            &format!(
                "IFCLENGTHMEASURE({}),{},{},{},#{basis}",
                step_real(solved.distance_mm),
                step_real(solved.lateral_mm),
                step_real(solved.vertical_mm),
                step_real(solved.longitudinal_mm)
            ),
        );
        let axes = self.w.data.add("IFCAXIS2PLACEMENTLINEAR", &format!("#{point},$,$"));
        // The fallback for tools that do not evaluate linear placement stands for the
        // `RelativePlacement`: the frame of the curve at the point, which is the frame the
        // element's geometry and parts are written in. It is relative to the alignment's
        // placement, which is the file's origin.
        let fallback = self.w.axes_of_turn(&solved.frame, [pose.x, pose.y, pose.z]);
        let placement = self.w.data.add("IFCLINEARPLACEMENT", &format!("#{alignment_placement},#{axes},#{fallback}"));
        if !rot::is_identity(&solved.q, 1e-13) {
            self.w.reframed.insert(placement, solved.q);
            self.w.frame = Some(solved.q);
        }
        self.linear_count += 1;
        Some(placement)
    }

    /// `IfcRelReferencedInSpatialStructure`: the road references its alignments ("Alignment
    /// Spatial Reference").
    pub(super) fn reference_alignments(&mut self, ids: &[u32]) {
        if ids.is_empty() {
            return;
        }
        let guid = step_string(&self.w.guid("rel-referenced-alignments"));
        self.w.data.add("IFCRELREFERENCEDINSPATIALSTRUCTURE", &format!("{guid},$,$,$,{},#{}", refs(ids), self.w.container));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_alignment::{AlignmentBuilder, Pvi, Turn};

    fn curve() -> Alignment {
        AlignmentBuilder::new("T", 1000.0, 500.0, -200.0, 0.4)
            .line(100.0)
            .spiral(60.0, None, Some(300.0), Turn::Right)
            .arc(300.0, 120.0, Turn::Right)
            .spiral(60.0, Some(300.0), None, Turn::Right)
            .line(80.0)
            .build()
            .unwrap()
    }

    #[test]
    fn exact_reals_keep_small_numbers() {
        assert_eq!(step_exact(1.25e-7), "1.25E-7");
        assert_eq!(step_exact(2e-9), "2.E-9");
        assert_eq!(step_exact(0.5), "5.E-1");
        assert_eq!(step_exact(0.0), "0.");
        assert_eq!(step_exact(-3e-5), "-3.E-5");
    }

    #[test]
    fn a_clothoid_has_the_documented_constant_and_start() {
        let a = curve();
        // Spiral in to the right: curvature 0 to -1/300 over 60 m.
        let s = h_segment(&a.elements[1]);
        let HParent::Clothoid { constant_mm } = s.parent else { panic!("a clothoid") };
        // A = sqrt(L R) = sqrt(60 000 * 300 000) mm, negative for a right-hand curve.
        assert!((constant_mm + (60_000.0f64 * 300_000.0).sqrt()).abs() < 1e-6);
        assert_eq!(s.start_mm, 0.0);
        assert!((s.length_mm - 60_000.0).abs() < 1e-9);
        // Spiral out: runs the last L of the clothoid before its inflection point.
        let s = h_segment(&a.elements[3]);
        let HParent::Clothoid { constant_mm } = s.parent else { panic!("a clothoid") };
        assert!((constant_mm - (60_000.0f64 * 300_000.0).sqrt()).abs() < 1e-6);
        assert!((s.start_mm + 60_000.0).abs() < 1e-9);
        // A right arc is a negative length on a circle of the radius.
        let s = h_segment(&a.elements[2]);
        let HParent::Circle { radius_mm } = s.parent else { panic!("a circle") };
        assert!((radius_mm - 300_000.0).abs() < 1e-6);
        assert!((s.length_mm + 120_000.0).abs() < 1e-9);
    }

    #[test]
    fn the_transitions_of_a_smooth_alignment_are_continuous() {
        let a = curve();
        assert_eq!(h_transition(&a.elements[0], &a.elements[1]), "CONTSAMEGRADIENTSAMECURVATURE");
        assert_eq!(h_transition(&a.elements[1], &a.elements[2]), "CONTSAMEGRADIENTSAMECURVATURE");
        let mut broken = a.elements.clone();
        broken[2].x += 0.01;
        assert_eq!(h_transition(&broken[1], &broken[2]), "DISCONTINUOUS");
        broken[2].x -= 0.01;
        broken[2].heading_rad += 0.01;
        assert_eq!(h_transition(&broken[1], &broken[2]), "CONTINUOUS");
    }

    #[test]
    fn the_profile_is_cut_into_straights_and_curves() {
        let mut a = curve();
        let end = a.end_station_m();
        a.profile = Some(
            afrobim_alignment::Profile::new(
                "P",
                vec![Pvi::point(1000.0, 100.0), Pvi::parabola(1100.0, 103.0, 60.0), Pvi::asymmetric(1200.0, 101.0, 30.0, 50.0), Pvi::circular(1300.0, 104.0, 500.0), Pvi::point(end, 102.0)],
            )
            .unwrap(),
        );
        let (segs, (lo, hi)) = vertical_segments(&a).expect("vertical segments");
        assert_eq!((lo, hi), (1000.0, end));
        let kinds: Vec<VKind> = segs.iter().map(|s| s.kind).collect();
        use VKind::*;
        assert_eq!(kinds, vec![Line, Parabola, Line, Parabola, Parabola, Line, Circle, Line]);
        // The pieces join: each starts where the one before ended.
        for w in segs.windows(2) {
            assert!((w[1].d0 - (w[0].d0 + w[0].length)).abs() < 1e-9);
            assert!((w[1].z0 - w[0].z1).abs() < 1e-9);
            assert!((w[1].g0 - w[0].g1).abs() < 1e-9);
            assert_ne!(v_transition(&w[0], &w[1]), "DISCONTINUOUS");
        }
        // A sag parabola has a positive radius, a crest a negative one.
        assert!(segs[1].radius.unwrap() < 0.0, "the first curve is a crest");
        // Total length: the profile's horizontal extent.
        let total: f64 = segs.iter().map(|s| s.length).sum();
        assert!((total - a.length()).abs() < 1e-9);
    }

    #[test]
    fn a_profile_longer_than_the_alignment_is_clipped() {
        let mut a = curve();
        a.profile = Some(afrobim_alignment::Profile::new("P", vec![Pvi::point(900.0, 100.0), Pvi::parabola(1100.0, 105.0, 100.0), Pvi::point(2000.0, 90.0)]).unwrap());
        let (segs, (lo, hi)) = vertical_segments(&a).expect("vertical segments");
        assert_eq!(lo, 1000.0);
        assert!((hi - a.end_station_m()).abs() < 1e-9);
        assert!(segs[0].d0.abs() < 1e-9);
        let profile = a.profile.as_ref().unwrap();
        assert!((segs[0].z0 - profile.elevation_at(1000.0).unwrap()).abs() < 1e-9);
    }

    #[test]
    fn the_parabola_length_matches_numeric_integration() {
        let (h, g0, g1) = (200_000.0, 0.03, -0.02);
        let n = 100_000;
        let c = (g1 - g0) / (2.0 * h);
        let numeric: f64 = (0..n).map(|i| (1.0 + (g0 + 2.0 * c * (i as f64 + 0.5) * h / n as f64).powi(2)).sqrt() * h / n as f64).sum();
        assert!((parabola_arc_length(h, g0, g1) - numeric).abs() < 1e-3);
    }

    fn geo_of(a: &Alignment) -> AlignGeo {
        let vertical = vertical_segments(a).map(|(_, r)| r);
        AlignGeo { alignment: a.clone(), ifc: 1, placement: 2, composite: 3, gradient: vertical.map(|_| 4), vertical }
    }

    #[test]
    fn the_offsets_reach_the_position_exactly() {
        let mut a = curve();
        a.profile = Some(afrobim_alignment::Profile::new("P", vec![Pvi::point(1000.0, 100.0), Pvi::parabola(1150.0, 103.0, 100.0), Pvi::point(a.end_station_m(), 99.0)]).unwrap());
        let g = geo_of(&a);
        for (station, offset, z, rot) in [(1050.0, 4.0, 103_000.0, 20.0), (1200.0, -7.5, 100_250.0, -170.0), (1350.0, 0.0, 99_500.0, 0.0)] {
            let at = a.point_at(station).unwrap();
            let (x, y) = at.offset(offset);
            let pose = Pose { x: x * 1000.0, y: y * 1000.0, z, rotation_deg: rot };
            let s = g.solve(pose, station, offset).expect("on the alignment");
            assert!(s.gradient);
            assert!((s.distance_mm - (station - 1000.0) * 1000.0).abs() < 1e-9);
            // Lateral is positive to the left: minus the offset to the right.
            assert!((s.lateral_mm + offset * 1000.0).abs() < 1e-6, "{} {}", s.lateral_mm, offset);
            // Rebuild the point from the frame and the three offsets.
            let f = g.frame_at(station, true).unwrap();
            let p: Vec<f64> = (0..3).map(|i| f.origin_mm[i] + s.lateral_mm * f.left[i] + s.vertical_mm * f.n[i] + s.longitudinal_mm * f.t[i]).collect();
            assert!((p[0] - pose.x).abs() < 1e-6 && (p[1] - pose.y).abs() < 1e-6 && (p[2] - pose.z).abs() < 1e-6);
            // The turn takes the frame of the curve onto the element's own.
            let world = rot::mul(&f.rotation(), &s.q);
            let want = rot::about_z(rot);
            for i in 0..3 {
                for j in 0..3 {
                    assert!((world[i][j] - want[i][j]).abs() < 1e-12);
                }
            }
        }
    }

    #[test]
    fn a_station_that_does_not_match_the_position_is_replaced_by_the_nearest_point() {
        let a = curve();
        let g = geo_of(&a);
        let at = a.point_at(1100.0).unwrap();
        let (x, y) = at.offset(3.0);
        // The element says station 1050 but stands at 1100.
        let s = g.solve(Pose { x: x * 1000.0, y: y * 1000.0, z: 0.0, rotation_deg: 0.0 }, 1050.0, 3.0).unwrap();
        assert!((s.distance_mm - 100_000.0).abs() < 1e-3, "{}", s.distance_mm);
        assert!(!s.gradient);
        assert!(g.solve(Pose { x: 0.0, y: 0.0, z: 0.0, rotation_deg: 0.0 }, 5000.0, 0.0).is_none());
    }
}
