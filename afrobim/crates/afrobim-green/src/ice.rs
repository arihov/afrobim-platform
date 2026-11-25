//! Import of the user's own ICE (Inventory of Carbon and Energy, Circular Ecology) export.
//!
//! The ICE terms do not allow its values to be redistributed inside software (see
//! `docs/reference/green-carbon-factors.md`), so this crate holds none of them. It holds only
//! search hints (ICE material group names, no numbers) and a reader for the CSV the user exports
//! from the ICE spreadsheet they are licensed to use.

use crate::csv;
use crate::factors::{normalise_key, FactorSet};

/// ICE material group and sheet reference to look under for a factor key. Words to search for,
/// never a number; the user confirms each name in the `material` column before importing.
pub fn ice_hint(key: &str) -> Option<(&'static str, &'static str)> {
    let k = normalise_key(key);
    let has = |w: &str| k.contains(w);
    let hint = if has("galvanis") || has("galvaniz") {
        ("Steel, galvanised", "ICE sheet: Steel")
    } else if has("post") || has("steel") || has("barrier") || has("gantry") {
        ("Steel", "ICE sheet: Steel")
    } else if has("alumin") || has("sign face") {
        ("Aluminium", "ICE sheet: Aluminium")
    } else if has("precast") || has("culvert") && has("concrete") {
        ("Precast concrete", "ICE sheet: Concrete")
    } else if has("concrete") {
        ("Concrete", "ICE sheet: Concrete")
    } else if has("cement") {
        ("Cement", "ICE sheet: Cement and mortar")
    } else if has("asphalt") || has("bitumen") || has("hot mix") {
        ("Asphalt / bitumen", "ICE sheet: Bitumen and asphalt")
    } else if has("hdpe") || has("pvc") || has("pipe") || has("culvert") {
        ("Plastic pipe (HDPE, uPVC)", "ICE sheet: Plastics")
    } else if has("gravel") || has("aggregate") || has("granular") || has("crushed") || has("base") || has("subbase") || has("sub-base") {
        ("Aggregate", "ICE sheet: Aggregates")
    } else if has("timber") || has("wood") {
        ("Timber", "ICE sheet: Timber")
    } else if has("marking") || has("paint") || has("thermoplastic") {
        ("Paint", "ICE sheet: Paint")
    } else if has("glass") {
        ("Glass", "ICE sheet: Glass")
    } else if has("sheeting") || has("retroreflective") {
        ("Plastic sheeting", "ICE sheet: Plastics")
    } else {
        return None;
    };
    Some(hint)
}

/// A factor file for `keys`, every number blank, whose `material` is the ICE name to look for and
/// whose `source` names the ICE sheet to take the figure from. Header comments name the boundary.
pub fn ice_template_csv(keys: &[&str], ice_version: &str) -> String {
    let mut set = FactorSet {
        name: "ICE factors (import)".into(),
        source: format!("ICE {ice_version} (Circular Ecology)"),
        edition: ice_version.into(),
        boundary: "A1-A3".into(),
        ..FactorSet::default()
    };
    for k in keys {
        let (name, sheet) = ice_hint(k).map(|(n, s)| (n.to_string(), s.to_string())).unwrap_or_else(|| (k.to_string(), "no ICE hint: enter the ICE name or your own source".to_string()));
        set.factors.push(crate::factors::Factor { key: k.to_string(), material: name, source: sheet, edition: ice_version.into(), ..Default::default() });
    }
    crate::factors::factors_to_csv(&set)
}

/// The outcome of [`import_ice`].
#[derive(Debug, Clone)]
pub struct IceImport {
    /// The factor set with ICE figures filled into the blank rows.
    pub set: FactorSet,
    /// `(key, ICE name, kgCO2e per tonne)` for each row filled.
    pub filled: Vec<(String, String, f64)>,
    /// `(key, reason)` for each blank row left blank.
    pub skipped: Vec<(String, String)>,
    /// The ICE column the carbon figure was read from.
    pub column: String,
}

