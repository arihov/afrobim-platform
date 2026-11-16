//! COGO points and the point store.
//!
//! Points keep the coordinates they were given (metres: easting, northing, elevation). Nothing in
//! this crate moves them behind the caller's back. The only way to change coordinates in bulk is
//! [`PointStore::apply_transform`], which needs an explicit list of point numbers and a reason, and
//! writes a [`LogEntry`] with the old coordinates.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::angle::normalize_deg;
use crate::error::{CogoError, Result};
use crate::geom::Xy;

/// Where a point came from.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SourceRecord {
    /// File name or other origin label.
    pub file: Option<String>,
    /// Line in the file, from 1.
    pub line: Option<usize>,
    /// The text of the record as read.
    pub raw: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CogoPoint {
    pub number: u32,
    pub name: Option<String>,
    pub easting: f64,
    pub northing: f64,
    pub elevation: Option<f64>,
    /// The description as surveyed (field code).
    pub raw_description: String,
    /// The description after description keys (empty until applied).
    pub full_description: String,
    pub attributes: BTreeMap<String, String>,
    pub locked: bool,
    pub source: SourceRecord,
}

impl CogoPoint {
    pub fn new(number: u32, easting: f64, northing: f64, elevation: Option<f64>, raw_description: &str) -> CogoPoint {
        CogoPoint {
            number,
            name: None,
            easting,
            northing,
            elevation,
            raw_description: raw_description.to_string(),
            full_description: String::new(),
            attributes: BTreeMap::new(),
            locked: false,
            source: SourceRecord::default(),
        }
    }
    pub fn xy(&self) -> Xy {
        Xy::new(self.easting, self.northing)
    }
}

