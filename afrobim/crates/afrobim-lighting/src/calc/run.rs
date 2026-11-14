//! Running a calculation for a lighting element, keeping what is needed to repeat it on the
//! element, and filling [`PhotometricResults`] from it.
//!
//! # Keys stored on the element
//!
//! [`set_photometry`] writes one JSON object under the key `photometry` of the element's `data`
//! (next to `definition` and `layout`; [`crate::refresh_layout`] keeps it):
//!
//! | key | value |
//! |---|---|
//! | `photometry.file_path` | the path of the photometric file as entered |
//! | `photometry.file_sha256` | SHA-256 of the file's bytes, lower-case hex |
//! | `photometry.settings.maintenance_factor` | the maintenance factor entered |
//! | `photometry.settings.contribution_distance_m` | luminaires further than this in plan do not contribute |
//! | `photometry.settings.orientation` | `c_toward_carriageway_deg`, `counterclockwise` |
//! | `photometry.settings.areas[]` | each area: `name`, `kind`, stations, offsets, `along`, `across` division |
//! | `photometry.settings.plane_level_m`, `.vertical`, `.observers`, `.eir_lane_width_m` | as entered, when used |
//! | `photometry.rtable_path`, `photometry.rtable_sha256` | the r-table file, when one is used |
//! | `photometry.distribution` | the parsed distribution, **only** when it has at most [`MAX_EMBEDDED_VALUES`] intensities |
//!
//! When the distribution is not embedded the file is referenced: [`resolve_photometry`] reads it
//! again, checks the hash, and warns when the file is missing or has changed. When it is
//! embedded, a missing or changed file falls back to the embedded distribution (with a warning),
//! so the run can still be repeated.

use std::path::{Path, PathBuf};

use afrobim_alignment::Alignment;
use afrobim_kernel::{Document, ElementId};
use serde::{Deserialize, Serialize};

use crate::design::PhotometricResults;
use crate::photometry::{load_photometry_file, sha256_hex, FluxCheck, LoadedPhotometry, Photometry};
use crate::{lighting_of, LightingData, LightingError, CATEGORY};

use super::engine::{calculate, AreaResult, CalcResult, CalcSettings};
use super::grid::{AreaKind, GridFrame};
use super::rtable::RTable;
use super::CalcError;

/// The key of the object on the element's `data`.
pub const PHOTOMETRY_KEY: &str = "photometry";

/// A distribution with more intensities than this is referenced by file, not embedded in the
/// document (a 5 degree by 5 degree full-sphere table has about 2 700).
pub const MAX_EMBEDDED_VALUES: usize = 4000;

/// The beginning of the `source` text of [`PhotometricResults`] filled by [`PhotometryRun::apply_to`].
/// The class check does not describe such results as calculated outside this crate.
pub const COMPUTED_SOURCE_PREFIX: &str = "Computed by AfroBIM (afrobim-lighting)";

/// What is stored on the element (see the module documentation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhotometrySettings {
    pub file_path: String,
    pub file_sha256: String,
    pub settings: CalcSettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtable_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtable_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distribution: Option<Photometry>,
}

/// Stores the photometry reference, the hash, the maintenance factor and the grid settings on a
/// lighting element. `file_path` is stored as entered. `rtable` is the r-table file's path and
/// bytes' hash, if one is used.
pub fn set_photometry(
    doc: &mut Document,
    id: ElementId,
    file_path: &str,
    loaded: &LoadedPhotometry,
    settings: &CalcSettings,
    rtable: Option<(&str, &str)>,
) -> Result<(), LightingError> {
    // Reads the element first so a wrong category is reported.
    lighting_of(doc, id)?;
    let embed = (loaded.photometry.candela.len() <= MAX_EMBEDDED_VALUES).then(|| loaded.photometry.clone());
    let value = PhotometrySettings {
        file_path: file_path.to_string(),
        file_sha256: loaded.sha256.clone(),
        settings: settings.clone(),
        rtable_path: rtable.map(|r| r.0.to_string()),
        rtable_sha256: rtable.map(|r| r.1.to_string()),
        distribution: embed,
    };
    let e = doc.find_mut(id).ok_or(LightingError::ElementNotFound(id.0))?;
    let obj = e.data.as_object_mut().ok_or_else(|| LightingError::InvalidDefinition("the element data is not an object".to_string()))?;
    obj.insert(PHOTOMETRY_KEY.to_string(), serde_json::to_value(&value).expect("photometry settings serialise"));
    Ok(())
}

/// The photometry settings stored on a lighting element, `None` when there are none.
pub fn photometry_settings_of(doc: &Document, id: ElementId) -> Result<Option<PhotometrySettings>, LightingError> {
    let e = doc.find(id).ok_or(LightingError::ElementNotFound(id.0))?;
    if e.category != CATEGORY {
        return Err(LightingError::NotLighting(id.0));
    }
    match e.data.get(PHOTOMETRY_KEY) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone()).map(Some).map_err(|e| LightingError::InvalidDefinition(format!("the stored photometry settings cannot be read: {e}"))),
    }
}

