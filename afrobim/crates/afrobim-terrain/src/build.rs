//! Replaying a definition into a TIN. See [`SurfaceDefinition::build`].

use afrobim_corridor::Tin;

use crate::definition::*;
use crate::error::TerrainError;
use crate::geom::{area2, orient, point_in_polygon, proper_crossing};
use crate::triangulate::{triangulate, Constraint, ConstraintKind, VertexTable};

const MAX_PASTE_DEPTH: usize = 8;

struct Loose {
    id: String,
    p: [f64; 3],
}

enum Filter {
    MaxEdge(f64),
    Boundary { kind: BoundaryKind, verts: Vec<[f64; 2]> },
}

enum Topo {
    DeleteTriangle { x: f64, y: f64 },
    Swap { a: [f64; 2], b: [f64; 2], tol: f64 },
}

#[derive(Default)]
struct State {
    loose: Vec<Loose>,
    lines: Vec<Breakline>,
    boundaries: Vec<(BoundaryKind, String, Vec<[f64; 2]>)>,
    max_edge: Option<f64>,
    filters: Vec<Filter>,
    topo: Vec<Topo>,
    added: usize,
    pasted: usize,
}

fn finite(index: usize, what: &str, values: &[f64]) -> Result<(), TerrainError> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(TerrainError::NonFinite(format!("a value of {what} in operation {index}")))
    }
}

fn invalid(index: usize, message: impl Into<String>) -> TerrainError {
    TerrainError::InvalidOperation { index, message: message.into() }
}

impl SurfaceDefinition {
    /// Replays the operations on an empty surface and returns the TIN and a report.
    ///
    /// The steps are: (1) data operations in list order; (2) points and breakline vertices are
    /// merged within `settings.duplicate_tolerance`; (3) breaklines that cross are cut at the
    /// crossing, which becomes a new point; (4) a constrained Delaunay triangulation of
    /// everything, with every breakline and boundary segment as an edge; (5) triangle filters:
    /// maximum edge length, then boundaries in list order; (6) delete-triangle and swap-edge
    /// edits in list order; (7) points no triangle uses are dropped.
    pub fn build(&self) -> Result<BuiltSurface, TerrainError> {
        self.build_depth(0)
    }

    fn build_depth(&self, depth: usize) -> Result<BuiltSurface, TerrainError> {
        let tol = self.settings.duplicate_tolerance;
        if !(tol.is_finite() && tol >= 0.0) {
            return Err(TerrainError::InvalidInput("the duplicate tolerance must be a finite number, zero or more".into()));
        }
        let mut report = BuildReport::default();
        let mut st = State::default();
        for (i, op) in self.operations.iter().enumerate() {
            apply(&mut st, &mut report, i, op, depth)?;
        }
        finish(st, report, &self.settings)
    }
}

