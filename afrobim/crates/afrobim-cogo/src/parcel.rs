//! Parcels: boundaries of straight lines and circular arcs, with area, perimeter, centroid,
//! closure and point-in-parcel.
//!
//! # Boundary
//!
//! A [`Boundary`] is a start point and a list of segments, each ending at its `to` point. If the last
//! segment does not end at the start, a straight closing line is understood (`closed_segments`).
//! An arc is given by its end points, its radius, the direction it turns ([`Turn::Right`] is
//! clockwise, increasing bearing) and whether it is the long way round (more than 180 degrees).
//!
//! # Arc geometry
//!
//! For chord length c and radius R the central angle of the short arc is D = 2 asin(c / 2R); the long
//! arc has 2 pi - D. The centre is on the perpendicular bisector of the chord at distance
//! h = sqrt(R^2 - c^2/4) from the chord midpoint. Travelling clockwise about the centre keeps the
//! centre on the right of the chord for a short arc, so the centre is on the right when
//! (turn is Right) differs from (arc is long), else on the left. A right arc (short or long) lies on
//! the left of its chord, a left arc on the right (a short right arc starts left of the chord
//! direction and ends right of it).
//!
//! # Area (derivation)
//!
//! Green's theorem gives the signed area (positive anticlockwise) of a closed curve. Replace each
//! arc by its chord: the shoelace formula gives the signed area A_poly of the chord polygon. The
//! closed loop "arc forward, chord back" encloses a circular segment of area
//! S = R^2 (D - sin D) / 2 and runs clockwise for a Right arc, anticlockwise for a Left arc, so its
//! signed area is -S for Right and +S for Left (for the long arc as well: the loop still runs the
//! way the arc turns). The signed area of the true boundary is A_poly plus these, because the chords
//! cancel between the polygon and the loops. Area = |signed|. The boundary must not cross itself.
//!
//! # Centroid (derivation)
//!
//! The first moment of the true region is the sum of the moments of the chord polygon and of the
//! segment loops, each weighted by its signed area. The centroid of a circular segment of central
//! angle D lies on its axis at distance d = 4 R sin^3(D/2) / (3 (D - sin D)) from the centre towards
//! the arc: integrate r cos(phi) over the segment, r from R cos(D/2)/cos(phi) to R, phi from -D/2 to D/2,
//! which gives (2/3) R^3 sin^3(D/2), and divide by S. The same expression holds for the long arc (the
//! circle minus the opposite short segment, whose moment about the centre is the negative of it).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::angle::normalize_deg;
use crate::calc::Turn;
use crate::error::{CogoError, Result};
use crate::geom::{bearing_of, cross, dir, ring_area, ring_winding, seg_nearest, Xy};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Segment {
    Line { to: Xy },
    Arc { to: Xy, radius: f64, turn: Turn, large: bool },
}

impl Segment {
    pub fn to(&self) -> Xy {
        match self {
            Segment::Line { to } | Segment::Arc { to, .. } => *to,
        }
    }
}

/// Centre and sweep of an arc.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArcGeom {
    pub center: Xy,
    pub radius: f64,
    /// Central angle in radians, in (0, 2 pi).
    pub delta: f64,
    /// Bearing (degrees) from the centre to the start of the arc.
    pub start_bearing_deg: f64,
    pub turn: Turn,
    pub large: bool,
    pub from: Xy,
    pub to: Xy,
}

impl ArcGeom {
    pub fn new(from: Xy, to: Xy, radius: f64, turn: Turn, large: bool) -> Result<ArcGeom> {
        let (de, dn) = to.sub(from);
        let c = de.hypot(dn);
        if c == 0.0 {
            return Err(CogoError::Invalid("an arc needs distinct end points".into()));
        }
        if !(radius > 0.0) || !radius.is_finite() {
            return Err(CogoError::Invalid("arc radius must be positive".into()));
        }
        let half = c / 2.0;
        if half > radius * (1.0 + 1e-9) {
            return Err(CogoError::Invalid(format!("chord {c} is longer than the diameter {}", 2.0 * radius)));
        }
        let h = (radius * radius - half * half).max(0.0).sqrt();
        let (ue, un) = (de / c, dn / c);
        let right = (un, -ue);
        let side = if (turn == Turn::Right) != large { 1.0 } else { -1.0 };
        let center = Xy::new(from.e + de / 2.0 + side * h * right.0, from.n + dn / 2.0 + side * h * right.1);
        let small = 2.0 * (half / radius).min(1.0).asin();
        let delta = if large { 2.0 * std::f64::consts::PI - small } else { small };
        let (fe, fn_) = from.sub(center);
        Ok(ArcGeom { center, radius, delta, start_bearing_deg: bearing_of(fe, fn_), turn, large, from, to })
    }

