// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Point groups: named, ordered sets of points defined by rules.
//!
//! A point is in a group when it matches at least one include rule (or the group includes all
//! points) and no exclude rule. Groups are rules, not stored lists, so they follow the store:
//! [`PointGroupSet::refresh`] re-evaluates them when the store's revision changed.
//!
//! Order: groups are kept in the order given, with the built-in `_All Points` group always last.
//! [`PointGroupSet::controlling_group`] returns the first group in order that holds a point, which is
//! the rule this engine uses for "which group's styles win".

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::deskey::glob_match;
use crate::error::{CogoError, Result};
use crate::point::{CogoPoint, PointStore};

pub const ALL_POINTS: &str = "_All Points";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Rule {
    /// Point numbers from..=to.
    NumberRange { from: u32, to: u32 },
    /// Wildcard on the point name (points without a name never match).
    Name { pattern: String },
    /// Wildcard on the whole raw description.
    RawDescription { pattern: String },
    /// Elevation between min and max inclusive (points without elevation never match).
    ElevationRange { min: f64, max: f64 },
}

impl Rule {
    pub fn matches(&self, p: &CogoPoint) -> bool {
        match self {
            Rule::NumberRange { from, to } => (*from..=*to).contains(&p.number),
            Rule::Name { pattern } => p.name.as_deref().is_some_and(|n| glob_match(pattern, n, false)),
            Rule::RawDescription { pattern } => glob_match(pattern, p.raw_description.trim(), false),
            Rule::ElevationRange { min, max } => p.elevation.is_some_and(|z| z >= *min && z <= *max),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointGroup {
    pub name: String,
    pub include_all: bool,
    pub include: Vec<Rule>,
    pub exclude: Vec<Rule>,
    /// The group is meant to feed a surface: [`PointGroupSet::surface_points`] returns only its points
    /// that have an elevation.
    pub surface_points: bool,
}

impl PointGroup {
    pub fn new(name: &str) -> PointGroup {
        PointGroup { name: name.to_string(), include_all: false, include: vec![], exclude: vec![], surface_points: false }
    }
    pub fn contains(&self, p: &CogoPoint) -> bool {
        (self.include_all || self.include.iter().any(|r| r.matches(p))) && !self.exclude.iter().any(|r| r.matches(p))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointGroupSet {
    groups: Vec<PointGroup>,
    #[serde(skip)]
    cache: BTreeMap<String, Vec<u32>>,
    #[serde(skip)]
    cache_rev: Option<u64>,
}

impl Default for PointGroupSet {
    fn default() -> Self {
        PointGroupSet::new()
    }
}

impl PointGroupSet {
    pub fn new() -> PointGroupSet {
        let mut all = PointGroup::new(ALL_POINTS);
        all.include_all = true;
        PointGroupSet { groups: vec![all], cache: BTreeMap::new(), cache_rev: None }
    }
    pub fn groups(&self) -> &[PointGroup] {
        &self.groups
    }
    pub fn get(&self, name: &str) -> Option<&PointGroup> {
        self.groups.iter().find(|g| g.name == name)
    }
    /// Add a group before `_All Points`. Names are unique.
    pub fn add(&mut self, g: PointGroup) -> Result<()> {
        if self.get(&g.name).is_some() {
            return Err(CogoError::Invalid(format!("group '{}' already exists", g.name)));
        }
        let at = self.groups.len() - 1;
        self.groups.insert(at, g);
        self.cache_rev = None;
        Ok(())
    }
    pub fn remove(&mut self, name: &str) -> Result<()> {
        if name == ALL_POINTS {
            return Err(CogoError::Invalid("the all points group cannot be removed".into()));
        }
        let i = self.groups.iter().position(|g| g.name == name).ok_or_else(|| CogoError::NoSuchGroup(name.into()))?;
        self.groups.remove(i);
        self.cache_rev = None;
        Ok(())
    }
    /// Move a group to `index` among the user groups (it never goes past `_All Points`).
    pub fn move_to(&mut self, name: &str, index: usize) -> Result<()> {
        if name == ALL_POINTS {
            return Err(CogoError::Invalid("the all points group stays last".into()));
        }
        let i = self.groups.iter().position(|g| g.name == name).ok_or_else(|| CogoError::NoSuchGroup(name.into()))?;
        let g = self.groups.remove(i);
        let at = index.min(self.groups.len() - 1);
        self.groups.insert(at, g);
        self.cache_rev = None;
        Ok(())
    }
    /// Edit a group's rules.
    pub fn edit<F: FnOnce(&mut PointGroup)>(&mut self, name: &str, f: F) -> Result<()> {
        if name == ALL_POINTS {
            return Err(CogoError::Invalid("the all points group cannot be edited".into()));
        }
        let g = self.groups.iter_mut().find(|g| g.name == name).ok_or_else(|| CogoError::NoSuchGroup(name.into()))?;
        f(g);
        self.cache_rev = None;
        Ok(())
    }

    /// Re-evaluate every group if the store changed (or a group did) since the last call.
    pub fn refresh(&mut self, store: &PointStore) {
        if self.cache_rev == Some(store.revision()) {
            return;
        }
        self.cache.clear();
        for g in &self.groups {
            self.cache.insert(g.name.clone(), store.iter().filter(|p| g.contains(p)).map(|p| p.number).collect());
        }
        self.cache_rev = Some(store.revision());
    }

    /// Point numbers of a group, in number order. Call [`refresh`](Self::refresh) first, or use
    /// [`members_of`](Self::members_of).
    pub fn members(&self, name: &str) -> Option<&[u32]> {
        self.cache.get(name).map(Vec::as_slice)
    }

    pub fn members_of(&mut self, store: &PointStore, name: &str) -> Result<Vec<u32>> {
        self.refresh(store);
        self.members(name).map(<[u32]>::to_vec).ok_or_else(|| CogoError::NoSuchGroup(name.into()))
    }

    /// The first group in order that holds the point.
    pub fn controlling_group(&mut self, store: &PointStore, number: u32) -> Option<String> {
        self.refresh(store);
        self.groups.iter().find(|g| self.cache.get(&g.name).is_some_and(|v| v.binary_search(&number).is_ok())).map(|g| g.name.clone())
    }

    /// Points of a surface group that carry an elevation. A group not flagged `surface_points` is an error.
    pub fn surface_points<'a>(&mut self, store: &'a PointStore, name: &str) -> Result<Vec<&'a CogoPoint>> {
        let g = self.get(name).ok_or_else(|| CogoError::NoSuchGroup(name.into()))?;
        if !g.surface_points {
            return Err(CogoError::Invalid(format!("group '{name}' is not flagged as surface points")));
        }
        let m = self.members_of(store, name)?;
        Ok(m.iter().filter_map(|n| store.get(*n)).filter(|p| p.elevation.is_some()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> PointStore {
        let mut s = PointStore::new();
        for (n, d, z) in [(1, "TR", Some(5.0)), (2, "EP", Some(7.0)), (3, "EP", None), (10, "CL", Some(9.0))] {
            s.add(CogoPoint::new(n, n as f64, 0.0, z, d)).unwrap();
        }
        s
    }

    #[test]
    fn rules_and_all_group() {
        let st = store();
        let mut gs = PointGroupSet::new();
        let mut g = PointGroup::new("EPs");
        g.include.push(Rule::RawDescription { pattern: "ep*".into() });
        g.exclude.push(Rule::NumberRange { from: 3, to: 3 });
        gs.add(g).unwrap();
        assert_eq!(gs.members_of(&st, "EPs").unwrap(), vec![2]);
        assert_eq!(gs.members_of(&st, ALL_POINTS).unwrap(), vec![1, 2, 3, 10]);
        assert_eq!(gs.groups().last().unwrap().name, ALL_POINTS);
        assert_eq!(gs.controlling_group(&st, 2).as_deref(), Some("EPs"));
        assert_eq!(gs.controlling_group(&st, 1).as_deref(), Some(ALL_POINTS));
        assert!(gs.remove(ALL_POINTS).is_err());
    }

    #[test]
    fn groups_follow_the_store() {
        let mut st = store();
        let mut gs = PointGroupSet::new();
        let mut g = PointGroup::new("hi");
        g.include.push(Rule::ElevationRange { min: 6.0, max: 20.0 });
        gs.add(g).unwrap();
        assert_eq!(gs.members_of(&st, "hi").unwrap(), vec![2, 10]);
        st.move_point(1, 1.0, 0.0, Some(8.0), "fix").unwrap();
        st.add(CogoPoint::new(11, 0.0, 0.0, Some(6.0), "")).unwrap();
        assert_eq!(gs.members_of(&st, "hi").unwrap(), vec![1, 2, 10, 11]);
    }

    #[test]
    fn name_rule_and_surface() {
        let mut st = store();
        st.edit(3, |p| p.name = Some("BM-1".into())).unwrap();
        let mut gs = PointGroupSet::new();
        let mut g = PointGroup::new("surf");
        g.include_all = true;
        g.surface_points = true;
        g.exclude.push(Rule::Name { pattern: "bm*".into() });
        gs.add(g).unwrap();
        gs.add(PointGroup::new("plain")).unwrap();
        let pts = gs.surface_points(&st, "surf").unwrap();
        assert_eq!(pts.iter().map(|p| p.number).collect::<Vec<_>>(), vec![1, 2, 10]);
        assert!(gs.surface_points(&st, "plain").is_err());
        gs.move_to("plain", 0).unwrap();
        assert_eq!(gs.groups()[0].name, "plain");
    }
}
