//! Pack discovery, loading and font binding shared by the tools (port of C# `SignCoreEngine`).
//!
//! Packs are addressed by id only (never by path), so a caller cannot point the server at an
//! arbitrary file. The catalog is read from the pack alone and the fonts are loaded only when a
//! sign is laid out, so the catalog tools work on a machine with no Transport fonts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use afrobim_signs::pack::RulePack;
use afrobim_signs::std_pack::{PackCatalogEntry, StdRules};
use afrobim_signs::{SignDefinition, SignEngine, SignLayout};

use crate::settings::{Settings, FOLDER_MANIFEST, FONTS_ENV};
use crate::tools::ToolError;

/// A rule pack discovered next to the default pack.
#[derive(Debug, Clone, PartialEq)]
pub struct PackLocation {
    pub id: String,
    pub path: PathBuf,
    /// `json` for a single-file pack, `xml-folder` for a folder holding `pack.xml`.
    pub format: &'static str,
}

/// A pack read without its fonts: enough for the catalog tools.
#[derive(Debug)]
pub struct LoadedPack {
    pub pack: RulePack,
    pub catalog: Vec<PackCatalogEntry>,
}

pub struct Engine {
    pub settings: Settings,
    packs: HashMap<String, Rc<LoadedPack>>,
    engines: HashMap<String, Rc<SignEngine>>,
}

impl Engine {
    pub fn new(settings: Settings) -> Engine {
        Engine {
            settings,
            packs: HashMap::new(),
            engines: HashMap::new(),
        }
    }

    fn pack_file(&self) -> Result<&Path, ToolError> {
        self.settings
            .pack_file
            .as_deref()
            .map_err(|e| ToolError::new(e.clone()))
    }

    /// The id of the default pack.
    pub fn default_pack_id(&self) -> Result<String, ToolError> {
        self.pack_file()?;
        self.settings
            .default_pack_id()
            .ok_or_else(|| ToolError::new("the default rule pack has no file name"))
    }

