// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Grouping clashes so one issue can cover a group.
//!
//! Two clashes belong together when they share an element, or when their points lie within a
//! distance the user entered. Groups are the connected components of that relation (a union of
//! pairs, so a chain A-B, B-C, C-D is one group even if A and D are far apart). No distance is
//! built in: with `distance_mm = None` only shared elements group.

use std::collections::{BTreeSet, HashMap};

use afrobim_ifc::name_uuid;
use afrobim_kernel::ElementId;
use serde::{Deserialize, Serialize};

use crate::geom::{dist, Aabb, V3};
use crate::results::Clash;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroupOptions {
    /// Group clashes that share an element.
    pub share_element: bool,
    /// Group clashes whose points are at most this far apart (mm). `None` switches the
    /// distance rule off. The caller enters the value; nothing is assumed.
    pub distance_mm: Option<f64>,
    /// Elements that never link clashes by being shared, for example a terrain surface or a
    /// pavement layer that touches everything. Clashes can still group by distance.
    pub ignore_shared: Vec<ElementId>,
}

impl GroupOptions {
    pub fn by_element() -> GroupOptions {
        GroupOptions { share_element: true, ..GroupOptions::default() }
    }
    pub fn by_distance(mm: f64) -> GroupOptions {
        GroupOptions { distance_mm: Some(mm), ..GroupOptions::default() }
    }
    pub fn by_element_or_distance(mm: f64) -> GroupOptions {
        GroupOptions { share_element: true, distance_mm: Some(mm), ignore_shared: Vec::new() }
    }
}

/// A set of clashes that belong together.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClashGroup {
    /// Stable for the same member clashes: a hash of their sorted ids.
    pub id: String,
    /// Member clash ids, sorted.
    pub clash_ids: Vec<String>,
    /// The distinct elements of the members, sorted.
    pub elements: Vec<ElementId>,
    /// Mean of the member points, world mm.
    pub centroid: V3,
    /// Box over the member points, world mm.
    pub bounds: Aabb,
}

struct Dsu(Vec<usize>);

impl Dsu {
    fn find(&mut self, mut x: usize) -> usize {
        while self.0[x] != x {
            self.0[x] = self.0[self.0[x]];
            x = self.0[x];
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a.max(b)] = a.min(b);
        }
    }
}