    pub fn length(&self) -> f64 {
        self.radius * self.delta
    }

    fn sign(&self) -> f64 {
        if self.turn == Turn::Right {
            1.0
        } else {
            -1.0
        }
    }

    /// Point at fraction t (0 start, 1 end) of the sweep.
    pub fn point_at(&self, t: f64) -> Xy {
        if t <= 0.0 {
            return self.from;
        }
        if t >= 1.0 {
            return self.to;
        }
        let b = self.start_bearing_deg + self.sign() * t * self.delta.to_degrees();
        let (s, c) = dir(b);
        self.center.add(self.radius * s, self.radius * c)
    }

    /// Area of the circular segment between chord and arc: R^2 (D - sin D) / 2.
    pub fn segment_area(&self) -> f64 {
        self.radius * self.radius * (self.delta - self.delta.sin()) / 2.0
    }

    /// Signed area of the loop arc forward then chord back (anticlockwise positive).
    pub fn signed_segment_area(&self) -> f64 {
        -self.sign() * self.segment_area()
    }

    /// Centroid of the circular segment region.
    pub fn segment_centroid(&self) -> Xy {
        let d = 4.0 * self.radius * (self.delta / 2.0).sin().powi(3) / (3.0 * (self.delta - self.delta.sin()));
        let b = self.start_bearing_deg + self.sign() * self.delta.to_degrees() / 2.0;
        let (s, c) = dir(b);
        self.center.add(d * s, d * c)
    }

    /// Number of chords needed so that no point of the arc is further than `tol` from the chords.
    /// The sagitta of a chord spanning angle a is R (1 - cos(a/2)) <= tol.
    pub fn chords_for(&self, tol: f64) -> usize {
        let r = self.radius;
        let ratio = (tol / r).clamp(1e-12, 1.0);
        let step = 2.0 * (1.0 - ratio).acos();
        ((self.delta / step).ceil() as usize).max(1)
    }

    /// The arc as points from start to end (both included), `chords_for(tol)` chords. The points
    /// depend only on the geometry, so the same arc walked the other way gives the same points.
    pub fn flatten(&self, tol: f64) -> Vec<Xy> {
        let n = self.chords_for(tol);
        (0..=n).map(|i| self.point_at(i as f64 / n as f64)).collect()
    }

    /// Distance from p to the arc.
    pub fn distance_to(&self, p: Xy) -> f64 {
        let (pe, pn) = p.sub(self.center);
        let dc = pe.hypot(pn);
        if dc == 0.0 {
            return self.radius;
        }
        let tb = bearing_of(pe, pn);
        let diff = normalize_deg(self.sign() * (tb - self.start_bearing_deg));
        if diff <= self.delta.to_degrees() {
            (dc - self.radius).abs()
        } else {
            p.dist(self.from).min(p.dist(self.to))
        }
    }