fn norm(s: &str) -> String {
    normalise_key(&s.replace(['-', '_', '/', ','], " "))
}

/// Fills the blank rows of `base` from the user's ICE export (CSV, one figure per kg).
///
/// * The header row is the first row with a `material` column and a carbon column whose header
///   says kgCO2e per kg. Other layouts are an error, never guessed.
/// * Rows are matched by name: the factor row's `material` (else its `key`) against the ICE
///   material, or `material, sub-material`, ignoring case, spacing and hyphens. Exactly one ICE
///   row must match; none or several leave the row blank and say why.
/// * A row that already has a carbon figure (the user's, an EPD) is never changed.
/// * Figures are per kg in ICE and converted to per tonne (x 1000). Densities are not imported.
pub fn import_ice(base: &FactorSet, ice_csv: &str, ice_version: &str) -> Result<IceImport, String> {
    let text = ice_csv.strip_prefix('\u{feff}').unwrap_or(ice_csv);
    let mut table: Vec<Vec<String>> = Vec::new();
    for (i, l) in text.lines().enumerate() {
        if l.trim().is_empty() {
            continue;
        }
        table.push(csv::split(l).map_err(|e| format!("line {}: {e}", i + 1))?);
    }
    let h_idx = table
        .iter()
        .position(|r| r.iter().any(|c| norm(c) == "material") && r.iter().any(|c| carbon_header(c)))
        .ok_or("no header row with a Material column and a carbon column in kgCO2e per kg was found; export the ICE sheet as CSV with its header row")?;
    let header = &table[h_idx];
    let name_cols: Vec<usize> = header.iter().enumerate().filter(|(_, c)| matches!(norm(c).as_str(), "material" | "sub material")).map(|(i, _)| i).collect();
    let carbon_cols: Vec<usize> = header.iter().enumerate().filter(|(_, c)| carbon_header(c)).map(|(i, _)| i).collect();
    // Prefer a cradle-to-gate (A1-A3) column when there are several.
    let carbon = *carbon_cols.iter().find(|&&i| { let n = norm(&header[i]); n.contains("a1 a3") || n.contains("a1 to a3") || n.contains("cradle to gate") }).unwrap_or(&carbon_cols[0]);
    let column = header[carbon].trim().to_string();

    // (names this row answers to, kgCO2e per kg)
    let mut ice: Vec<(Vec<String>, String, f64)> = Vec::new();
    for r in &table[h_idx + 1..] {
        let Some(v) = r.get(carbon).and_then(|c| c.trim().parse::<f64>().ok()).filter(|v| v.is_finite() && *v >= 0.0) else { continue };
        let parts: Vec<&str> = name_cols.iter().filter_map(|&i| r.get(i)).map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            continue;
        }
        let mut names = vec![norm(&parts.join(" "))];
        if let Some(last) = parts.last() {
            if parts.len() > 1 {
                names.push(norm(last));
            }
        }
        ice.push((names, parts.join(", "), v));
    }

    let mut set = base.clone();
    let (mut filled, mut skipped) = (Vec::new(), Vec::new());
    for f in &mut set.factors {
        if f.has_carbon() {
            continue;
        }
        let want = norm(if f.material.trim().is_empty() { &f.key } else { &f.material });
        let hits: Vec<&(Vec<String>, String, f64)> = ice.iter().filter(|(n, _, _)| n.iter().any(|x| *x == want)).collect();
        match hits.as_slice() {
            [] => skipped.push((f.key.clone(), format!("no ICE row named '{}'", f.material.trim()))),
            [(_, name, v)] => {
                f.kgco2e_per_t = v * 1000.0;
                f.source = format!("ICE {ice_version} (Circular Ecology): {name}");
                f.edition = ice_version.to_string();
                filled.push((f.key.clone(), name.clone(), f.kgco2e_per_t));
            }
            many => skipped.push((f.key.clone(), format!("{} ICE rows are named '{}'; give the full 'Material, Sub-material' name", many.len(), f.material.trim()))),
        }
    }
    if !filled.is_empty() {
        if set.source.trim().is_empty() {
            set.source = format!("ICE {ice_version} (Circular Ecology)");
        }
        if set.edition.trim().is_empty() {
            set.edition = ice_version.to_string();
        }
        if set.boundary.trim().is_empty() {
            set.boundary = "A1-A3".to_string();
        }
    }
    Ok(IceImport { set, filled, skipped, column })
}

