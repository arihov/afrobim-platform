// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! A reader for the STEP text the exporter writes, just enough for the tests to follow
//! references and placements without an IFC toolkit.

use std::collections::BTreeMap;

pub struct StepFile {
    /// Instance id → (entity name, arguments).
    pub instances: BTreeMap<u32, (String, Vec<String>)>,
}

/// Splits an argument list at its top-level commas; strings and nested lists stay whole.
pub fn split_args(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let (mut depth, mut in_string) = (0i32, false);
    for c in args.chars() {
        match c {
            '\'' => in_string = !in_string, // a doubled apostrophe toggles twice
            '(' if !in_string => depth += 1,
            ')' if !in_string => depth -= 1,
            ',' if !in_string && depth == 0 => {
                out.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    out.push(current);
    out
}

/// `#12` → 12.
pub fn reference(arg: &str) -> u32 {
    arg.strip_prefix('#').and_then(|d| d.parse().ok()).unwrap_or_else(|| panic!("not a reference: {arg}"))
}

/// `(#1,#2)` → [1, 2].
pub fn references(arg: &str) -> Vec<u32> {
    split_args(arg.strip_prefix('(').and_then(|a| a.strip_suffix(')')).unwrap_or_else(|| panic!("not a list: {arg}"))).iter().map(|a| reference(a)).collect()
}

/// `(1.,2.5)` → [1.0, 2.5].
pub fn numbers(arg: &str) -> Vec<f64> {
    split_args(arg.strip_prefix('(').and_then(|a| a.strip_suffix(')')).unwrap_or_else(|| panic!("not a list: {arg}"))).iter().map(|a| number(a)).collect()
}

pub fn number(arg: &str) -> f64 {
    arg.parse().unwrap_or_else(|_| panic!("not a number: {arg}"))
}

impl StepFile {
    pub fn parse(text: &str) -> StepFile {
        let data = text.split("DATA;\n").nth(1).expect("a DATA section").split("ENDSEC;").next().unwrap();
        let instances = data
            .lines()
            .map(|line| {
                let (id, rest) = line.strip_prefix('#').and_then(|l| l.split_once('=')).unwrap_or_else(|| panic!("not an instance: {line}"));
                let open = rest.find('(').unwrap();
                assert!(rest.ends_with(");"), "{line}");
                (id.parse().unwrap(), (rest[..open].to_string(), split_args(&rest[open + 1..rest.len() - 2])))
            })
            .collect();
        StepFile { instances }
    }

    pub fn name(&self, id: u32) -> &str {
        &self.instances[&id].0
    }

    pub fn args(&self, id: u32) -> &[String] {
        &self.instances[&id].1
    }

    pub fn of_type(&self, name: &str) -> Vec<u32> {
        self.instances.iter().filter(|(_, (n, _))| n == name).map(|(id, _)| *id).collect()
    }

    pub fn count(&self, name: &str) -> usize {
        self.of_type(name).len()
    }

    /// The instance whose first argument is this GlobalId.
    pub fn by_guid(&self, guid: &str) -> u32 {
        let quoted = format!("'{guid}'");
        let found: Vec<u32> = self.instances.iter().filter(|(_, (_, args))| args.first() == Some(&quoted)).map(|(id, _)| *id).collect();
        assert_eq!(found.len(), 1, "GlobalId {guid} found {} times", found.len());
        found[0]
    }

    /// The world position (mm) and turn about +Z (degrees) of an `IfcLocalPlacement`, following
    /// `PlacementRelTo` to the top. The exporter only ever turns placements about +Z.
    pub fn world(&self, placement: u32) -> [f64; 4] {
        assert_eq!(self.name(placement), "IFCLOCALPLACEMENT");
        let args = self.args(placement);
        let parent = if args[0] == "$" { [0.0; 4] } else { self.world(reference(&args[0])) };
        let axes = self.args(reference(&args[1]));
        let p = numbers(&self.args(reference(&axes[0]))[0]);
        let turn = if axes[2] == "$" {
            assert_eq!(axes[1], "$");
            0.0
        } else {
            assert_eq!(numbers(&self.args(reference(&axes[1]))[0]), [0.0, 0.0, 1.0], "the axis is not +Z");
            let x = numbers(&self.args(reference(&axes[2]))[0]);
            assert_eq!(x[2], 0.0);
            x[1].atan2(x[0]).to_degrees()
        };
        let (sin, cos) = parent[3].to_radians().sin_cos();
        [parent[0] + p[0] * cos - p[1] * sin, parent[1] + p[0] * sin + p[1] * cos, parent[2] + p[2], parent[3] + turn]
    }

    /// Property name → encoded value, for the property set `set` defined on `product`.
    pub fn pset(&self, product: u32, set: &str) -> Option<BTreeMap<String, String>> {
        let quoted = format!("'{set}'");
        for rel in self.of_type("IFCRELDEFINESBYPROPERTIES") {
            let args = self.args(rel);
            let definition = reference(&args[5]);
            if !references(&args[4]).contains(&product) || self.args(definition)[2] != quoted {
                continue;
            }
            let members = references(self.args(definition).last().unwrap());
            return Some(
                members
                    .iter()
                    .map(|m| {
                        let a = self.args(*m);
                        let value = if self.name(*m) == "IFCPROPERTYSINGLEVALUE" { a[2].clone() } else { a[3].clone() };
                        (a[0].trim_matches('\'').to_string(), value)
                    })
                    .collect(),
            );
        }
        None
    }
}
