//! The dependency table: which fields of an element's `data` name other objects.
//!
//! The kernel does not know what a corridor is, so the table is data. Each rule says: for an
//! element of `category`, the JSON at `path` names other elements, either by name or by element
//! id, and those elements are of one of `targets` categories. A path is dot separated; a segment
//! ending in `[]` walks every item of an array (`model.baselines[].alignment`). The last segment
//! may name a single value or an array of values.
//!
//! The application extends the table with [`DependencyTable::add`] or loads one from JSON; the
//! built-in [`DependencyTable::civil`] covers the `civil-*` categories of the application and the
//! older `corridor` and `surface` categories of `afrobim-corridor`.

use std::collections::{BTreeMap, HashMap, HashSet};

use afrobim_kernel::{Document, ElementId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::project::{DocCache, ShortcutProject};
use crate::reference::reference_info;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetBy {
    /// The field holds the element's name.
    Name,
    /// The field holds the element's id in the same document (rewritten to the host id when copied).
    Id,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DependencyRule {
    pub category: String,
    pub path: String,
    pub by: TargetBy,
    pub targets: Vec<String>,
    /// When false, a field that names nothing findable (or nothing published) is skipped and
    /// reported as a note instead of refusing the reference.
    #[serde(default = "yes")]
    pub required: bool,
}

impl DependencyRule {
    pub fn new(category: &str, path: &str, by: TargetBy, targets: &[&str], required: bool) -> DependencyRule {
        DependencyRule { category: category.into(), path: path.into(), by, targets: targets.iter().map(|s| s.to_string()).collect(), required }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DependencyTable {
    pub rules: Vec<DependencyRule>,
}

impl DependencyTable {
    pub fn empty() -> DependencyTable {
        DependencyTable::default()
    }

    /// The rules for the application's civil categories and the corridor crate's own.
    ///
    /// | category | field | names | target categories |
    /// |---|---|---|---|
    /// | `civil-profile` | `alignment` | name | `alignment` |
    /// | `civil-profile` | `surface` | id | `civil-surface`, `surface` |
    /// | `civil-profile-view` | `alignment` | name | `alignment` |
    /// | `civil-profile-view` | `profiles[]` | id | `civil-profile` |
    /// | `civil-corridor` | `model.baselines[].alignment` | name | `alignment` |
    /// | `civil-corridor` | `model.baselines[].regions[].assembly.name` | name | `civil-assembly` (optional: the assembly is also stored inside the corridor) |
    /// | `civil-corridor` | `ground` | id | `civil-surface`, `surface` |
    /// | `civil-samples` | `group.alignment` | name | `alignment` |
    /// | `civil-samples` | `corridor` | id | `civil-corridor` |
    /// | `civil-samples` | `surfaces[]` | id | `civil-surface`, `surface` |
    /// | `corridor` | `alignment` | id | `alignment` |
    /// | `corridor` | `surface` | id | `surface` |
    /// | `civil-pipenet` | `surface` | id | `civil-surface`, `surface` (optional) |
    /// | `civil-pipenet` | `partners[]` | id | `civil-pipenet` (optional) |
    /// | `civil-pipeprofile` | `network` | id | `civil-pipenet` |
    /// | `civil-pipeprofile` | `surface` | id | `civil-surface`, `surface` (optional) |
    /// | `civil-pipeprofile` | `alignment` | name | `alignment` (optional) |
    /// | `civil-grading` | `ground`, `base` | id | `civil-surface`, `surface` (optional) |
    /// | `civil-grading` | `group.gradings[].footprint` | name | `civil-featureline` (optional) |
    /// | `civil-intersection` | `inputs.primary`, `inputs.secondary` | name | `alignment` |
    /// | `civil-intersection` | `created[]` | id | `alignment` (optional) |
    /// | `civil-intersection` | `corridor` | id | `civil-corridor` (optional) |
    /// | `civil-intersection` | `ground` | id | `civil-surface`, `surface` (optional) |
    /// | `civil-roundabout` | `inputs.arms[].alignment` | name | `alignment` |
    /// | `civil-roundabout` | `created[]` | id | `alignment` (optional) |
    ///
    /// `civil-featureline`, `civil-points`, `civil-surface`, `civil-figures` and `civil-pointcloud`
    /// name no other element (a point cloud names a file; a grading names the feature lines of its
    /// footprint by the name the line has in its site).
    pub fn civil() -> DependencyTable {
        use TargetBy::{Id, Name};
        let surf = ["civil-surface", "surface"];
        DependencyTable {
            rules: vec![
                DependencyRule::new("civil-profile", "alignment", Name, &["alignment"], true),
                DependencyRule::new("civil-profile", "surface", Id, &surf, true),
                DependencyRule::new("civil-profile-view", "alignment", Name, &["alignment"], true),
                DependencyRule::new("civil-profile-view", "profiles[]", Id, &["civil-profile"], true),
                DependencyRule::new("civil-corridor", "model.baselines[].alignment", Name, &["alignment"], true),
                DependencyRule::new("civil-corridor", "model.baselines[].regions[].assembly.name", Name, &["civil-assembly"], false),
                DependencyRule::new("civil-corridor", "ground", Id, &surf, true),
                DependencyRule::new("civil-samples", "group.alignment", Name, &["alignment"], true),
                DependencyRule::new("civil-samples", "corridor", Id, &["civil-corridor"], false),
                DependencyRule::new("civil-samples", "surfaces[]", Id, &surf, true),
                DependencyRule::new("corridor", "alignment", Id, &["alignment"], true),
                DependencyRule::new("corridor", "surface", Id, &["surface"], true),
                // Round 6 categories. The fields were read from the application's data types.
                DependencyRule::new("civil-pipenet", "surface", Id, &surf, false),
                DependencyRule::new("civil-pipenet", "partners[]", Id, &["civil-pipenet"], false),
                DependencyRule::new("civil-pipeprofile", "network", Id, &["civil-pipenet"], true),
                DependencyRule::new("civil-pipeprofile", "surface", Id, &surf, false),
                DependencyRule::new("civil-pipeprofile", "alignment", Name, &["alignment"], false),
                DependencyRule::new("civil-grading", "ground", Id, &surf, false),
                DependencyRule::new("civil-grading", "base", Id, &surf, false),
                DependencyRule::new("civil-grading", "group.gradings[].footprint", Name, &["civil-featureline"], false),
                DependencyRule::new("civil-intersection", "inputs.primary", Name, &["alignment"], true),
                DependencyRule::new("civil-intersection", "inputs.secondary", Name, &["alignment"], true),
                DependencyRule::new("civil-intersection", "created[]", Id, &["alignment"], false),
                DependencyRule::new("civil-intersection", "corridor", Id, &["civil-corridor"], false),
                DependencyRule::new("civil-intersection", "ground", Id, &surf, false),
                DependencyRule::new("civil-roundabout", "inputs.arms[].alignment", Name, &["alignment"], true),
                DependencyRule::new("civil-roundabout", "created[]", Id, &["alignment"], false),
            ],
        }
    }

    pub fn add(&mut self, rule: DependencyRule) {
        self.rules.push(rule);
    }

    pub fn rules_for<'a>(&'a self, category: &'a str) -> impl Iterator<Item = &'a DependencyRule> {
        self.rules.iter().filter(move |r| r.category == category)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a table serialises")
    }

    pub fn from_json(text: &str) -> Result<DependencyTable, serde_json::Error> {
        serde_json::from_str(text)
    }
}

// ─── walking JSON paths ───────────────────────────────────────────────────

struct Seg {
    key: String,
    each: bool,
}

fn parse(path: &str) -> Vec<Seg> {
    path.split('.')
        .filter(|s| !s.is_empty())
        .map(|s| match s.strip_suffix("[]") {
            Some(k) => Seg { key: k.to_string(), each: true },
            None => Seg { key: s.to_string(), each: false },
        })
        .collect()
}

fn collect(v: &Value, segs: &[Seg], out: &mut Vec<Value>) {
    match segs.split_first() {
        None => match v {
            Value::Array(a) => out.extend(a.iter().cloned()),
            Value::Null => {}
            x => out.push(x.clone()),
        },
        Some((s, rest)) => {
            if let Some(c) = v.get(&s.key) {
                if s.each {
                    if let Some(a) = c.as_array() {
                        for i in a {
                            collect(i, rest, out);
                        }
                    }
                } else {
                    collect(c, rest, out);
                }
            }
        }
    }
}

fn map_mut(v: &mut Value, segs: &[Seg], f: &mut dyn FnMut(&mut Value)) {
    match segs.split_first() {
        None => match v {
            Value::Array(a) => a.iter_mut().for_each(|x| f(x)),
            Value::Null => {}
            x => f(x),
        },
        Some((s, rest)) => {
            if let Some(c) = v.get_mut(&s.key) {
                if s.each {
                    if let Some(a) = c.as_array_mut() {
                        for i in a {
                            map_mut(i, rest, f);
                        }
                    }
                } else {
                    map_mut(c, rest, f);
                }
            }
        }
    }
}

/// The values found at `path` (cloned, nulls skipped).
pub fn values_at(data: &Value, path: &str) -> Vec<Value> {
    let mut out = Vec::new();
    collect(data, &parse(path), &mut out);
    out
}

/// Rewrites the id fields of `data` named by the table's `Id` rules, using `map` (source id to host id).
/// Ids not in the map are left as they are.
pub fn remap_ids(data: &mut Value, category: &str, table: &DependencyTable, map: &BTreeMap<u64, u64>) {
    for rule in table.rules_for(category).filter(|r| r.by == TargetBy::Id) {
        map_mut(data, &parse(&rule.path), &mut |v| {
            if let Some(id) = v.as_u64() {
                if let Some(new) = map.get(&id) {
                    *v = Value::from(*new);
                }
            }
        });
    }
}

// ─── closure ──────────────────────────────────────────────────────────────

/// Elements of one document that `root` depends on, dependencies first, `root` last. Reference
/// elements are not followed (they are already shortcuts). Fields that name nothing are skipped.
pub fn local_closure(doc: &Document, root: ElementId, table: &DependencyTable) -> Vec<ElementId> {
    fn visit(doc: &Document, id: ElementId, table: &DependencyTable, seen: &mut HashSet<ElementId>, out: &mut Vec<ElementId>) {
        if !seen.insert(id) {
            return;
        }
        let Some(e) = doc.find(id) else { return };
        if reference_info(e).is_some() {
            return;
        }
        for rule in table.rules_for(&e.category) {
            for dep in find_targets(doc, rule, &e.data) {
                visit(doc, dep, table, seen, out);
            }
        }
        out.push(id);
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    visit(doc, root, table, &mut seen, &mut out);
    out
}

fn find_targets(doc: &Document, rule: &DependencyRule, data: &Value) -> Vec<ElementId> {
    values_at(data, &rule.path).iter().filter_map(|v| find_target(doc, rule, v)).collect()
}

fn find_target(doc: &Document, rule: &DependencyRule, v: &Value) -> Option<ElementId> {
    let ok = |cat: &str| rule.targets.iter().any(|t| t == cat);
    match rule.by {
        TargetBy::Id => doc.find(ElementId(v.as_u64()?)).filter(|e| ok(&e.category)).map(|e| e.id),
        TargetBy::Name => {
            let n = v.as_str().filter(|s| !s.is_empty())?;
            doc.elements().iter().find(|e| e.name == n && ok(&e.category)).map(|e| e.id)
        }
    }
}

/// The shortcuts a shortcut depends on, as the manifest knows them.
pub(crate) struct KeyClosure {
    /// Dependency keys first, the root key last.
    pub order: Vec<String>,
    /// For each key, the source-local element id of each dependency mapped to that dependency's key.
    pub local_maps: HashMap<String, BTreeMap<u64, String>>,
    /// Reasons the closure could not be completed (unpublished or missing dependencies).
    pub problems: Vec<String>,
    /// Optional dependencies that were skipped.
    pub notes: Vec<String>,
}

pub(crate) fn key_closure(project: &ShortcutProject, cache: &mut DocCache, root_key: &str, table: &DependencyTable) -> KeyClosure {
    let mut c = KeyClosure { order: Vec::new(), local_maps: HashMap::new(), problems: Vec::new(), notes: Vec::new() };
    let mut seen = HashSet::new();
    visit_key(project, cache, root_key, table, &mut seen, &mut c);
    c
}

fn visit_key(project: &ShortcutProject, cache: &mut DocCache, key: &str, table: &DependencyTable, seen: &mut HashSet<String>, c: &mut KeyClosure) {
    if !seen.insert(key.to_string()) {
        return;
    }
    let Some(sc) = project.get(key) else {
        c.problems.push(format!("there is no shortcut '{key}'"));
        return;
    };
    let doc = match cache.get(&sc.source_path) {
        Ok(d) => d,
        Err(e) => {
            c.problems.push(format!("'{}': source document {} cannot be read: {e}", sc.name, sc.source_path));
            return;
        }
    };
    let Some(el) = doc.find(ElementId(sc.source_element_id)) else {
        c.problems.push(format!("'{}': element {} is not in {}", sc.name, sc.source_element_id, sc.source_path));
        return;
    };
    let mut map: BTreeMap<u64, String> = BTreeMap::new();
    let mut deps: Vec<String> = Vec::new();
    for rule in table.rules_for(&el.category) {
        for v in values_at(&el.data, &rule.path) {
            let label = v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string());
            let Some(dep_id) = find_target(doc, rule, &v) else {
                let m = format!("'{}': {} '{}' is not in {}", sc.name, rule.path, label, sc.source_path);
                if rule.required {
                    c.problems.push(m);
                } else {
                    c.notes.push(m);
                }
                continue;
            };
            let dep_el = doc.find(dep_id).expect("found a moment ago");
            let dep_key = match reference_info(dep_el) {
                Some(info) => Some(info.shortcut_key),
                None => project.find_by_source(&sc.source_path, dep_id.0).map(|s| s.key.clone()),
            };
            match dep_key {
                Some(k) => {
                    map.insert(dep_id.0, k.clone());
                    deps.push(k);
                }
                None => {
                    let m = format!("'{}' needs '{}' ({}), which is not published in the shortcut project", sc.name, dep_el.name, dep_el.category);
                    if rule.required {
                        c.problems.push(m);
                    } else {
                        c.notes.push(m);
                    }
                }
            }
        }
    }
    c.local_maps.insert(key.to_string(), map);
    for k in deps {
        visit_key(project, cache, &k, table, seen, c);
    }
    c.order.push(key.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn paths_walk_objects_arrays_and_array_leaves() {
        let v = json!({"model": {"baselines": [{"alignment": "A", "regions": [{"assembly": {"name": "X"}}, {"assembly": {"name": "Y"}}]}, {"alignment": "B", "regions": []}]}, "profiles": [3, 4], "ground": 9, "none": null});
        assert_eq!(values_at(&v, "model.baselines[].alignment"), vec![json!("A"), json!("B")]);
        assert_eq!(values_at(&v, "model.baselines[].regions[].assembly.name"), vec![json!("X"), json!("Y")]);
        assert_eq!(values_at(&v, "profiles[]"), vec![json!(3), json!(4)]);
        assert_eq!(values_at(&v, "profiles"), vec![json!(3), json!(4)]);
        assert_eq!(values_at(&v, "none"), Vec::<Value>::new());
        assert_eq!(values_at(&v, "missing.field"), Vec::<Value>::new());
    }

    #[test]
    fn remap_rewrites_only_id_fields_and_leaves_unknown_ids() {
        let table = DependencyTable::civil();
        let mut d = json!({"alignment": "A", "profiles": [3, 4, 5]});
        let map: BTreeMap<u64, u64> = [(3, 30), (4, 40)].into();
        remap_ids(&mut d, "civil-profile-view", &table, &map);
        assert_eq!(d, json!({"alignment": "A", "profiles": [30, 40, 5]}));
    }

    #[test]
    fn the_table_is_data() {
        let mut t = DependencyTable::empty();
        t.add(DependencyRule::new("drain", "outfall", TargetBy::Name, &["culvert"], false));
        let back = DependencyTable::from_json(&t.to_json()).unwrap();
        assert_eq!(back, t);
        assert_eq!(DependencyTable::civil().rules_for("civil-corridor").count(), 3);
    }
}
