// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The folder rule pack: a `pack.xml` manifest (or a directory holding one) that lists one XML
//! file per concern (text sizes, panel, route arms, palettes, symbols, sign catalog ...) in
//! load order. This is the format of the Uganda TSM 2004 pack.
//!
//! Port of the XML half of C# `SignCore.Rules.RulePackLoader` (`LoadXmlPack` and its section
//! readers), with the loader's own checks and error messages. The sections are gathered into
//! the same document a single-file JSON pack holds (the same section and value names), so the
//! records and semantic checks of `pack.rs`, `std_pack.rs` and `ug_pack.rs` read both formats.
//!
//! Not ported: validation against `Schema/rulepack.xsd`. The C# loader skips it too when the
//! schema file is not beside the pack; the checks here and the semantic checks always run.
//! Two readings are narrower than .NET's: a number must be finite (`NaN` and `Infinity` are
//! refused as not a number), and a colour or face must be given by name, not by its ordinal.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use roxmltree::{Document, Node};
use serde_json::{json, Map, Value};

use crate::SignError;

/// Name of the manifest inside a pack folder.
pub const MANIFEST_FILE_NAME: &str = "pack.xml";

/// True when `path` is read as a folder pack: a directory, or any file that is not `.json`
/// (C# `RulePackLoader.Load`).
pub(crate) fn is_folder_pack(path: &Path) -> bool {
    path.is_dir()
        || !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
}

/// A folder pack as one document, and the manifest path that names it in error messages.
pub(crate) struct FolderPack {
    pub value: Value,
    pub manifest: String,
}

/// Accepts a `pack.xml` path or a pack directory containing one.
fn resolve_manifest(path: &Path) -> Result<PathBuf, SignError> {
    if path.is_dir() {
        let inside = path.join(MANIFEST_FILE_NAME);
        if !inside.is_file() {
            return Err(SignError::Pack(format!(
                "Rule pack not found: {} (no {MANIFEST_FILE_NAME} in directory {})",
                inside.display(),
                path.display()
            )));
        }
        return Ok(inside);
    }
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    Err(SignError::Pack(format!(
        "Rule pack not found: {}",
        path.display()
    )))
}

fn read_text(file: &Path) -> Result<String, SignError> {
    let text = std::fs::read_to_string(file).map_err(|e| {
        SignError::Pack(format!(
            "Rule pack could not be read: {}: {e}",
            file.display()
        ))
    })?;
    Ok(match text.strip_prefix('\u{feff}') {
        Some(rest) => rest.to_string(),
        None => text,
    })
}

fn parse<'a>(text: &'a str, file: &str) -> Result<Document<'a>, SignError> {
    Document::parse(text)
        .map_err(|e| SignError::Pack(format!("Rule pack is not well-formed XML: {file}: {e}")))
}

/// Loads the manifest and every section file it names.
pub(crate) fn load(path: &Path) -> Result<FolderPack, SignError> {
    let manifest_path = resolve_manifest(path)?;
    let manifest = manifest_path.display().to_string();
    let directory = manifest_path.parent().unwrap_or(Path::new(""));

    let manifest_text = read_text(&manifest_path)?;
    let doc = parse(&manifest_text, &manifest)?;
    let root = doc.root_element();

    let mut files: Vec<(String, String)> = Vec::new(); // (shown path, text)
    if let Some(sections) = child(root, "sections") {
        for section in sections.children().filter(Node::is_element) {
            let tag = section.tag_name().name();
            if tag != "section" {
                return Err(SignError::Pack(format!(
                    "Rule pack 'sections' may only contain <section> elements, found '<{tag}>': {manifest}"
                )));
            }
            let file = attr(section, "file").map(str::trim).unwrap_or("");
            if file.is_empty() {
                return Err(SignError::Pack(format!(
                    "Rule pack <section> has no 'file' attribute: {manifest}"
                )));
            }
            let section_path = directory.join(file);
            let shown = section_path.display().to_string();
            if !section_path.is_file() {
                return Err(SignError::Pack(format!(
                    "Rule pack section file not found: {shown} (referenced by {manifest})"
                )));
            }
            files.push((shown, read_text(&section_path)?));
        }
    }

    let section_docs = files
        .iter()
        .map(|(shown, text)| parse(text, shown))
        .collect::<Result<Vec<Document>, SignError>>()?;
    let sections: Vec<Node> = section_docs.iter().map(Document::root_element).collect();
    let value = from_documents(root, &sections, &manifest)?;
    Ok(FolderPack { value, manifest })
}

