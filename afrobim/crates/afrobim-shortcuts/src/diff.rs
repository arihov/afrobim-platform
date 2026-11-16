//! A field-level summary of how a reference changed on synchronise.

use afrobim_kernel::Element;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A box in model millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Bounds {
    pub fn of(e: &Element) -> Bounds {
        Bounds { min_x: e.origin.x, min_y: e.origin.y, max_x: e.origin.x + e.width, max_y: e.origin.y + e.height }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldChange {
    /// `a.b[2].c` style path inside the element's data.
    pub path: String,
    pub kind: ChangeKind,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeSummary {
    pub fields: Vec<FieldChange>,
    /// True when more than [`MAX_FIELDS`] fields differ and the list was cut.
    pub truncated: bool,
    /// Old and new name when the source was renamed.
    pub renamed: Option<(String, String)>,
    /// The origin, size or site placement changed.
    pub placement_changed: bool,
    pub bbox_before: Bounds,
    pub bbox_after: Bounds,
    /// Dependencies of the new version that have no reference in the host yet (their ids in the
    /// copied data are left as the source wrote them until they are referenced and synchronised).
    pub missing_dependencies: Vec<String>,
}

pub const MAX_FIELDS: usize = 500;
/// Arrays longer than this are reported as one change, not item by item (a TIN has thousands).
const MAX_ARRAY_ITEMS: usize = 64;

fn short(v: &Value) -> String {
    let s = match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    if s.chars().count() > 80 {
        let cut: String = s.chars().take(77).collect();
        format!("{cut}...")
    } else {
        s
    }
}

pub fn json_diff(path: &str, a: &Value, b: &Value, out: &mut Vec<FieldChange>, truncated: &mut bool) {
    if a == b {
        return;
    }
    if out.len() >= MAX_FIELDS {
        *truncated = true;
        return;
    }
    let sub = |k: &str| if path.is_empty() { k.to_string() } else { format!("{path}.{k}") };
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) => json_diff(&sub(k), p, q, out, truncated),
                    (Some(p), None) => push(out, truncated, FieldChange { path: sub(k), kind: ChangeKind::Removed, before: Some(short(p)), after: None }),
                    (None, Some(q)) => push(out, truncated, FieldChange { path: sub(k), kind: ChangeKind::Added, before: None, after: Some(short(q)) }),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() <= MAX_ARRAY_ITEMS && y.len() <= MAX_ARRAY_ITEMS => {
            for i in 0..x.len().max(y.len()) {
                let p = format!("{path}[{i}]");
                match (x.get(i), y.get(i)) {
                    (Some(p0), Some(q)) => json_diff(&p, p0, q, out, truncated),
                    (Some(p0), None) => push(out, truncated, FieldChange { path: p, kind: ChangeKind::Removed, before: Some(short(p0)), after: None }),
                    (None, Some(q)) => push(out, truncated, FieldChange { path: p, kind: ChangeKind::Added, before: None, after: Some(short(q)) }),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            let differing = x.iter().zip(y.iter()).filter(|(p, q)| p != q).count() + x.len().abs_diff(y.len());
            push(
                out,
                truncated,
                FieldChange { path: path.to_string(), kind: ChangeKind::Changed, before: Some(format!("list of {} items", x.len())), after: Some(format!("list of {} items, {differing} differ", y.len())) },
            );
        }
        _ => push(out, truncated, FieldChange { path: path.to_string(), kind: ChangeKind::Changed, before: Some(short(a)), after: Some(short(b)) }),
    }
}

fn push(out: &mut Vec<FieldChange>, truncated: &mut bool, c: FieldChange) {
    if out.len() >= MAX_FIELDS {
        *truncated = true;
    } else {
        out.push(c);
    }
}

/// What differs between two versions of the same element.
pub fn summarise(old: &Element, new: &Element, missing_dependencies: Vec<String>) -> ChangeSummary {
    let mut fields = Vec::new();
    let mut truncated = false;
    json_diff("", &old.data, &new.data, &mut fields, &mut truncated);
    ChangeSummary {
        fields,
        truncated,
        renamed: (old.name != new.name).then(|| (old.name.clone(), new.name.clone())),
        placement_changed: old.origin != new.origin || old.width != new.width || old.height != new.height || old.site != new.site,
        bbox_before: Bounds::of(old),
        bbox_after: Bounds::of(new),
        missing_dependencies,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reports_added_removed_changed_keys_and_array_items() {
        let a = json!({"w": 3.5, "gone": 1, "list": [1, 2, 3], "n": {"x": "a"}});
        let b = json!({"w": 3.6, "new": true, "list": [1, 9], "n": {"x": "a"}});
        let (mut out, mut t) = (vec![], false);
        json_diff("", &a, &b, &mut out, &mut t);
        let find = |p: &str| out.iter().find(|c| c.path == p).unwrap_or_else(|| panic!("{p} in {out:?}"));
        assert_eq!(find("w").kind, ChangeKind::Changed);
        assert_eq!(find("gone").kind, ChangeKind::Removed);
        assert_eq!(find("new").kind, ChangeKind::Added);
        assert_eq!(find("list[1]").kind, ChangeKind::Changed);
        assert_eq!(find("list[2]").kind, ChangeKind::Removed);
        assert_eq!(out.len(), 5);
        assert!(!t);
    }

    #[test]
    fn long_arrays_are_one_line() {
        let a = json!({"pts": (0..1000).collect::<Vec<_>>()});
        let mut v: Vec<i32> = (0..1000).collect();
        v[10] = -1;
        v.push(5);
        let b = json!({ "pts": v });
        let (mut out, mut t) = (vec![], false);
        json_diff("", &a, &b, &mut out, &mut t);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].after.as_deref(), Some("list of 1001 items, 2 differ"));
    }
}
