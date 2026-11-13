//! Sign-face take-off for a bill of quantities. Port of C# `SignCore.Layout.SignQuantities`.
//!
//! A laid-out sign becomes the quantities a QS prices: panel size, face area, ground colour and
//! legend count. Everything comes from the same [`SignLayout`] the drawings are written from, so
//! the schedule and the drawings cannot disagree.
//!
//! Out of scope by design: posts, foundations and fixings. Those depend on siting (mounting
//! height, wind zone, ground conditions), which the sign definition does not carry.

use afrobim_draw::Primitive;
use afrobim_signs::{Colour, ColourScheme, Severity, SignDefinition, SignEngine, SignKind, SignLayout};

use crate::num::fixed;
use crate::palette;
use crate::FabricationError;

/// One sign face measured for a bill of quantities. Dimensions are the finished panel extents
/// in millimetres at the sign's text size (or the stroke-width override), optionally rounded up
/// to a fabrication module. The face area is the gross bounding rectangle of the panel, which
/// is how a sign face is normally measured and priced (per m² of sheeted plate); it is not
/// reduced for corner radii.
#[derive(Debug, Clone, PartialEq)]
pub struct SignQuantity {
    pub code: String,
    pub catalog_key: Option<String>,
    pub kind: SignKind,
    pub scheme: ColourScheme,
    pub ground: Colour,
    pub speed_kph: u32,
    pub x_height_mm: f64,
    pub sw_mm: f64,
    pub width_mm: f64,
    pub height_mm: f64,
    /// Non-blank characters of lettering on the face.
    pub legend_characters: usize,
    pub symbols: usize,
    pub warnings: usize,
    pub errors: usize,
}

impl SignQuantity {
    pub fn area_m2(&self) -> f64 {
        self.width_mm * self.height_mm / 1_000_000.0
    }
}

/// Per-ground-colour roll-up of [`SignQuantity`] rows.
#[derive(Debug, Clone, PartialEq)]
pub struct QuantitySummaryRow {
    pub ground: Colour,
    pub count: usize,
    pub area_m2: f64,
    pub legend_characters: usize,
}

/// How [`measure_with`] measures.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MeasureOptions {
    /// Stroke width in mm to use instead of the sign's own (the C# batch `--sw-mm` switch).
    /// The x-height follows it: 4 sw.
    pub sw_mm_override: Option<f64>,
    /// Rounds each panel dimension up to the next multiple, e.g. 25 or 50 mm plate modules.
    /// 0 keeps the exact size.
    pub round_up_mm: f64,
}

/// Diagnostics the lettering outliner adds. The C# take-off measures the layout before its
/// lettering is outlined, so its warning count never includes them; a [`SignLayout`] is already
/// outlined, so they are left out here to give the same count.
const OUTLINER_CODES: [&str; 2] = ["GLYPHS_OVERLAP", "TEXT_WIDER_THAN_TILES"];

/// Measures one sign at its own size, exact dimensions.
pub fn measure(engine: &SignEngine, sign: &SignDefinition, layout: &SignLayout) -> SignQuantity {
    measure_with(engine, sign, layout, MeasureOptions::default()).expect("the default options are valid")
}