fn apply(st: &mut State, report: &mut BuildReport, i: usize, op: &Operation, depth: usize) -> Result<(), TerrainError> {
    match op {
        Operation::AddPoints { group, points } => {
            for p in points {
                finite(i, &format!("point '{}' of group '{}'", p.id, group), &[p.x, p.y, p.z])?;
                st.loose.push(Loose { id: p.id.clone(), p: [p.x, p.y, p.z] });
            }
            report.points_input += points.len();
        }
        Operation::AddBreaklines { lines } => {
            for l in lines {
                for v in &l.vertices {
                    finite(i, &format!("breakline '{}'", l.name), v)?;
                }
                if l.vertices.len() < 2 {
                    report.warnings.push(format!("breakline '{}' has fewer than two vertices and was ignored", l.name));
                    continue;
                }
                report.breakline_vertices += l.vertices.len();
                st.lines.push(l.clone());
            }
        }
        Operation::AddBoundary { name, kind, vertices } => {
            for v in vertices {
                finite(i, &format!("boundary '{name}'"), v)?;
            }
            let mut ring = vertices.clone();
            if ring.len() > 1 && ring.first() == ring.last() {
                ring.pop();
            }
            if ring.len() < 3 {
                return Err(invalid(i, format!("boundary '{name}' needs at least three distinct vertices")));
            }
            st.boundaries.push((*kind, name.clone(), ring.clone()));
            st.filters.push(Filter::Boundary { kind: *kind, verts: ring });
        }
        Operation::AddContourData { name, lines, thin_distance, max_segment } => {
            for (label, v) in [("thin_distance", thin_distance), ("max_segment", max_segment)] {
                if let Some(v) = v {
                    if !(v.is_finite() && *v > 0.0) {
                        return Err(invalid(i, format!("{label} must be a positive number")));
                    }
                }
            }
            let mut k = 0usize;
            for line in lines {
                for v in line {
                    finite(i, &format!("contour data '{name}'"), v)?;
                }
                let mut kept: Vec<[f64; 3]> = Vec::with_capacity(line.len());
                for (n, v) in line.iter().enumerate() {
                    let keep = match (kept.last(), thin_distance) {
                        (Some(l), Some(d)) => (v[0] - l[0]).hypot(v[1] - l[1]) >= *d || (n + 1 == line.len() && l != v),
                        _ => true,
                    };
                    if keep {
                        if let (Some(l), Some(m)) = (kept.last().copied(), max_segment) {
                            let len = (v[0] - l[0]).hypot(v[1] - l[1]);
                            let parts = (len / m).ceil() as usize;
                            for s in 1..parts {
                                let t = s as f64 / parts as f64;
                                kept.push([l[0] + t * (v[0] - l[0]), l[1] + t * (v[1] - l[1]), l[2] + t * (v[2] - l[2])]);
                            }
                        }
                        kept.push(*v);
                    }
                }
                for p in kept {
                    st.loose.push(Loose { id: format!("contour:{name}:{k}"), p });
                    k += 1;
                    report.points_input += 1;
                }
            }
        }
        Operation::MaxTriangleEdge { length } => {
            if !(length.is_finite() && *length > 0.0) {
                return Err(invalid(i, "the maximum edge length must be a positive number"));
            }
            st.max_edge = Some(*length);
            st.filters.retain(|f| !matches!(f, Filter::MaxEdge(_)));
            st.filters.insert(0, Filter::MaxEdge(*length));
        }
        Operation::ExcludeElevations { below, above } => {
            for v in [below, above].into_iter().flatten() {
                finite(i, "an elevation limit", &[*v])?;
            }
            let before = st.loose.len();
            st.loose.retain(|l| below.is_none_or(|b| l.p[2] >= b) && above.is_none_or(|a| l.p[2] <= a));
            report.points_excluded += before - st.loose.len();
        }
        Operation::DeleteTriangle { x, y } => {
            finite(i, "delete triangle", &[*x, *y])?;
            st.topo.push(Topo::DeleteTriangle { x: *x, y: *y });
        }
        Operation::SwapEdge { a, b, tolerance } => {
            finite(i, "swap edge", &[a[0], a[1], b[0], b[1]])?;
            let tol = tolerance.unwrap_or(0.001);
            if !(tol.is_finite() && tol >= 0.0) {
                return Err(invalid(i, "the tolerance must be zero or more"));
            }
            st.topo.push(Topo::Swap { a: *a, b: *b, tol });
        }
        Operation::AddPoint { x, y, z } => {
            finite(i, "added point", &[*x, *y, *z])?;
            st.loose.push(Loose { id: format!("added:{}", st.added), p: [*x, *y, *z] });
            st.added += 1;
            report.points_input += 1;
        }
        Operation::DeletePoint { target } => {
            let before = st.loose.len();
            match target {
                PointRef::Id(id) => st.loose.retain(|l| &l.id != id),
                PointRef::Near { x, y, tolerance } => {
                    finite(i, "delete point", &[*x, *y, *tolerance])?;
                    if *tolerance < 0.0 {
                        return Err(invalid(i, "the tolerance must be zero or more"));
                    }
                    st.loose.retain(|l| (l.p[0] - x).hypot(l.p[1] - y) > *tolerance);
                }
            }
            let n = before - st.loose.len();
            if n == 0 {
                report.warnings.push(format!("operation {i} (delete point) found no point to delete"));
            }
            report.points_deleted += n;
        }
        Operation::RaiseLower { delta } => {
            finite(i, "raise or lower", &[*delta])?;
            for l in &mut st.loose {
                l.p[2] += delta;
            }
            for l in &mut st.lines {
                for v in &mut l.vertices {
                    v[2] += delta;
                }
            }
        }
        Operation::Paste { source } => {
            if depth >= MAX_PASTE_DEPTH {
                return Err(invalid(i, "pasted surfaces are nested too deeply"));
            }
            let tin = match source {
                PasteSource::Definition(d) => {
                    let built = d.build_depth(depth + 1)?;
                    for w in built.report.warnings {
                        report.warnings.push(format!("pasted surface (operation {i}): {w}"));
                    }
                    built.tin
                }
                PasteSource::Tin(t) => t.clone(),
            };
            let before = st.loose.len();
            st.loose.retain(|l| tin.elevation_at(l.p[0], l.p[1]).is_none());
            report.points_replaced_by_paste += before - st.loose.len();
            let mut edges: Vec<(u32, u32)> = Vec::with_capacity(tin.triangles().len() * 3);
            for t in tin.triangles() {
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    edges.push((a.min(b), a.max(b)));
                }
            }
            edges.sort_unstable();
            edges.dedup();
            let name = format!("paste {}", st.pasted);
            st.pasted += 1;
            for (a, b) in edges {
                st.lines.push(Breakline { name: name.clone(), kind: BreaklineKind::Standard, vertices: vec![tin.points()[a as usize], tin.points()[b as usize]] });
                report.breakline_vertices += 2;
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Acc {
    first: f64,
    min: f64,
    max: f64,
    sum: f64,
    count: u32,
    brk: Option<f64>,
}

fn nearest_z(pts: &[[f64; 3]], x: f64, y: f64) -> f64 {
    pts.iter().fold((f64::INFINITY, 0.0), |b, p| {
        let d = (p[0] - x).powi(2) + (p[1] - y).powi(2);
        if d < b.0 {
            (d, p[2])
        } else {
            b
        }
    })
    .1
}

fn finish(st: State, mut report: BuildReport, settings: &BuildSettings) -> Result<BuiltSurface, TerrainError> {
    // Origin of the local frame: the south-west corner of everything given.
    let mut origin = [f64::INFINITY, f64::INFINITY];
    let mut see = |x: f64, y: f64| {
        origin[0] = origin[0].min(x);
        origin[1] = origin[1].min(y);
    };
    st.loose.iter().for_each(|l| see(l.p[0], l.p[1]));
    st.lines.iter().flat_map(|l| &l.vertices).for_each(|v| see(v[0], v[1]));
    st.boundaries.iter().flat_map(|b| &b.2).for_each(|v| see(v[0], v[1]));
    if !origin[0].is_finite() {
        return Err(TerrainError::TooFewPoints(0));
    }
    let mut table = VertexTable::new(origin, settings.duplicate_tolerance);

    // Merge the points: breakline vertices first, so a point on a breakline takes its elevation.
    let mut acc: Vec<Acc> = Vec::new();
    let mut merged = 0usize;
    let mut max_dz = 0.0_f64;
    let mut put = |table: &mut VertexTable, acc: &mut Vec<Acc>, p: [f64; 3], brk: bool| -> u32 {
        let (idx, new) = table.add(p);
        if new {
            acc.push(Acc { first: p[2], min: p[2], max: p[2], sum: p[2], count: 1, brk: brk.then_some(p[2]) });
        } else {
            let a = &mut acc[idx as usize];
            merged += 1;
            max_dz = max_dz.max((p[2] - a.brk.unwrap_or(a.first)).abs());
            a.min = a.min.min(p[2]);
            a.max = a.max.max(p[2]);
            a.sum += p[2];
            a.count += 1;
            if brk && a.brk.is_none() {
                a.brk = Some(p[2]);
            }
        }
        idx
    };
    let mut standard: Vec<Constraint> = Vec::new();
    let mut proximity: Vec<&Breakline> = Vec::new();
    for l in &st.lines {
        match l.kind {
            BreaklineKind::Standard => {
                let verts: Vec<u32> = l.vertices.iter().map(|v| put(&mut table, &mut acc, *v, true)).collect();
                standard.push(Constraint { verts: dedup_run(verts), closed: false, kind: ConstraintKind::Break, name: l.name.clone() });
            }
            BreaklineKind::Proximity => proximity.push(l),
        }
    }
    for l in &st.loose {
        put(&mut table, &mut acc, l.p, false);
    }
    // Pasted surfaces are not part of `points_input`; the merge count is for everything.
    let apply_policy = |table: &mut VertexTable, acc: &[Acc]| {
        for (i, a) in acc.iter().enumerate() {
            table.pts[i][2] = match (a.brk, settings.duplicate_policy) {
                (Some(z), _) => z,
                (None, DuplicatePolicy::First) => a.first,
                (None, DuplicatePolicy::Lowest) => a.min,
                (None, DuplicatePolicy::Highest) => a.max,
                (None, DuplicatePolicy::Average) => a.sum / a.count as f64,
            };
        }
    };
    apply_policy(&mut table, &acc);
    report.duplicates_merged = merged;
    report.duplicate_max_dz = max_dz;
    if table.pts.len() < 3 {
        return Err(TerrainError::TooFewPoints(table.pts.len()));
    }

    // Elevations of proximity breaklines and boundary vertices come from the surface of the
    // other data, so triangulate that first when either exists.
    let mut constraints: Vec<Constraint> = standard;
    if !proximity.is_empty() || !st.boundaries.is_empty() {
        let first = triangulate(&mut table, &constraints);
        let sampler = first.ok().and_then(|o| {
            let tris: Vec<[u32; 3]> = o.triangles.into_iter().filter(|t| area2(table.pts[t[0] as usize], table.pts[t[1] as usize], table.pts[t[2] as usize]) != 0.0).collect();
            if tris.is_empty() {
                None
            } else {
                Tin::new(table.pts.clone(), tris).ok()
            }
        });
        let known = table.pts.len();
        let snapshot: Vec<[f64; 3]> = table.pts.clone();
        let sample = |x: f64, y: f64| -> f64 { sampler.as_ref().and_then(|t| t.elevation_at(x, y)).unwrap_or_else(|| nearest_z(&snapshot[..known], x, y)) };
        for l in &proximity {
            let verts: Vec<u32> = l.vertices.iter().map(|v| table.add([v[0], v[1], sample(v[0], v[1])]).0).collect();
            let verts = dedup_run(verts);
            if verts.len() >= 2 {
                constraints.push(Constraint { verts, closed: false, kind: ConstraintKind::Break, name: l.name.clone() });
            } else {
                report.warnings.push(format!("proximity breakline '{}' collapsed to one point and was ignored", l.name));
            }
        }
        for (_, name, ring) in &st.boundaries {
            let verts: Vec<u32> = ring.iter().map(|v| table.add([v[0], v[1], sample(v[0], v[1])]).0).collect();
            let verts = dedup_run(verts);
            if verts.len() >= 3 {
                constraints.push(Constraint { verts, closed: true, kind: ConstraintKind::Boundary, name: name.clone() });
            } else {
                report.warnings.push(format!("boundary '{name}' collapsed to fewer than three points and was ignored"));
            }
        }
    }

    let out = triangulate(&mut table, &constraints)?;
    report.crossings = out.crossings;
    report.triangles_degenerate = out.degenerate_dropped;
    let pts = &table.pts;
    let mut tris: Vec<[u32; 3]> = Vec::with_capacity(out.triangles.len());
    for t in out.triangles {
        if area2(pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]) == 0.0 {
            report.triangles_degenerate += 1;
        } else {
            tris.push(t);
        }
    }
    report.triangles_formed = tris.len();
    if tris.is_empty() {
        return Err(TerrainError::NoSurface("every triangle has zero plan area".into()));
    }

    // Filters.
    let mut vis = vec![true; tris.len()];
    let mut after_max = vis.clone();
    let local = |i: u32| table.local(i);
    let centroid = |t: &[u32; 3]| {
        let (a, b, c) = (local(t[0]), local(t[1]), local(t[2]));
        [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0]
    };
    for f in &st.filters {
        match f {
            Filter::MaxEdge(l) => {
                let l2 = l * l;
                for (k, t) in tris.iter().enumerate() {
                    let side2 = |a: u32, b: u32| (pts[a as usize][0] - pts[b as usize][0]).powi(2) + (pts[a as usize][1] - pts[b as usize][1]).powi(2);
                    if side2(t[0], t[1]) > l2 || side2(t[1], t[2]) > l2 || side2(t[2], t[0]) > l2 {
                        vis[k] = false;
                    }
                }
                after_max = vis.clone();
                report.triangles_removed_max_edge = vis.iter().filter(|v| !**v).count();
            }
            Filter::Boundary { kind, verts } => {
                let poly: Vec<[f64; 2]> = verts.iter().map(|v| [v[0] - origin[0], v[1] - origin[1]]).collect();
                for (k, t) in tris.iter().enumerate() {
                    let inside = point_in_polygon(centroid(t), &poly);
                    match kind {
                        BoundaryKind::Outer => vis[k] &= inside,
                        BoundaryKind::Hide => vis[k] &= !inside,
                        BoundaryKind::Show => vis[k] |= inside,
                    }
                }
            }
        }
    }
    report.triangles_removed_boundary = (0..tris.len()).filter(|&k| after_max[k] && !vis[k]).count();
    report.triangles_restored_by_show = (0..tris.len()).filter(|&k| !after_max[k] && vis[k]).count();
    let mut tris: Vec<[u32; 3]> = tris.into_iter().zip(vis).filter(|(_, v)| *v).map(|(t, _)| t).collect();

    // Edits of the triangulation.
    for (n, e) in st.topo.iter().enumerate() {
        match e {
            Topo::DeleteTriangle { x, y } => {
                let hit = tris.iter().position(|t| contains(pts, t, [*x, *y]));
                match hit {
                    Some(k) => {
                        tris.remove(k);
                        report.triangles_deleted += 1;
                    }
                    None => report.warnings.push(format!("delete triangle at ({x}, {y}): no triangle there")),
                }
            }
            Topo::Swap { a, b, tol } => match swap_edge(pts, &mut tris, &out.constrained, *a, *b, *tol) {
                Ok(()) => report.edges_swapped += 1,
                Err(why) => report.warnings.push(format!("swap edge {n}: {why}")),
            },
        }
    }
    if tris.is_empty() {
        return Err(TerrainError::NoSurface("every triangle was removed by boundaries, the maximum edge length or edits".into()));
    }

    // Only the points in use.
    let mut renumber = vec![u32::MAX; pts.len()];
    let mut used: Vec<[f64; 3]> = Vec::new();
    for c in tris.iter_mut().flatten() {
        let old = *c as usize;
        if renumber[old] == u32::MAX {
            renumber[old] = used.len() as u32;
            used.push(pts[old]);
        }
        *c = renumber[old];
    }
    report.points_unused = pts.len() - used.len();
    report.points_used = used.len();
    report.triangles = tris.len();
    let tin = Tin::new(used, tris)?;
    Ok(BuiltSurface { tin, report })
}

/// Removes consecutive repeats (and a repeated last vertex of a ring is left to the caller).
fn dedup_run(mut v: Vec<u32>) -> Vec<u32> {
    v.dedup();
    v
}

fn contains(pts: &[[f64; 3]], t: &[u32; 3], p: [f64; 2]) -> bool {
    let q = |i: usize| [pts[t[i] as usize][0], pts[t[i] as usize][1]];
    (0..3).all(|i| orient(q(i), q((i + 1) % 3), p) >= 0.0)
}

fn nearest_vertex(pts: &[[f64; 3]], tris: &[[u32; 3]], at: [f64; 2], tol: f64) -> Option<u32> {
    let mut best: Option<(f64, u32)> = None;
    for &v in tris.iter().flatten() {
        let d = (pts[v as usize][0] - at[0]).hypot(pts[v as usize][1] - at[1]);
        if d <= tol && best.is_none_or(|b| d < b.0) {
            best = Some((d, v));
        }
    }
    best.map(|b| b.1)
}

fn swap_edge(pts: &[[f64; 3]], tris: &mut [[u32; 3]], constrained: &std::collections::HashSet<(u32, u32)>, a: [f64; 2], b: [f64; 2], tol: f64) -> Result<(), String> {
    let va = nearest_vertex(pts, tris, a, tol).ok_or("no vertex at the first end")?;
    let vb = nearest_vertex(pts, tris, b, tol).ok_or("no vertex at the second end")?;
    if va == vb {
        return Err("both ends are the same vertex".into());
    }
    if constrained.contains(&(va.min(vb), va.max(vb))) {
        return Err("the edge is a breakline or boundary edge and cannot be swapped".into());
    }
    let sharing: Vec<usize> = (0..tris.len()).filter(|&k| tris[k].contains(&va) && tris[k].contains(&vb)).collect();
    if sharing.len() != 2 {
        return Err(if sharing.is_empty() { "the two vertices are not joined by an edge".into() } else { "the edge is on the surface boundary, only one triangle uses it".into() });
    }
    let other = |k: usize| *tris[k].iter().find(|&&v| v != va && v != vb).unwrap();
    let (vc, vd) = (other(sharing[0]), other(sharing[1]));
    let xy = |v: u32| [pts[v as usize][0], pts[v as usize][1]];
    if !proper_crossing(xy(va), xy(vb), xy(vc), xy(vd)) {
        return Err("the two triangles do not form a convex quadrilateral".into());
    }
    let ccw = |t: [u32; 3]| if orient(xy(t[0]), xy(t[1]), xy(t[2])) > 0.0 { t } else { [t[0], t[2], t[1]] };
    let (n1, n2) = (ccw([vc, vd, va]), ccw([vd, vc, vb]));
    if area2(pts[n1[0] as usize], pts[n1[1] as usize], pts[n1[2] as usize]) == 0.0 || area2(pts[n2[0] as usize], pts[n2[1] as usize], pts[n2[2] as usize]) == 0.0 {
        return Err("the swap would make a triangle with no area".into());
    }
    tris[sharing[0]] = n1;
    tris[sharing[1]] = n2;
    Ok(())
}