/// Removes the photometry settings from an element.
pub fn clear_photometry(doc: &mut Document, id: ElementId) -> Result<(), LightingError> {
    lighting_of(doc, id)?;
    let e = doc.find_mut(id).ok_or(LightingError::ElementNotFound(id.0))?;
    if let Some(o) = e.data.as_object_mut() {
        o.remove(PHOTOMETRY_KEY);
    }
    Ok(())
}

fn resolve_path(path: &str, base_dir: Option<&Path>) -> PathBuf {
    let p = Path::new(path);
    match base_dir {
        Some(b) if p.is_relative() => b.join(p),
        _ => p.to_path_buf(),
    }
}

/// The distribution for stored settings, with warnings. See the module documentation for the
/// rules when the file is missing or changed.
pub fn resolve_photometry(stored: &PhotometrySettings, base_dir: Option<&Path>) -> Result<(Photometry, String, Vec<String>), CalcError> {
    let path = resolve_path(&stored.file_path, base_dir);
    let mut warnings = Vec::new();
    match load_photometry_file(&path) {
        Ok(loaded) if loaded.sha256 == stored.file_sha256 => Ok((loaded.photometry, loaded.sha256, warnings)),
        Ok(loaded) => match &stored.distribution {
            Some(d) => {
                warnings.push(format!("The photometric file '{}' has changed since the settings were stored (hash differs); the distribution stored in the document was used.", path.display()));
                Ok((d.clone(), stored.file_sha256.clone(), warnings))
            }
            None => {
                warnings.push(format!("The photometric file '{}' has changed since the settings were stored (hash differs) and the document holds no copy; the file as it is now was used.", path.display()));
                Ok((loaded.photometry, loaded.sha256, warnings))
            }
        },
        Err(e) => match &stored.distribution {
            Some(d) => {
                warnings.push(format!("The photometric file '{}' cannot be read ({e}); the distribution stored in the document was used.", path.display()));
                Ok((d.clone(), stored.file_sha256.clone(), warnings))
            }
            None => Err(CalcError::Photometry(format!("the photometric file '{}' is missing or unreadable ({e}) and the document holds no copy of the distribution", path.display()))),
        },
    }
}

/// A finished run: the numbers and what they came from.
#[derive(Debug, Clone, PartialEq)]
pub struct PhotometryRun {
    pub result: CalcResult,
    /// The file name the distribution came from.
    pub file_name: String,
    pub file_sha256: String,
    /// The flux check of the distribution (1 degree step).
    pub flux_check: FluxCheck,
    /// Warnings from reading the file, resolving it and calculating.
    pub warnings: Vec<String>,
}

/// Runs a calculation for a lighting element given the distribution and its identity.
pub fn run_photometry(
    data: &LightingData,
    phot: &Photometry,
    file_name: &str,
    file_sha256: &str,
    settings: &CalcSettings,
    rtable: Option<&RTable>,
    alignment: Option<&Alignment>,
) -> Result<PhotometryRun, CalcError> {
    let mut warnings: Vec<String> = phot.warnings.clone();
    let result = match alignment {
        Some(a) => calculate(phot, data, &GridFrame::Alignment(a), settings, rtable)?,
        None => {
            let (frame, warn) = GridFrame::from_layout(&data.layout)?;
            warnings.extend(warn);
            calculate(phot, data, &frame, settings, rtable)?
        }
    };
    warnings.extend(result.warnings.iter().cloned());
    Ok(PhotometryRun { flux_check: phot.flux_check(1.0), result, file_name: file_name.to_string(), file_sha256: file_sha256.to_string(), warnings })
}

/// Repeats the run stored on a lighting element: reads the element, its stored settings and the
/// file (or the embedded copy), and calculates. `base_dir` is where a relative file path is
/// resolved from. `alignment` is optional; without it the straight local frame of the layout is used.
pub fn run_stored_photometry(doc: &Document, id: ElementId, alignment: Option<&Alignment>, base_dir: Option<&Path>) -> Result<PhotometryRun, CalcError> {
    let data = lighting_of(doc, id).map_err(|e| CalcError::InvalidSettings(e.to_string()))?;
    let stored = photometry_settings_of(doc, id)
        .map_err(|e| CalcError::InvalidSettings(e.to_string()))?
        .ok_or_else(|| CalcError::InvalidSettings("the element has no stored photometry settings".to_string()))?;
    let (phot, sha, resolve_warnings) = resolve_photometry(&stored, base_dir)?;
    let rtable = match &stored.rtable_path {
        Some(p) => {
            let path = resolve_path(p, base_dir);
            let bytes = std::fs::read(&path).map_err(|e| CalcError::RTable(format!("{}: {e}", path.display())))?;
            let mut t = RTable::parse(&String::from_utf8_lossy(&bytes))?;
            if stored.rtable_sha256.as_deref().is_some_and(|h| h != sha256_hex(&bytes)) {
                t.name.push_str(" (file changed since the settings were stored)");
            }
            Some(t)
        }
        None => None,
    };
    let name = Path::new(&stored.file_path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| stored.file_path.clone());
    let mut run = run_photometry(&data, &phot, &name, &sha, &stored.settings, rtable.as_ref(), alignment)?;
    let mut w = resolve_warnings;
    w.append(&mut run.warnings);
    run.warnings = w;
    if let Some(t) = &rtable {
        if t.name.ends_with("(file changed since the settings were stored)") {
            run.warnings.push("The r-table file has changed since the settings were stored.".to_string());
        }
    }
    Ok(run)
}