/// Measures one sign. Fails only when `round_up_mm` is negative.
pub fn measure_with(
    engine: &SignEngine,
    sign: &SignDefinition,
    layout: &SignLayout,
    options: MeasureOptions,
) -> Result<SignQuantity, FabricationError> {
    if options.round_up_mm < 0.0 || options.round_up_mm.is_nan() {
        return Err(FabricationError::InvalidOption(format!("round_up_mm must not be negative (got {})", options.round_up_mm)));
    }
    let sw_mm = options.sw_mm_override.unwrap_or(layout.sw_mm);
    // The x-height is 4 sw, so an override scales it with the sign.
    let x_height_mm = options.sw_mm_override.map_or(layout.x_height_mm, |o| 4.0 * o);

    let width = round_up(layout.drawing.extents.width() * sw_mm, options.round_up_mm);
    let height = round_up(layout.drawing.extents.height() * sw_mm, options.round_up_mm);

    let legend = layout
        .drawing
        .primitives
        .iter()
        .map(|p| match p {
            Primitive::Glyphs { text, .. } => legend_characters(text),
            _ => 0,
        })
        .sum();

    let counted = |severity: Severity| {
        layout.diagnostics.iter().filter(|d| d.severity == severity && !OUTLINER_CODES.contains(&d.code.as_str())).count()
    };

    Ok(SignQuantity {
        code: sign.code.clone(),
        catalog_key: sign.catalog_key.clone(),
        kind: sign.kind,
        scheme: sign.scheme,
        ground: palette::resolve(sign, engine.pack()).ground,
        speed_kph: sign.speed_kph,
        x_height_mm,
        sw_mm,
        width_mm: width,
        height_mm: height,
        legend_characters: legend,
        symbols: count_symbols(sign),
        warnings: counted(Severity::Warning),
        errors: counted(Severity::Error),
    })
}

/// Non-blank characters as C# counts them: UTF-16 code units that are not white space.
fn legend_characters(text: &str) -> usize {
    text.chars().filter(|c| !(c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(c))).map(char::len_utf16).sum()
}

fn round_up(value: f64, module: f64) -> f64 {
    if module > 0.0 {
        (value / module - 1e-9).ceil() * module
    } else {
        value
    }
}

fn count_symbols(sign: &SignDefinition) -> usize {
    let blank = |s: &Option<String>| s.as_deref().is_none_or(|s| s.trim().is_empty());
    usize::from(!blank(&sign.symbol_id))
        + sign.destinations.iter().filter(|d| !blank(&d.symbol_id)).count()
        + sign.embedded.as_ref().map_or(0, Vec::len)
}

