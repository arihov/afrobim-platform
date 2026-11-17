//! Display helpers: plain geometry and label text for a point, no rendering.

use serde::{Deserialize, Serialize};

use crate::deskey::KeyMatch;
use crate::geom::Xy;
use crate::point::CogoPoint;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointDisplay {
    pub at: Xy,
    pub point_style: String,
    pub label_style: String,
    pub layer: String,
    /// Label lines: number (and name), elevation, description.
    pub label_lines: Vec<String>,
}

/// What to draw for a point. Styles and layer come from the description key match, if any.
/// Elevation is shown with `decimals` places.
pub fn point_display(p: &CogoPoint, key: Option<&KeyMatch>, decimals: usize) -> PointDisplay {
    let mut lines = vec![match &p.name {
        Some(n) => format!("{} {}", p.number, n),
        None => p.number.to_string(),
    }];
    if let Some(z) = p.elevation {
        lines.push(format!("{:.*}", decimals, z));
    }
    let d = if p.full_description.is_empty() { &p.raw_description } else { &p.full_description };
    if !d.is_empty() {
        lines.push(d.clone());
    }
    PointDisplay {
        at: p.xy(),
        point_style: key.map(|k| k.point_style.clone()).unwrap_or_default(),
        label_style: key.map(|k| k.label_style.clone()).unwrap_or_default(),
        layer: key.map(|k| k.layer.clone()).unwrap_or_default(),
        label_lines: lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn label_lines() {
        let mut p = CogoPoint::new(5, 1.0, 2.0, Some(3.456), "EP");
        p.name = Some("A".into());
        let d = point_display(&p, None, 2);
        assert_eq!(d.label_lines, vec!["5 A", "3.46", "EP"]);
        assert_eq!(d.at, Xy::new(1.0, 2.0));
    }
}
