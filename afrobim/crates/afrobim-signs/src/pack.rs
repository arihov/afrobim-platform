//! The rule pack: every dimension the layout uses, read from a pack JSON file.
//!
//! Port of the JSON half of C# `SignCore.Rules.RulePackLoader` and of the `RulePack` sections
//! the stack-type layout reads. Sections that only other sign families use (route arms,
//! map symbol, roundabout, pictograms, sign sizes, ...) are left unread. No value is defaulted
//! here: a length the layout needs comes from the pack or the pack fails to load.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::model::{colour_name, Colour, FontFace, RouteStatus};
use crate::SignError;

/// App.1 §2.1.1 Table 1. Text sizes by traffic speed band.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSize {
    /// Inclusive upper bound of the band; `None` = unbounded.
    pub speed_max_kph: Option<i64>,
    pub x_height_mm: f64,
    pub sw_mm: f64,
    pub cap_height_mm: f64,
}

/// App.1 §1.3 and §1.1 Figure 1. All values in sw. `tile_widths` is the standard's own table
/// of character tile widths, when the standard prints one (UK TSM Chapter 7 Appendix C); a
/// pack without it measures characters by the font's advances.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelRules {
    pub border_width: f64,
    pub corner_radius: f64,
    pub pad_top: f64,
    pub pad_side: f64,
    pub pad_bottom: f64,
    pub word_spacing: f64,
    pub text_tile_height: f64,
    #[serde(default)]
    pub tile_widths: Option<TileWidthRules>,
}

/// One alphabet's tile widths, in sw at a 4 sw x-height. `widths` is keyed by the characters
/// a tile holds: usually one, but a standard may print a multi-character tile (Appendix C's
/// feet mark `'-`), which takes precedence over its parts.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TileWidthTable {
    pub source: String,
    pub widths: BTreeMap<String, f64>,
    #[serde(default)]
    pub pair_rules: Option<Vec<TilePairRule>>,
}

impl TileWidthTable {
    /// Length in characters of the longest key.
    pub(crate) fn longest_key(&self) -> usize {
        self.widths
            .keys()
            .map(|k| k.chars().count())
            .max()
            .unwrap_or(1)
    }
}

/// A tile whose width depends on the next character: `character` is `width` sw wide when the
/// character after it in the same word is one of `followed_by` (Appendix C: T, V, Y before
/// a, e, g, o, r, u; W before T, V, X, Y, Z).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TilePairRule {
    pub character: String,
    pub followed_by: String,
    pub width: f64,
}

/// Tile widths per alphabet. Either may be absent.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TileWidthRules {
    #[serde(default)]
    pub transport_medium: Option<TileWidthTable>,
    #[serde(default)]
    pub transport_heavy: Option<TileWidthTable>,
}

/// A font face named by the standard, resolved from the user's own font directory.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontRef {
    pub face: String,
    pub file: String,
}

/// App.1 §1.2. Transport Medium on dark grounds, Transport Heavy on white.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontRules {
    pub on_dark: FontRef,
    pub on_light: FontRef,
}

/// App.1 §2.1.2.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintRules {
    pub max_destinations: usize,
    pub max_destinations_per_direction: usize,
    pub nearer_destination_above: bool,
    pub uniform_text_size: bool,
}

/// App.1 §2.6 Figure 5: the standard route/destination arrow. Lengths, widths and radii are
/// in sw; angles are in degrees.
///
/// `style` selects the outline. `"flared"` (the default, and the Uganda Figure 5 arrow) flares
/// the back of the head forward from the shaft. `"chevron"` is the UK TSM Chapter 7 Figure 4-4
/// arrow (TSRGD 2016 S12-5-1): the head flanks run back from the tip at `point_half_angle_deg`,
/// end in square barbs, and the back edges sweep forward to the shaft at `flare_angle_deg` to
/// the axis. For the chevron, `head_length` is tip to barb back corner and `short_shaft_length`
/// (when set) is the shaft used beside a single-line legend (Chapter 7 §4.2.2: 16 sw reduced
/// to 14 sw).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrowRules {
    pub shaft_length: f64,
    pub shaft_width: f64,
    pub head_length: f64,
    pub head_flare: f64,
    pub flare_angle_deg: f64,
    pub point_half_angle_deg: f64,
    pub inner_corner_radius: f64,
    pub tip_radius: f64,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub short_shaft_length: Option<f64>,
}