fn carbon_header(c: &str) -> bool {
    let n = norm(c).replace(' ', "");
    n.contains("kgco2e") && n.contains("kg") && !n.contains("tonne") && (n.contains("perkg") || n.contains("kgco2ekg") || n.contains("kgco2e") && n.ends_with("kg"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factors::Factor;

    // Invented fixture figures, not ICE data.
    const ICE: &str = "Notes\nMaterial,Sub-material,Embodied Carbon A1-A3 (kgCO2e per kg)\nAlpha,General,2.0\nAlpha,Special,3.0\nBeta,,0.5\nBeta,Dup,1\nGamma,Dup,2\n";

    fn base() -> FactorSet {
        let row = |k: &str, m: &str| Factor { key: k.into(), material: m.into(), ..Default::default() };
        FactorSet {
            name: "f".into(),
            factors: vec![row("a", "Alpha, Special"), row("b", "Beta"), row("c", "Dup"), row("d", "Nothing"), Factor { kgco2e_per_t: 7.0, source: "EPD x".into(), ..row("e", "Alpha, General") }],
            ..Default::default()
        }
    }

    #[test]
    fn fills_by_name_converts_per_tonne_and_labels() {
        let r = import_ice(&base(), ICE, "v5.0").unwrap();
        assert_eq!(r.set.find("a").unwrap().kgco2e_per_t, 3000.0);
        assert_eq!(r.set.find("b").unwrap().kgco2e_per_t, 500.0);
        assert_eq!(r.set.find("a").unwrap().edition, "v5.0");
        assert_eq!(r.set.source_label(), "Source: ICE v5.0 (Circular Ecology), A1-A3; 1 material(s) use their own source");
    }

    #[test]
    fn missing_and_ambiguous_stay_nan_and_user_values_win() {
        let r = import_ice(&base(), ICE, "v5.0").unwrap();
        assert!(r.set.find("c").unwrap().kgco2e_per_t.is_nan(), "two rows named Dup");
        assert!(r.set.find("d").unwrap().kgco2e_per_t.is_nan());
        assert_eq!(r.skipped.len(), 2);
        let e = r.set.find("e").unwrap();
        assert_eq!((e.kgco2e_per_t, e.source.as_str()), (7.0, "EPD x"), "an entered figure is never overwritten");
    }

    #[test]
    fn an_export_without_a_per_kg_carbon_column_is_an_error() {
        assert!(import_ice(&base(), "Material,Carbon (kgCO2e per tonne)\nA,1\n", "v5.0").is_err());
        assert!(import_ice(&base(), "nothing useful\n", "v5.0").is_err());
    }

    #[test]
    fn template_carries_names_and_sheets_but_no_numbers() {
        let t = ice_template_csv(&["concrete C30/37", "steel post UB", "weird thing"], "v5.0");
        let set = crate::factors::factors_from_csv(&t).unwrap();
        assert!(set.factors.iter().all(|f| f.kgco2e_per_t.is_nan()), "no numbers shipped");
        assert_eq!(set.find("concrete C30/37").unwrap().material, "Concrete");
        assert_eq!(set.factors[2].material, "weird thing");
    }

    #[test]
    fn label_without_a_source_says_so() {
        assert_eq!(FactorSet::default().source_label(), "Source: not stated");
    }
}
