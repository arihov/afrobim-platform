//! The sign tools ported from the C# `SignCoreTools`: `list_packs`, `search_catalog`,
//! `layout_sign`, `export_dxf` and `validate_schedule`. Each public method is one tool; the
//! registry (`registry.rs`) binds JSON arguments to them and tests call them directly. Results
//! serialise to the same snake_case JSON the C# server returns.

use std::collections::{BTreeMap, HashSet};

use afrobim_draw::Primitive;
use afrobim_signs::{Diagnostic, Severity, SignDefinition, SignKind, SignLayout};
use serde::Serialize;

use crate::engine::{self, round1, Engine};
use crate::settings::Settings;
use crate::{paths, schedule, svg};

/// A tool failure the client is shown as `isError` text (C# `McpException`).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolError(pub String);

impl ToolError {
    pub fn new(message: impl Into<String>) -> ToolError {
        ToolError(message.into())
    }
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<schedule::CsvError> for ToolError {
    fn from(e: schedule::CsvError) -> ToolError {
        ToolError(e.0)
    }
}

// ─── result shapes ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PackInfo {
    pub id: String,
    pub name: String,
    pub source: String,
    pub path: String,
    pub format: String,
    pub catalog_entries: usize,
    pub is_default: bool,
    pub fonts_found: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ListPacksResult {
    pub default_pack: String,
    /// Null when no fonts folder was found.
    pub fonts_dir: Option<String>,
    pub packs: Vec<PackInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CatalogEntryInfo {
    pub key: String,
    pub family: String,
    pub family_label: String,
    pub series: String,
    pub scheme: String,
    pub description: String,
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchCatalogResult {
    pub pack: String,
    pub count: usize,
    pub entries: Vec<CatalogEntryInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiagnosticInfo {
    /// `Error`, `Warning` or `Info`.
    pub severity: &'static str,
    pub code: String,
    pub message: String,
    /// The clause of the standard the finding cites; null when it cites none.
    pub clause: Option<String>,
}

impl DiagnosticInfo {
    pub fn from(d: &Diagnostic) -> DiagnosticInfo {
        DiagnosticInfo {
            severity: match d.severity {
                Severity::Error => "Error",
                Severity::Warning => "Warning",
                Severity::Info => "Info",
            },
            code: d.code.clone(),
            message: d.message.clone(),
            clause: (!d.source.trim().is_empty()).then(|| d.source.clone()),
        }
    }

    pub fn error(code: &str, message: String) -> DiagnosticInfo {
        DiagnosticInfo {
            severity: "Error",
            code: code.to_string(),
            message,
            clause: None,
        }
    }

    pub fn all(diagnostics: &[Diagnostic]) -> Vec<DiagnosticInfo> {
        diagnostics.iter().map(DiagnosticInfo::from).collect()
    }

    pub fn of(diagnostics: &[Diagnostic], severity: Severity) -> Vec<DiagnosticInfo> {
        diagnostics
            .iter()
            .filter(|d| d.severity == severity)
            .map(DiagnosticInfo::from)
            .collect()
    }
}

pub fn count(diagnostics: &[Diagnostic], severity: Severity) -> usize {
    diagnostics
        .iter()
        .filter(|d| d.severity == severity)
        .count()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LayoutSignResult {
    pub pack: String,
    pub code: String,
    pub kind: String,
    pub catalog_key: Option<String>,
    pub width_mm: f64,
    pub height_mm: f64,
    pub sw_mm: f64,
    pub x_height_mm: f64,
    pub primitive_counts: BTreeMap<String, usize>,
    pub has_errors: bool,
    pub diagnostics: Vec<DiagnosticInfo>,
    pub svg: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExportedFile {
    pub code: String,
    pub path: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub sw_mm: f64,
    pub diagnostics: Vec<DiagnosticInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExportFailure {
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExportDxfResult {
    pub pack: String,
    pub out_dir: String,
    pub files: Vec<ExportedFile>,
    pub failures: Vec<ExportFailure>,
    pub error_count: usize,
    pub warning_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RowValidation {
    pub row: usize,
    pub code: Option<String>,
    /// `ok`, `warnings` or `errors`.
    pub status: &'static str,
    pub errors: Vec<DiagnosticInfo>,
    pub warnings: Vec<DiagnosticInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ValidateScheduleResult {
    pub pack: String,
    pub csv_path: String,
    pub valid: bool,
    pub rows: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub results: Vec<RowValidation>,
}

pub fn status_of(errors: usize, warnings: usize) -> &'static str {
    if errors > 0 {
        "errors"
    } else if warnings > 0 {
        "warnings"
    } else {
        "ok"
    }
}

// ─── vocabulary shared with the C# engine ────────────────────────────────────

/// The canonical family token of a layout kind (C# `SignFamily.Token`).
pub fn family_token(kind: SignKind) -> &'static str {
    match kind {
        SignKind::StackType => "stack",
        SignKind::MapTypeCrossroads => "map-crossroads",
        SignKind::MapTypeRoundabout => "map-roundabout",
        SignKind::MapTypeTee => "map-tee",
        SignKind::DirectionSign => "direction",
        SignKind::Confirmatory => "confirmatory",
        SignKind::PlaceIdentification => "place",
        SignKind::LanePreselection => "lane",
        SignKind::Diversion => "diversion",
        SignKind::InformationSign => "information",
        _ => "stack",
    }
}

/// A short, human label for a catalog family token (C# `SignFamily.Label`): the fallback (the
/// series) when the pack names no family, and "Stack type" for a family it does not know.
pub fn family_label(family: &str, fallback: &str) -> String {
    if family.trim().is_empty() {
        return fallback.to_string();
    }
    match family.trim().to_lowercase().as_str() {
        "map-roundabout" | "roundabout" => "Map · roundabout",
        "map-tee" | "tee" | "t-junction" => "Map · T-intersection",
        "map" | "map-crossroads" | "crossroads" => "Map · crossroads",
        "direction" | "direction-sign" => "Direction",
        "confirmatory" => "Confirmatory",
        "place" | "place-id" | "place-identification" => "Place identification",
        "lane" | "lane-preselection" | "lane-distributor" => "Lane pre-selection",
        "information" | "info" | "service" | "tourist" => "Information",
        "diversion" => "Diversion (temporary)",
        "warning" => "Warning",
        "regulatory-prohibitory" | "prohibitory" => "Regulatory · prohibitory",
        "regulatory-mandatory" | "mandatory" => "Regulatory · mandatory",
        "stop" => "Stop",
        "give-way" | "giveway" => "Give way",
        "supplementary" => "Supplementary",
        _ => "Stack type",
    }
    .to_string()
}

/// Primitive counts under the C# primitive type names, so both servers report the same keys.
pub fn primitive_counts(primitives: &[Primitive]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for p in primitives {
        let name = match p {
            Primitive::Fill { .. } => "PolyFill",
            Primitive::Line { .. } => "Line",
            Primitive::Arc { .. } => "Arc",
            Primitive::Glyphs { .. } => "TextOutline",
            Primitive::Symbol { .. } => "SymbolRef",
        };
        *counts.entry(name.to_string()).or_insert(0) += 1;
    }
    counts
}

/// A string argument, with blank meaning absent (C# `IsNullOrWhiteSpace`).
pub fn blank_str(s: Option<&str>) -> Option<&str> {
    s.filter(|s| !s.trim().is_empty())
}

// ─── the tools ───────────────────────────────────────────────────────────────

pub struct Tools {
    pub engine: Engine,
}

impl Tools {
    pub fn new(settings: Settings) -> Tools {
        Tools {
            engine: Engine::new(settings),
        }
    }

    pub fn list_packs(&mut self) -> Result<ListPacksResult, ToolError> {
        let default_pack = self.engine.default_pack_id()?;
        let root = self.engine.settings.root.clone();
        let mut packs = Vec::new();
        for loc in self.engine.discover() {
            let path = loc
                .path
                .strip_prefix(&root)
                .unwrap_or(&loc.path)
                .to_string_lossy()
                .replace('\\', "/");
            let is_default = loc.id.eq_ignore_ascii_case(&default_pack);
            packs.push(match self.engine.load_from(&loc) {
                Ok(p) => PackInfo {
                    id: loc.id.clone(),
                    name: p.pack.name.clone(),
                    source: p.pack.source.clone(),
                    path,
                    format: loc.format.to_string(),
                    catalog_entries: p.catalog.len(),
                    is_default,
                    fonts_found: self.engine.fonts_present(&p.pack),
                    error: None,
                },
                Err(e) => PackInfo {
                    id: loc.id.clone(),
                    name: String::new(),
                    source: String::new(),
                    path,
                    format: loc.format.to_string(),
                    catalog_entries: 0,
                    is_default,
                    fonts_found: false,
                    error: Some(e.0),
                },
            });
        }
        let fonts_dir = self
            .engine
            .settings
            .fonts_dir
            .as_ref()
            .ok()
            .map(|p| p.to_string_lossy().into_owned());
        Ok(ListPacksResult {
            default_pack,
            fonts_dir,
            packs,
        })
    }

    pub fn search_catalog(
        &mut self,
        pack: Option<&str>,
        query: Option<&str>,
        series: Option<&str>,
    ) -> Result<SearchCatalogResult, ToolError> {
        let loaded = self.engine.load_pack(pack)?;
        let query = blank_str(query).map(|q| q.trim().to_lowercase());
        let series = blank_str(series).map(str::trim);
        let entries: Vec<CatalogEntryInfo> = loaded
            .catalog
            .iter()
            .filter(|e| {
                query.as_ref().is_none_or(|q| {
                    [&e.key, &e.name, &e.family]
                        .iter()
                        .any(|field| field.to_lowercase().contains(q.as_str()))
                })
            })
            .filter(|e| series.is_none_or(|s| e.series.eq_ignore_ascii_case(s)))
            .map(|e| CatalogEntryInfo {
                key: e.key.clone(),
                family: e.family.clone(),
                family_label: family_label(&e.family, &e.series),
                series: e.series.clone(),
                scheme: e.scheme.clone(),
                description: e.name.clone(),
                symbol: e.symbol_id.clone(),
            })
            .collect();
        Ok(SearchCatalogResult {
            pack: loaded.pack.id.clone(),
            count: entries.len(),
            entries,
        })
    }

    pub fn layout_sign(
        &mut self,
        pack: Option<&str>,
        key: Option<&str>,
        schedule_row: Option<&str>,
        speed_kph: Option<i64>,
    ) -> Result<LayoutSignResult, ToolError> {
        let (key, row) = (blank_str(key), blank_str(schedule_row));
        if key.is_some() == row.is_some() {
            return Err(ToolError::new(
                "give exactly one of 'key' or 'schedule_row'",
            ));
        }
        let speed = check_speed(speed_kph)?;
        let signs = self.engine.sign_engine(pack)?;
        let sign = match (key, row) {
            (Some(key), _) => engine::from_catalog(&signs, key, speed)?,
            (_, Some(row)) => schedule::parse_schedule_row(row)?,
            _ => unreachable!("exactly one of key and row is set"),
        };
        let layout = engine::run(&signs, &sign)?;
        let (w, h) = engine::size_mm(&layout);
        Ok(LayoutSignResult {
            pack: signs.pack_id().to_string(),
            code: sign.code.clone(),
            kind: family_token(sign.kind).to_string(),
            catalog_key: sign.catalog_key.clone(),
            width_mm: round1(w),
            height_mm: round1(h),
            sw_mm: layout.sw_mm,
            x_height_mm: layout.x_height_mm,
            primitive_counts: primitive_counts(&layout.drawing.primitives),
            has_errors: layout.has_errors(),
            diagnostics: DiagnosticInfo::all(&layout.diagnostics),
            svg: svg::render(&layout.drawing, layout.sw_mm),
        })
    }

    pub fn export_dxf(
        &mut self,
        out_dir: &str,
        pack: Option<&str>,
        signs: Option<&[String]>,
        csv_path: Option<&str>,
    ) -> Result<ExportDxfResult, ToolError> {
        let (keys, csv_path) = (signs.filter(|s| !s.is_empty()), blank_str(csv_path));
        if keys.is_some() == csv_path.is_some() {
            return Err(ToolError::new("give exactly one of 'signs' or 'csv_path'"));
        }
        let root = self.engine.settings.root.clone();
        let csv = csv_path.map(|p| paths::resolve_csv(p, &root)).transpose()?;
        let engine = self.engine.sign_engine(pack)?;

        let mut failures = Vec::new();
        let mut defs: Vec<(String, SignDefinition)> = Vec::new();
        if let Some(csv) = &csv {
            for r in schedule::read_schedule(csv)? {
                match r.sign {
                    Ok(sign) => defs.push((format!("row {}", r.row), sign)),
                    Err(message) => failures.push(ExportFailure {
                        source: format!("row {}", r.row),
                        message,
                    }),
                }
            }
        }
        for key in keys.unwrap_or_default() {
            if key.trim().is_empty() {
                failures.push(ExportFailure {
                    source: "signs".into(),
                    message: "empty catalog key".into(),
                });
                continue;
            }
            match engine::from_catalog(&engine, key, None) {
                Ok(sign) => defs.push((key.clone(), sign)),
                Err(e) => failures.push(ExportFailure {
                    source: key.clone(),
                    message: e.0,
                }),
            }
        }

        let out_dir = paths::prepare_out_dir(out_dir, &root)?;
        let mut files = Vec::new();
        let mut used: HashSet<String> = HashSet::new();
        let (mut errors, mut warnings) = (0, 0);
        for (source, sign) in &defs {
            let written = (|| -> Result<(std::path::PathBuf, SignLayout), ToolError> {
                let layout = engine::run(&engine, sign)?;
                let stem = paths::safe_stem(&sign.code);
                let mut name = format!("{stem}.dxf");
                let mut n = 2;
                while !used.insert(name.to_lowercase()) {
                    name = format!("{stem}-{n}.dxf");
                    n += 1;
                }
                let path = paths::file_in_out_dir(&out_dir, &name)?;
                std::fs::write(&path, afrobim_dxf::write(&layout.drawing, layout.sw_mm)).map_err(
                    |e| ToolError::new(format!("could not write {}: {e}", path.display())),
                )?;
                Ok((path, layout))
            })();
            match written {
                Ok((path, layout)) => {
                    errors += count(&layout.diagnostics, Severity::Error);
                    warnings += count(&layout.diagnostics, Severity::Warning);
                    let (w, h) = engine::size_mm(&layout);
                    files.push(ExportedFile {
                        code: sign.code.clone(),
                        path: path.to_string_lossy().into_owned(),
                        width_mm: round1(w),
                        height_mm: round1(h),
                        sw_mm: layout.sw_mm,
                        diagnostics: DiagnosticInfo::all(&layout.diagnostics),
                    });
                }
                Err(e) => failures.push(ExportFailure {
                    source: format!("{source} ({})", sign.code),
                    message: e.0,
                }),
            }
        }
        Ok(ExportDxfResult {
            pack: engine.pack_id().to_string(),
            out_dir: out_dir.to_string_lossy().into_owned(),
            error_count: errors + failures.len(),
            warning_count: warnings,
            files,
            failures,
        })
    }

    pub fn validate_schedule(
        &mut self,
        csv_path: &str,
        pack: Option<&str>,
    ) -> Result<ValidateScheduleResult, ToolError> {
        let csv = paths::resolve_csv(csv_path, &self.engine.settings.root)?;
        let engine = self.engine.sign_engine(pack)?;
        let mut results = Vec::new();
        for r in schedule::read_schedule(&csv)? {
            let sign = match r.sign {
                Ok(sign) => sign,
                Err(message) => {
                    results.push(RowValidation {
                        row: r.row,
                        code: None,
                        status: "errors",
                        errors: vec![DiagnosticInfo::error("CSV_FORMAT", message)],
                        warnings: Vec::new(),
                    });
                    continue;
                }
            };
            let (errors, warnings) = match engine.layout(&sign) {
                Ok(layout) => (
                    DiagnosticInfo::of(&layout.diagnostics, Severity::Error),
                    DiagnosticInfo::of(&layout.diagnostics, Severity::Warning),
                ),
                Err(e) => (
                    vec![DiagnosticInfo::error("LAYOUT_FAILED", e.to_string())],
                    Vec::new(),
                ),
            };
            results.push(RowValidation {
                row: r.row,
                code: Some(sign.code),
                status: status_of(errors.len(), warnings.len()),
                errors,
                warnings,
            });
        }
        let error_count: usize = results.iter().map(|r| r.errors.len()).sum();
        let warning_count: usize = results.iter().map(|r| r.warnings.len()).sum();
        Ok(ValidateScheduleResult {
            pack: engine.pack_id().to_string(),
            csv_path: csv.to_string_lossy().into_owned(),
            valid: error_count == 0,
            rows: results.len(),
            error_count,
            warning_count,
            results,
        })
    }
}

/// A design speed argument: 1 to 200 km/h.
pub fn check_speed(speed_kph: Option<i64>) -> Result<Option<u32>, ToolError> {
    match speed_kph {
        None => Ok(None),
        Some(s) if (1..=200).contains(&s) => Ok(Some(s as u32)),
        Some(_) => Err(ToolError::new("speed_kph must be between 1 and 200")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{self, TempDir};
    use std::path::Path;

    const HEADER: &str = "code,kind,speed_kph,approach_class,background,destinations";

    fn tools() -> Tools {
        Tools::new(testing::settings())
    }

    fn write_csv(dir: &Path, name: &str, lines: &[&str]) -> String {
        let p = dir.join(name);
        std::fs::write(&p, lines.join("\n")).unwrap();
        p.to_string_lossy().into_owned()
    }

    fn seven_destinations() -> String {
        (1..=7)
            .map(|i| format!("Town{i}:Ahead:{i}"))
            .collect::<Vec<_>>()
            .join(";")
    }

    // ─── list_packs / search_catalog ──────────────────────────────────────

    #[test]
    fn list_packs_finds_the_uk_pack_as_default() {
        let r = tools().list_packs().unwrap();
        assert_eq!(r.default_pack, "UK-TSM-2019");
        let uk = r.packs.iter().find(|p| p.id == "UK-TSM-2019").unwrap();
        assert!(uk.is_default && uk.fonts_found && uk.error.is_none());
        assert_eq!(uk.format, "json");
        assert_eq!(uk.path, "packs/UK-TSM-2019.json");
        assert!(uk.catalog_entries > 0);
        assert!(uk.name.contains("Traffic Signs Manual"));
        assert!(!r.packs.iter().any(|p| p.id == "Schema"));
        assert_eq!(r.packs.iter().filter(|p| p.is_default).count(), 1);
        assert!(r.fonts_dir.unwrap().ends_with("fonts"));
    }

    #[test]
    fn a_pack_that_does_not_load_is_listed_with_its_error() {
        let tmp = TempDir::new("badpack");
        let packs = tmp.path().join("packs");
        std::fs::create_dir_all(packs.join("XX-FOLDER")).unwrap();
        std::fs::copy(
            testing::repo_root().join("packs/UK-TSM-2019.json"),
            packs.join("UK-TSM-2019.json"),
        )
        .unwrap();
        std::fs::write(packs.join("broken.json"), "{ not a pack").unwrap();
        std::fs::write(packs.join("XX-FOLDER/pack.xml"), "<pack/>").unwrap();
        let fonts = testing::repo_root().join("fonts");
        let mut tools = Tools::new(Settings::new(
            Ok(packs.join("UK-TSM-2019.json")),
            Ok(fonts),
            tmp.path(),
        ));

        let r = tools.list_packs().unwrap();
        let ids: Vec<&str> = r.packs.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["UK-TSM-2019", "broken", "XX-FOLDER"]);
        assert!(r.packs[0].error.is_none());
        assert!(r.packs[1]
            .error
            .as_ref()
            .unwrap()
            .contains("rule pack 'broken' failed to load"));
        assert_eq!(r.packs[2].format, "xml-folder");
        assert_eq!(r.packs[2].path, "packs/XX-FOLDER/pack.xml");
        assert!(r.packs[2].error.is_some());
        assert!(tools.search_catalog(Some("broken"), None, None).is_err());
    }

    #[test]
    fn search_catalog_defaults_to_the_uk_pack() {
        let r = tools().search_catalog(None, Some("stack"), None).unwrap();
        assert_eq!(r.pack, "UK-TSM-2019");
        assert_eq!(r.count, r.entries.len());
        let e = r.entries.iter().find(|e| e.key == "UK04.1").unwrap();
        assert_eq!(
            (
                e.family.as_str(),
                e.family_label.as_str(),
                e.series.as_str(),
                e.scheme.as_str()
            ),
            ("stack", "Stack type", "UK", "primary")
        );
        assert!(!e.description.is_empty());
    }

    #[test]
    fn search_catalog_filters_by_text_and_series_ignoring_case() {
        let mut t = tools();
        let all = t.search_catalog(None, None, None).unwrap();
        assert!(all.count > 10);
        assert_eq!(
            t.search_catalog(None, Some("  "), Some("")).unwrap().count,
            all.count
        );
        let uk = t.search_catalog(None, None, Some("uk")).unwrap();
        assert!(uk.count > 0 && uk.entries.iter().all(|e| e.series == "UK"));
        assert_eq!(t.search_catalog(None, None, Some("ZZ")).unwrap().count, 0);
        let by_key = t.search_catalog(None, Some("uk04"), None).unwrap();
        assert!(by_key.entries.iter().all(|e| e.key.starts_with("UK04")) && by_key.count >= 2);
        let warning = t.search_catalog(None, Some("WARNING"), None).unwrap();
        assert!(warning.entries.iter().any(|e| e.family_label == "Warning"));
    }

    #[test]
    fn unknown_pack_is_refused_with_the_available_ids() {
        let e = tools()
            .search_catalog(Some("../packs/UK-TSM-2019.json"), None, None)
            .unwrap_err();
        assert!(e.0.contains("unknown pack") && e.0.contains("UK-TSM-2019"));
    }

    #[test]
    fn family_labels_match_the_csharp_browser() {
        assert_eq!(
            family_label("regulatory-prohibitory", "D"),
            "Regulatory · prohibitory"
        );
        assert_eq!(family_label("Give-Way", "D"), "Give way");
        assert_eq!(family_label("", "UK"), "UK");
        assert_eq!(family_label("something-new", "UK"), "Stack type");
        assert_eq!(family_token(SignKind::PlaceIdentification), "place");
        assert_eq!(family_token(SignKind::WarningSign), "stack");
    }

    // ─── layout_sign ──────────────────────────────────────────────────────

    #[test]
    fn layout_sign_by_key_returns_size_counts_and_outlined_svg() {
        let r = tools()
            .layout_sign(None, Some("UK04.1"), None, None)
            .unwrap();
        assert_eq!(r.pack, "UK-TSM-2019");
        assert_eq!(r.code, "UK04.1");
        assert_eq!(r.kind, "stack");
        assert_eq!(r.catalog_key.as_deref(), Some("UK04.1"));
        // The figures the C# server returns for the same key.
        assert_eq!(
            (r.width_mm, r.height_mm, r.sw_mm, r.x_height_mm),
            (2111.9, 1125.0, 31.25, 125.0)
        );
        assert_eq!(
            r.primitive_counts,
            BTreeMap::from([("PolyFill".to_string(), 5), ("TextOutline".to_string(), 2)])
        );
        assert!(!r.has_errors && r.diagnostics.is_empty());
        assert!(r.svg.starts_with("<svg") && r.svg.ends_with("</svg>"));
        assert!(r.svg.contains("fill-rule=\"evenodd\""));
        assert!(r.svg.contains("#025C2E")); // primary (green) ground
        assert!(r.svg.contains("viewBox=\"0 0 2111.88 1125\""));
    }

    #[test]
    fn layout_sign_takes_a_design_speed_for_a_catalog_key() {
        let mut t = tools();
        let slow = t.layout_sign(None, Some("UK04.1"), None, Some(30)).unwrap();
        let fast = t
            .layout_sign(None, Some("UK04.1"), None, Some(110))
            .unwrap();
        assert!(fast.x_height_mm > slow.x_height_mm);
        assert!(fast.width_mm > slow.width_mm);
        for bad in [0, -1, 201] {
            assert_eq!(
                t.layout_sign(None, Some("UK04.1"), None, Some(bad))
                    .unwrap_err()
                    .0,
                "speed_kph must be between 1 and 200"
            );
        }
    }

    #[test]
    fn layout_sign_from_a_schedule_row() {
        let r = tools()
            .layout_sign(
                None,
                None,
                Some("ADS-1,stack,64,primary,green,Lampton:Ahead:0;Slough:Left:0"),
                None,
            )
            .unwrap();
        assert_eq!((r.code.as_str(), r.kind.as_str()), ("ADS-1", "stack"));
        assert!(r.catalog_key.is_none());
        assert!(r.width_mm > 0.0 && !r.has_errors);
    }

    #[test]
    fn layout_sign_needs_exactly_one_of_key_and_row() {
        let mut t = tools();
        let both = t
            .layout_sign(
                None,
                Some("UK04.1"),
                Some("A,stack,64,primary,green,X:Ahead:0"),
                None,
            )
            .unwrap_err();
        assert_eq!(both.0, "give exactly one of 'key' or 'schedule_row'");
        assert!(t.layout_sign(None, None, None, None).is_err());
        assert!(t.layout_sign(None, Some(" "), Some(""), None).is_err());
    }

    #[test]
    fn layout_sign_reports_unknown_key_and_bad_row() {
        let mut t = tools();
        assert!(t
            .layout_sign(None, Some("NOPE"), None, None)
            .unwrap_err()
            .0
            .contains("not found"));
        let e = t
            .layout_sign(
                None,
                None,
                Some("A,notakind,64,primary,green,X:Ahead:0"),
                None,
            )
            .unwrap_err();
        assert!(
            e.0.starts_with("schedule_row: row 1: unknown kind 'notakind'"),
            "{e}"
        );
    }

    #[test]
    fn layout_sign_diagnostics_cite_clauses() {
        // Seven destinations exceed the Appendix D limit of six.
        let dests: Vec<String> = (1..=7).map(|i| format!("T{i}:Ahead:{i}")).collect();
        let row = format!("ADS-7,stack,64,primary,green,{}", dests.join(";"));
        let r = tools().layout_sign(None, None, Some(&row), None).unwrap();
        assert!(r.has_errors);
        let d = r
            .diagnostics
            .iter()
            .find(|d| d.code == "MAX_DESTINATIONS")
            .unwrap();
        assert_eq!(
            (d.severity, d.message.as_str()),
            ("Error", "7 destinations; maximum is 6")
        );
        assert_eq!(d.clause.as_deref(), Some("UK TSM Chapter 7 Appendix D"));
        // The size the C# server reports for the same row.
        assert_eq!((r.width_mm, r.height_mm), (879.4, 1968.8));
    }

    #[test]
    fn every_catalog_sign_lays_out() {
        let mut t = tools();
        for e in t.search_catalog(None, None, None).unwrap().entries {
            let r = t
                .layout_sign(None, Some(&e.key), None, None)
                .unwrap_or_else(|err| panic!("{}: {err}", e.key));
            assert!(
                r.width_mm > 0.0 && r.height_mm > 0.0 && r.sw_mm > 0.0,
                "{}",
                e.key
            );
            assert_eq!(r.catalog_key.as_deref(), Some(e.key.as_str()));
        }
    }

    #[test]
    fn font_tools_fail_clearly_without_fonts_but_catalog_tools_still_work() {
        let tmp = TempDir::new("no-fonts");
        let mut t = Tools::new(testing::settings_with_fonts(tmp.path()));
        assert!(!t
            .search_catalog(None, None, None)
            .unwrap()
            .entries
            .is_empty());
        assert!(t.list_packs().unwrap().packs.iter().all(|p| !p.fonts_found));
        let e = t.layout_sign(None, Some("UK04.1"), None, None).unwrap_err();
        assert!(e.0.contains("AFROBIM_FONTS"), "{e}");

        let mut t = Tools::new(Settings::new(
            testing::settings().pack_file,
            Err("the fonts folder was not found".into()),
            tmp.path(),
        ));
        assert!(t.list_packs().unwrap().fonts_dir.is_none());
        assert_eq!(
            t.layout_sign(None, Some("UK04.1"), None, None)
                .unwrap_err()
                .0,
            "the fonts folder was not found"
        );
    }

    // ─── export_dxf ───────────────────────────────────────────────────────

    #[test]
    fn export_dxf_writes_r12_files_for_catalog_keys() {
        let tmp = TempDir::new("export");
        let out = tmp.path().join("out");
        let r = tools()
            .export_dxf(
                out.to_str().unwrap(),
                None,
                Some(&["UK04.1".to_string(), "UK06.2".to_string()]),
                None,
            )
            .unwrap();
        assert_eq!(r.files.len(), 2);
        assert!(r.failures.is_empty());
        assert_eq!((r.error_count, r.warning_count), (0, 0));
        assert_eq!(r.out_dir, out.to_string_lossy());
        for f in &r.files {
            assert!(paths::is_directly_inside(&out, Path::new(&f.path)));
            let dxf = std::fs::read_to_string(&f.path).unwrap();
            assert!(dxf.contains("AC1009"));
            assert!(dxf.ends_with("0\nEOF\n"));
        }
        // The sizes the C# server reports for the same keys.
        assert_eq!(
            (r.files[0].width_mm, r.files[0].height_mm, r.files[0].sw_mm),
            (2111.9, 1125.0, 31.25)
        );
        assert_eq!(
            (r.files[1].width_mm, r.files[1].height_mm, r.files[1].sw_mm),
            (1518.5, 600.0, 25.0)
        );
    }

    #[test]
    fn export_dxf_from_the_uk_example_schedule() {
        let tmp = TempDir::new("export-uk");
        let out = tmp.path().join("uk");
        // A relative csv_path resolves against the root, not the working directory.
        let r = tools()
            .export_dxf(
                out.to_str().unwrap(),
                None,
                None,
                Some("examples/uk-directional.csv"),
            )
            .unwrap();
        assert!(!r.files.is_empty());
        assert!(r.failures.is_empty(), "{:?}", r.failures);
        assert_eq!(r.files.len(), std::fs::read_dir(&out).unwrap().count());
    }

    #[test]
    fn export_dxf_keeps_a_traversal_sign_code_inside_out_dir() {
        let tmp = TempDir::new("export-evil");
        let csv = write_csv(
            tmp.path(),
            "evil.csv",
            &[
                HEADER,
                "../../evil,stack,64,primary,green,Lampton:Ahead:0",
                "..,stack,64,primary,green,Lampton:Ahead:0",
                "CON,stack,64,primary,green,Lampton:Ahead:0",
            ],
        );
        let out = tmp.path().join("nested").join("out");
        let r = tools()
            .export_dxf(out.to_str().unwrap(), None, None, Some(&csv))
            .unwrap();
        assert_eq!(r.files.len(), 3);
        assert!(r
            .files
            .iter()
            .all(|f| paths::is_directly_inside(&out, Path::new(&f.path))));
        assert_eq!(std::fs::read_dir(&out).unwrap().count(), 3);
        assert!(!tmp.path().join("evil.dxf").exists());
        assert_eq!(
            std::fs::read_dir(tmp.path().join("nested"))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn export_dxf_suffixes_duplicate_codes_instead_of_overwriting() {
        let tmp = TempDir::new("export-dup");
        let csv = write_csv(
            tmp.path(),
            "dup.csv",
            &[
                HEADER,
                "SAME,stack,64,primary,green,Lampton:Ahead:0",
                "same,stack,64,primary,green,Slough:Ahead:0",
            ],
        );
        let r = tools()
            .export_dxf(
                tmp.path().join("dup").to_str().unwrap(),
                None,
                None,
                Some(&csv),
            )
            .unwrap();
        let names: Vec<String> = r
            .files
            .iter()
            .map(|f| {
                Path::new(&f.path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, ["SAME.dxf", "same-2.dxf"]);
    }

    #[test]
    fn export_dxf_reports_bad_rows_and_keys_without_aborting() {
        let tmp = TempDir::new("export-mixed");
        let csv = write_csv(
            tmp.path(),
            "mixed.csv",
            &[
                HEADER,
                "OK-1,stack,64,primary,green,Lampton:Ahead:0",
                "BAD-1,notakind,64,primary,green,Lampton:Ahead:0",
            ],
        );
        let mut t = tools();
        let r = t
            .export_dxf(
                tmp.path().join("mixed").to_str().unwrap(),
                None,
                None,
                Some(&csv),
            )
            .unwrap();
        assert_eq!(r.files.len(), 1);
        assert!(r.failures.iter().any(|f| f.source == "row 3"));
        assert_eq!(r.error_count, 1);

        let keys = ["UK04.1".to_string(), "NOPE".to_string(), " ".to_string()];
        let k = t
            .export_dxf(
                tmp.path().join("keys").to_str().unwrap(),
                None,
                Some(&keys),
                None,
            )
            .unwrap();
        assert_eq!(k.files.len(), 1);
        assert!(k
            .failures
            .iter()
            .any(|f| f.source == "NOPE" && f.message.contains("not found")));
        assert!(k
            .failures
            .iter()
            .any(|f| f.source == "signs" && f.message == "empty catalog key"));
        assert_eq!(k.error_count, 2);
    }

    #[test]
    fn export_dxf_counts_diagnostics_of_written_signs() {
        let tmp = TempDir::new("export-diag");
        let row = format!("TOO-MANY,stack,64,primary,green,{}", seven_destinations());
        let csv = write_csv(tmp.path(), "d.csv", &[HEADER, &row]);
        let r = tools()
            .export_dxf(
                tmp.path().join("o").to_str().unwrap(),
                None,
                None,
                Some(&csv),
            )
            .unwrap();
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.error_count, 2);
        assert!(r.files[0]
            .diagnostics
            .iter()
            .all(|d| d.severity == "Error" && d.clause.is_some()));
    }

    #[test]
    fn export_dxf_refuses_bad_arguments() {
        let tmp = TempDir::new("export-bad");
        let dir = tmp.path().to_str().unwrap();
        let file = write_csv(tmp.path(), "file.csv", &[HEADER]);
        let keys = ["UK04.1".to_string()];
        let mut t = tools();
        assert!(t.export_dxf(dir, None, None, None).is_err()); // neither
        assert!(t.export_dxf(dir, None, Some(&[]), None).is_err()); // an empty list is neither
        assert!(t.export_dxf(dir, None, Some(&keys), Some(&file)).is_err()); // both
        assert!(t
            .export_dxf(&file, None, Some(&keys), None)
            .unwrap_err()
            .0
            .contains("out_dir is a file")); // out_dir is a file
        assert!(t
            .export_dxf(
                dir,
                None,
                None,
                Some(tmp.path().join("missing.csv").to_str().unwrap())
            )
            .is_err());
        assert!(t.export_dxf("", None, Some(&keys), None).is_err());
    }

    // ─── validate_schedule ────────────────────────────────────────────────

    #[test]
    fn validate_schedule_reports_per_row_errors_and_warnings_with_clauses() {
        let tmp = TempDir::new("validate");
        let too_many = format!("TOO-MANY,stack,64,primary,green,{}", seven_destinations());
        let csv = write_csv(
            tmp.path(),
            "v.csv",
            &[
                HEADER,
                "# comment",
                "OK-1,stack,64,primary,green,Lampton:Ahead:0",
                &too_many,
                "BAD,notakind,64,primary,green,Lampton:Ahead:0",
            ],
        );
        let r = tools().validate_schedule(&csv, None).unwrap();
        assert!(!r.valid);
        assert_eq!(r.rows, 3);
        assert_eq!(r.csv_path, csv);
        let ok = r.results.iter().find(|x| x.row == 3).unwrap();
        assert_eq!((ok.code.as_deref(), ok.status), (Some("OK-1"), "ok"));
        assert!(ok.errors.is_empty());
        let too_many = r.results.iter().find(|x| x.row == 4).unwrap();
        assert_eq!(too_many.status, "errors");
        assert!(!too_many.errors.is_empty() && too_many.errors.iter().all(|d| d.clause.is_some()));
        let bad = r.results.iter().find(|x| x.row == 5).unwrap();
        assert_eq!(
            (
                bad.errors.len(),
                bad.errors[0].code.as_str(),
                bad.code.as_deref()
            ),
            (1, "CSV_FORMAT", None)
        );
        assert_eq!(
            r.error_count,
            r.results.iter().map(|x| x.errors.len()).sum::<usize>()
        );
    }

    #[test]
    fn validate_schedule_writes_nothing() {
        let tmp = TempDir::new("validate-ro");
        let csv = write_csv(
            tmp.path(),
            "w.csv",
            &[HEADER, "OK-1,stack,64,primary,green,Lampton:Ahead:0"],
        );
        let before = std::fs::read_dir(tmp.path()).unwrap().count();
        let r = tools().validate_schedule(&csv, None).unwrap();
        assert!(r.valid);
        assert_eq!((r.rows, r.error_count, r.warning_count), (1, 0, 0));
        assert_eq!(before, std::fs::read_dir(tmp.path()).unwrap().count());
    }

    #[test]
    fn validate_schedule_agrees_with_the_csharp_server_on_the_uk_example() {
        let r = tools()
            .validate_schedule("examples/uk-directional.csv", None)
            .unwrap();
        assert_eq!(
            (r.valid, r.rows, r.error_count, r.warning_count),
            (true, 20, 0, 0)
        );
        assert_eq!(
            (r.results[0].row, r.results[0].code.as_deref()),
            (10, Some("UK-STK-01"))
        );
    }

    #[test]
    fn csv_paths_must_be_existing_csv_files() {
        let tmp = TempDir::new("validate-paths");
        let txt = tmp.path().join("schedule.txt");
        std::fs::write(&txt, HEADER).unwrap();
        std::fs::create_dir(tmp.path().join("dir.csv")).unwrap();
        let mut t = tools();
        assert!(t
            .validate_schedule(txt.to_str().unwrap(), None)
            .unwrap_err()
            .0
            .contains(".csv"));
        assert!(t
            .validate_schedule(tmp.path().join("absent.csv").to_str().unwrap(), None)
            .unwrap_err()
            .0
            .contains("not found"));
        assert!(t
            .validate_schedule(tmp.path().join("dir.csv").to_str().unwrap(), None)
            .unwrap_err()
            .0
            .contains("folder"));
        let empty = write_csv(tmp.path(), "empty.csv", &[]);
        assert!(t
            .validate_schedule(&empty, None)
            .unwrap_err()
            .0
            .contains("schedule is empty"));
    }

    #[test]
    fn results_serialise_in_the_csharp_field_order_with_nulls() {
        let r = tools().search_catalog(None, Some("UK04.1"), None).unwrap();
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.starts_with(
            "{\"pack\":\"UK-TSM-2019\",\"count\":1,\"entries\":[{\"key\":\"UK04.1\",\"family\":\"stack\",\"family_label\":\"Stack type\",\"series\":\"UK\",\"scheme\":\"primary\",\"description\":"
        ));
        assert!(json.ends_with("\"symbol\":null}]}"));
    }
}