/// Builds the pack document from manifest and section texts held in memory; `manifest` only
/// names the pack in error messages. This is [`load`] without the file system.
pub fn from_xml(
    manifest_xml: &str,
    section_xml: &[&str],
    manifest: &str,
) -> Result<Value, SignError> {
    let doc = parse(manifest_xml, manifest)?;
    let section_docs = section_xml
        .iter()
        .map(|text| parse(text, manifest))
        .collect::<Result<Vec<Document>, SignError>>()?;
    let sections: Vec<Node> = section_docs.iter().map(Document::root_element).collect();
    from_documents(doc.root_element(), &sections, manifest)
}

/// The section readers of C# `LoadXmlPack`, in its order. `sections` are the root elements of
/// the section files in manifest order; a later file with the same root name replaces an
/// earlier one.
fn from_documents(root: Node, sections: &[Node], path: &str) -> Result<Value, SignError> {
    let id = attr(root, "id").map(str::trim).unwrap_or("");
    if id.is_empty() {
        return Err(SignError::Pack(format!("Rule pack has no id: {path}")));
    }
    let name = attr(root, "name").map(str::trim).unwrap_or("");
    let source = attr(root, "source").map(str::trim).unwrap_or("");

    // Keyed by the root element name of the section file, without regard to case.
    let mut concerns: BTreeMap<String, Node> = BTreeMap::new();
    for s in sections {
        concerns.insert(s.tag_name().name().to_ascii_lowercase(), *s);
    }
    let section = |root_name: &str| concerns.get(&root_name.to_ascii_lowercase()).copied();
    let required = |root_name: &str, key: &str| {
        section(root_name)
            .ok_or_else(|| SignError::Pack(format!("Rule pack is missing section '{key}': {path}")))
    };

    let mut pack = Map::new();
    pack.insert("id".into(), json!(id));
    pack.insert("name".into(), json!(name));
    pack.insert("source".into(), json!(source));

    // ─── required sections ───
    let mut text_sizes = Vec::new();
    if let Some(el) = section("textSizesSection") {
        for size in elements(el, "textSize") {
            text_sizes.push(json!({
                "speedMaxKph": nullable_int(size, "speedMaxKph", path)?,
                "xHeightMm": dbl(size, "xHeightMm", path)?,
                "swMm": dbl(size, "swMm", path)?,
                "capHeightMm": dbl(size, "capHeightMm", path)?,
            }));
        }
    }
    if text_sizes.is_empty() {
        return Err(SignError::Pack(format!(
            "Rule pack has no textSizes: {path}"
        )));
    }
    pack.insert("textSizes".into(), Value::Array(text_sizes));

    let el = required("panelSection", "panel")?;
    pack.insert(
        "panel".into(),
        doubles(
            el,
            &[
                "borderWidth",
                "cornerRadius",
                "padTop",
                "padSide",
                "padBottom",
                "wordSpacing",
                "textTileHeight",
            ],
            path,
        )?,
    );

    let el = required("routeArmsSection", "routeArms")?;
    pack.insert(
        "routeArms".into(),
        doubles(
            el,
            &[
                "widthPrimary",
                "widthSecondary",
                "widthLocal",
                "minLengthVertical",
                "minLengthHorizontal",
                "armToTopBorder",
                "armToBottomBorder",
            ],
            path,
        )?,
    );

    let el = required("mapTypeSpacingSection", "mapTypeSpacing")?;
    pack.insert(
        "mapTypeSpacing".into(),
        doubles(
            el,
            &[
                "tileToVerticalArmTop",
                "tileToVerticalArmSide",
                "tileToHorizontalArmBottom",
                "tileToArmTop",
                "tileToDiagonalArmEnd",
            ],
            path,
        )?,
    );

    let el = required("roundaboutSection", "roundabout")?;
    let Value::Object(mut roundabout) = doubles(el, &["innerDiameter", "outerDiameter"], path)?
    else {
        unreachable!("doubles returns an object")
    };
    for name in [
        "exitMinLength",
        "approachMinLength",
        "cutOutFromApproachDeg",
        "cutOutAngleDeg",
        "cutOutMinAngleDeg",
        "legendKeepOutRadius",
        "ringToBorder",
        "legendToRingSide",
    ] {
        if let Some(v) = opt_dbl(el, name, path)? {
            roundabout.insert(name.into(), v);
        }
    }
    if let Some(note) = attr(el, "provisional") {
        roundabout.insert("provisional".into(), json!(note.trim()));
    }
    pack.insert("roundabout".into(), Value::Object(roundabout));

    let el = required("embeddedSignsSection", "embeddedSigns")?;
    pack.insert(
        "embeddedSigns".into(),
        doubles(el, &["heightSw", "whiteEdgeOnGreen"], path)?,
    );

    let el = required("fontsSection", "fonts")?;
    let font_ref = |element_name: &str| -> Result<Value, SignError> {
        let f = child(el, element_name).ok_or_else(|| {
            SignError::Pack(format!(
                "Rule pack 'fonts' is missing <{element_name}>: {path}"
            ))
        })?;
        Ok(json!({"face": req(f, "face", path)?, "file": req(f, "file", path)?}))
    };
    pack.insert(
        "fonts".into(),
        json!({"onDark": font_ref("onDark")?, "onLight": font_ref("onLight")?}),
    );

    let el = required("constraintsSection", "constraints")?;
    pack.insert(
        "constraints".into(),
        json!({
            "maxDestinations": int(el, "maxDestinations", path)?,
            "maxDestinationsPerDirection": int(el, "maxDestinationsPerDirection", path)?,
            "nearerDestinationAbove": boolean(el, "nearerDestinationAbove", path)?,
            "uniformTextSize": boolean(el, "uniformTextSize", path)?,
        }),
    );

    // ─── optional sections: read when present ───
    if let Some(el) = section("arrowsSection") {
        pack.insert(
            "arrows".into(),
            doubles(
                el,
                &[
                    "shaftLength",
                    "shaftWidth",
                    "headLength",
                    "headFlare",
                    "flareAngleDeg",
                    "pointHalfAngleDeg",
                    "innerCornerRadius",
                    "tipRadius",
                ],
                path,
            )?,
        );
    }

    if let Some(el) = section("embeddedShapesSection") {
        pack.insert(
            "embeddedShapes".into(),
            doubles(
                el,
                &["triangleBorderWidth", "roundelRingWidth", "roundelDiameter"],
                path,
            )?,
        );
    }

    if let Some(el) = section("palettesSection") {
        let mut entries = Vec::new();
        for p in elements(el, "palette") {
            entries.push(json!({
                "scheme": req(p, "scheme", path)?,
                "ground": named(p, "ground", &COLOURS, path)?,
                "lettering": named(p, "lettering", &COLOURS, path)?,
                "arm": named(p, "arm", &COLOURS, path)?,
                "face": named(p, "face", &FACES, path)?,
            }));
        }
        pack.insert("palettes".into(), json!({ "entries": entries }));
    }

    if let Some(el) = section("symbolsSection") {
        let mut symbols = Vec::new();
        for s in elements(el, "symbol") {
            symbols.push(json!({
                "id": req(s, "id", path)?,
                "name": req(s, "name", path)?,
                "sizeSw": dbl(s, "sizeSw", path)?,
                "group": req(s, "group", path)?,
                "artworkRef": attr(s, "artworkRef").map(str::trim),
            }));
        }
        pack.insert("symbols".into(), json!({ "symbols": symbols }));
    }

    if let Some(el) = section("catalogSection") {
        let mut entries = Vec::new();
        for s in elements(el, "signPage") {
            let embedded: Option<Vec<&str>> = attr(s, "embeddedSymbols")
                .filter(|e| !e.trim().is_empty())
                .map(|e| {
                    e.split([' ', '\t', '\r', '\n'])
                        .filter(|t| !t.is_empty())
                        .collect()
                });
            let mut entry = Map::new();
            entry.insert("key".into(), json!(req(s, "key", path)?));
            entry.insert("name".into(), json!(req(s, "name", path)?));
            entry.insert("family".into(), json!(req(s, "family", path)?));
            entry.insert("series".into(), json!(req(s, "series", path)?));
            entry.insert("scheme".into(), json!(req(s, "scheme", path)?));
            entry.insert("symbolId".into(), json!(attr(s, "symbolId").map(str::trim)));
            entry.insert("embeddedSymbols".into(), json!(embedded));
            entry.insert("template".into(), json!(attr(s, "template").map(str::trim)));
            entries.push(Value::Object(entry));
        }
        pack.insert("catalog".into(), json!({ "entries": entries }));
    }

    if let Some(el) = section("standardSizesSection") {
        let mut bands = Vec::new();
        for b in elements(el, "sizeBand") {
            bands.push(json!({
                "class": req(b, "class", path)?,
                "speedMaxKph": nullable_int(b, "speedMaxKph", path)?,
                "warningTriangleSideMm": dbl(b, "warningTriangleSideMm", path)?,
                "regulatoryDiameterMm": dbl(b, "regulatoryDiameterMm", path)?,
                "supplementaryXHeightMm": dbl(b, "supplementaryXHeightMm", path)?,
                "supplementarySwMm": dbl(b, "supplementarySwMm", path)?,
                "supplementaryCapMm": dbl(b, "supplementaryCapMm", path)?,
            }));
        }
        pack.insert(
            "standardSizes".into(),
            json!({
                "bands": bands,
                "lowSpeedRegulatoryDiameterMm": dbl(el, "lowSpeedRegulatoryDiameterMm", path)?,
            }),
        );
    }

    if let Some(el) = section("roundelsTrianglesSection") {
        let mut slashes = Vec::new();
        for s in elements(el, "slash") {
            slashes.push(json!({
                "id": req(s, "id", path)?,
                "appliesTo": req(s, "appliesTo", path)?,
                "colour": req(s, "colour", path)?,
                "direction": req(s, "direction", path)?,
                "count": int(s, "count", path)?,
            }));
        }
        let mut provisional = Vec::new();
        for p in elements(el, "proportional") {
            provisional.push(json!({
                "key": req(p, "key", path)?,
                "fraction": dbl(p, "fraction", path)?,
                "source": req(p, "source", path)?,
            }));
        }
        pack.insert(
            "roundelsTriangles".into(),
            json!({
                "warningShape": req(el, "warningShape", path)?,
                "giveWayShape": req(el, "giveWayShape", path)?,
                "stopShape": req(el, "stopShape", path)?,
                "regulatoryDiscShape": req(el, "regulatoryDiscShape", path)?,
                "stopOrientation": req(el, "stopOrientation", path)?,
                "slashes": slashes,
                "provisionalFractions": provisional,
            }),
        );
    }

    // Optional `mapSymbolSection`: the filled map-type route symbol.
    if let Some(el) = section("mapSymbolSection") {
        let Value::Object(mut ms) = doubles(
            el,
            &[
                "filletRadius",
                "tipHalfAngleDeg",
                "sideArmReach",
                "approachToBottomBorder",
                "panelToArmTip",
                "stubToBorder",
                "stubToBottomBorder",
            ],
            path,
        )?
        else {
            unreachable!("doubles returns an object")
        };
        let is = |name: &str, word: &str| {
            attr(el, name).is_some_and(|v| v.trim().eq_ignore_ascii_case(word))
        };
        if let Some(from) = attr(el, "sideArmReachFrom") {
            ms.insert("sideArmReachFrom".into(), json!(from.trim()));
        }
        ms.insert("exitMinToPoint".into(), json!(is("exitMinToPoint", "true")));
        ms.insert("stubsCited".into(), json!(!is("stubsCited", "false")));
        if let Some(note) = attr(el, "provisional") {
            ms.insert("provisional".into(), json!(note.trim()));
        }
        pack.insert("mapSymbol".into(), Value::Object(ms));
    }

    Ok(Value::Object(pack))
}

