//! UK warning and regulatory signs drawn from TSRGD 2016 pack data, and the sign with its
//! associated plate. Port of C# `TsrgdSign`.
//!
//! A sign is its body (the pack's `signBodies`), the pictogram read from the TSRGD vector
//! diagram (`pictograms`) and its size by speed band (the catalog entry's `sizes` with
//! `signSizes`). The symbol is the TSRGD diagram number, optionally followed by `:` and the
//! numerals of a diagram that lets them vary (`670:30`).
//!
//! Geometry is in sw with the nominal dimension (triangle and octagon height, disc diameter)
//! [`NOMINAL_SW`] long, so the stroke width of a sign is its size in mm divided by
//! [`NOMINAL_SW`]. The body's foot is on y = 0 and its left extreme on x = 0.

use std::f64::consts::PI;

use afrobim_draw::Pt2;

use crate::measurer::TextTileMeasurer;
use crate::model::{
    Colour, ColourScheme, Diagnostic, FontFace, SignClass, SignDefinition, SignKind,
};
use crate::pack::RulePack;
use crate::prim::{Prim, TextRun};
use crate::std_pack::{
    colour_named, PackCatalogEntry, Pictogram, PlateLegendRule, SignBody, StdRules,
};
use crate::std_plate::{legend_lines, SupplementaryPlateLayout};
use crate::std_signs::StdLayout;
use crate::SignError;

/// The nominal dimension of a sign body in sw (a drawing unit, not a manual value).
pub(crate) const NOMINAL_SW: f64 = 50.0;

/// Chords per 120° rounded triangle corner (a drawing setting: within 0.4% of the radius).
const CORNER_SEGMENTS: u32 = 12;

/// Chords round a disc (a drawing setting).
const DISC_SEGMENTS: u32 = 96;

/// A sign resolved against the pack: its pictogram, body, catalog entry and chosen numerals.
#[derive(Debug, Clone)]
pub(crate) struct Art<'a> {
    pub pictogram: &'a Pictogram,
    pub body: &'a SignBody,
    pub entry: Option<&'a PackCatalogEntry>,
    pub numerals: Option<String>,
}

pub(crate) fn is_warning_or_regulatory(kind: SignKind) -> bool {
    matches!(
        kind,
        SignKind::WarningSign
            | SignKind::RegulatoryProhibitory
            | SignKind::RegulatoryMandatory
            | SignKind::Stop
            | SignKind::GiveWay
    )
}

/// The TSRGD artwork for a warning or regulatory sign, or `None` when the pack has none.
pub(crate) fn resolve<'a>(std: &'a StdRules, sign: &SignDefinition) -> Option<Art<'a>> {
    if !is_warning_or_regulatory(sign.kind) {
        return None;
    }
    let pics = std.pictograms.as_ref()?;
    let bodies = std.sign_bodies.as_ref()?;
    let symbol = sign.symbol_id.as_deref().filter(|s| !s.is_empty())?;

    let (diagram, numerals) = match symbol.split_once(':') {
        Some((d, n)) => (d, Some(n)),
        None => (symbol, None),
    };
    let pictogram = pics.find(diagram.trim())?;
    let body = bodies.get(&pictogram.body)?;

    // The entry the sign names, when it is this diagram's; otherwise the diagram's first entry.
    let names_diagram = |e: &&PackCatalogEntry| {
        e.diagram
            .as_deref()
            .is_some_and(|d| d.eq_ignore_ascii_case(&pictogram.id))
    };
    let entry = sign
        .catalog_key
        .as_deref()
        .and_then(|key| std.find_entry(key))
        .filter(names_diagram)
        .or_else(|| {
            std.catalog
                .iter()
                .flat_map(|c| &c.entries)
                .find(names_diagram)
        });
    let numerals = numerals
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string);
    Some(Art {
        pictogram,
        body,
        entry,
        numerals,
    })
}

/// The sign's size in mm for its speed band, or `None` when the size table gives none.
pub(crate) fn size_mm(std: &StdRules, sign: &SignDefinition, art: &Art) -> Option<f64> {
    let sizes = std.sign_sizes.as_ref()?;
    let s = art.entry?.sizes.as_ref()?;
    s.by_band_mm
        .get(sizes.band_index(sign.speed_kph))
        .copied()
        .flatten()
}