impl PhotometryRun {
    /// The name of the area to take the class criteria from, for a class label: P classes use the
    /// first footway area, C classes the first conflict area (else the first carriageway), M classes
    /// the first carriageway; the first entered area when none matches.
    pub fn primary_area_for_class(&self, class_label: &str) -> Option<&str> {
        let entered = self.result.areas.iter().filter(|a| a.kind != AreaKind::EdgeStrip);
        let pick = |kinds: &[AreaKind]| kinds.iter().find_map(|k| entered.clone().find(|a| a.kind == *k)).map(|a| a.name.as_str());
        let by_class = match class_label.trim().chars().next().map(|c| c.to_ascii_uppercase()) {
            Some('P') => pick(&[AreaKind::Footway]),
            Some('C') => pick(&[AreaKind::Conflict, AreaKind::Carriageway]),
            Some('M') => pick(&[AreaKind::Carriageway]),
            _ => None,
        };
        by_class.or_else(|| entered.clone().next().map(|a| a.name.as_str()))
    }

    fn primary<'a>(&'a self, name: &str) -> Option<&'a AreaResult> {
        self.result.area(name)
    }

    /// Fills the fields of `ph` that were computed and leaves the others as they were; sets
    /// `ph.source`. Returns the names of the fields filled.
    ///
    /// From the area `primary_area`: `e_av_lx` (average horizontal illuminance), `u_o_e`
    /// (minimum over average), `e_h_min_lx` (minimum), and, when the vertical settings were
    /// used, `e_v_min_lx` and `e_sc_min_lx` as the minimum over all points and over all the
    /// facing directions calculated (a conservative rule chosen here, not one of a standard).
    /// From the run: `eir` when edge strips were calculated; `l_av_cd_m2` and `u_o` (minimum
    /// over average) from the first carriageway area, only when exactly one observer was used.
    /// Not filled: `u_l`, `u_o_wet`, `ti_percent` (formulae and observer rules of EN 13201-3 are
    /// not held), `facial_recognition_necessary`, `low_or_very_low_speed`.
    pub fn apply_to(&self, ph: &mut PhotometricResults, primary_area: &str) -> Result<Vec<&'static str>, CalcError> {
        let area = self.primary(primary_area).ok_or_else(|| CalcError::InvalidSettings(format!("there is no calculated area named '{primary_area}'")))?;
        let mut filled = Vec::new();
        ph.e_av_lx = Some(area.e_h.average);
        ph.u_o_e = Some(area.e_h.uniformity_min_avg);
        ph.e_h_min_lx = Some(area.e_h.minimum);
        filled.extend(["e_av_lx", "u_o_e", "e_h_min_lx"]);
        if !area.vertical.is_empty() {
            ph.e_v_min_lx = Some(area.vertical.iter().map(|d| d.ev.minimum).fold(f64::INFINITY, f64::min));
            ph.e_sc_min_lx = Some(area.vertical.iter().map(|d| d.esc.minimum).fold(f64::INFINITY, f64::min));
            filled.extend(["e_v_min_lx", "e_sc_min_lx"]);
        }
        if let Some(eir) = &self.result.eir {
            ph.eir = Some(eir.value);
            filled.push("eir");
        }
        let carriageway = self.result.areas.iter().find(|a| a.kind == AreaKind::Carriageway);
        if let Some(c) = carriageway {
            if c.luminance.len() == 1 {
                ph.l_av_cd_m2 = Some(c.luminance[0].stats.average);
                ph.u_o = Some(c.luminance[0].stats.uniformity_min_avg);
                filled.extend(["l_av_cd_m2", "u_o"]);
            }
        }
        ph.source = format!(
            "{COMPUTED_SOURCE_PREFIX} from '{}' (sha256 {}), maintenance factor {}, area '{}' ({} by {} points, cell-centre grid entered by the user), contribution distance {} m; maintained values = initial values times the maintenance factor; not validated against DIALux, Relux or AGi32",
            self.file_name,
            &self.file_sha256[..12.min(self.file_sha256.len())],
            crate::fixed(self.result.maintenance_factor, 0, 3),
            area.name,
            area.n_along,
            area.n_across,
            crate::fixed(self.result.contribution_distance_m, 0, 2),
        );
        Ok(filled)
    }
}
