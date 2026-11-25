// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! `IfcLinearPlacement`: the placement of a product on the curve of an alignment.
//!
//! A linear placement is `PlacementRelTo`, an `IfcAxis2PlacementLinear` whose `Location` is an
//! `IfcPointByDistanceExpression`, and optionally `CartesianPosition`, "an optional fallback for
//! the `RelativePlacement` attribute, which may be used by importing applications that do not
//! support linear placement" (IFC 4.3, `IfcLinearPlacement`). This reader evaluates the placement
//! when it understands the basis curve, and otherwise takes the fallback.
//!
//! What is understood, which is what this crate's exporter writes and what IfcOpenShell's
//! alignment API and the buildingSMART alignment samples write for roads:
//!
//! * the basis curve is an `IfcCompositeCurve` of `IfcCurveSegment`s whose parent curves are
//!   `IfcLine`, `IfcCircle` and `IfcClothoid` (placed in 2D, parent at the origin along +X), or an
//!   `IfcGradientCurve` on such a curve whose segments are `IfcLine`, `IfcPolynomialCurve` (a
//!   quadratic in the distance along, `CoefficientsX` (0, 1)) and `IfcCircle`, placed in (distance
//!   along, level);
//! * the distance along is an `IfcLengthMeasure` (an `IfcParameterValue` is not read);
//! * the placement has no `Axis` and no `RefDirection`, so its axes are those of the curve: X along
//!   the tangent (in 3D on a gradient curve), Y to the left, Z perpendicular to the tangent in the
//!   vertical plane, upwards. An `OffsetLateral` is to the left, an `OffsetVertical` along that Z
//!   and an `OffsetLongitudinal` along the tangent (`IfcPointByDistanceExpression`).
//!
//! Anything else (another curve, a segment of negative length on a clothoid or a line, an
//! `Axis` or `RefDirection`, a distance beyond the curve) is not evaluated: the fallback is used
//! when there is one, and otherwise the placement is reported as unresolved.

use afrobim_alignment::AlignmentElement;

use super::*;

/// One horizontal curve segment as an alignment element (file units), with the distance along
/// the composite curve at which it starts.
struct HSeg {
    element: AlignmentElement,
    from: f64,
    length: f64,
}

/// One vertical segment, in (distance along, level).
struct VSeg {
    d0: f64,
    z0: f64,
    /// Horizontal extent: up to the start of the next segment.
    extent: f64,
    shape: VShape,
}

enum VShape {
    /// `z = z0 + g u`.
    Line { g: f64 },
    /// `z = z0 + a1 u + a2 u^2 + ...`, coefficients from the linear term.
    Polynomial(Vec<f64>),
    /// The circle's centre and radius in the segment's own coordinates (its start point is the
    /// origin), and the start angle and signed angular length.
    Circle { cx: f64, cy: f64, r: f64, theta0: f64, sweep: f64 },
}

pub(super) struct CurveEval {
    horizontal: Vec<HSeg>,
    total: f64,
    vertical: Option<Vec<VSeg>>,
}

/// The frame of the curve at a distance along: origin and the unit tangent, left and normal.
struct Frame {
    origin: V3,
    t: V3,
    left: V3,
    n: V3,
}

fn real(v: &Value) -> Option<f64> {
    v.as_f64().filter(|x| x.is_finite())
}