/// The stroke width (mm) that draws the sign at its tabled size, or `None`.
pub(crate) fn sw_mm(std: &StdRules, sign: &SignDefinition) -> Option<f64> {
    let art = resolve(std, sign)?;
    size_mm(std, sign, &art).map(|mm| mm / NOMINAL_SW)
}

/// The colour a pictogram path or numeral names: a colour, or the sign's ground.
fn colour_or_ground(name: &str, ground: Colour) -> Result<Colour, SignError> {
    if name == "Ground" {
        return Ok(ground);
    }
    colour_named(name)
        .ok_or_else(|| SignError::Pack(format!("Rule pack pictogram colour '{name}' is unknown")))
}

// ─── the sign ─────────────────────────────────────────────────────────────────

/// Draws the body, then the pictogram and any numerals. Raises `SIGN_SIZE_UNSPECIFIED` when the
/// size table gives no size for the sign's speed band.
pub(crate) fn layout(
    pack: &RulePack,
    std: &StdRules,
    measurer: &TextTileMeasurer,
    sign: &SignDefinition,
    art: &Art,
    mut diags: Vec<Diagnostic>,
) -> Result<StdLayout, SignError> {
    let body = art.body;
    let temporary = sign.scheme == ColourScheme::Temporary
        || (sign.scheme == ColourScheme::Auto && sign.background == Colour::Yellow);
    // A temporary sign changes its white ground to yellow; the border and symbol stay
    // (TSM Chapter 4 §1.44).
    let ground = if temporary && body.ground == Colour::White {
        Colour::Yellow
    } else {
        body.ground
    };

    let h = NOMINAL_SW;
    let (outline, inner, width) = outlines(body, h);
    let cx = width / 2.0;

    let mut prims = vec![
        Prim::Fill {
            outline,
            colour: body.border,
        },
        Prim::Fill {
            outline: inner.clone(),
            colour: ground,
        },
    ];

    // The pictogram's coloured shapes are the sign's symbols for the overlap check; shapes in
    // the ground colour are cut-outs inside them.
    let mut symbols = Vec::new();
    for path in &art.pictogram.paths {
        let colour = colour_or_ground(&path.colour, ground)?;
        let shape: Vec<Pt2> = path
            .points
            .iter()
            .map(|p| Pt2::new(cx + p[0] * h, p[1] * h))
            .collect();
        if path.colour != "Ground" {
            symbols.push(shape.clone());
        }
        prims.push(Prim::Fill {
            outline: shape,
            colour,
        });
    }

    if let Some(n) = &art.pictogram.numeral {
        let text = art.numerals.as_deref().unwrap_or(&n.text);
        let cap = n.height * h;
        let xh = cap / TextRun::CAP_HEIGHT_PER_X_HEIGHT;
        let w = measurer.width(text, FontFace::TransportHeavy)? * (xh / 4.0);
        let baseline = Pt2::new(cx + n.centre_x * h - w / 2.0, n.centre_y * h - cap / 2.0);
        let ink = colour_or_ground(&n.colour, ground)?;
        prims.push(Prim::Text(TextRun::from_baseline(
            baseline,
            text,
            FontFace::TransportHeavy,
            cap,
            Some(ink),
        )));
    }

    if let Some(sizes) = art.entry.and_then(|e| e.sizes.as_ref()) {
        if size_mm(std, sign, art).is_none() {
            let permitted: Vec<String> = sizes
                .permitted_mm
                .iter()
                .flatten()
                .map(f64::to_string)
                .collect();
            diags.push(Diagnostic::warning(
                "SIGN_SIZE_UNSPECIFIED",
                format!(
                    "Diagram {}: {} gives no size at {} km/h; choose one of the sizes TSRGD permits ({} mm)",
                    art.pictogram.id,
                    sizes.source,
                    sign.speed_kph,
                    permitted.join(", ")
                ),
                pack.clause("signSize", &sizes.source),
            ));
        }
    }

    Ok(StdLayout {
        prims,
        diagnostics: diags,
        width,
        height: h,
        symbols,
        regions: vec![inner],
    })
}