// ─── element and attribute reading ───────────────────────────────────────────

const COLOURS: [&str; 8] = [
    "White", "Black", "Green", "Blue", "Brown", "Yellow", "Red", "Grey",
];
const FACES: [&str; 2] = ["TransportMedium", "TransportHeavy"];

/// The first child element with this name, without regard to case or namespace.
fn child<'a, 'i>(parent: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    parent
        .children()
        .find(|c| c.is_element() && c.tag_name().name().eq_ignore_ascii_case(name))
}

/// The child elements with exactly this name and no namespace (C# `Elements(name)`).
fn elements<'a, 'i: 'a>(parent: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> {
    parent.children().filter(move |c| {
        c.is_element() && c.tag_name().name() == name && c.tag_name().namespace().is_none()
    })
}

/// An attribute's value, by name without regard to case.
fn attr<'a>(el: Node<'a, '_>, name: &str) -> Option<&'a str> {
    el.attributes()
        .find(|a| a.name().eq_ignore_ascii_case(name))
        .map(|a| a.value())
}

fn req<'a>(el: Node<'a, '_>, name: &str, path: &str) -> Result<&'a str, SignError> {
    match attr(el, name).map(str::trim) {
        Some(v) if !v.is_empty() => Ok(v),
        _ => Err(SignError::Pack(format!(
            "Rule pack section <{}> is missing attribute '{name}': {path}",
            el.tag_name().name()
        ))),
    }
}

