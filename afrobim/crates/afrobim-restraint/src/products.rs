//! Optional: a user-supplied list of barrier products.
//!
//! **No product data is bundled with this crate.** Lists of systems tested to EN 1317 exist
//! (for example Highways England's *List of EN1317 Compliant Road Restraint Systems*, March
//! 2016) but they are dated, belong to their publishers and change; the declared performance
//! of a product is whatever its current declaration says. A user who keeps such a list as a CSV
//! file can load it here to fill in a run's classes and post spacing instead of typing them.
//!
//! The file has this header line and one product per line:
//!
//! ```text
//! name,containment,working_width_class,impact_severity,post_spacing_m
//! Example W-beam,N2,W4,A,3.2
//! "Example concrete, in situ",H2,W2,B,
//! ```
//!
//! Classes are written as BS EN 1317-2:2010 prints them (`N2`, `H4a`, `W4`, `A`). The post
//! spacing is in metres and is left empty for a system without posts. Fields may be quoted
//! with `"`, doubled inside a quoted field.

use serde::{Deserialize, Serialize};

use crate::{BarrierRun, ContainmentLevel, ImpactSeverityLevel, WorkingWidthClass};

pub const PRODUCTS_CSV_HEADER: &str = "name,containment,working_width_class,impact_severity,post_spacing_m";

/// One line of a user's product list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Product {
    pub name: String,
    pub containment: ContainmentLevel,
    pub working_width: WorkingWidthClass,
    pub impact_severity: ImpactSeverityLevel,
    pub post_spacing_m: Option<f64>,
}

impl Product {
    /// Writes the product's name, classes and post spacing into a run. Everything else is left
    /// as it was.
    pub fn apply_to(&self, run: &mut BarrierRun) {
        run.system = self.name.clone();
        run.containment = self.containment;
        run.working_width = self.working_width;
        run.impact_severity = self.impact_severity;
        run.post_spacing_m = self.post_spacing_m;
    }
}

/// A line of a product list that could not be read.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("product list line {line}: {message}")]
pub struct ProductCsvError {
    /// 1 for the header line.
    pub line: usize,
    pub message: String,
}

fn split(line: &str) -> Result<Vec<String>, String> {
    let mut fields = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        let field = fields.last_mut().expect("never empty");
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(String::new()),
            c => field.push(c),
        }
    }
    if quoted {
        return Err("a quoted field is not closed".into());
    }
    Ok(fields.into_iter().map(|f| f.trim().to_string()).collect())
}

fn class<T: serde::de::DeserializeOwned>(what: &str, text: &str) -> Result<T, String> {
    serde_json::from_value(serde_json::Value::String(text.to_string())).map_err(|_| format!("'{text}' is not a {what}"))
}

/// Reads a product list. Blank lines are skipped; the first error stops the reading.
pub fn parse_products_csv(text: &str) -> Result<Vec<Product>, ProductCsvError> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let err = |message: String| ProductCsvError { line: i + 1, message };
        if i == 0 {
            if line.trim().trim_start_matches('\u{feff}') != PRODUCTS_CSV_HEADER {
                return Err(err(format!("the first line must be '{PRODUCTS_CSV_HEADER}'")));
            }
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let f = split(line).map_err(err)?;
        if f.len() != 5 {
            return Err(err(format!("expected 5 fields, found {}", f.len())));
        }
        if f[0].is_empty() {
            return Err(err("the product has no name".into()));
        }
        let post_spacing_m = if f[4].is_empty() {
            None
        } else {
            match f[4].parse::<f64>() {
                Ok(v) if v.is_finite() && v > 0.0 => Some(v),
                _ => return Err(err(format!("'{}' is not a post spacing in metres", f[4]))),
            }
        };
        out.push(Product {
            name: f[0].clone(),
            containment: class("containment level", &f[1]).map_err(err)?,
            working_width: class("working width class", &f[2]).map_err(err)?,
            impact_severity: class("impact severity level", &f[3]).map_err(err)?,
            post_spacing_m,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::sample_run;

    #[test]
    fn reads_a_user_list_and_applies_a_product() {
        let text = format!("{PRODUCTS_CSV_HEADER}\nExample W-beam,N2,W4,A,3.2\n\n\"Example concrete, \"\"in situ\"\"\",H2,W2,B,\n");
        let products = parse_products_csv(&text).unwrap();
        assert_eq!(products.len(), 2);
        assert_eq!(
            products[0],
            Product {
                name: "Example W-beam".into(),
                containment: ContainmentLevel::N2,
                working_width: WorkingWidthClass::W4,
                impact_severity: ImpactSeverityLevel::A,
                post_spacing_m: Some(3.2)
            }
        );
        assert_eq!(products[1].name, "Example concrete, \"in situ\"");
        assert_eq!(products[1].post_spacing_m, None);
        let mut run = sample_run();
        products[1].apply_to(&mut run);
        assert_eq!((run.containment, run.working_width, run.impact_severity, run.post_spacing_m), (ContainmentLevel::H2, WorkingWidthClass::W2, ImpactSeverityLevel::B, None));
        assert_eq!(run.system, "Example concrete, \"in situ\"");
        assert_eq!(run.name, "VRS-001");
    }

    #[test]
    fn bad_lines_are_reported_with_their_number() {
        let read = |body: &str| parse_products_csv(&format!("{PRODUCTS_CSV_HEADER}\n{body}")).unwrap_err();
        assert_eq!(parse_products_csv("name,containment\n").unwrap_err().line, 1);
        assert_eq!(read("A,N2,W4,A,3.2\nB,N9,W4,A,").to_string(), "product list line 3: 'N9' is not a containment level");
        assert!(read("A,N2,W9,A,").message.contains("working width class"));
        assert!(read("A,N2,W4,D,").message.contains("impact severity level"));
        assert!(read("A,N2,W4,A,-1").message.contains("post spacing"));
        assert!(read("A,N2,W4,A").message.contains("expected 5 fields, found 4"));
        assert!(read(",N2,W4,A,").message.contains("no name"));
        assert!(read("\"A,N2,W4,A,").message.contains("not closed"));
        assert_eq!(parse_products_csv(PRODUCTS_CSV_HEADER).unwrap(), []);
    }
}