impl ArrowRules {
    pub fn is_chevron(&self) -> bool {
        self.style
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case("chevron"))
    }
}

/// App.1 §1.2 + App.3 scheme table: how one colour scheme resolves the panel ground,
/// lettering colour, route-arm colour and lettering face.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaletteEntry {
    pub scheme: String,
    pub ground: Colour,
    pub lettering: Colour,
    pub arm: Colour,
    pub face: FontFace,
}

/// Lookup of [`PaletteEntry`] rows keyed by scheme name.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaletteRules {
    pub entries: Vec<PaletteEntry>,
}

impl PaletteRules {
    pub fn find(&self, scheme: &str) -> Option<&PaletteEntry> {
        self.entries
            .iter()
            .find(|e| e.scheme.eq_ignore_ascii_case(scheme))
    }
}

/// A named vector symbol. `size_sw` is the nominal box side where the standard fixes one,
/// else 0 to mean "scale to host".
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolSpec {
    pub id: String,
    #[serde(default)]
    pub size_sw: f64,
}

/// Lookup of [`SymbolSpec`] rows keyed by symbol id.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolLibrary {
    pub symbols: Vec<SymbolSpec>,
}

impl SymbolLibrary {
    pub fn find(&self, id: &str) -> Option<&SymbolSpec> {
        self.symbols.iter().find(|s| s.id.eq_ignore_ascii_case(id))
    }
}

/// A registered sign page. Only the key and its colour scheme are read here.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignCatalogEntry {
    pub key: String,
    #[serde(default)]
    pub scheme: Option<String>,
}

/// Lookup of [`SignCatalogEntry`] rows keyed by sign-page key.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignCatalog {
    pub entries: Vec<SignCatalogEntry>,
}

impl SignCatalog {
    pub fn find(&self, key: &str) -> Option<&SignCatalogEntry> {
        self.entries
            .iter()
            .find(|e| e.key.eq_ignore_ascii_case(key))
    }
}

/// Panelled stack-type layout (UK TSM Chapter 7 §4.2–4.3, Figure 4-9). When a pack carries
/// this section each direction gets its own section of the sign, separated by `divider_width`
/// dividers, with one arrow per section centred vertically. All values in sw.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackRules {
    /// Divider between direction sections (§4.3.2: 1 sw).
    pub divider_width: f64,
    /// Arrow to the nearer side border (§4.3.5 b: 2.5 sw).
    pub arrow_to_border: f64,
    /// Arrow to its destination block (Figure 2-2: 2.5 sw).
    pub arrow_to_legend: f64,
    /// Minimum arrow gap to the section top and bottom (§4.2.1: 2.5 sw).
    pub arrow_clearance: f64,
    /// Where the arrow sets the section height, the legend's top gap exceeds its bottom gap
    /// by this much (§4.2.1, Figure 4-1 "H+1": 1 sw).
    pub legend_raise: f64,
}

/// Destination blocks with route numbers, route-number patches and panels (UK TSM Chapter 7
/// §3.2–3.3, §3.5–3.7, Figures 3-9, 3-11, 3-12, 3-16 and 3-17). Lengths in sw.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockRules {
    /// A patch's coloured area starts this far above its route-number tiles and ends at
    /// their foot (Figure 3-9: 1.5, giving the 9.5 sw patch of Figure 3-11).
    pub patch_rise: f64,
    /// Patch edge to route-number tiles (Figure 3-9: 1).
    pub patch_pad_side: f64,
    /// White border round a dark patch or panel on a dark ground (§3.3.1: 0.5).
    pub edge_border: f64,
    /// Between route letter(s) and number, and before "(M)" (Figure 3-9: 1).
    pub route_letter_gap: f64,
    /// Place name to its route number or patch on the same line (Figure 3-16: 2.5).
    pub name_to_route_number: f64,
    /// Panel inner edge to the first element (§3.3.1, Figure 3-9: 2.5).
    pub panel_pad_top: f64,
    /// Panel inner edge to the legend at the sides (Figure 3-9: 2.5).
    pub panel_pad_side: f64,
    /// Panel inner edge below a tiled last line (Figure 3-9: 1.5).
    pub panel_pad_bottom_tiles: f64,
    /// Panel inner edge below a patch or symbol (§3.3.1 with Figure 3-11: 2.5).
    pub panel_pad_bottom_patch: f64,
    /// Legend over an unpatched bracketed route number (§3.7.2: 0.5).
    pub line_spacing_bracketed: f64,
    /// Legend over a patch, no descender (§3.7.3: 0.5).
    pub line_spacing_patch: f64,
    /// Legend with a descender over a patch (§3.7.3: 1).
    pub line_spacing_patch_descender: f64,
    /// Tiles over tiles (Figure 3-17: 2).
    pub block_spacing_tiles: f64,
    /// Tiles over a panel, patch or symbol (Figure 3-17: 2.5).
    pub block_spacing_tiles_over_patch: f64,
    /// Panel, patch or symbol over tiles (Figure 3-17: 4).
    pub block_spacing_patch_over_tiles: f64,
    /// Panel, patch or symbol over another (Figure 3-17: 2.5).
    pub block_spacing_patch_over_patch: f64,
    /// Ground colour for each route status (§3.2.1), keyed `motorway`, `primary`, `nonPrimary`.
    pub status_colours: BTreeMap<String, Colour>,
    /// Route-number colour on each ground (§2.1.1: yellow on dark green).
    pub route_number_ink: BTreeMap<String, Colour>,
}