fn dbl(el: Node, name: &str, path: &str) -> Result<Value, SignError> {
    let raw = req(el, name, path)?;
    raw.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(|v| json!(v))
        .ok_or_else(|| {
            SignError::Pack(format!(
                "Rule pack section <{}> attribute '{name}' is not a number: '{raw}' in {path}",
                el.tag_name().name()
            ))
        })
}

fn opt_dbl(el: Node, name: &str, path: &str) -> Result<Option<Value>, SignError> {
    match attr(el, name) {
        Some(v) if !v.trim().is_empty() => dbl(el, name, path).map(Some),
        _ => Ok(None),
    }
}

/// An object of required numbers, keyed by attribute name.
fn doubles(el: Node, names: &[&str], path: &str) -> Result<Value, SignError> {
    let mut map = Map::new();
    for name in names {
        map.insert((*name).to_string(), dbl(el, name, path)?);
    }
    Ok(Value::Object(map))
}

fn parse_int(el: Node, name: &str, raw: &str, path: &str) -> Result<i32, SignError> {
    raw.parse::<i32>().map_err(|_| {
        SignError::Pack(format!(
            "Rule pack section <{}> attribute '{name}' is not an integer: '{raw}' in {path}",
            el.tag_name().name()
        ))
    })
}

fn int(el: Node, name: &str, path: &str) -> Result<i32, SignError> {
    parse_int(el, name, req(el, name, path)?, path)
}