/// The body's outer outline, the inner edge of its border (the ground) and its width.
fn outlines(body: &SignBody, h: f64) -> (Vec<Pt2>, Vec<Pt2>, f64) {
    let b = body.border_width * h;
    match body.shape.as_str() {
        "triangle-up" | "triangle-down" => {
            let down = body.shape == "triangle-down";
            let r = body.corner_radius * h;
            let sqrt3 = 3f64.sqrt();
            // The sharp triangle whose corners are rounded: its apex rises r above the rounded
            // outline (a 60° corner of radius r is cut back by r/sin 30° − r = r), its base
            // does not.
            let hs = h + r;
            let side = hs * 2.0 / sqrt3;
            let width = side - 2.0 * (sqrt3 - 1.0) * r;
            let cx = width / 2.0;
            let sharp = [(cx - side / 2.0, 0.0), (cx + side / 2.0, 0.0), (cx, hs)];
            let gy = hs / 3.0; // centroid
            let mut outer = Vec::new();
            // Each corner's arc runs between the outward normals of its two edges.
            for ((vx, vy), a0) in sharp.into_iter().zip([150.0, 270.0, 30.0]) {
                let (dx, dy) = (cx - vx, gy - vy);
                let len = (dx * dx + dy * dy).sqrt();
                // The arc's centre is 2r from the vertex.
                let (ccx, ccy) = (vx + dx / len * 2.0 * r, vy + dy / len * 2.0 * r);
                for k in 0..=CORNER_SEGMENTS {
                    let t = (a0 + 120.0 * f64::from(k) / f64::from(CORNER_SEGMENTS)) * PI / 180.0;
                    outer.push(Pt2::new(ccx + r * t.cos(), ccy + r * t.sin()));
                }
            }
            // The ground: the sharp triangle whose edges are b inside the outer edges.
            let mut inner = vec![
                Pt2::new(cx - (side / 2.0 - b * sqrt3), b),
                Pt2::new(cx + (side / 2.0 - b * sqrt3), b),
                Pt2::new(cx, hs - 2.0 * b),
            ];
            if down {
                let flip = |pts: &mut Vec<Pt2>| {
                    pts.iter_mut().for_each(|p| p.y = h - p.y);
                    pts.reverse();
                };
                flip(&mut outer);
                flip(&mut inner);
            }
            (outer, inner, width)
        }
        // Regular octagon, flat top and bottom, h across the flats.
        "octagon" => (
            octagon(h / 2.0, h / 2.0, h / 2.0),
            octagon(h / 2.0, h / 2.0, h / 2.0 - b),
            h,
        ),
        // circle
        _ => (
            disc(h / 2.0, h / 2.0, h / 2.0),
            disc(h / 2.0, h / 2.0, h / 2.0 - b),
            h,
        ),
    }
}

fn octagon(cx: f64, cy: f64, apothem: f64) -> Vec<Pt2> {
    let rc = apothem / (PI / 8.0).cos();
    // The flats are snapped exactly (cos/sin round-off would tilt them by 1e-15).
    let snap = |y: f64| {
        if (y - (cy + apothem)).abs() < 1e-9 {
            cy + apothem
        } else if (y - (cy - apothem)).abs() < 1e-9 {
            cy - apothem
        } else {
            y
        }
    };
    (0..8)
        .map(|i| {
            let t = (22.5 + 45.0 * f64::from(i)) * PI / 180.0;
            Pt2::new(cx + rc * t.cos(), snap(cy + rc * t.sin()))
        })
        .collect()
}

fn disc(cx: f64, cy: f64, r: f64) -> Vec<Pt2> {
    (0..DISC_SEGMENTS)
        .map(|i| {
            let t = 2.0 * PI * f64::from(i) / f64::from(DISC_SEGMENTS);
            Pt2::new(cx + r * t.cos(), cy + r * t.sin())
        })
        .collect()
}

// ─── the sign with its plate ──────────────────────────────────────────────────