    /// Is p strictly inside the circular segment region (between chord and arc)?
    fn in_segment(&self, p: Xy) -> bool {
        if p.dist(self.center) >= self.radius {
            return false;
        }
        let chord = self.to.sub(self.from);
        let side = cross(chord, p.sub(self.from));
        // The region lies on the left of the chord for a Right arc and on the right for a Left arc,
        // short or long (the long Right arc is the complement of the short Left arc).
        let bulge = if self.turn == Turn::Right { 1.0 } else { -1.0 };
        side * bulge > 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Location {
    Inside,
    Outside,
    OnBoundary,
}

/// One call of a deed: a course or a curve.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DeedCall {
    Line { bearing_deg: f64, distance: f64 },
    /// A curve given by radius, direction, chord bearing and chord length (as deeds usually give
    /// them). `large` marks a curve of more than 180 degrees.
    Curve { radius: f64, turn: Turn, chord_bearing_deg: f64, chord_length: f64, large: bool },
    /// A curve given by radius, arc length and the bearing of the tangent at its start. Central
    /// angle D = L / R; chord bearing = tangent bearing + D/2 for a right curve (minus for left);
    /// chord = 2 R sin(D/2).
    CurveTangent { radius: f64, arc_length: f64, tangent_bearing_deg: f64, turn: Turn },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DeedClosure {
    /// Where the calls end (not snapped).
    pub end: Xy,
    pub error_e: f64,
    pub error_n: f64,
    pub linear: f64,
    /// Sum of line lengths and arc lengths.
    pub perimeter: f64,
    /// perimeter / linear; `None` when closed exactly.
    pub precision_denominator: Option<f64>,
}

fn resolve_call(from: Xy, c: &DeedCall) -> Result<(Segment, f64, (f64, f64))> {
    match *c {
        DeedCall::Line { bearing_deg, distance } => {
            if !(distance >= 0.0) || !distance.is_finite() {
                return Err(CogoError::Invalid("distance must be zero or more".into()));
            }
            let (s, co) = dir(bearing_deg);
            Ok((Segment::Line { to: from.add(distance * s, distance * co) }, distance, (distance * s, distance * co)))
        }
        DeedCall::Curve { radius, turn, chord_bearing_deg, chord_length, large } => {
            let (s, co) = dir(chord_bearing_deg);
            let to = from.add(chord_length * s, chord_length * co);
            let g = ArcGeom::new(from, to, radius, turn, large)?;
            Ok((Segment::Arc { to, radius, turn, large }, g.length(), (chord_length * s, chord_length * co)))
        }
        DeedCall::CurveTangent { radius, arc_length, tangent_bearing_deg, turn } => {
            if !(radius > 0.0) || !(arc_length > 0.0) {
                return Err(CogoError::Invalid("curve radius and arc length must be positive".into()));
            }
            let d = arc_length / radius;
            if d >= 2.0 * std::f64::consts::PI {
                return Err(CogoError::Invalid("curve turns through a full circle or more".into()));
            }
            let chord = 2.0 * radius * (d / 2.0).sin();
            let half = (d / 2.0).to_degrees();
            let cb = if turn == Turn::Right { tangent_bearing_deg + half } else { tangent_bearing_deg - half };
            let (s, co) = dir(cb);
            let to = from.add(chord * s, chord * co);
            Ok((Segment::Arc { to, radius, turn, large: d > std::f64::consts::PI }, arc_length, (chord * s, chord * co)))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Boundary {
    pub start: Xy,
    pub segments: Vec<Segment>,
}

impl Boundary {
    pub fn polygon(points: &[Xy]) -> Result<Boundary> {
        if points.len() < 3 {
            return Err(CogoError::Invalid("a polygon needs three points".into()));
        }
        Ok(Boundary { start: points[0], segments: points[1..].iter().map(|p| Segment::Line { to: *p }).collect() })
    }

    /// Resolve deed calls from `start` and report how well they close.
    pub fn deed_closure(start: Xy, calls: &[DeedCall]) -> Result<(Vec<Segment>, DeedClosure)> {
        let mut at = start;
        let mut segs = Vec::new();
        let mut per = 0.0;
        // The misclosure is summed from the call vectors, not from differences of large coordinates.
        let (mut ee, mut en) = (0.0, 0.0);
        for c in calls {
            let (s, l, d) = resolve_call(at, c)?;
            at = s.to();
            ee += d.0;
            en += d.1;
            per += l;
            segs.push(s);
        }
        let lin = ee.hypot(en);
        Ok((segs, DeedClosure { end: at, error_e: ee, error_n: en, linear: lin, perimeter: per, precision_denominator: if lin > 0.0 { Some(per / lin) } else { None } }))
    }

    /// A boundary from deed calls. The calls must close to within `tol_m` (the caller's tolerance);
    /// the last end point is then set to the start point (the misclosure is thrown away and is in the
    /// returned [`DeedClosure`]).
    pub fn from_deed(start: Xy, calls: &[DeedCall], tol_m: f64) -> Result<(Boundary, DeedClosure)> {
        let (mut segs, cl) = Boundary::deed_closure(start, calls)?;
        if cl.linear > tol_m {
            return Err(CogoError::NotClosed(cl.linear));
        }
        match segs.last_mut() {
            Some(Segment::Line { to }) | Some(Segment::Arc { to, .. }) => *to = start,
            None => return Err(CogoError::Invalid("no calls".into())),
        }
        Ok((Boundary { start, segments: segs }, cl))
    }

    /// Segments with the implicit closing line added when needed, as (from, segment).
    pub fn closed_segments(&self) -> Vec<(Xy, Segment)> {
        let mut out = Vec::new();
        let mut at = self.start;
        for s in &self.segments {
            out.push((at, *s));
            at = s.to();
        }
        if at != self.start {
            out.push((at, Segment::Line { to: self.start }));
        }
        out
    }

    fn arcs(&self) -> Result<Vec<Option<ArcGeom>>> {
        self.closed_segments()
            .iter()
            .map(|(f, s)| match s {
                Segment::Line { .. } => Ok(None),
                Segment::Arc { to, radius, turn, large } => ArcGeom::new(*f, *to, *radius, *turn, *large).map(Some),
            })
            .collect()
    }

    /// Chord polygon vertices (the start, then each segment's end, without repeating the start).
    pub fn chord_ring(&self) -> Vec<Xy> {
        let mut r = vec![self.start];
        for (_, s) in self.closed_segments() {
            r.push(s.to());
        }
        r.pop();
        r
    }

    /// The boundary as a ring of points, arcs replaced by chords no further than `arc_tol` from them.
    pub fn flatten(&self, arc_tol: f64) -> Result<Vec<Xy>> {
        let mut out = vec![self.start];
        let segs = self.closed_segments();
        let arcs = self.arcs()?;
        for ((_, s), a) in segs.iter().zip(arcs) {
            match a {
                None => out.push(s.to()),
                Some(g) => out.extend(g.flatten(arc_tol).into_iter().skip(1)),
            }
        }
        out.pop();
        Ok(out)
    }

    pub fn perimeter(&self) -> Result<f64> {
        let segs = self.closed_segments();
        let arcs = self.arcs()?;
        Ok(segs.iter().zip(arcs).map(|((f, s), a)| a.map_or_else(|| f.dist(s.to()), |g| g.length())).sum())
    }

    /// Signed area, positive when the boundary runs anticlockwise.
    pub fn signed_area(&self) -> Result<f64> {
        let a: f64 = self.arcs()?.iter().flatten().map(|g| g.signed_segment_area()).sum();
        Ok(ring_area(&self.chord_ring()) + a)
    }

    pub fn area(&self) -> Result<f64> {
        Ok(self.signed_area()?.abs())
    }

    pub fn is_anticlockwise(&self) -> Result<bool> {
        Ok(self.signed_area()? > 0.0)
    }

    pub fn centroid(&self) -> Result<Xy> {
        let ring = self.chord_ring();
        let o = ring[0];
        let n = ring.len();
        let (mut mx, mut my, mut a2) = (0.0, 0.0, 0.0);
        for i in 0..n {
            let p = ring[i].sub(o);
            let q = ring[(i + 1) % n].sub(o);
            let cr = cross(p, q);
            a2 += cr;
            mx += (p.0 + q.0) * cr;
            my += (p.1 + q.1) * cr;
        }
        // polygon: area a2/2, first moments mx/6, my/6
        let mut area = a2 / 2.0;
        let (mut mx, mut my) = (mx / 6.0, my / 6.0);
        for g in self.arcs()?.iter().flatten() {
            let s = g.signed_segment_area();
            let c = g.segment_centroid().sub(o);
            area += s;
            mx += s * c.0;
            my += s * c.1;
        }
        if area == 0.0 {
            return Err(CogoError::Invalid("boundary has no area".into()));
        }
        Ok(o.add(mx / area, my / area))
    }

    /// Smallest distance from p to the boundary.
    pub fn distance_to(&self, p: Xy) -> Result<f64> {
        let segs = self.closed_segments();
        let arcs = self.arcs()?;
        Ok(segs
            .iter()
            .zip(arcs)
            .map(|((f, s), a)| match a {
                None => seg_nearest(p, *f, s.to()).0,
                Some(g) => g.distance_to(p),
            })
            .fold(f64::INFINITY, f64::min))
    }

    /// Where p lies. Points within `tol` of the boundary are `OnBoundary` (tol is the caller's).
    ///
    /// Inside means a nonzero winding number of the true boundary: winding number of the chord polygon
    /// plus, for every arc, +1 (Left arc) or -1 (Right arc) when p is in the circular segment of the arc.
    pub fn locate(&self, p: Xy, tol: f64) -> Result<Location> {
        if self.distance_to(p)? <= tol {
            return Ok(Location::OnBoundary);
        }
        let mut w = ring_winding(&self.chord_ring(), p);
        for g in self.arcs()?.iter().flatten() {
            if g.in_segment(p) {
                w += if g.turn == Turn::Left { 1 } else { -1 };
            }
        }
        Ok(if w != 0 { Location::Inside } else { Location::Outside })
    }

    /// Problems found: zero-length segments, a boundary that crosses itself (tested on the flattened
    /// ring with `arc_tol`), zero area.
    pub fn check(&self, arc_tol: f64) -> Vec<String> {
        let mut w = Vec::new();
        for (i, (f, s)) in self.closed_segments().iter().enumerate() {
            if *f == s.to() {
                w.push(format!("segment {} has zero length", i + 1));
            }
        }
        match self.flatten(arc_tol) {
            Err(e) => w.push(e.to_string()),
            Ok(r) => {
                let n = r.len();
                'o: for i in 0..n {
                    for j in i + 1..n {
                        if j == i + 1 || (i == 0 && j == n - 1) {
                            continue;
                        }
                        if segs_cross(r[i], r[(i + 1) % n], r[j], r[(j + 1) % n]) {
                            w.push("boundary crosses itself".into());
                            break 'o;
                        }
                    }
                }
            }
        }
        if self.area().map_or(true, |a| a == 0.0) {
            w.push("boundary has zero area".into());
        }
        w
    }
}

/// Proper crossing of two segments (interiors cross, not merely touch).
pub(crate) fn segs_cross(a: Xy, b: Xy, c: Xy, d: Xy) -> bool {
    let d1 = cross(b.sub(a), c.sub(a));
    let d2 = cross(b.sub(a), d.sub(a));
    let d3 = cross(d.sub(c), a.sub(c));
    let d4 = cross(d.sub(c), b.sub(c));
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

/// A point inside a ring (anticlockwise or clockwise, simple), by the centroid of an ear triangle.
/// Rings with holes: pass the hole rings in `holes`; a point inside a hole is not returned.
pub(crate) fn interior_point(ring: &[Xy], holes: &[Vec<Xy>]) -> Option<Xy> {
    let n = ring.len();
    if n < 3 {
        return None;
    }
    let ccw = ring_area(ring) > 0.0;
    let mut best: Option<(f64, Xy)> = None;
    for i in 0..n {
        let (a, b, c) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
        let cr = cross(b.sub(a), c.sub(b));
        if (cr > 0.0) != ccw || cr == 0.0 {
            continue;
        }
        let inside_other = ring.iter().enumerate().any(|(k, p)| {
            k != i && k != (i + 1) % n && k != (i + n - 1) % n && in_tri(*p, a, b, c)
        });
        if inside_other {
            continue;
        }
        let g = Xy::new((a.e + b.e + c.e) / 3.0, (a.n + b.n + c.n) / 3.0);
        if holes.iter().any(|h| ring_winding(h, g) != 0) {
            continue;
        }
        // prefer the biggest ear: most robust against tolerance
        let area = cr.abs();
        if best.map_or(true, |(ba, _)| area > ba) {
            best = Some((area, g));
        }
    }
    best.map(|b| b.1)
}

fn in_tri(p: Xy, a: Xy, b: Xy, c: Xy) -> bool {
    let d1 = cross(b.sub(a), p.sub(a));
    let d2 = cross(c.sub(b), p.sub(b));
    let d3 = cross(a.sub(c), p.sub(c));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParcelStatus {
    Existing,
    ToBeAcquired,
    Acquired,
    Other(String),
}

impl ParcelStatus {
    pub fn label(&self) -> String {
        match self {
            ParcelStatus::Existing => "existing".into(),
            ParcelStatus::ToBeAcquired => "to be acquired".into(),
            ParcelStatus::Acquired => "acquired".into(),
            ParcelStatus::Other(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parcel {
    pub name: String,
    pub number: u32,
    pub boundary: Boundary,
    /// Free text.
    pub owner: String,
    /// Free text: title, plot or lot reference as the land records give it.
    pub land_reference: String,
    pub land_use: String,
    pub status: ParcelStatus,
    pub attributes: BTreeMap<String, String>,
}

/// Text and position for a parcel label (no rendering).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParcelLabel {
    pub at: Xy,
    pub lines: Vec<String>,
}

impl Parcel {
    pub fn new(name: &str, number: u32, boundary: Boundary) -> Parcel {
        Parcel {
            name: name.to_string(),
            number,
            boundary,
            owner: String::new(),
            land_reference: String::new(),
            land_use: String::new(),
            status: ParcelStatus::Existing,
            attributes: BTreeMap::new(),
        }
    }
    pub fn area(&self) -> Result<f64> {
        self.boundary.area()
    }
    pub fn perimeter(&self) -> Result<f64> {
        self.boundary.perimeter()
    }
    pub fn centroid(&self) -> Result<Xy> {
        self.boundary.centroid()
    }
    pub fn contains(&self, p: Xy, tol: f64) -> Result<Location> {
        self.boundary.locate(p, tol)
    }
    /// The outline for drawing, arcs flattened to within `arc_tol`.
    pub fn outline(&self, arc_tol: f64) -> Result<Vec<Xy>> {
        self.boundary.flatten(arc_tol)
    }
    /// A point inside the parcel suitable for a label: the centroid if it is inside, else an ear point.
    pub fn label_point(&self, arc_tol: f64) -> Result<Xy> {
        let c = self.boundary.centroid()?;
        if self.boundary.locate(c, 0.0)? == Location::Inside {
            return Ok(c);
        }
        interior_point(&self.boundary.flatten(arc_tol)?, &[]).ok_or_else(|| CogoError::Invalid("no interior point".into()))
    }
    /// Label lines: name, number, area in m2 and hectares (with `decimals` places for m2).
    pub fn label(&self, decimals: usize, arc_tol: f64) -> Result<ParcelLabel> {
        let a = self.area()?;
        Ok(ParcelLabel {
            at: self.label_point(arc_tol)?,
            lines: vec![
                format!("{} ({})", self.name, self.number),
                format!("{:.*} m\u{b2}", decimals, a),
                format!("{:.4} ha", a / HECTARE_M2),
            ],
        })
    }
}

/// One hectare in square metres (the SI-accepted hectare is 10 000 m2 by definition).
pub const HECTARE_M2: f64 = 10_000.0;

/// One international acre in square metres, derived from its definition: 1 acre = 4840 square yards
/// and the international yard is exactly 0.9144 m (the 1959 international yard and pound agreement),
/// so 4840 x 0.9144 x 0.9144 = 4046.8564224 m2. The US survey acre is a different (slightly larger)
/// unit and is **not** used here. Not read from a fetched document; derived from the two definitions.
pub const ACRE_M2: f64 = 4840.0 * 0.9144 * 0.9144;

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn close(a: f64, b: f64, t: f64) -> bool {
        (a - b).abs() <= t
    }

    fn rect(w: f64, h: f64, e0: f64, n0: f64) -> Boundary {
        Boundary::polygon(&[Xy::new(e0, n0), Xy::new(e0 + w, n0), Xy::new(e0 + w, n0 + h), Xy::new(e0, n0 + h)]).unwrap()
    }

    #[test]
    fn polygon_area_at_utm_magnitude() {
        let b = rect(100.0, 50.0, 500000.0, 9999950.0);
        assert!(close(b.area().unwrap(), 5000.0, 1e-6));
        assert!(close(b.perimeter().unwrap(), 300.0, 1e-9));
        assert!(b.is_anticlockwise().unwrap());
        let c = b.centroid().unwrap();
        assert!(close(c.e, 500050.0, 1e-6) && close(c.n, 9999975.0, 1e-6));
        assert!(b.check(0.001).is_empty());
    }

    #[test]
    fn semicircle_area_and_centroid() {
        // diameter from (0,0) to (0,... use chord (-R,0)->(R,0), Right turn bulges left (north)
        let r = 10.0;
        let b = Boundary { start: Xy::new(-r, 0.0), segments: vec![Segment::Arc { to: Xy::new(r, 0.0), radius: r, turn: Turn::Right, large: false }] };
        assert!(close(b.area().unwrap(), PI * r * r / 2.0, 1e-9));
        assert!(close(b.perimeter().unwrap(), PI * r + 2.0 * r, 1e-9));
        let c = b.centroid().unwrap();
        assert!(close(c.e, 0.0, 1e-9) && close(c.n, 4.0 * r / (3.0 * PI), 1e-9), "{c:?}");
        // the same region run the other way: a Left arc from east to west... start east, end west, arc north
        let b2 = Boundary { start: Xy::new(r, 0.0), segments: vec![Segment::Arc { to: Xy::new(-r, 0.0), radius: r, turn: Turn::Left, large: false }] };
        assert!(close(b2.area().unwrap(), PI * r * r / 2.0, 1e-9));
        assert!(close(b2.centroid().unwrap().n, 4.0 * r / (3.0 * PI), 1e-9));
        assert_eq!(b.locate(Xy::new(0.0, 5.0), 1e-6).unwrap(), Location::Inside);
        assert_eq!(b.locate(Xy::new(0.0, -1.0), 1e-6).unwrap(), Location::Outside);
        assert_eq!(b.locate(Xy::new(0.0, 10.0), 1e-6).unwrap(), Location::OnBoundary);
        assert_eq!(b.locate(Xy::new(0.0, 10.5), 1e-6).unwrap(), Location::Outside);
    }

    #[test]
    fn rounded_rectangle_closed_form() {
        // Rectangle 40 x 20 with the right-hand short side replaced by a short arc bulging out (east):
        // walking anticlockwise the right side runs south to north, bulge to the right of travel = Left turn.
        let (w, h, r) = (40.0, 20.0, 15.0);
        let b = Boundary {
            start: Xy::new(0.0, 0.0),
            segments: vec![
                Segment::Line { to: Xy::new(w, 0.0) },
                Segment::Arc { to: Xy::new(w, h), radius: r, turn: Turn::Left, large: false },
                Segment::Line { to: Xy::new(0.0, h) },
            ],
        };
        let d: f64 = 2.0 * (h / (2.0 * r)).asin();
        let seg = r * r * (d - d.sin()) / 2.0;
        assert!(close(b.area().unwrap(), w * h + seg, 1e-9));
        assert!(close(b.perimeter().unwrap(), w + h + w + r * d, 1e-9));
        // a point in the bulge, east of x = w
        let sag = r - (r * r - 100.0f64).sqrt();
        assert_eq!(b.locate(Xy::new(w + sag * 0.5, h / 2.0), 1e-9).unwrap(), Location::Inside);
        assert_eq!(b.locate(Xy::new(w + sag * 1.5, h / 2.0), 1e-9).unwrap(), Location::Outside);
        // the same bulge pointing in removes the segment area
        let b_in = Boundary {
            start: Xy::new(0.0, 0.0),
            segments: vec![
                Segment::Line { to: Xy::new(w, 0.0) },
                Segment::Arc { to: Xy::new(w, h), radius: r, turn: Turn::Right, large: false },
                Segment::Line { to: Xy::new(0.0, h) },
            ],
        };
        assert!(close(b_in.area().unwrap(), w * h - seg, 1e-9));
        assert_eq!(b_in.locate(Xy::new(w - sag * 0.5, h / 2.0), 1e-9).unwrap(), Location::Outside);
        // centroid check by flattening
        let ring = b.flatten(1e-7).unwrap();
        assert!(close(ring_area(&ring), b.area().unwrap(), 1e-3));
        let c = b.centroid().unwrap();
        let (mut mx, mut my, mut a) = (0.0, 0.0, 0.0);
        for i in 0..ring.len() {
            let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
            let cr = p.e * q.n - q.e * p.n;
            a += cr;
            mx += (p.e + q.e) * cr;
            my += (p.n + q.n) * cr;
        }
        assert!(close(c.e, mx / (3.0 * a), 1e-4) && close(c.n, my / (3.0 * a), 1e-4), "{c:?} {} {}", mx / (3.0 * a), my / (3.0 * a));
    }

    #[test]
    fn large_arc_and_full_disc_limit() {
        // A boundary that is a large arc (270 degrees) closed by its chord.
        let r = 10.0;
        let from = Xy::new(r, 0.0);
        let to = Xy::new(0.0, r); // chord of a quarter; the long way round is 270 degrees
        let b = Boundary { start: from, segments: vec![Segment::Arc { to, radius: r, turn: Turn::Right, large: true }] };
        // Clockwise from east, around via south and west back to north: 270 degrees
        let want = PI * r * r * 0.75 + r * r / 2.0; // three quarter disc plus the triangle over the chord
        assert!(close(b.area().unwrap(), want, 1e-9), "{} {}", b.area().unwrap(), want);
        assert_eq!(b.locate(Xy::new(-5.0, -5.0), 1e-9).unwrap(), Location::Inside);
        assert_eq!(b.locate(Xy::new(3.0, 3.0), 1e-9).unwrap(), Location::Inside);
        assert_eq!(b.locate(Xy::new(9.0, 9.0), 1e-9).unwrap(), Location::Outside);
        assert!(close(b.perimeter().unwrap(), 1.5 * PI * r + r * 2f64.sqrt(), 1e-9));
    }

    #[test]
    fn deed_closure_and_snap() {
        let calls = [
            DeedCall::Line { bearing_deg: 0.0, distance: 50.0 },
            DeedCall::Line { bearing_deg: 90.0, distance: 100.0 },
            DeedCall::Line { bearing_deg: 180.0, distance: 50.004 },
            DeedCall::Line { bearing_deg: 270.0, distance: 100.0 },
        ];
        let start = Xy::new(500000.0, 9999950.0);
        let (_, cl) = Boundary::deed_closure(start, &calls).unwrap();
        assert!(close(cl.linear, 0.004, 1e-9));
        assert!(close(cl.precision_denominator.unwrap(), 300.004 / 0.004, 1e-3));
        assert!(matches!(Boundary::from_deed(start, &calls, 0.001), Err(CogoError::NotClosed(_))));
        let (b, _) = Boundary::from_deed(start, &calls, 0.01).unwrap();
        assert_eq!(b.segments.last().unwrap().to(), start);
        assert!(close(b.area().unwrap(), 5000.0, 0.5));
    }

    #[test]
    fn deed_curves_both_forms_agree() {
        // A right curve of radius 100 and 50 m arc, tangent bearing 90 (east).
        let (r, l): (f64, f64) = (100.0, 50.0);
        let d = l / r;
        let chord = 2.0 * r * (d / 2.0).sin();
        let start = Xy::new(0.0, 0.0);
        let (s1, c1) = Boundary::deed_closure(start, &[DeedCall::CurveTangent { radius: r, arc_length: l, tangent_bearing_deg: 90.0, turn: Turn::Right }]).unwrap();
        let (s2, c2) = Boundary::deed_closure(start, &[DeedCall::Curve { radius: r, turn: Turn::Right, chord_bearing_deg: 90.0 + (d / 2.0).to_degrees(), chord_length: chord, large: false }]).unwrap();
        assert!(close(s1[0].to().e, s2[0].to().e, 1e-9) && close(s1[0].to().n, s2[0].to().n, 1e-9));
        assert!(close(c1.perimeter, l, 1e-9) && close(c2.perimeter, l, 1e-9));
        // heading east and turning right ends south of the start line
        assert!(s1[0].to().n < 0.0);
        assert!(Boundary::deed_closure(start, &[DeedCall::Curve { radius: 10.0, turn: Turn::Right, chord_bearing_deg: 0.0, chord_length: 30.0, large: false }]).is_err());
    }

    #[test]
    fn self_crossing_is_flagged() {
        let b = Boundary::polygon(&[Xy::new(0.0, 0.0), Xy::new(10.0, 10.0), Xy::new(10.0, 0.0), Xy::new(0.0, 10.0)]).unwrap();
        assert!(b.check(0.01).iter().any(|m| m.contains("crosses")));
    }

    #[test]
    fn acre_constant_and_labels() {
        assert!(close(ACRE_M2, 4046.8564224, 1e-9));
        let mut p = Parcel::new("P1", 7, rect(100.0, 100.0, 0.0, 0.0));
        p.owner = "Someone".into();
        let l = p.label(2, 0.01).unwrap();
        assert_eq!(l.lines[1], "10000.00 m\u{b2}");
        assert_eq!(l.lines[2], "1.0000 ha");
        assert!(close(l.at.e, 50.0, 1e-9));
        // a C shaped parcel: centroid outside, label point inside
        let c = Parcel::new("C", 1, Boundary::polygon(&[Xy::new(0.0, 0.0), Xy::new(30.0, 0.0), Xy::new(30.0, 10.0), Xy::new(10.0, 10.0), Xy::new(10.0, 20.0), Xy::new(30.0, 20.0), Xy::new(30.0, 30.0), Xy::new(0.0, 30.0)]).unwrap());
        let cc = c.centroid().unwrap();
        assert_eq!(c.contains(cc, 0.0).unwrap(), Location::Outside);
        let lp = c.label_point(0.01).unwrap();
        assert_eq!(c.contains(lp, 0.0).unwrap(), Location::Inside);
    }

    #[test]
    fn serde_round_trip() {
        let mut p = Parcel::new("P", 1, rect(10.0, 10.0, 500000.0, 9999999.1234));
        p.status = ParcelStatus::ToBeAcquired;
        p.attributes.insert("k".into(), "v".into());
        let j = serde_json::to_string(&p).unwrap();
        let q: Parcel = serde_json::from_str(&j).unwrap();
        assert_eq!(p, q);
        assert_eq!(q.boundary.start.n, 9999999.1234);
    }
}