    /// Single-file JSON packs and folder packs (a folder holding `pack.xml`) in the folder of
    /// the default pack, JSON files first, each group in ordinal name order.
    pub fn discover(&self) -> Vec<PackLocation> {
        let Some(dir) = self.settings.packs_dir() else {
            return Vec::new();
        };
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map(|rd| rd.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        entries.sort();
        let name = |p: &Path, stem: bool| {
            if stem { p.file_stem() } else { p.file_name() }
                .map(|n| n.to_string_lossy().into_owned())
        };

        let mut list = Vec::new();
        for p in entries
            .iter()
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "json"))
        {
            if let Some(id) = name(p, true) {
                list.push(PackLocation {
                    id,
                    path: p.clone(),
                    format: "json",
                });
            }
        }
        for p in entries
            .iter()
            .filter(|p| p.is_dir() && p.join(FOLDER_MANIFEST).is_file())
        {
            if let Some(id) = name(p, false) {
                list.push(PackLocation {
                    id,
                    path: p.join(FOLDER_MANIFEST),
                    format: "xml-folder",
                });
            }
        }
        // A default pack named with --pack or AFROBIM_PACK is always offered, wherever it lives.
        if let (Ok(file), Some(id)) = (self.pack_file(), self.settings.default_pack_id()) {
            if !list.iter().any(|l| l.id.eq_ignore_ascii_case(&id)) {
                let format = if file
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("xml"))
                {
                    "xml-folder"
                } else {
                    "json"
                };
                list.insert(
                    0,
                    PackLocation {
                        id,
                        path: file.to_path_buf(),
                        format,
                    },
                );
            }
        }
        list
    }

    /// The location of a pack by id (file or folder name, case-insensitive). `None` or blank
    /// means the default pack.
    pub fn locate(&self, id: Option<&str>) -> Result<PackLocation, ToolError> {
        let wanted = match id.map(str::trim).filter(|s| !s.is_empty()) {
            Some(id) => id.to_string(),
            None => self.default_pack_id()?,
        };
        self.pack_file()?;
        let found = self.discover();
        match found.iter().find(|p| p.id.eq_ignore_ascii_case(&wanted)) {
            Some(loc) => Ok(loc.clone()),
            None => {
                let ids: Vec<&str> = found.iter().map(|p| p.id.as_str()).collect();
                Err(ToolError::new(format!(
                    "unknown pack '{wanted}'; available: {}",
                    ids.join(", ")
                )))
            }
        }
    }

    /// Loads a pack by id, without fonts.
    pub fn load_pack(&mut self, id: Option<&str>) -> Result<Rc<LoadedPack>, ToolError> {
        let loc = self.locate(id)?;
        self.load_from(&loc)
    }

    pub fn load_from(&mut self, loc: &PackLocation) -> Result<Rc<LoadedPack>, ToolError> {
        if let Some(p) = self.packs.get(&loc.id) {
            return Ok(p.clone());
        }
        let failed = |e: afrobim_signs::SignError| {
            let hint = if loc.format == "xml-folder" {
                " (XML folder packs are not read by the AfroBIM engine yet)"
            } else {
                ""
            };
            ToolError::new(format!("rule pack '{}' failed to load{hint}: {e}", loc.id))
        };
        let pack = RulePack::load(&loc.path).map_err(failed)?;
        let std = StdRules::load(&loc.path, &pack).map_err(failed)?;
        let catalog = std.catalog.map(|c| c.entries).unwrap_or_default();
        let loaded = Rc::new(LoadedPack { pack, catalog });
        self.packs.insert(loc.id.clone(), loaded.clone());
        Ok(loaded)
    }

    /// The fonts folder, or why there is none.
    pub fn fonts_dir(&self) -> Result<&Path, ToolError> {
        self.settings
            .fonts_dir
            .as_deref()
            .map_err(|e| ToolError::new(e.clone()))
    }

    /// True when both Transport faces the pack names are in the fonts folder (in any letter
    /// case, as the sign engine reads them).
    pub fn fonts_present(&self, pack: &RulePack) -> bool {
        let Ok(dir) = self.fonts_dir() else {
            return false;
        };
        let names: Vec<String> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        [&pack.fonts.on_dark.file, &pack.fonts.on_light.file]
            .iter()
            .all(|f| names.iter().any(|n| n.eq_ignore_ascii_case(f)))
    }

    /// The pack with Transport Medium and Heavy loaded, ready to lay signs out.
    pub fn sign_engine(&mut self, id: Option<&str>) -> Result<Rc<SignEngine>, ToolError> {
        let loc = self.locate(id)?;
        if let Some(e) = self.engines.get(&loc.id) {
            return Ok(e.clone());
        }
        let loaded = self.load_from(&loc)?;
        let fonts = self.fonts_dir()?.to_path_buf();
        if !self.fonts_present(&loaded.pack) {
            return Err(ToolError::new(format!(
                "Transport fonts not found: put {} and {} in {} or set {FONTS_ENV} to the folder holding them",
                loaded.pack.fonts.on_dark.file,
                loaded.pack.fonts.on_light.file,
                fonts.display()
            )));
        }
        let engine = SignEngine::load(&loc.path, &fonts).map_err(|e| {
            ToolError::new(format!(
                "could not load rule pack '{}' with the fonts in {}: {e}",
                loc.id,
                fonts.display()
            ))
        })?;
        let engine = Rc::new(engine);
        self.engines.insert(loc.id.clone(), engine.clone());
        Ok(engine)
    }
}

/// Lays a sign out, lettering as glyph outlines, as the DXF export does.
pub fn run(engine: &SignEngine, sign: &SignDefinition) -> Result<SignLayout, ToolError> {
    engine
        .layout(sign)
        .map_err(|e| ToolError::new(format!("{}: layout failed: {e}", sign.code)))
}