impl CurveEval {
    fn frame(&self, d: f64) -> Result<Frame, String> {
        let tolerance = 1e-6 * self.total.max(1.0);
        if !(d >= -tolerance && d <= self.total + tolerance) {
            return Err(format!("the distance along {d} is outside the basis curve, which is {} long", self.total));
        }
        let d = d.clamp(0.0, self.total);
        let seg = self
            .horizontal
            .iter()
            .rev()
            .find(|s| s.length > 0.0 && s.from <= d + 1e-12)
            .or_else(|| self.horizontal.iter().find(|s| s.length > 0.0))
            .ok_or("the basis curve has no length")?;
        let s = (d - seg.from).clamp(0.0, seg.length);
        let (x, y) = seg.element.point_at(s);
        let heading = seg.element.heading_at(s);
        let (sin_h, cos_h) = heading.sin_cos();
        let (mut z, mut theta) = (0.0, 0.0);
        if let Some(vertical) = &self.vertical {
            let v = vertical
                .iter()
                .rev()
                .find(|v| v.extent > 0.0 && v.d0 <= d + 1e-9)
                .ok_or_else(|| format!("the distance along {d} is before the vertical layout"))?;
            let u = d - v.d0;
            if u > v.extent + 1e-6 * self.total.max(1.0) {
                return Err(format!("the distance along {d} is beyond the vertical layout"));
            }
            let (dz, grade) = v.level(u.min(v.extent));
            z = v.z0 + dz;
            theta = grade.atan();
        }
        let (sin_t, cos_t) = theta.sin_cos();
        Ok(Frame {
            origin: [x, y, z],
            t: [cos_t * cos_h, cos_t * sin_h, sin_t],
            left: [-sin_h, cos_h, 0.0],
            n: [-sin_t * cos_h, -sin_t * sin_h, cos_t],
        })
    }
}