impl BlockRules {
    /// The ground colour for a route status, or `None` when the pack names none.
    pub fn colour_for(&self, status: RouteStatus) -> Option<Colour> {
        let key = match status {
            RouteStatus::Motorway => "motorway",
            RouteStatus::Primary => "primary",
            RouteStatus::NonPrimary => "nonPrimary",
            RouteStatus::Unspecified => return None,
        };
        self.status_colours
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| *v)
    }

    /// Route-number ink on a ground; falls back to `lettering`.
    pub fn ink_on(&self, ground: Colour, lettering: Colour) -> Colour {
        self.route_number_ink
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(colour_name(ground)))
            .map(|(_, v)| *v)
            .unwrap_or(lettering)
    }
}

/// Distance legends beside destinations. Value in sw.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistanceRules {
    /// Minimum horizontal gap between a place name and its distance (UK TSM Chapter 7
    /// §3.9.1: 7 sw).
    pub min_gap: f64,
}

/// The sections of a rule pack the stack-type layout reads.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulePack {
    pub id: String,
    pub name: String,
    pub source: String,
    pub text_sizes: Vec<TextSize>,
    pub panel: PanelRules,
    pub fonts: FontRules,
    pub constraints: ConstraintRules,
    #[serde(default)]
    pub arrows: Option<ArrowRules>,
    #[serde(default)]
    pub palettes: Option<PaletteRules>,
    #[serde(default)]
    pub symbols: Option<SymbolLibrary>,
    #[serde(default)]
    pub catalog: Option<SignCatalog>,
    #[serde(default)]
    pub stack: Option<StackRules>,
    #[serde(default)]
    pub distances: Option<DistanceRules>,
    #[serde(default)]
    pub blocks: Option<BlockRules>,
    /// `signBorder` and `routeArms`, read by the destination-column family (direction,
    /// confirmatory, place, lane and diversion signs).
    #[serde(flatten)]
    pub dir: crate::dir_rules::DirRules,
    #[serde(default)]
    pub text_sizes_by_family: Option<BTreeMap<String, Vec<TextSize>>>,
    #[serde(default)]
    pub clauses: Option<BTreeMap<String, String>>,
}

impl RulePack {
    /// The clause a diagnostic cites, looked up by `key` in the pack's `clauses` section so a
    /// UK sign never cites a Uganda clause. Falls back to `fallback` when the pack does not
    /// name one.
    pub fn clause<'a>(&'a self, key: &str, fallback: &'a str) -> &'a str {
        match self.clauses.as_ref().and_then(|c| c.get(key)) {
            Some(c) if !c.trim().is_empty() => c,
            _ => fallback,
        }
    }

    /// Loads a single-file JSON pack. `//` and `/* */` comments are skipped, as the C#
    /// loader skips them.
    pub fn load(path: &Path) -> Result<RulePack, SignError> {
        let shown = path.display();
        let text = std::fs::read_to_string(path)
            .map_err(|e| SignError::Pack(format!("Rule pack not found: {shown} ({e})")))?;
        Self::from_json(&text, &shown.to_string())
    }

