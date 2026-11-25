// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Description keys: raw description codes to full description, styles, layer and classification.
//!
//! A key's `code` pattern is split at blanks. The first word is the code and may use the wildcards
//! `*` (any run of characters) and `?` (one character). Later words of the pattern are parameter
//! slots: `$1`, `$2` ... take the words that follow the code in the raw description, and `$*` takes all
//! the remaining words. So the key `EP $1` matches the raw description `EP 3.5` with `$1 = 3.5`, and
//! `TR*` matches `TREE` and `TR`. The full description may use the same `$n` and `$*` tokens.
//!
//! Matching rules (these are this engine's rules, not a standard): the first key in set order that
//! matches wins, so put the specific keys before the wildcard ones. A key whose pattern lists `$n`
//! still matches when the raw description has fewer words; the missing parameters become empty and
//! a warning is returned. Code matching ignores case unless `case_sensitive` is set.

use serde::{Deserialize, Serialize};

use crate::point::PointStore;

/// A classification slot for BIM. The scheme and code are the caller's; nothing is checked against
/// a published classification table.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Classification {
    pub scheme: String,
    pub code: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DescriptionKey {
    pub code: String,
    pub full_description: String,
    pub point_style: String,
    pub label_style: String,
    pub layer: String,
    pub classification: Option<Classification>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyMatch {
    pub key_index: usize,
    pub full_description: String,
    pub point_style: String,
    pub label_style: String,
    pub layer: String,
    pub classification: Option<Classification>,
    pub params: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DescriptionKeySet {
    pub name: String,
    pub keys: Vec<DescriptionKey>,
    pub case_sensitive: bool,
}

/// Wildcard match of `*` and `?`.
pub fn glob_match(pattern: &str, text: &str, case_sensitive: bool) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = if case_sensitive {
        (pattern.chars().collect(), text.chars().collect())
    } else {
        (pattern.to_lowercase().chars().collect(), text.to_lowercase().chars().collect())
    };
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

fn substitute(template: &str, params: &[String]) -> String {
    let mut out = String::new();
    let cs: Vec<char> = template.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '$' && i + 1 < cs.len() {
            if cs[i + 1] == '*' {
                out.push_str(&params.join(" "));
                i += 2;
                continue;
            }
            let mut j = i + 1;
            while j < cs.len() && cs[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 {
                let k: usize = cs[i + 1..j].iter().collect::<String>().parse().unwrap_or(0);
                if k >= 1 {
                    out.push_str(params.get(k - 1).map(String::as_str).unwrap_or(""));
                }
                i = j;
                continue;
            }
        }
        out.push(cs[i]);
        i += 1;
    }
    out.trim().to_string()
}

impl DescriptionKeySet {
    pub fn new(name: &str) -> DescriptionKeySet {
        DescriptionKeySet { name: name.to_string(), keys: vec![], case_sensitive: false }
    }

    /// The first key matching `raw`, with its parameters filled in.
    pub fn match_raw(&self, raw: &str) -> Option<KeyMatch> {
        let words: Vec<&str> = raw.split_whitespace().collect();
        let code = *words.first()?;
        for (i, k) in self.keys.iter().enumerate() {
            let mut pw = k.code.split_whitespace();
            let Some(cpat) = pw.next() else { continue };
            if !glob_match(cpat, code, self.case_sensitive) {
                continue;
            }
            let slots: Vec<&str> = pw.collect();
            let params: Vec<String> = words[1..].iter().map(|s| s.to_string()).collect();
            let mut warnings = Vec::new();
            let need = slots
                .iter()
                .filter_map(|s| s.strip_prefix('$').and_then(|n| n.parse::<usize>().ok()))
                .max()
                .unwrap_or(0);
            if params.len() < need {
                warnings.push(format!("key '{}' lists {} parameter(s) but the description has {}", k.code, need, params.len()));
            }
            return Some(KeyMatch {
                key_index: i,
                full_description: substitute(&k.full_description, &params),
                point_style: k.point_style.clone(),
                label_style: k.label_style.clone(),
                layer: k.layer.clone(),
                classification: k.classification.clone(),
                params,
                warnings,
            });
        }
        None
    }

    /// Fill `full_description` of every unlocked point from its raw description, and record the
    /// styles, layer and classification as attributes `dk.point_style`, `dk.label_style`, `dk.layer`,
    /// `dk.class_scheme`, `dk.class_code`. Points with no matching key keep an empty full description
    /// and are listed in `unmatched`. Locked points are listed in `locked_skipped` and not touched.
    pub fn apply(&self, store: &mut PointStore) -> ApplyReport {
        let mut rep = ApplyReport::default();
        for n in store.numbers() {
            let p = store.get(n).unwrap();
            if p.locked {
                rep.locked_skipped.push(n);
                continue;
            }
            let m = self.match_raw(&p.raw_description);
            match m {
                Some(m) => {
                    rep.warnings.extend(m.warnings.iter().map(|w| (n, w.clone())));
                    let _ = store.edit(n, |p| {
                        p.full_description = m.full_description.clone();
                        p.attributes.insert("dk.point_style".into(), m.point_style.clone());
                        p.attributes.insert("dk.label_style".into(), m.label_style.clone());
                        p.attributes.insert("dk.layer".into(), m.layer.clone());
                        match &m.classification {
                            Some(c) => {
                                p.attributes.insert("dk.class_scheme".into(), c.scheme.clone());
                                p.attributes.insert("dk.class_code".into(), c.code.clone());
                            }
                            None => {
                                p.attributes.remove("dk.class_scheme");
                                p.attributes.remove("dk.class_code");
                            }
                        }
                    });
                    rep.matched.push(n);
                }
                None => {
                    let _ = store.edit(n, |p| p.full_description.clear());
                    rep.unmatched.push(n);
                }
            }
        }
        rep
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ApplyReport {
    pub matched: Vec<u32>,
    pub unmatched: Vec<u32>,
    pub locked_skipped: Vec<u32>,
    pub warnings: Vec<(u32, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::point::CogoPoint;

    fn set() -> DescriptionKeySet {
        let mut s = DescriptionKeySet::new("t");
        s.keys.push(DescriptionKey {
            code: "EP $1".into(),
            full_description: "Edge of pavement, offset $1 m".into(),
            point_style: "EP".into(),
            label_style: "L1".into(),
            layer: "C-EP".into(),
            classification: Some(Classification { scheme: "user".into(), code: "X1".into(), title: None }),
        });
        s.keys.push(DescriptionKey { code: "TR*".into(), full_description: "Tree $*".into(), layer: "V-TREE".into(), ..Default::default() });
        s
    }

    #[test]
    fn glob() {
        assert!(glob_match("TR*", "TREE", false));
        assert!(glob_match("tr*", "TR", false));
        assert!(!glob_match("TR*", "XTR", false));
        assert!(glob_match("A?C*D", "ABCXXD", false));
        assert!(!glob_match("tr*", "TR", true));
        assert!(glob_match("*", "", false));
    }

    #[test]
    fn match_and_params() {
        let s = set();
        let m = s.match_raw("ep 3.5").unwrap();
        assert_eq!(m.full_description, "Edge of pavement, offset 3.5 m");
        assert_eq!(m.layer, "C-EP");
        assert!(m.warnings.is_empty());
        let m = s.match_raw("EP").unwrap();
        assert_eq!(m.warnings.len(), 1);
        let m = s.match_raw("TREE oak 12").unwrap();
        assert_eq!(m.full_description, "Tree oak 12");
        assert!(s.match_raw("XX").is_none());
        assert!(s.match_raw("").is_none());
    }

    #[test]
    fn apply_skips_locked() {
        let mut st = PointStore::new();
        st.add(CogoPoint::new(1, 0.0, 0.0, None, "EP 2")).unwrap();
        st.add(CogoPoint::new(2, 0.0, 0.0, None, "ZZ")).unwrap();
        st.add(CogoPoint::new(3, 0.0, 0.0, None, "EP 2")).unwrap();
        st.set_locked(3, true).unwrap();
        let r = set().apply(&mut st);
        assert_eq!((r.matched.clone(), r.unmatched.clone(), r.locked_skipped.clone()), (vec![1], vec![2], vec![3]));
        assert_eq!(st.get(1).unwrap().attributes["dk.layer"], "C-EP");
        assert_eq!(st.get(1).unwrap().attributes["dk.class_code"], "X1");
        assert_eq!(st.get(3).unwrap().full_description, "");
        assert_eq!(st.get(1).unwrap().easting, 0.0);
    }
}