/// Totals by ground colour, largest area first (ties in colour order).
pub fn summarise(rows: &[SignQuantity]) -> Vec<QuantitySummaryRow> {
    let mut out: Vec<QuantitySummaryRow> = Vec::new();
    for r in rows {
        match out.iter_mut().find(|s| s.ground == r.ground) {
            Some(s) => {
                s.count += 1;
                s.area_m2 += r.area_m2();
                s.legend_characters += r.legend_characters;
            }
            None => out.push(QuantitySummaryRow { ground: r.ground, count: 1, area_m2: r.area_m2(), legend_characters: r.legend_characters }),
        }
    }
    // A stable sort, as LINQ's OrderByDescending / ThenBy.
    out.sort_by(|a, b| {
        b.area_m2
            .partial_cmp(&a.area_m2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(palette::colour_index(a.ground).cmp(&palette::colour_index(b.ground)))
    });
    out
}

pub const CSV_HEADER: &str =
    "code,catalog_key,family,scheme,ground,speed_kph,x_height_mm,sw_mm,width_mm,height_mm,area_m2,legend_chars,symbols,warnings,errors";

/// Line-item schedule, one row per sign face. Lines end with `\n`.
pub fn to_csv(rows: &[SignQuantity]) -> String {
    let mut out = String::from(CSV_HEADER);
    out.push('\n');
    for r in rows {
        let fields = [
            csv(&r.code),
            csv(r.catalog_key.as_deref().unwrap_or("")),
            format!("{:?}", r.kind),
            format!("{:?}", r.scheme),
            palette::colour_name(r.ground).to_string(),
            r.speed_kph.to_string(),
            fixed(r.x_height_mm, 0, 2),
            fixed(r.sw_mm, 0, 3),
            fixed(r.width_mm, 0, 1),
            fixed(r.height_mm, 0, 1),
            fixed(r.area_m2(), 3, 3),
            r.legend_characters.to_string(),
            r.symbols.to_string(),
            r.warnings.to_string(),
            r.errors.to_string(),
        ];
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out
}

/// Ground-colour summary with a grand-total row. Lines end with `\n`.
pub fn summary_to_csv(summary: &[QuantitySummaryRow]) -> String {
    let mut out = String::from("ground,count,area_m2,legend_chars\n");
    for s in summary {
        out.push_str(&format!("{},{},{},{}\n", palette::colour_name(s.ground), s.count, fixed(s.area_m2, 3, 3), s.legend_characters));
    }
    out.push_str(&format!(
        "TOTAL,{},{},{}\n",
        summary.iter().map(|s| s.count).sum::<usize>(),
        fixed(summary.iter().map(|s| s.area_m2).sum::<f64>(), 3, 3),
        summary.iter().map(|s| s.legend_characters).sum::<usize>()
    ));
    out
}

fn csv(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(code: &str, ground: Colour, w: f64, h: f64, legend: usize) -> SignQuantity {
        SignQuantity {
            code: code.to_string(),
            catalog_key: None,
            kind: SignKind::StackType,
            scheme: ColourScheme::Auto,
            ground,
            speed_kph: 60,
            x_height_mm: 125.0,
            sw_mm: 31.25,
            width_mm: w,
            height_mm: h,
            legend_characters: legend,
            symbols: 0,
            warnings: 0,
            errors: 0,
        }
    }

    #[test]
    fn area_is_the_gross_rectangle_in_square_metres() {
        assert_eq!(row("A", Colour::Green, 2000.0, 1500.0, 0).area_m2(), 3.0);
    }

    #[test]
    fn round_up_takes_the_next_module_and_keeps_exact_multiples() {
        assert_eq!(round_up(2303.75, 50.0), 2350.0);
        assert_eq!(round_up(750.0, 50.0), 750.0);
        assert_eq!(round_up(750.000000001, 25.0), 750.0);
        assert_eq!(round_up(1031.25, 0.0), 1031.25);
    }

    #[test]
    fn summary_groups_by_ground_largest_area_first() {
        let rows = [
            row("A", Colour::White, 1000.0, 1000.0, 5),
            row("B", Colour::Green, 2000.0, 1000.0, 7),
            row("C", Colour::White, 500.0, 1000.0, 1),
            row("D", Colour::Blue, 1500.0, 1000.0, 2),
        ];
        let s = summarise(&rows);
        assert_eq!(s.iter().map(|r| r.ground).collect::<Vec<_>>(), [Colour::Green, Colour::White, Colour::Blue]);
        assert_eq!((s[1].count, s[1].area_m2, s[1].legend_characters), (2, 1.5, 6));
        assert_eq!(summary_to_csv(&s), "ground,count,area_m2,legend_chars\nGreen,1,2.000,7\nWhite,2,1.500,6\nBlue,1,1.500,2\nTOTAL,4,5.000,15\n");
    }

    #[test]
    fn equal_areas_sort_in_colour_order() {
        let rows = [row("A", Colour::Red, 1000.0, 1000.0, 0), row("B", Colour::White, 1000.0, 1000.0, 0)];
        let s = summarise(&rows);
        assert_eq!(s[0].ground, Colour::White);
    }

    #[test]
    fn csv_quotes_fields_with_commas_and_quotes() {
        let mut r = row("A,1 \"x\"", Colour::Green, 2303.75, 2625.0, 44);
        r.catalog_key = Some("UK01.2".to_string());
        let text = to_csv(&[r]);
        let line = text.lines().nth(1).unwrap();
        assert_eq!(line, "\"A,1 \"\"x\"\"\",UK01.2,StackType,Auto,Green,60,125,31.25,2303.8,2625,6.047,44,0,0,0");
        assert!(text.starts_with(CSV_HEADER));
    }

    #[test]
    fn empty_summary_still_has_a_total() {
        assert_eq!(summary_to_csv(&[]), "ground,count,area_m2,legend_chars\nTOTAL,0,0.000,0\n");
    }

    #[test]
    fn legend_counts_non_blank_utf16_units() {
        assert_eq!(legend_characters("Town centre"), 10);
        assert_eq!(legend_characters("  A\t1 "), 2);
        assert_eq!(legend_characters("½é"), 2);
    }
}