/// The plate x-height (mm) the pack tables for `row` under a sign of `sign_mm` at the sign's
/// speed: by sign size (TSM Chapter 4 Appendix B, the smaller value up to the low-speed limit)
/// or by speed band (TSM Chapter 3 Appendix A).
fn plate_x_height(
    std: &StdRules,
    low_speed_max_kph: Option<i64>,
    row: &PlateLegendRule,
    sign_mm: f64,
    speed_kph: u32,
) -> Option<f64> {
    if let Some(by_size) = &row.x_height_by_sign_size_mm {
        let key = sign_mm.to_string();
        let low = low_speed_max_kph
            .filter(|slow| i64::from(speed_kph) <= *slow)
            .and_then(|_| row.low_speed_x_height_by_sign_size_mm.as_ref()?.get(&key));
        low.or_else(|| by_size.get(&key)).copied()
    } else if let Some(by_band) = &row.x_height_by_band_mm {
        let band = std.sign_sizes.as_ref()?.band_index(speed_kph);
        by_band.get(band).copied().flatten()
    } else {
        None
    }
}

/// Adds the associated plate (legend [`SignDefinition::info_distance`], lines split on `|`)
/// below the sign as one layout. The plate is the supplementary-plate engine's plate at the
/// x-height TSM gives for that legend with that sign: the pack's plate row whose pattern
/// matches the legend (TSM Chapter 4 Appendix B by triangle height for a warning sign, Chapter
/// 3 Appendix A by speed band for a regulatory sign). A legend no row covers is not drawn
/// (`PLATE_SIZE_UNSPECIFIED`). The plate is scaled into the sign's sw, centred under the sign,
/// and set the pack's gap below it (butted, with `PLATE_GAP_UNSPECIFIED`, when no gap is
/// stated).
pub(crate) fn with_plate(
    pack: &RulePack,
    std: &StdRules,
    measurer: &TextTileMeasurer,
    sign: &SignDefinition,
    art: &Art,
    mut sign_result: StdLayout,
) -> Result<StdLayout, SignError> {
    let Some(info_distance) = sign
        .info_distance
        .as_deref()
        .filter(|l| !l.trim().is_empty())
    else {
        return Ok(sign_result);
    };
    let Some(sizes) = &std.sign_sizes else {
        return Ok(sign_result);
    };

    let warning = sign.kind == SignKind::WarningSign;
    let plates = if warning {
        sizes.warning_plate.as_ref()
    } else {
        sizes.regulatory_plate.as_ref()
    };
    let sign_mm = size_mm(std, sign, art);
    let legend = legend_lines(info_distance).join(" ");
    let mut row = None;
    for r in plates.iter().flat_map(|p| &p.legends) {
        if r.matches(&legend)? {
            row = Some(r);
            break;
        }
    }
    let source = plates.map_or(pack.source.as_str(), |p| p.source.as_str());

    let x_height = match (row, sign_mm) {
        (Some(row), Some(mm)) => plate_x_height(
            std,
            plates.and_then(|p| p.low_speed_max_kph),
            row,
            mm,
            sign.speed_kph,
        ),
        _ => None,
    };

    let (Some(plates), Some(sign_mm), Some(plate_xh)) = (plates, sign_mm, x_height) else {
        let why = match (sign_mm, row) {
            (None, _) => "the sign has no tabled size".to_string(),
            (Some(_), None) => format!("no row of {source} covers the legend"),
            (Some(mm), Some(row)) => format!(
                "{source} row {} gives no x-height for a {mm} mm sign at {} km/h",
                row.diagram, sign.speed_kph
            ),
        };
        sign_result.diagnostics.push(Diagnostic::warning(
            "PLATE_SIZE_UNSPECIFIED",
            format!(
                "Plate \"{legend}\" under diagram {} not drawn: {why}",
                art.pictogram.id
            ),
            pack.clause(if warning { "warning" } else { "regulatory" }, source),
        ));
        return Ok(sign_result);
    };

    let gap_source = plates.gap_source.as_deref().unwrap_or(&plates.source);

    let plate_sign = SignDefinition {
        code: format!("{}-plate", sign.code),
        kind: SignKind::SupplementaryPlate,
        destinations: Vec::new(),
        background: if sign.scheme == ColourScheme::Temporary {
            Colour::Yellow
        } else {
            Colour::White
        },
        class: SignClass::Supplementary,
        symbol_id: Some("text".to_string()),
        info_distance: Some(info_distance.to_string()),
        ..blank_definition(sign)
    };
    let plate = SupplementaryPlateLayout {
        pack,
        std,
        measurer,
    }
    .layout(&plate_sign)?;

    let mut diags = sign_result.diagnostics;
    let sw = sign_mm / NOMINAL_SW;
    let k2 = plate_xh / 4.0 / sw; // plate sw (x-height / 4) in the sign's sw
    if plates.gap_mm.is_none() {
        diags.push(Diagnostic::warning(
            "PLATE_GAP_UNSPECIFIED",
            format!(
                "No gap between sign and plate is stated ({gap_source}); the plate is butted to the sign"
            ),
            pack.clause("plateGap", gap_source),
        ));
    }
    let gap_sw = plates.gap_mm.unwrap_or(0.0) / sw;

    let (sign_w, sign_h) = (sign_result.width, sign_result.height);
    let (plate_w, plate_h) = (plate.width * k2, plate.height * k2);
    let width = sign_w.max(plate_w);
    let (sign_dx, sign_dy) = ((width - sign_w) / 2.0, plate_h + gap_sw);
    let plate_dx = (width - plate_w) / 2.0;

    let mut prims: Vec<Prim> = sign_result
        .prims
        .into_iter()
        .map(|p| transform(p, 1.0, sign_dx, sign_dy))
        .collect();
    prims.extend(
        plate
            .prims
            .into_iter()
            .map(|p| transform(p, k2, plate_dx, 0.0)),
    );
    diags.extend(plate.diagnostics);

    // Regions for the overlap check: the sign's inner edge, and the plate inside its border.
    let moved = |outline: Vec<Pt2>| -> Vec<Pt2> {
        outline
            .into_iter()
            .map(|q| Pt2::new(q.x + sign_dx, q.y + sign_dy))
            .collect()
    };
    let (pb, pw, ph) = (pack.panel.border_width, plate.width, plate.height);
    let plate_inside: Vec<Pt2> = [(pb, pb), (pw - pb, pb), (pw - pb, ph - pb), (pb, ph - pb)]
        .into_iter()
        .map(|(x, y)| Pt2::new(x * k2 + plate_dx, y * k2))
        .collect();
    let mut regions: Vec<Vec<Pt2>> = sign_result.regions.into_iter().map(moved).collect();
    regions.push(plate_inside);

    Ok(StdLayout {
        prims,
        diagnostics: diags,
        width,
        height: sign_dy + sign_h,
        symbols: sign_result.symbols.into_iter().map(moved).collect(),
        regions,
    })
}