/// An omitted attribute (or an explicit null token) is the open-ended band.
fn nullable_int(el: Node, name: &str, path: &str) -> Result<Option<i32>, SignError> {
    match attr(el, name).map(str::trim) {
        None | Some("") => Ok(None),
        Some(raw) if raw.eq_ignore_ascii_case("null") => Ok(None),
        Some(raw) => parse_int(el, name, raw, path).map(Some),
    }
}

fn boolean(el: Node, name: &str, path: &str) -> Result<bool, SignError> {
    let raw = req(el, name, path)?;
    if raw.eq_ignore_ascii_case("true") || raw == "1" {
        return Ok(true);
    }
    if raw.eq_ignore_ascii_case("false") || raw == "0" {
        return Ok(false);
    }
    Err(SignError::Pack(format!(
        "Rule pack section <{}> attribute '{name}' is not true/false: '{raw}' in {path}",
        el.tag_name().name()
    )))
}

/// One of `names`, matched without regard to case and returned as the pack documents spell it.
fn named(
    el: Node,
    name: &str,
    names: &[&'static str],
    path: &str,
) -> Result<&'static str, SignError> {
    let raw = req(el, name, path)?;
    names
        .iter()
        .copied()
        .find(|n| n.eq_ignore_ascii_case(raw))
        .ok_or_else(|| {
            SignError::Pack(format!(
                "Rule pack section <{}> attribute '{name}' has unknown value '{raw}' (expected one of {}): {path}",
                el.tag_name().name(),
                names.join(", ")
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str =
        r#"<rulePack id=" T-1 " name="Test" source="Src"><sections/></rulePack>"#;
    const TEXT: &str = r#"<textSizesSection><textSize speedMaxKph="50" xHeightMm="100" swMm="25.0" capHeightMm="140"/>
        <textSize xHeightMm="200" swMm="50" capHeightMm="280"/></textSizesSection>"#;
    const PANEL: &str = r#"<panelSection borderWidth="1.5" cornerRadius="1.5" padTop="2.5" padSide="2.5" padBottom="1.5" wordSpacing="2.5" textTileHeight="8.0"/>"#;
    const ARMS: &str = r#"<routeArmsSection widthPrimary="6.0" widthSecondary="4.0" widthLocal="2.5" minLengthVertical="12.0" minLengthHorizontal="12.0" armToTopBorder="4.0" armToBottomBorder="1.5"/>"#;
    const SPACING: &str = r#"<mapTypeSpacingSection tileToVerticalArmTop="0.0" tileToVerticalArmSide="4.0" tileToHorizontalArmBottom="1.5" tileToArmTop="2.5" tileToDiagonalArmEnd="2.5"/>"#;
    const RING: &str = r#"<roundaboutSection innerDiameter="14.0" outerDiameter="24.0"/>"#;
    const EMBEDDED: &str = r#"<embeddedSignsSection heightSw="20.0" whiteEdgeOnGreen="0.5"/>"#;
    const FONTS: &str = r#"<fontsSection><onDark face="M" file="m.ttf"/><onLight face="H" file="h.ttf"/></fontsSection>"#;
    const CONSTRAINTS: &str = r#"<constraintsSection maxDestinations="4" maxDestinationsPerDirection="2" nearerDestinationAbove="TRUE" uniformTextSize="1"/>"#;

    fn required() -> Vec<&'static str> {
        vec![
            TEXT,
            PANEL,
            ARMS,
            SPACING,
            RING,
            EMBEDDED,
            FONTS,
            CONSTRAINTS,
        ]
    }

    fn err(manifest: &str, sections: &[&str]) -> String {
        from_xml(manifest, sections, "pack.xml")
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn the_required_sections_become_the_json_pack_document() {
        let v = from_xml(MANIFEST, &required(), "pack.xml").unwrap();
        assert_eq!(v["id"], "T-1");
        assert_eq!(v["textSizes"][0]["speedMaxKph"], 50);
        assert!(v["textSizes"][1]["speedMaxKph"].is_null());
        assert_eq!(v["panel"]["textTileHeight"], 8.0);
        assert_eq!(v["fonts"]["onLight"]["file"], "h.ttf");
        assert_eq!(v["constraints"]["nearerDestinationAbove"], true);
        assert_eq!(v["constraints"]["uniformTextSize"], true);
        assert!(v.get("arrows").is_none() && v.get("mapSymbol").is_none());
        // The document loads as a rule pack.
        let pack = crate::pack::RulePack::from_value(&v, "pack.xml").unwrap();
        assert_eq!(pack.id, "T-1");
        assert_eq!(pack.constraints.max_destinations_per_direction, 2);
        assert!(pack.roundabout.as_ref().unwrap().exit_min_length.is_none());
    }

    #[test]
    fn a_missing_required_section_is_named() {
        for (i, key) in [
            (1, "panel"),
            (2, "routeArms"),
            (3, "mapTypeSpacing"),
            (4, "roundabout"),
            (5, "embeddedSigns"),
            (6, "fonts"),
            (7, "constraints"),
        ] {
            let mut s = required();
            s.remove(i);
            assert_eq!(
                err(MANIFEST, &s),
                format!("Rule pack is missing section '{key}': pack.xml")
            );
        }
        let mut s = required();
        s.remove(0);
        assert_eq!(err(MANIFEST, &s), "Rule pack has no textSizes: pack.xml");
        assert_eq!(
            err(r#"<rulePack id=" "/>"#, &required()),
            "Rule pack has no id: pack.xml"
        );
    }

    #[test]
    fn bad_attributes_are_reported_as_the_loader_reports_them() {
        let swap = |i: usize, with: &'static str| {
            let mut s = required();
            s[i] = with;
            err(MANIFEST, &s)
        };
        assert_eq!(
            swap(4, r#"<roundaboutSection innerDiameter="14.0"/>"#),
            "Rule pack section <roundaboutSection> is missing attribute 'outerDiameter': pack.xml"
        );
        assert_eq!(
            swap(5, r#"<embeddedSignsSection heightSw="tall" whiteEdgeOnGreen="0.5"/>"#),
            "Rule pack section <embeddedSignsSection> attribute 'heightSw' is not a number: 'tall' in pack.xml"
        );
        assert_eq!(
            swap(5, r#"<embeddedSignsSection heightSw="NaN" whiteEdgeOnGreen="0.5"/>"#),
            "Rule pack section <embeddedSignsSection> attribute 'heightSw' is not a number: 'NaN' in pack.xml"
        );
        assert!(swap(7, r#"<constraintsSection maxDestinations="4.5" maxDestinationsPerDirection="2" nearerDestinationAbove="true" uniformTextSize="true"/>"#)
            .contains("attribute 'maxDestinations' is not an integer: '4.5'"));
        assert!(swap(7, r#"<constraintsSection maxDestinations="4" maxDestinationsPerDirection="2" nearerDestinationAbove="yes" uniformTextSize="true"/>"#)
            .contains("attribute 'nearerDestinationAbove' is not true/false: 'yes'"));
        assert_eq!(
            swap(
                6,
                r#"<fontsSection><onDark face="M" file="m.ttf"/></fontsSection>"#
            ),
            "Rule pack 'fonts' is missing <onLight>: pack.xml"
        );
        assert!(swap(0, r#"<textSizesSection><textSize speedMaxKph="fast" xHeightMm="1" swMm="1" capHeightMm="1"/></textSizesSection>"#)
            .contains("attribute 'speedMaxKph' is not an integer: 'fast'"));
        assert!(swap(1, "<panelSection").starts_with("Rule pack is not well-formed XML: pack.xml"));
    }

    #[test]
    fn optional_sections_are_read_when_present() {
        let mut s = required();
        s.push(r#"<palettesSection><palette scheme="primary" ground="green" lettering="White" arm="White" face="transportmedium"/></palettesSection>"#);
        s.push(r#"<catalogSection><signPage key="G01.1" name="N" family="map-roundabout" series="G" scheme="primary" embeddedSymbols=" a  b
            c " template=" plate "/><signPage key="W01" name="B" family="warning" series="W" scheme="primary" symbolId="bend"/></catalogSection>"#);
        s.push(r#"<mapSymbolSection filletRadius="0" tipHalfAngleDeg="45" sideArmReach="0.5" approachToBottomBorder="1.5" panelToArmTip="0" stubToBorder="2.5" stubToBottomBorder="4" sideArmReachFrom=" border " exitMinToPoint="True" stubsCited="FALSE"/>"#);
        s.push(r#"<standardSizesSection lowSpeedRegulatoryDiameterMm="400"><sizeBand class="high" warningTriangleSideMm="1100" regulatoryDiameterMm="900" supplementaryXHeightMm="90" supplementarySwMm="22.5" supplementaryCapMm="126"/></standardSizesSection>"#);
        let v = from_xml(MANIFEST, &s, "pack.xml").unwrap();
        assert_eq!(v["palettes"]["entries"][0]["ground"], "Green");
        assert_eq!(v["palettes"]["entries"][0]["face"], "TransportMedium");
        let e = &v["catalog"]["entries"];
        assert_eq!(e[0]["embeddedSymbols"], json!(["a", "b", "c"]));
        assert_eq!(e[0]["template"], "plate");
        assert!(e[0]["symbolId"].is_null() && e[1]["embeddedSymbols"].is_null());
        assert_eq!(e[1]["symbolId"], "bend");
        assert_eq!(v["mapSymbol"]["sideArmReachFrom"], "border");
        assert_eq!(v["mapSymbol"]["exitMinToPoint"], true);
        assert_eq!(v["mapSymbol"]["stubsCited"], false);
        assert!(v["standardSizes"]["bands"][0]["speedMaxKph"].is_null());

        s[8] = r#"<palettesSection><palette scheme="primary" ground="Pink" lettering="White" arm="White" face="TransportMedium"/></palettesSection>"#;
        assert_eq!(
            err(MANIFEST, &s),
            "Rule pack section <palette> attribute 'ground' has unknown value 'Pink' \
             (expected one of White, Black, Green, Blue, Brown, Yellow, Red, Grey): pack.xml"
        );
    }

    #[test]
    fn a_json_file_is_not_a_folder_pack_but_any_other_file_is() {
        assert!(!is_folder_pack(Path::new("no-such-dir/UK.JSON")));
        assert!(is_folder_pack(Path::new("no-such-dir/pack.xml")));
        assert!(is_folder_pack(Path::new("no-such-dir/pack")));
        assert!(is_folder_pack(Path::new(env!("CARGO_MANIFEST_DIR"))));
    }

    #[test]
    fn the_manifest_is_resolved_and_its_section_files_checked() {
        let dir =
            std::env::temp_dir().join(format!("afrobim-signs-xml-pack-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let shown = |p: &Path| p.display().to_string();

        let e = load(&dir).err().unwrap().to_string();
        assert_eq!(
            e,
            format!(
                "Rule pack not found: {} (no pack.xml in directory {})",
                shown(&dir.join("pack.xml")),
                shown(&dir)
            )
        );
        let e = load(&dir.join("other.xml")).err().unwrap().to_string();
        assert_eq!(
            e,
            format!("Rule pack not found: {}", shown(&dir.join("other.xml")))
        );

        let manifest = dir.join("pack.xml");
        std::fs::write(
            &manifest,
            r#"<rulePack id="T"><sections><part file="a.xml"/></sections></rulePack>"#,
        )
        .unwrap();
        assert!(load(&dir)
            .err()
            .unwrap()
            .to_string()
            .contains("may only contain <section> elements, found '<part>'"));
        std::fs::write(
            &manifest,
            r#"<rulePack id="T"><sections><section/></sections></rulePack>"#,
        )
        .unwrap();
        assert!(load(&manifest)
            .err()
            .unwrap()
            .to_string()
            .contains("<section> has no 'file' attribute"));
        std::fs::write(
            &manifest,
            r#"<rulePack id="T"><sections><section file="a.xml"/></sections></rulePack>"#,
        )
        .unwrap();
        assert!(load(&manifest)
            .err()
            .unwrap()
            .to_string()
            .starts_with("Rule pack section file not found: "));

        // With a BOM, and loaded by folder.
        std::fs::write(dir.join("a.xml"), format!("\u{feff}{TEXT}")).unwrap();
        assert!(load(&dir)
            .err()
            .unwrap()
            .to_string()
            .starts_with("Rule pack is missing section 'panel': "));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