    /// Parses pack JSON; `path` only names the pack in error messages.
    pub fn from_json(text: &str, path: &str) -> Result<RulePack, SignError> {
        let pack: RulePack = serde_json::from_str(&strip_json_comments(text))
            .map_err(|e| SignError::Pack(format!("Rule pack is not valid JSON: {path}: {e}")))?;
        if pack.id.trim().is_empty() {
            return Err(SignError::Pack(format!("Rule pack has no id: {path}")));
        }
        if pack.text_sizes.is_empty() {
            return Err(SignError::Pack(format!(
                "Rule pack has no textSizes: {path}"
            )));
        }
        pack.validate(path)?;
        Ok(pack)
    }

    /// The checks of C# `RulePackLoader.ValidateOptionalSections` for the sections read here,
    /// so a bad hand-authored pack fails at load rather than mid-layout.
    fn validate(&self, path: &str) -> Result<(), SignError> {
        let bad = |msg: String| Err(SignError::Pack(format!("{msg}: {path}")));

        if let Some(tiles) = &self.panel.tile_widths {
            for (name, table) in [
                ("transportMedium", &tiles.transport_medium),
                ("transportHeavy", &tiles.transport_heavy),
            ] {
                let Some(table) = table else { continue };
                let at = format!("panel.tileWidths.{name}");
                if table.source.trim().is_empty() {
                    return bad(format!("Rule pack '{at}' must cite its source"));
                }
                if table.widths.is_empty() {
                    return bad(format!("Rule pack '{at}.widths' is empty"));
                }
                for (key, w) in &table.widths {
                    if key.is_empty() || key.contains(' ') || w.is_nan() || *w <= 0.0 {
                        return bad(format!(
                            "Rule pack '{at}.widths' has a bad entry '{key}': {w}"
                        ));
                    }
                }
                for r in table.pair_rules.iter().flatten() {
                    if r.character.chars().count() != 1
                        || r.followed_by.is_empty()
                        || r.width.is_nan()
                        || r.width <= 0.0
                    {
                        return bad(format!(
                            "Rule pack '{at}.pairRules' has a bad rule for '{}'",
                            r.character
                        ));
                    }
                }
            }
        }

        if let Some(ar) = &self.arrows {
            if ar.shaft_length <= 0.0 || ar.head_length <= 0.0 || ar.head_flare <= 0.0 {
                return bad("Rule pack 'arrows' must have positive lengths".into());
            }
            if ar.shaft_width <= 0.0 {
                return bad("Rule pack 'arrows.shaftWidth' must be positive".into());
            }
            if ar.flare_angle_deg <= 0.0 || ar.flare_angle_deg >= 180.0 {
                return bad("Rule pack 'arrows.flareAngleDeg' must be in (0,180)".into());
            }
            if ar.point_half_angle_deg <= 0.0 || ar.point_half_angle_deg >= 90.0 {
                return bad("Rule pack 'arrows.pointHalfAngleDeg' must be in (0,90)".into());
            }
            if ar.inner_corner_radius < 0.0 || ar.tip_radius < 0.0 {
                return bad("Rule pack 'arrows' radii must be non-negative".into());
            }
            if let Some(style) = &ar.style {
                if !style.eq_ignore_ascii_case("flared") && !style.eq_ignore_ascii_case("chevron") {
                    return bad("Rule pack 'arrows.style' must be 'flared' or 'chevron'".into());
                }
            }
            if let Some(ss) = ar.short_shaft_length {
                if ss <= 0.0 || ss > ar.shaft_length {
                    return bad(
                        "Rule pack 'arrows.shortShaftLength' must be in (0, shaftLength]".into(),
                    );
                }
            }
            if ar.is_chevron() {
                // The flanks must reach the head's full half-width before the barb back corner,
                // or there is no square barb and the outline self-intersects.
                let half = ar.shaft_width / 2.0 + ar.head_flare;
                let flank = half / (ar.point_half_angle_deg * std::f64::consts::PI / 180.0).tan();
                if flank > ar.head_length + 1e-9 {
                    return bad(format!(
                        "Rule pack 'arrows': chevron flanks ({flank:.3} sw) exceed headLength"
                    ));
                }
                if ar.flare_angle_deg >= 90.0 {
                    return bad(
                        "Rule pack 'arrows.flareAngleDeg' must be below 90 for a chevron".into(),
                    );
                }
            }
        }

        if let Some(st) = &self.stack {
            if st.divider_width <= 0.0
                || st.arrow_to_border < 0.0
                || st.arrow_to_legend < 0.0
                || st.arrow_clearance < 0.0
                || st.legend_raise < 0.0
            {
                return bad(
                    "Rule pack 'stack' values must be non-negative (divider positive)".into(),
                );
            }
            if self.arrows.is_none() {
                return bad("Rule pack 'stack' needs an 'arrows' section".into());
            }
        }

        if let Some(b) = &self.blocks {
            let lengths = [
                b.patch_rise,
                b.patch_pad_side,
                b.edge_border,
                b.route_letter_gap,
                b.name_to_route_number,
                b.panel_pad_top,
                b.panel_pad_side,
                b.panel_pad_bottom_tiles,
                b.panel_pad_bottom_patch,
                b.line_spacing_bracketed,
                b.line_spacing_patch,
                b.line_spacing_patch_descender,
                b.block_spacing_tiles,
                b.block_spacing_tiles_over_patch,
                b.block_spacing_patch_over_tiles,
                b.block_spacing_patch_over_patch,
            ];
            if lengths.iter().any(|v| *v < 0.0) {
                return bad("Rule pack 'blocks' lengths must be non-negative".into());
            }
            for key in b.status_colours.keys() {
                if !["motorway", "primary", "nonPrimary"]
                    .iter()
                    .any(|k| k.eq_ignore_ascii_case(key))
                {
                    return bad(format!(
                        "Rule pack 'blocks.statusColours' has unknown status '{key}'"
                    ));
                }
            }
        }

        if let Some(d) = &self.distances {
            if d.min_gap < 0.0 {
                return bad("Rule pack 'distances.minGap' must be non-negative".into());
            }
        }
        self.dir.validate(&self.panel, path)?;

        for (family, sizes) in self.text_sizes_by_family.iter().flatten() {
            if sizes.is_empty() {
                return bad(format!(
                    "Rule pack 'textSizesByFamily.{family}' has no text sizes"
                ));
            }
        }

        if let Some(pal) = &self.palettes {
            for (i, e) in pal.entries.iter().enumerate() {
                if e.scheme.is_empty()
                    || pal.entries[..i]
                        .iter()
                        .any(|o| o.scheme.eq_ignore_ascii_case(&e.scheme))
                {
                    return bad(format!(
                        "Rule pack 'palettes' has duplicate or empty scheme '{}'",
                        e.scheme
                    ));
                }
            }
        }

        if let Some(sym) = &self.symbols {
            for (i, s) in sym.symbols.iter().enumerate() {
                if s.id.is_empty()
                    || sym.symbols[..i]
                        .iter()
                        .any(|o| o.id.eq_ignore_ascii_case(&s.id))
                {
                    return bad(format!(
                        "Rule pack 'symbols' has duplicate or empty id '{}'",
                        s.id
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Blanks `//` line comments and `/* */` block comments outside strings, keeping every
/// newline so a JSON error still reports the right line.
fn strip_json_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                }
                '"' => in_string = false,
                _ => {}
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == '/' && chars.peek() == Some(&'/') {
            for n in chars.by_ref() {
                if n == '\n' {
                    out.push('\n');
                    break;
                }
            }
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut prev = ' ';
            for n in chars.by_ref() {
                if n == '\n' {
                    out.push('\n');
                }
                if prev == '*' && n == '/' {
                    break;
                }
                prev = n;
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ─── text size selection (C# TextSizeSelector) ────────────────────────────────

/// App.1 §2.1.1 Table 1. Bands are inclusive upper bounds; `None` = unbounded.
pub fn text_size_for_speed(pack: &RulePack, speed_kph: u32) -> Result<&TextSize, SignError> {
    pick(&pack.text_sizes, speed_kph)
}

/// Text size for a sign of a given layout family. A pack may give some families their own
/// table in `textSizesByFamily` (UK TSM Chapter 7 Appendix E sizes direction and route
/// confirmatory signs smaller than advance direction signs at the same speed); families
/// without one use the pack-wide `textSizes`.
pub fn text_size_for_family<'a>(
    pack: &'a RulePack,
    family: &str,
    speed_kph: u32,
) -> Result<&'a TextSize, SignError> {
    if !family.is_empty() {
        for (key, sizes) in pack.text_sizes_by_family.iter().flatten() {
            if key.eq_ignore_ascii_case(family) {
                return pick(sizes, speed_kph);
            }
        }
    }
    text_size_for_speed(pack, speed_kph)
}

fn pick(sizes: &[TextSize], speed_kph: u32) -> Result<&TextSize, SignError> {
    let mut ordered: Vec<&TextSize> = sizes.iter().collect();
    ordered.sort_by_key(|t| t.speed_max_kph.unwrap_or(i64::MAX)); // stable, as OrderBy
    ordered
        .into_iter()
        .find(|t| {
            t.speed_max_kph
                .is_none_or(|max| i64::from(speed_kph) <= max)
        })
        .ok_or_else(|| SignError::Pack(format!("No text size band covers {speed_kph} km/h")))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn uk_pack() -> RulePack {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../packs/UK-TSM-2019.json");
        RulePack::load(&path).expect("UK pack loads")
    }

    #[test]
    fn comments_are_stripped_outside_strings_only() {
        let s = strip_json_comments("{ // c\n \"a\": \"x // y\", /* b\n */ \"b\": 1 }");
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["a"], "x // y");
        assert_eq!(v["b"], 1);
        assert_eq!(s.matches('\n').count(), 2);
    }

    #[test]
    fn uk_pack_loads_the_stack_sections() {
        let p = uk_pack();
        assert_eq!(p.id, "UK-TSM-2019");
        assert!(p.arrows.as_ref().unwrap().is_chevron());
        assert_eq!(p.stack.as_ref().unwrap().divider_width, 1.0);
        let b = p.blocks.as_ref().unwrap();
        assert_eq!(b.colour_for(RouteStatus::Motorway), Some(Colour::Blue));
        assert_eq!(b.colour_for(RouteStatus::Unspecified), None);
        assert_eq!(b.ink_on(Colour::Green, Colour::White), Colour::Yellow);
        assert_eq!(b.ink_on(Colour::Yellow, Colour::Black), Colour::Black);
        assert_eq!(p.clause("constraints", "x"), "UK TSM Chapter 7 Appendix D");
        assert_eq!(p.clause("no-such-key", "x"), "x");
        let medium = p
            .panel
            .tile_widths
            .as_ref()
            .unwrap()
            .transport_medium
            .as_ref()
            .unwrap();
        assert_eq!(medium.widths["K"], 5.52);
        assert_eq!(medium.longest_key(), 2);
    }

    /// UK TSM Chapter 7 Appendix E (advance direction signs), bands converted to km/h.
    #[test]
    fn x_height_follows_the_speed_band() {
        let p = uk_pack();
        let at = |kph| {
            let t = text_size_for_family(&p, "stack", kph).unwrap();
            (t.x_height_mm, t.sw_mm)
        };
        assert_eq!(at(30), (75.0, 18.75));
        assert_eq!(at(32), (75.0, 18.75)); // the bound is inclusive
        assert_eq!(at(33), (100.0, 25.0));
        assert_eq!(at(60), (125.0, 31.25));
        assert_eq!(at(100), (250.0, 62.5));
        assert_eq!(at(200), (300.0, 75.0)); // the open top band
    }

    #[test]
    fn a_family_table_wins_over_the_pack_wide_one() {
        let p = uk_pack();
        assert_eq!(
            text_size_for_family(&p, "Direction", 60)
                .unwrap()
                .x_height_mm,
            100.0
        );
        assert_eq!(
            text_size_for_family(&p, "stack", 60).unwrap().x_height_mm,
            125.0
        );
    }

    #[test]
    fn a_speed_no_band_covers_is_an_error() {
        let mut p = uk_pack();
        p.text_sizes.retain(|t| t.speed_max_kph.is_some());
        assert!(matches!(
            text_size_for_speed(&p, 500),
            Err(SignError::Pack(_))
        ));
    }

    #[test]
    fn a_stack_section_without_arrows_fails_to_load() {
        let text = r#"{"id":"X","name":"X","source":"X",
            "textSizes":[{"speedMaxKph":null,"xHeightMm":100,"swMm":25,"capHeightMm":140}],
            "panel":{"borderWidth":1.5,"cornerRadius":1.5,"padTop":2.5,"padSide":2.5,"padBottom":1.5,"wordSpacing":2.5,"textTileHeight":8},
            "fonts":{"onDark":{"face":"m","file":"m.ttf"},"onLight":{"face":"h","file":"h.ttf"}},
            "constraints":{"maxDestinations":6,"maxDestinationsPerDirection":6,"nearerDestinationAbove":true,"uniformTextSize":true},
            "stack":{"dividerWidth":1,"arrowToBorder":2.5,"arrowToLegend":2.5,"arrowClearance":2.5,"legendRaise":1}}"#;
        let err = RulePack::from_json(text, "x.json").unwrap_err();
        assert!(
            err.to_string().contains("needs an 'arrows' section"),
            "{err}"
        );
    }
}