/// Groups `clashes` under `options`. Every clash lands in exactly one group; a clash linked to
/// nothing is a group of one. Groups are ordered by their first member's position in `clashes`.
pub fn group_clashes(clashes: &[Clash], options: &GroupOptions) -> Vec<ClashGroup> {
    let n = clashes.len();
    let mut dsu = Dsu((0..n).collect());
    if options.share_element {
        let mut first: HashMap<ElementId, usize> = HashMap::new();
        for (i, c) in clashes.iter().enumerate() {
            for e in [c.element_a, c.element_b] {
                if options.ignore_shared.contains(&e) {
                    continue;
                }
                match first.get(&e) {
                    Some(&j) => dsu.union(i, j),
                    None => {
                        first.insert(e, i);
                    }
                }
            }
        }
    }
    if let Some(d) = options.distance_mm.filter(|d| d.is_finite() && *d >= 0.0) {
        // Sweep along x: sort by x, compare each clash with the following ones while the x gap
        // is within the distance.
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| clashes[a].point[0].total_cmp(&clashes[b].point[0]));
        for (pos, &i) in order.iter().enumerate() {
            for &j in &order[pos + 1..] {
                if clashes[j].point[0] - clashes[i].point[0] > d {
                    break;
                }
                if dist(clashes[i].point, clashes[j].point) <= d {
                    dsu.union(i, j);
                }
            }
        }
    }
    let mut members: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut slot: HashMap<usize, usize> = HashMap::new();
    for i in 0..n {
        let root = dsu.find(i);
        let k = *slot.entry(root).or_insert_with(|| {
            members.push((root, Vec::new()));
            members.len() - 1
        });
        members[k].1.push(i);
    }
    members
        .into_iter()
        .map(|(_, idx)| {
            let mut ids: Vec<String> = idx.iter().map(|&i| clashes[i].id.clone()).collect();
            ids.sort();
            let elements: BTreeSet<ElementId> = idx.iter().flat_map(|&i| [clashes[i].element_a, clashes[i].element_b]).collect();
            let mut sum = [0.0; 3];
            let mut bounds = Aabb::empty();
            for &i in &idx {
                for (acc, v) in sum.iter_mut().zip(clashes[i].point) {
                    *acc += v;
                }
                bounds.grow(clashes[i].point);
            }
            let m = idx.len() as f64;
            ClashGroup {
                id: name_uuid(&format!("clash-group/{}", ids.join(","))).replace('-', "")[..16].to_string(),
                clash_ids: ids,
                elements: elements.into_iter().collect(),
                centroid: [sum[0] / m, sum[1] / m, sum[2] / m],
                bounds,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::results::{clash_id, Status};
    use crate::rules::ClashKind;

    fn clash(a: u64, b: u64, point: V3) -> Clash {
        Clash {
            id: clash_id("T", ElementId(a), ElementId(b)),
            test: "T".into(),
            element_a: ElementId(a.min(b)),
            element_b: ElementId(a.max(b)),
            mark_a: format!("M{a}"),
            mark_b: format!("M{b}"),
            category_a: "x".into(),
            category_b: "y".into(),
            kind: ClashKind::Hard,
            depth_mm: Some(1.0),
            distance_mm: None,
            containment: false,
            point,
            overlap_box: Aabb::of_points(&[point]),
            status: Status::New,
            comment: String::new(),
            assignee: String::new(),
        }
    }

    #[test]
    fn shared_elements_chain_into_one_group_and_others_stay_apart() {
        let cs = [clash(1, 2, [0.0; 3]), clash(2, 3, [1e6, 0.0, 0.0]), clash(3, 4, [2e6, 0.0, 0.0]), clash(8, 9, [5.0, 5.0, 5.0])];
        let g = group_clashes(&cs, &GroupOptions::by_element());
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].clash_ids.len(), 3);
        assert_eq!(g[0].elements, vec![ElementId(1), ElementId(2), ElementId(3), ElementId(4)]);
        assert_eq!(g[1].clash_ids.len(), 1);
    }

    #[test]
    fn distance_groups_nearby_points_and_no_distance_means_none() {
        let cs = [clash(1, 2, [0.0; 3]), clash(3, 4, [400.0, 0.0, 0.0]), clash(5, 6, [900.0, 0.0, 0.0]), clash(7, 8, [0.0, 0.0, 5000.0])];
        assert_eq!(group_clashes(&cs, &GroupOptions::default()).len(), 4);
        let g = group_clashes(&cs, &GroupOptions::by_distance(500.0));
        // 0-400 and 400-900 are within 500 mm, so the first three chain; the last is 5 m up.
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].clash_ids.len(), 3);
        assert_eq!(group_clashes(&cs, &GroupOptions::by_distance(399.0)).len(), 4);
        assert_eq!(group_clashes(&cs, &GroupOptions::by_distance(400.0)).len(), 3);
    }

    #[test]
    fn ignored_shared_elements_do_not_link_and_ids_are_stable() {
        let cs = [clash(1, 100, [0.0; 3]), clash(2, 100, [9e5, 0.0, 0.0])];
        let linked = group_clashes(&cs, &GroupOptions::by_element());
        assert_eq!(linked.len(), 1);
        let opts = GroupOptions { share_element: true, distance_mm: None, ignore_shared: vec![ElementId(100)] };
        let apart = group_clashes(&cs, &opts);
        assert_eq!(apart.len(), 2);
        let reversed = [cs[1].clone(), cs[0].clone()];
        assert_eq!(group_clashes(&reversed, &GroupOptions::by_element())[0].id, linked[0].id);
        assert_ne!(apart[0].id, apart[1].id);
        assert_eq!(linked[0].centroid, [4.5e5, 0.0, 0.0]);
    }
}