impl VSeg {
    /// Height above the start and gradient after a horizontal distance `u`.
    fn level(&self, u: f64) -> (f64, f64) {
        match &self.shape {
            VShape::Line { g } => (g * u, *g),
            VShape::Polynomial(c) => {
                let (mut y, mut dy) = (0.0, 0.0);
                for (k, a) in c.iter().enumerate() {
                    y += a * u.powi(k as i32 + 1);
                    dy += a * (k as f64 + 1.0) * u.powi(k as i32);
                }
                (y, dy)
            }
            VShape::Circle { cx, cy, r, theta0, sweep } => {
                // x(theta) = cx + r cos(theta) runs monotonically along the arc; find the angle
                // that has advanced by u.
                let x0 = cx + r * theta0.cos();
                let (mut lo, mut hi) = (0.0, 1.0);
                let advanced = |t: f64| cx + r * (theta0 + sweep * t).cos() - x0;
                for _ in 0..80 {
                    let mid = 0.5 * (lo + hi);
                    if advanced(mid) < u {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let theta = theta0 + sweep * 0.5 * (lo + hi);
                let y0 = cy + r * theta0.sin();
                (cy + r * theta.sin() - y0, -theta.cos() / theta.sin())
            }
        }
    }
}

impl<'a> Imp<'a> {
    fn point2(&self, v: &Value) -> Option<[f64; 2]> {
        let e = self.ent(v)?;
        let c = e.attr(0).as_reals()?;
        Some([*c.first()?, *c.get(1)?])
    }

    /// `IfcAxis2Placement2D` as its location and the direction of its X axis.
    fn placement2(&self, v: &Value) -> Option<([f64; 2], [f64; 2])> {
        let e = self.ent(v)?;
        if e.ty != "IFCAXIS2PLACEMENT2D" {
            return None;
        }
        let loc = self.point2(&self.a(&e, "Location"))?;
        let dir = self.direction(&self.a(&e, "RefDirection")).map_or([1.0, 0.0], |d| [d[0], d[1]]);
        let l = dir[0].hypot(dir[1]);
        (l > 1e-12).then(|| (loc, [dir[0] / l, dir[1] / l]))
    }

    fn horizontal_segments(&self, curve: &Entity) -> Result<Vec<HSeg>, String> {
        let mut out = Vec::new();
        let mut from = 0.0;
        for id in self.a(curve, "Segments").refs() {
            let Some(seg) = self.get(id) else { return Err(format!("curve segment #{id} does not exist")) };
            if seg.ty != "IFCCURVESEGMENT" {
                return Err(format!("{} in a composite curve is not read", seg.ty));
            }
            let (loc, dir) = self.placement2(&self.a(&seg, "Placement")).ok_or("a curve segment has no 2D placement")?;
            let start = real(&self.a(&seg, "SegmentStart")).ok_or("a curve segment has no start")?;
            let length = real(&self.a(&seg, "SegmentLength")).ok_or("a curve segment has no length")?;
            let parent = self.ent(&self.a(&seg, "ParentCurve")).ok_or("a curve segment has no parent curve")?;
            let heading = dir[1].atan2(dir[0]);
            let (k0, k1, len) = match parent.ty {
                "IFCLINE" => (0.0, 0.0, length),
                "IFCCIRCLE" => {
                    let r = real(&self.a(&parent, "Radius")).filter(|r| *r > 0.0).ok_or("a circle has no radius")?;
                    let k = length.signum() / r;
                    (k, k, length.abs())
                }
                "IFCCLOTHOID" => {
                    let a = real(&self.a(&parent, "ClothoidConstant")).filter(|a| *a != 0.0).ok_or("a clothoid has no constant")?;
                    // Curvature on the parent is s / (A |A|): the segment runs from start to start + length on it.
                    let c = 1.0 / (a * a.abs());
                    (start * c, (start + length) * c, length)
                }
                other => return Err(format!("{other} as the parent of a horizontal curve segment is not read")),
            };
            if len < 0.0 {
                return Err("a curve segment of negative length is not read".into());
            }
            if len > 0.0 {
                out.push(HSeg { element: AlignmentElement { x: loc[0], y: loc[1], heading_rad: heading, length: len, curvature_start: k0, curvature_end: k1 }, from, length: len });
            }
            from += len;
        }
        if out.is_empty() {
            return Err("a composite curve without length".into());
        }
        Ok(out)
    }

    fn vertical_segments(&self, curve: &Entity) -> Result<Vec<VSeg>, String> {
        let ids = self.a(curve, "Segments").refs();
        let mut raw = Vec::new();
        for id in &ids {
            let Some(seg) = self.get(*id) else { return Err(format!("curve segment #{id} does not exist")) };
            if seg.ty != "IFCCURVESEGMENT" {
                return Err(format!("{} in a gradient curve is not read", seg.ty));
            }
            let (loc, dir) = self.placement2(&self.a(&seg, "Placement")).ok_or("a vertical curve segment has no 2D placement")?;
            let start = real(&self.a(&seg, "SegmentStart")).ok_or("a vertical curve segment has no start")?;
            let length = real(&self.a(&seg, "SegmentLength")).ok_or("a vertical curve segment has no length")?;
            let parent = self.ent(&self.a(&seg, "ParentCurve")).ok_or("a vertical curve segment has no parent curve")?;
            let shape = match parent.ty {
                "IFCLINE" => VShape::Line { g: dir[1] / dir[0] },
                "IFCPOLYNOMIALCURVE" => {
                    let x = self.a(&parent, "CoefficientsX").as_reals().unwrap_or_default();
                    let y = self.a(&parent, "CoefficientsY").as_reals().ok_or("a polynomial has no coefficients")?;
                    if x != [0.0, 1.0] || start != 0.0 || y.len() < 2 || self.a(&parent, "CoefficientsZ").as_reals().is_some() {
                        return Err("only a polynomial in the distance along, from its start, is read".into());
                    }
                    VShape::Polynomial(y[1..].to_vec())
                }
                "IFCCIRCLE" => {
                    let r = real(&self.a(&parent, "Radius")).filter(|r| *r > 0.0).ok_or("a circle has no radius")?;
                    let (centre, _) = self.placement2(&self.a(&parent, "Position")).ok_or("a vertical circle has no position")?;
                    VShape::Circle { cx: centre[0], cy: centre[1], r, theta0: start / r, sweep: length / r }
                }
                other => return Err(format!("{other} as the parent of a vertical curve segment is not read")),
            };
            raw.push((loc, shape));
        }
        let mut out = Vec::new();
        for (i, (loc, shape)) in raw.iter().enumerate() {
            let extent = raw.get(i + 1).map_or(0.0, |next| next.0[0] - loc[0]);
            out.push(VSeg { d0: loc[0], z0: loc[1], extent, shape: match shape {
                VShape::Line { g } => VShape::Line { g: *g },
                VShape::Polynomial(c) => VShape::Polynomial(c.clone()),
                VShape::Circle { cx, cy, r, theta0, sweep } => VShape::Circle { cx: *cx, cy: *cy, r: *r, theta0: *theta0, sweep: *sweep },
            } });
        }
        if out.iter().all(|v| v.extent <= 0.0) {
            return Err("a gradient curve without length".into());
        }
        Ok(out)
    }

    fn curve_eval(&mut self, id: u32) -> Result<Rc<CurveEval>, String> {
        if let Some(c) = self.curves.get(&id) {
            return c.clone();
        }
        let built = (|| {
            let curve = self.get(id).ok_or_else(|| format!("curve #{id} does not exist"))?;
            let (horizontal_curve, vertical) = match curve.ty {
                "IFCCOMPOSITECURVE" => (curve, None),
                "IFCGRADIENTCURVE" => {
                    let base = self.ent(&self.a(&curve, "BaseCurve")).ok_or("a gradient curve without a base curve")?;
                    if base.ty != "IFCCOMPOSITECURVE" {
                        return Err(format!("a gradient curve on {} is not read", base.ty));
                    }
                    let v = self.vertical_segments(&curve)?;
                    (base, Some(v))
                }
                other => return Err(format!("{other} as a basis curve is not read")),
            };
            let horizontal = self.horizontal_segments(&horizontal_curve)?;
            let total = horizontal.iter().map(|s| s.length).sum();
            Ok(Rc::new(CurveEval { horizontal, total, vertical }))
        })();
        self.curves.insert(id, built.clone());
        built
    }

    /// The placement a linear placement gives, relative to its `PlacementRelTo`.
    fn evaluate_linear(&mut self, placement: &Entity) -> Result<Mat, String> {
        let axes = self.ent(&self.a(placement, "RelativePlacement")).ok_or("a linear placement without a relative placement")?;
        if axes.ty != "IFCAXIS2PLACEMENTLINEAR" {
            return Err(format!("{} as the relative placement of a linear placement is not read", axes.ty));
        }
        if self.a(&axes, "Axis").as_ref().is_some() || self.a(&axes, "RefDirection").as_ref().is_some() {
            return Err("an Axis or RefDirection on a linear placement is not read".into());
        }
        let point = self.ent(&self.a(&axes, "Location")).ok_or("a linear placement without a location")?;
        if point.ty != "IFCPOINTBYDISTANCEEXPRESSION" {
            return Err(format!("{} as the location of a linear placement is not read", point.ty));
        }
        let distance = match self.a(&point, "DistanceAlong") {
            Value::Typed(name, v) if name == "IFCLENGTHMEASURE" => real(&v),
            _ => None,
        }
        .ok_or("a distance along that is not an IfcLengthMeasure is not read")?;
        let offset = |name: &str| real(&self.a(&point, name)).unwrap_or(0.0);
        let (lateral, vertical, longitudinal) = (offset("OffsetLateral"), offset("OffsetVertical"), offset("OffsetLongitudinal"));
        let curve_id = self.a(&point, "BasisCurve").as_ref().ok_or("a point by distance without a basis curve")?;
        let curve = self.curve_eval(curve_id)?;
        let f = curve.frame(distance)?;
        let at = [
            f.origin[0] + lateral * f.left[0] + vertical * f.n[0] + longitudinal * f.t[0],
            f.origin[1] + lateral * f.left[1] + vertical * f.n[1] + longitudinal * f.t[1],
            f.origin[2] + lateral * f.left[2] + vertical * f.n[2] + longitudinal * f.t[2],
        ];
        Ok(Mat::from_axes(f.t, f.left, f.n, at))
    }

    /// The relative placement of an `IfcLinearPlacement`: evaluated, or its `CartesianPosition`.
    pub(super) fn linear_relative(&mut self, placement: &Entity) -> Result<Mat, String> {
        match self.evaluate_linear(placement) {
            Ok(m) => {
                self.report.linear_placements_resolved += 1;
                Ok(m)
            }
            Err(why) => match self.a(placement, "CartesianPosition").as_ref().and_then(|c| self.axis_placement(c)) {
                Some(m) => {
                    self.report.linear_placements_fallback += 1;
                    if self.report.linear_placements_fallback == 1 {
                        self.report.warnings.push(format!("linear placement not evaluated ({why}); CartesianPosition used"));
                    }
                    Ok(m)
                }
                None => {
                    *self.report.placements_unresolved.entry("IFCLINEARPLACEMENT".to_string()).or_default() += 1;
                    Err(format!("IFCLINEARPLACEMENT not resolved: {why}"))
                }
            },
        }
    }
}
