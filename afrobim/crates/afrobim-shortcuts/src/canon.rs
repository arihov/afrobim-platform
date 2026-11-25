// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Canonical JSON and content hashes.
//!
//! A hash must not change because a file was saved again: key order, whitespace and the way a
//! number is written (`1`, `1.0`, `1e0`) are not content. [`canonical_json`] sorts object keys,
//! writes no whitespace and writes every number in one form: an integer when the value is a whole
//! number below 2^53, otherwise the shortest decimal that reads back to the same f64.

use afrobim_kernel::Element;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub fn canonical_json(v: &Value) -> String {
    let mut out = String::new();
    write(v, &mut out);
    out
}

fn write(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                out.push_str(&i.to_string());
            } else if let Some(u) = n.as_u64() {
                out.push_str(&u.to_string());
            } else {
                let f = n.as_f64().unwrap_or(0.0);
                if f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    out.push_str(&(f as i64).to_string());
                } else {
                    out.push_str(&format!("{f}"));
                }
            }
        }
        Value::String(s) => out.push_str(&serde_json::to_string(s).expect("a string serialises")),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(x, out);
            }
            out.push(']');
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).expect("a string serialises"));
                out.push(':');
                write(&m[k], out);
            }
            out.push('}');
        }
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// Hash of a JSON value in canonical form.
pub fn value_hash(v: &Value) -> String {
    sha256_hex(canonical_json(v).as_bytes())
}

/// The content hash of an element: what a reference copies. It covers the category, the name,
/// the placement (origin, size, site placement) and the module-owned data. It does not cover the
/// layer, the host id or the free-form properties (revision counters and status labels change
/// without the object changing), so saving a source again does not flag its references.
pub fn element_hash(e: &Element) -> String {
    let v = json!({
        "category": e.category,
        "name": e.name,
        "origin": [e.origin.x, e.origin.y],
        "width": e.width,
        "height": e.height,
        "site": e.site,
        "data": e.data,
    });
    value_hash(&v)
}

/// Hash of the bytes of a file (the cheap alternative to a modified time).
pub fn file_hash(path: &std::path::Path) -> std::io::Result<String> {
    Ok(sha256_hex(&std::fs::read(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_order_whitespace_and_number_form_do_not_matter() {
        let a: Value = serde_json::from_str(r#"{"b":1,"a":[1.0,2.50,{"y":1e0,"x":-0.0}]}"#).unwrap();
        let b: Value = serde_json::from_str("{ \"a\" : [1, 2.5, {\"x\": 0, \"y\": 1}],\n \"b\": 1.0 }").unwrap();
        assert_eq!(canonical_json(&a), canonical_json(&b));
        assert_eq!(canonical_json(&a), r#"{"a":[1,2.5,{"x":0,"y":1}],"b":1}"#);
        assert_ne!(value_hash(&a), value_hash(&json!({"a": 1})));
    }

    #[test]
    fn sha256_matches_the_published_test_vector() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