/// A definition with the sign's code, speed, approach class and scheme and every other field
/// at the C# `SignDefinition` record's default.
fn blank_definition(sign: &SignDefinition) -> SignDefinition {
    SignDefinition {
        approach_class: sign.approach_class,
        scheme: sign.scheme,
        ..SignDefinition::stack(&sign.code, sign.speed_kph, Vec::new())
    }
}

/// Scales a primitive by `k` about the origin, then moves it.
fn transform(p: Prim, k: f64, dx: f64, dy: f64) -> Prim {
    let t = |q: Pt2| Pt2::new(q.x * k + dx, q.y * k + dy);
    match p {
        Prim::Fill { outline, colour } => Prim::Fill {
            outline: outline.into_iter().map(t).collect(),
            colour,
        },
        Prim::Line {
            from,
            to,
            width,
            colour,
        } => Prim::Line {
            from: t(from),
            to: t(to),
            width: width * k,
            colour,
        },
        Prim::Text(run) => Prim::Text(TextRun {
            origin: t(run.origin),
            x_height: run.x_height * k,
            ..run
        }),
        Prim::Symbol {
            origin,
            id,
            size,
            rotation_deg,
        } => Prim::Symbol {
            origin: t(origin),
            id,
            size: size * k,
            rotation_deg,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::std_pack::tests::uk_std;

    fn sign(kind: SignKind, symbol: &str, speed_kph: u32) -> SignDefinition {
        SignDefinition {
            kind,
            symbol_id: Some(symbol.to_string()),
            background: Colour::White,
            ..SignDefinition::stack("T", speed_kph, Vec::new())
        }
    }

    fn uk() -> StdRules {
        uk_std(&crate::pack::tests::uk_pack())
    }

    fn bounds(o: &[Pt2]) -> (f64, f64, f64, f64) {
        o.iter()
            .fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |b, p| {
                (b.0.min(p.x), b.1.min(p.y), b.2.max(p.x), b.3.max(p.y))
            })
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn a_symbol_resolves_to_its_diagram_body_entry_and_numerals() {
        let std = uk();
        let art = resolve(&std, &sign(SignKind::WarningSign, "512", 60)).unwrap();
        assert_eq!(art.pictogram.id, "512");
        assert_eq!(art.body.shape, "triangle-up");
        assert_eq!(art.entry.unwrap().key, "D512");
        assert_eq!(art.numerals, None);

        let limit = resolve(
            &std,
            &sign(SignKind::RegulatoryProhibitory, " 670 : 30 ", 60),
        )
        .unwrap();
        assert_eq!(limit.pictogram.id, "670");
        assert_eq!(limit.numerals.as_deref(), Some("30"));
        let plain = resolve(&std, &sign(SignKind::RegulatoryProhibitory, "670:", 60)).unwrap();
        assert_eq!(plain.numerals, None);

        // Not a warning or regulatory kind, no symbol, or no such diagram: no artwork.
        assert!(resolve(&std, &sign(SignKind::StackType, "512", 60)).is_none());
        assert!(resolve(&std, &sign(SignKind::WarningSign, "", 60)).is_none());
        assert!(resolve(&std, &sign(SignKind::WarningSign, "hump", 60)).is_none());
        assert!(resolve(
            &StdRules::default(),
            &sign(SignKind::WarningSign, "512", 60)
        )
        .is_none());
    }

    /// A catalog key of another diagram does not lend the sign its sizes.
    #[test]
    fn the_entry_is_the_one_that_names_the_diagram() {
        let std = uk();
        let mut s = sign(SignKind::RegulatoryProhibitory, "619", 60);
        s.catalog_key = Some("D616".into());
        assert_eq!(resolve(&std, &s).unwrap().entry.unwrap().key, "D619");
        s.catalog_key = Some("d619".into());
        assert_eq!(resolve(&std, &s).unwrap().entry.unwrap().key, "D619");
    }

    /// TSM Chapter 4 Appendix A column 2: 600 mm up to 30 mph, then 750, 900 and 1200 mm.
    #[test]
    fn warning_sign_height_follows_chapter_4_appendix_a() {
        let std = uk();
        for (kph, mm) in [
            (32, 600.0),
            (48, 600.0),
            (64, 750.0),
            (80, 900.0),
            (96, 1200.0),
            (120, 1200.0),
        ] {
            let s = sign(SignKind::WarningSign, "504.1", kph);
            let art = resolve(&std, &s).unwrap();
            assert_eq!(size_mm(&std, &s, &art), Some(mm), "{kph} km/h");
            assert_eq!(sw_mm(&std, &s), Some(mm / 50.0));
        }
    }

    /// TSM Chapter 3 Table 14-1 gives the speed limit sign no size over 60 mph.
    #[test]
    fn a_band_with_no_tabled_size_has_no_stroke_width() {
        let std = uk();
        assert_eq!(
            sw_mm(&std, &sign(SignKind::RegulatoryProhibitory, "670", 96)),
            Some(15.0)
        );
        assert_eq!(
            sw_mm(&std, &sign(SignKind::RegulatoryProhibitory, "670", 120)),
            None
        );
        assert_eq!(sw_mm(&std, &sign(SignKind::WarningSign, "hump", 60)), None);
    }

    /// The triangle body: 50 sw high with 3.75 sw (0.075) rounded outside corners and a sharp
    /// white ground 5 sw (0.1) inside the red edges.
    #[test]
    fn the_triangle_is_rounded_outside_and_sharp_inside() {
        let std = uk();
        let body = &std.sign_bodies.as_ref().unwrap()["triangle"];
        let (outer, inner, width) = outlines(body, 50.0);
        assert_eq!(outer.len(), 39);
        let (x0, y0, x1, y1) = bounds(&outer);
        assert!(close(x0, 0.0) && close(y0, 0.0) && close(x1, width) && close(y1, 50.0));
        // Width of an equilateral triangle of height 53.75 less the two rounded base corners.
        assert!(close(
            width,
            53.75 * 2.0 / 3f64.sqrt() - 2.0 * (3f64.sqrt() - 1.0) * 3.75
        ));
        assert_eq!(inner.len(), 3);
        assert!(close(inner[0].y, 5.0) && close(inner[1].y, 5.0));
        assert!(close(inner[2].x, width / 2.0) && close(inner[2].y, 53.75 - 10.0));
        // The ground's left edge is 5 sw inside the body's left edge, measured square to it.
        let left_edge_gap = (inner[0].x - (width / 2.0 - 53.75 / 3f64.sqrt())) * 3f64.sqrt() / 2.0
            - inner[0].y / 2.0;
        assert!(close(left_edge_gap, 5.0), "{left_edge_gap}");
    }

    #[test]
    fn give_way_points_down() {
        let std = uk();
        let body = &std.sign_bodies.as_ref().unwrap()["give-way"];
        let (outer, inner, width) = outlines(body, 50.0);
        let (x0, y0, x1, y1) = bounds(&outer);
        assert!(close(x0, 0.0) && close(y0, 0.0) && close(x1, width) && close(y1, 50.0));
        // The apex of the ground is at the bottom, the base 5 sw below the top.
        assert!(close(inner[0].y, 50.0 - (53.75 - 10.0)) && close(inner[0].x, width / 2.0));
        assert!(close(inner[1].y, 45.0) && close(inner[2].y, 45.0));
    }

    #[test]
    fn the_octagon_is_flat_topped_and_the_disc_a_96_gon() {
        let std = uk();
        let bodies = std.sign_bodies.as_ref().unwrap();
        let (outer, inner, width) = outlines(&bodies["octagon"], 50.0);
        assert_eq!((outer.len(), inner.len(), width), (8, 8, 50.0));
        assert_eq!(bounds(&outer).1, 0.0);
        assert_eq!(bounds(&outer).3, 50.0);
        assert_eq!(outer[1].y, 50.0);
        assert_eq!(outer[2].y, 50.0); // the top flat is exactly level
        assert!(close(bounds(&inner).3, 50.0 - 0.0331 * 50.0));

        let (outer, inner, width) = outlines(&bodies["roundel"], 50.0);
        assert_eq!((outer.len(), inner.len(), width), (96, 96, 50.0));
        assert!(close(outer[0].x, 50.0) && close(outer[0].y, 25.0));
        assert!(close(inner[0].x, 45.0)); // red ring 0.1 of the diameter
    }

    #[test]
    fn a_primitive_scales_about_the_origin_then_moves() {
        let line = transform(
            Prim::Line {
                from: Pt2::new(1.0, 2.0),
                to: Pt2::new(3.0, 2.0),
                width: 2.0,
                colour: Some(Colour::Black),
            },
            0.5,
            10.0,
            20.0,
        );
        assert_eq!(
            line,
            Prim::Line {
                from: Pt2::new(10.5, 21.0),
                to: Pt2::new(11.5, 21.0),
                width: 1.0,
                colour: Some(Colour::Black),
            }
        );
        let text = transform(
            Prim::Text(TextRun {
                origin: Pt2::new(4.0, 8.0),
                text: "For 2 miles".into(),
                face: FontFace::TransportHeavy,
                x_height: 4.0,
                ink: Some(Colour::Black),
            }),
            0.25,
            1.0,
            0.0,
        );
        assert!(
            matches!(text, Prim::Text(t) if t.origin == Pt2::new(2.0, 2.0) && t.x_height == 1.0)
        );
    }
}