/// A bulk coordinate operation. Plan operations leave elevations alone.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TransformOp {
    Translate { de: f64, dn: f64, dz: f64 },
    /// Rotate clockwise (as bearings increase) by `angle_deg` about (e, n).
    Rotate { e: f64, n: f64, angle_deg: f64 },
    /// Scale plan distances about (e, n) by `factor`.
    Scale { e: f64, n: f64, factor: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LogAction {
    Transform(TransformOp),
    Renumber { from: u32, to: u32 },
    Move { number: u32 },
    Remove { number: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub seq: u64,
    pub action: LogAction,
    pub reason: String,
    /// Point numbers touched (after the change for renumbering).
    pub numbers: Vec<u32>,
    /// Coordinates before the change, same order as `numbers` (easting, northing, elevation).
    pub before: Vec<(f64, f64, Option<f64>)>,
}

/// Two points closer than the tolerance, or with the same number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicatePair {
    pub a: u32,
    pub b: u32,
    pub plan_distance: f64,
    pub dz: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PointStore {
    points: BTreeMap<u32, CogoPoint>,
    log: Vec<LogEntry>,
    revision: u64,
}

impl PointStore {
    pub fn new() -> PointStore {
        PointStore::default()
    }
    pub fn len(&self) -> usize {
        self.points.len()
    }
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
    /// Increases with every change; groups use it to know when to refresh.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn log(&self) -> &[LogEntry] {
        &self.log
    }
    pub fn get(&self, n: u32) -> Option<&CogoPoint> {
        self.points.get(&n)
    }
    /// Points in number order.
    pub fn iter(&self) -> impl Iterator<Item = &CogoPoint> {
        self.points.values()
    }
    pub fn numbers(&self) -> Vec<u32> {
        self.points.keys().copied().collect()
    }
    /// The smallest unused number at or above `from`.
    pub fn next_free(&self, from: u32) -> u32 {
        let mut n = from.max(1);
        while self.points.contains_key(&n) {
            n += 1;
        }
        n
    }

    fn push_log(&mut self, action: LogAction, reason: &str, numbers: Vec<u32>, before: Vec<(f64, f64, Option<f64>)>) {
        let seq = self.log.len() as u64 + 1;
        self.log.push(LogEntry { seq, action, reason: reason.to_string(), numbers, before });
    }

    /// Add a point; its number must be unused. Coordinates must be finite.
    pub fn add(&mut self, p: CogoPoint) -> Result<()> {
        if !p.easting.is_finite() || !p.northing.is_finite() || p.elevation.is_some_and(|z| !z.is_finite()) {
            return Err(CogoError::Invalid(format!("point {} has a non-finite coordinate", p.number)));
        }
        if self.points.contains_key(&p.number) {
            return Err(CogoError::DuplicateNumber(p.number));
        }
        self.points.insert(p.number, p);
        self.revision += 1;
        Ok(())
    }

    /// Add with the next free number at or above `p.number`; returns the number used.
    pub fn add_next_free(&mut self, mut p: CogoPoint) -> Result<u32> {
        p.number = self.next_free(p.number);
        let n = p.number;
        self.add(p)?;
        Ok(n)
    }

    fn unlocked_mut(&mut self, n: u32) -> Result<&mut CogoPoint> {
        let p = self.points.get_mut(&n).ok_or(CogoError::NoSuchPoint(n))?;
        if p.locked {
            return Err(CogoError::Locked(n));
        }
        Ok(p)
    }

    pub fn remove(&mut self, n: u32, reason: &str) -> Result<CogoPoint> {
        self.unlocked_mut(n)?;
        let p = self.points.remove(&n).unwrap();
        self.revision += 1;
        self.push_log(LogAction::Remove { number: n }, reason, vec![n], vec![(p.easting, p.northing, p.elevation)]);
        Ok(p)
    }

    /// Set a point's coordinates (an explicit single-point edit, logged). Locked points refuse.
    pub fn move_point(&mut self, n: u32, e: f64, nn: f64, z: Option<f64>, reason: &str) -> Result<()> {
        let p = self.unlocked_mut(n)?;
        let before = (p.easting, p.northing, p.elevation);
        p.easting = e;
        p.northing = nn;
        p.elevation = z;
        self.revision += 1;
        self.push_log(LogAction::Move { number: n }, reason, vec![n], vec![before]);
        Ok(())
    }

    /// Edit non-coordinate fields. Locked points refuse.
    pub fn edit<F: FnOnce(&mut CogoPoint)>(&mut self, n: u32, f: F) -> Result<()> {
        let p = self.unlocked_mut(n)?;
        let (e, nn, z, num) = (p.easting, p.northing, p.elevation, p.number);
        f(p);
        // The edit may not change coordinates or the number; those have their own calls.
        p.easting = e;
        p.northing = nn;
        p.elevation = z;
        p.number = num;
        self.revision += 1;
        Ok(())
    }

    pub fn set_locked(&mut self, n: u32, locked: bool) -> Result<()> {
        self.points.get_mut(&n).ok_or(CogoError::NoSuchPoint(n))?.locked = locked;
        self.revision += 1;
        Ok(())
    }

    /// Give a point a new number. The new number must be free and the point unlocked.
    pub fn renumber(&mut self, from: u32, to: u32) -> Result<()> {
        if from == to {
            return Ok(());
        }
        if self.points.contains_key(&to) {
            return Err(CogoError::DuplicateNumber(to));
        }
        self.unlocked_mut(from)?;
        let mut p = self.points.remove(&from).unwrap();
        p.number = to;
        self.points.insert(to, p);
        self.revision += 1;
        self.push_log(LogAction::Renumber { from, to }, "renumber", vec![to], vec![]);
        Ok(())
    }

    /// Add `delta` to every point number. Refused as a whole if any point is locked or a number would
    /// leave the u32 range.
    pub fn renumber_all_by(&mut self, delta: i64) -> Result<()> {
        if let Some(p) = self.points.values().find(|p| p.locked) {
            return Err(CogoError::Locked(p.number));
        }
        let mut next = BTreeMap::new();
        for (n, p) in &self.points {
            let m = *n as i64 + delta;
            if !(1..=u32::MAX as i64).contains(&m) {
                return Err(CogoError::Invalid(format!("number {n} + {delta} is out of range")));
            }
            let mut q = p.clone();
            q.number = m as u32;
            next.insert(m as u32, q);
        }
        let from_to: Vec<(u32, u32)> = self.points.keys().map(|n| (*n, (*n as i64 + delta) as u32)).collect();
        self.points = next;
        self.revision += 1;
        for (f, t) in from_to {
            self.push_log(LogAction::Renumber { from: f, to: t }, "renumber all", vec![t], vec![]);
        }
        Ok(())
    }

    /// Points that share a position to within `tol_xy` metres in plan (and, if `tol_z` is given and
    /// both points have elevations, within `tol_z` in elevation). Tolerances are the caller's.
    /// Pairs are returned once, lower number first.
    pub fn find_position_duplicates(&self, tol_xy: f64, tol_z: Option<f64>) -> Result<Vec<DuplicatePair>> {
        if !(tol_xy >= 0.0) || !tol_xy.is_finite() {
            return Err(CogoError::Invalid("tolerance must be a finite number, zero or more".into()));
        }
        let cell = if tol_xy > 0.0 { tol_xy } else { 1.0 };
        let key = |p: &CogoPoint| ((p.easting / cell).floor() as i64, (p.northing / cell).floor() as i64);
        let mut grid: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
        for p in self.points.values() {
            grid.entry(key(p)).or_default().push(p.number);
        }
        let mut out = Vec::new();
        for p in self.points.values() {
            let (cx, cy) = key(p);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let Some(list) = grid.get(&(cx + dx, cy + dy)) else { continue };
                    for &m in list {
                        if m <= p.number {
                            continue;
                        }
                        let q = &self.points[&m];
                        let d = p.xy().dist(q.xy());
                        if d <= tol_xy {
                            let dz = match (p.elevation, q.elevation) {
                                (Some(a), Some(b)) => Some(b - a),
                                _ => None,
                            };
                            if let (Some(tz), Some(dz)) = (tol_z, dz) {
                                if dz.abs() > tz {
                                    continue;
                                }
                            }
                            out.push(DuplicatePair { a: p.number, b: m, plan_distance: d, dz });
                        }
                    }
                }
            }
        }
        out.sort_by_key(|d| (d.a, d.b));
        Ok(out)
    }

    /// Explicit, logged bulk transform. The caller must name the points and give a reason.
    ///
    /// Rotation, clockwise by `a` about (e0, n0): with bearing convention, a vector (x, y) = (de, dn)
    /// becomes (x cos a + y sin a, -x sin a + y cos a), which turns its bearing by +a.
    pub fn apply_transform(&mut self, op: TransformOp, numbers: &[u32], reason: &str) -> Result<()> {
        if numbers.is_empty() || reason.trim().is_empty() {
            return Err(CogoError::TransformNotNamed);
        }
        if let TransformOp::Scale { factor, .. } = op {
            if !(factor > 0.0) || !factor.is_finite() {
                return Err(CogoError::Invalid("scale factor must be positive".into()));
            }
        }
        for n in numbers {
            let p = self.points.get(n).ok_or(CogoError::NoSuchPoint(*n))?;
            if p.locked {
                return Err(CogoError::Locked(*n));
            }
        }
        let mut before = Vec::new();
        let mut done = Vec::new();
        for n in numbers {
            if done.contains(n) {
                continue;
            }
            done.push(*n);
            let p = self.points.get_mut(n).unwrap();
            before.push((p.easting, p.northing, p.elevation));
            match op {
                TransformOp::Translate { de, dn, dz } => {
                    p.easting += de;
                    p.northing += dn;
                    if let Some(z) = p.elevation.as_mut() {
                        *z += dz;
                    }
                }
                TransformOp::Rotate { e, n: n0, angle_deg } => {
                    let a = normalize_deg(angle_deg).to_radians();
                    let (x, y) = (p.easting - e, p.northing - n0);
                    p.easting = e + x * a.cos() + y * a.sin();
                    p.northing = n0 - x * a.sin() + y * a.cos();
                }
                TransformOp::Scale { e, n: n0, factor } => {
                    p.easting = e + (p.easting - e) * factor;
                    p.northing = n0 + (p.northing - n0) * factor;
                }
            }
        }
        self.revision += 1;
        self.push_log(LogAction::Transform(op), reason, done, before);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> PointStore {
        let mut s = PointStore::new();
        s.add(CogoPoint::new(1, 500000.0, 9999999.1234, Some(10.0), "TR")).unwrap();
        s.add(CogoPoint::new(2, 500010.0, 9999999.1234, None, "EP")).unwrap();
        s
    }

    #[test]
    fn unique_numbers_and_exact_storage() {
        let mut s = store();
        assert_eq!(s.add(CogoPoint::new(1, 0.0, 0.0, None, "")), Err(CogoError::DuplicateNumber(1)));
        assert_eq!(s.get(1).unwrap().northing, 9999999.1234);
        assert_eq!(s.next_free(1), 3);
        assert_eq!(s.add_next_free(CogoPoint::new(1, 1.0, 2.0, None, "")).unwrap(), 3);
    }

    #[test]
    fn renumber_and_lock() {
        let mut s = store();
        s.renumber(2, 20).unwrap();
        assert!(s.get(2).is_none() && s.get(20).unwrap().number == 20);
        assert_eq!(s.renumber(1, 20), Err(CogoError::DuplicateNumber(20)));
        s.set_locked(1, true).unwrap();
        assert_eq!(s.renumber(1, 5), Err(CogoError::Locked(1)));
        assert_eq!(s.move_point(1, 0.0, 0.0, None, "x"), Err(CogoError::Locked(1)));
        assert_eq!(s.renumber_all_by(100), Err(CogoError::Locked(1)));
        s.set_locked(1, false).unwrap();
        s.renumber_all_by(100).unwrap();
        assert_eq!(s.numbers(), vec![101, 120]);
    }

    #[test]
    fn duplicates_by_position() {
        let mut s = store();
        s.add(CogoPoint::new(3, 500000.0004, 9999999.1234, Some(10.2), "")).unwrap();
        let d = s.find_position_duplicates(0.001, None).unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!((d[0].a, d[0].b), (1, 3));
        assert!(s.find_position_duplicates(0.001, Some(0.1)).unwrap().is_empty());
        assert!(s.find_position_duplicates(0.0, None).unwrap().is_empty());
        assert!(s.find_position_duplicates(-1.0, None).is_err());
    }

    #[test]
    fn transforms_need_names_and_log() {
        let mut s = store();
        let op = TransformOp::Translate { de: 1.0, dn: -1.0, dz: 0.5 };
        assert_eq!(s.apply_transform(op, &[], "r"), Err(CogoError::TransformNotNamed));
        assert_eq!(s.apply_transform(op, &[1], " "), Err(CogoError::TransformNotNamed));
        s.apply_transform(op, &[1], "datum shift").unwrap();
        assert_eq!(s.get(1).unwrap().easting, 500001.0);
        assert!((s.get(1).unwrap().northing - 9999998.1234).abs() < 1e-9);
        assert_eq!(s.get(1).unwrap().elevation, Some(10.5));
        assert_eq!(s.get(2).unwrap().easting, 500010.0);
        assert_eq!(s.log().len(), 1);
        assert_eq!(s.log()[0].before[0].0, 500000.0);
    }

    #[test]
    fn rotate_is_clockwise_like_bearings() {
        let mut s = PointStore::new();
        s.add(CogoPoint::new(1, 0.0, 10.0, None, "")).unwrap(); // due north of origin
        s.apply_transform(TransformOp::Rotate { e: 0.0, n: 0.0, angle_deg: 90.0 }, &[1], "t").unwrap();
        let p = s.get(1).unwrap();
        assert!((p.easting - 10.0).abs() < 1e-12 && p.northing.abs() < 1e-12); // now due east
        s.apply_transform(TransformOp::Scale { e: 0.0, n: 0.0, factor: 2.0 }, &[1], "t").unwrap();
        assert!((s.get(1).unwrap().easting - 20.0).abs() < 1e-12);
    }

    #[test]
    fn serde_round_trip_keeps_utm_values() {
        let s = store();
        let j = serde_json::to_string(&s).unwrap();
        let t: PointStore = serde_json::from_str(&j).unwrap();
        assert_eq!(t.get(1).unwrap().northing, 9999999.1234);
        assert_eq!(t.get(1).unwrap().easting, 500000.0);
    }
}