/// A catalog page turned into a seed sign.
pub fn from_catalog(
    engine: &SignEngine,
    key: &str,
    speed_kph: Option<u32>,
) -> Result<SignDefinition, ToolError> {
    let key = key.trim();
    let Some(entry) = engine.catalog_entry(key) else {
        return Err(ToolError::new(if engine.catalog().is_empty() {
            format!("rule pack '{}' registers no sign catalog", engine.pack_id())
        } else {
            format!(
                "catalog sign page '{key}' not found in rule pack '{}'",
                engine.pack_id()
            )
        }));
    };
    Ok(match speed_kph {
        Some(speed) => engine.definition_for_speed(entry, speed),
        None => engine.definition_for(entry),
    })
}

/// A size in millimetres to one decimal (C# `Math.Round(v, 1)`).
pub fn round1(v: f64) -> f64 {
    (v * 10.0).round_ties_even() / 10.0
}

/// The finished size of a laid-out sign in millimetres, unrounded.
pub fn size_mm(layout: &SignLayout) -> (f64, f64) {
    (
        layout.drawing.extents.width() * layout.sw_mm,
        layout.drawing.extents.height() * layout.sw_mm,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{self, TempDir};

    #[test]
    fn discovery_lists_json_packs_then_folder_packs_and_skips_other_folders() {
        let engine = Engine::new(testing::settings());
        let found = engine.discover();
        let ids: Vec<&str> = found.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids[0], "UK-TSM-2019");
        assert_eq!(found[0].format, "json");
        assert!(!ids.contains(&"Schema"));
        for folder in found.iter().filter(|p| p.format == "xml-folder") {
            assert!(folder.path.ends_with(FOLDER_MANIFEST));
        }
    }

    #[test]
    fn packs_are_found_by_id_in_any_case_and_never_by_path() {
        let mut engine = Engine::new(testing::settings());
        assert_eq!(engine.load_pack(None).unwrap().pack.id, "UK-TSM-2019");
        assert_eq!(
            engine.load_pack(Some(" uk-tsm-2019 ")).unwrap().pack.id,
            "UK-TSM-2019"
        );
        let err = engine
            .load_pack(Some("../packs/UK-TSM-2019.json"))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("unknown pack") && err.contains("available: UK-TSM-2019"),
            "{err}"
        );
    }

    #[test]
    fn the_catalog_loads_without_fonts_but_layout_says_what_is_missing() {
        let tmp = TempDir::new("nofonts");
        let mut engine = Engine::new(testing::settings_with_fonts(tmp.path()));
        let loaded = engine.load_pack(None).unwrap();
        assert!(!loaded.catalog.is_empty());
        assert!(!engine.fonts_present(&loaded.pack));
        let err = engine
            .sign_engine(None)
            .map(|_| ())
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("Transport fonts not found") && err.contains(FONTS_ENV),
            "{err}"
        );
    }

    #[test]
    fn a_missing_pack_or_fonts_folder_is_reported_by_the_tool_not_at_start_up() {
        let mut engine = Engine::new(Settings::new(
            Err("rule pack not found: x".into()),
            Err("no fonts".into()),
            Path::new("/"),
        ));
        assert!(engine.discover().is_empty());
        assert_eq!(
            engine.load_pack(None).unwrap_err().to_string(),
            "rule pack not found: x"
        );
        assert_eq!(
            engine
                .load_pack(Some("UK-TSM-2019"))
                .unwrap_err()
                .to_string(),
            "rule pack not found: x"
        );
    }

    #[test]
    fn a_catalog_key_seeds_a_sign_and_an_unknown_key_is_named() {
        let mut engine = Engine::new(testing::settings());
        let signs = engine.sign_engine(None).unwrap();
        let sign = from_catalog(&signs, " uk04.1 ", None).unwrap();
        assert_eq!(sign.catalog_key.as_deref(), Some("UK04.1"));
        assert_eq!(
            from_catalog(&signs, "UK04.1", Some(100)).unwrap().speed_kph,
            100
        );
        let err = from_catalog(&signs, "NOPE", None).unwrap_err().to_string();
        assert_eq!(
            err,
            "catalog sign page 'NOPE' not found in rule pack 'UK-TSM-2019'"
        );
        let layout = run(&signs, &sign).unwrap();
        let (w, h) = size_mm(&layout);
        assert!(w > 0.0 && h > 0.0);
        assert_eq!(round1(2111.875), 2111.9);
    }
}
