//! The schematic plan and solids of lighting columns.
//!
//! A diagram, not a product model. Only the positions, the mounting height, the outreach and the
//! tilt are entered data; the column diameter, the bracket section and the luminaire block are
//! the display sizes below, chosen to be visible and taken from no product or standard.

use afrobim_draw::{Colour, Drawing, Extents, Mesh3, Primitive, Pt2};

use crate::layout::ColumnPosition;
use crate::model::ColumnSpec;

/// Display diameter of a column in the 3D view, mm. A screen size, not a dimension.
pub const DISPLAY_COLUMN_DIAMETER_MM: f64 = 200.0;
/// Display size of the luminaire block, mm: length along the bracket, width, depth.
pub const DISPLAY_LUMINAIRE_MM: [f64; 3] = [800.0, 300.0, 120.0];
/// Radius of the column symbol on the plan, mm at site scale. A symbol size, not a dimension.
pub const DISPLAY_PLAN_SYMBOL_RADIUS_MM: f64 = 500.0;
/// Display side of the square bracket arm, mm.
const DISPLAY_BRACKET_MM: f64 = 80.0;
const COLUMN_SEGMENTS: u32 = 16;
const COLUMN_RGB: [u8; 3] = [0x8A, 0x8F, 0x94];
const LUMINAIRE_RGB: [u8; 3] = [0xF2, 0xD0, 0x4B];

/// The plan and solids of a set of columns, in millimetres from a local origin.
///
/// A drawing point (u, v) in millimetres is at easting `origin_x_m + u / 1000`, northing
/// `origin_y_m + v / 1000`, the same convention as `afrobim_alignment::PlanDrawing`. The solids
/// use the same origin in plan, with z the level in millimetres (the profile level of the
/// column, or zero where the alignment has no profile).
#[derive(Debug, Clone, PartialEq)]
pub struct LightingGeometry {
    pub drawing: Drawing,
    pub solids: Vec<Mesh3>,
    pub origin_x_m: f64,
    pub origin_y_m: f64,
}

#[allow(clippy::too_many_arguments)]
/// A box with its base centre line starting at `start`, running `length` along the unit plan
/// direction `dir` raised by `tilt_rad`, `width` across and `depth` square to both.
fn oriented_box(name: &str, start: [f64; 3], dir: (f64, f64), tilt_rad: f64, length: f64, width: f64, depth: f64, colour: [u8; 3]) -> Mesh3 {
    let (c, s) = (tilt_rad.cos(), tilt_rad.sin());
    // A right-handed frame: u along the box, v across in plan, w square to both.
    let u = [dir.0 * c, dir.1 * c, s];
    let v = [-dir.1, dir.0, 0.0];
    let w = [-dir.0 * s, -dir.1 * s, c];
    let at = |a: f64, b: f64, h: f64| [start[0] + a * u[0] + b * v[0] + h * w[0], start[1] + a * u[1] + b * v[1] + h * w[1], start[2] + a * u[2] + b * v[2] + h * w[2]];
    let (hw, hd) = (width / 2.0, depth / 2.0);
    let vertices = vec![
        at(0.0, -hw, -hd),
        at(length, -hw, -hd),
        at(length, hw, -hd),
        at(0.0, hw, -hd),
        at(0.0, -hw, hd),
        at(length, -hw, hd),
        at(length, hw, hd),
        at(0.0, hw, hd),
    ];
    // Counter-clockwise seen from outside.
    let triangles = vec![
        [0, 2, 1], [0, 3, 2], // bottom
        [4, 5, 6], [4, 6, 7], // top
        [0, 1, 5], [0, 5, 4], // -v
        [3, 7, 6], [3, 6, 2], // +v
        [0, 4, 7], [0, 7, 3], // start
        [1, 2, 6], [1, 6, 5], // end
    ];
    Mesh3 { name: name.to_string(), vertices, triangles, colour }
}

/// Draws the columns in plan (a circle for the column, a hairline for each bracket and a small
/// filled rectangle for each luminaire) and builds their solids. `None` for no columns.
pub fn lighting_geometry(columns: &[ColumnPosition], spec: &ColumnSpec) -> Option<LightingGeometry> {
    if columns.is_empty() {
        return None;
    }
    let r_m = DISPLAY_PLAN_SYMBOL_RADIUS_MM / 1000.0;
    let reach_m = spec.bracket_outreach_m.max(0.0) + DISPLAY_LUMINAIRE_MM[0] / 1000.0;
    let pad = r_m.max(reach_m);
    let min_x = columns.iter().map(|c| c.easting_m).fold(f64::INFINITY, f64::min) - pad;
    let min_y = columns.iter().map(|c| c.northing_m).fold(f64::INFINITY, f64::min) - pad;
    let max_x = columns.iter().map(|c| c.easting_m).fold(f64::NEG_INFINITY, f64::max) + pad;
    let max_y = columns.iter().map(|c| c.northing_m).fold(f64::NEG_INFINITY, f64::max) + pad;

    let out_mm = spec.bracket_outreach_m.max(0.0) * 1000.0;
    let height_mm = spec.mounting_height_m * 1000.0;
    let [lum_l, lum_w, lum_d] = DISPLAY_LUMINAIRE_MM;
    let mut primitives = Vec::new();
    let mut solids = Vec::new();
    for c in columns {
        // Subtract in metres before scaling, so large map coordinates lose nothing.
        let (x, y) = ((c.easting_m - min_x) * 1000.0, (c.northing_m - min_y) * 1000.0);
        let z = c.level_m.unwrap_or(0.0) * 1000.0;
        primitives.push(Primitive::Arc { centre: Pt2::new(x, y), radius: DISPLAY_PLAN_SYMBOL_RADIUS_MM, start_deg: 0.0, end_deg: 360.0, colour: None });
        solids.push(Mesh3::cylinder(&format!("Column {}", c.mark), x, y, z, z + height_mm, DISPLAY_COLUMN_DIAMETER_MM / 2.0, COLUMN_SEGMENTS, COLUMN_RGB));
        for &d in &c.bracket_directions_rad {
            let (dx, dy) = (d.cos(), d.sin());
            let tip = Pt2::new(x + out_mm * dx, y + out_mm * dy);
            if out_mm > 0.0 {
                primitives.push(Primitive::Line { from: Pt2::new(x, y), to: tip, width: 0.0, colour: None });
                solids.push(oriented_box(&format!("Bracket {}", c.mark), [x, y, z + height_mm], (dx, dy), 0.0, out_mm, DISPLAY_BRACKET_MM, DISPLAY_BRACKET_MM, COLUMN_RGB));
            }
            // The luminaire block is centred on the end of the bracket.
            let (hl, hw) = (lum_l / 2.0, lum_w / 2.0);
            let corner = |a: f64, b: f64| Pt2::new(tip.x + a * dx - b * dy, tip.y + a * dy + b * dx);
            primitives.push(Primitive::Fill { outline: vec![corner(-hl, -hw), corner(hl, -hw), corner(hl, hw), corner(-hl, hw)], colour: Colour::Yellow });
            let tilt = spec.tilt_deg.to_radians();
            let start = [tip.x - hl * dx * tilt.cos(), tip.y - hl * dy * tilt.cos(), z + height_mm - hl * tilt.sin()];
            solids.push(oriented_box(&format!("Luminaire {}", c.mark), start, (dx, dy), tilt, lum_l, lum_w, lum_d, LUMINAIRE_RGB));
        }
    }
    let extents = Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new((max_x - min_x) * 1000.0, (max_y - min_y) * 1000.0) };
    Some(LightingGeometry { drawing: Drawing { primitives, extents }, solids, origin_x_m: min_x, origin_y_m: min_y })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::tests::{installation, spec, straight};
    use crate::layout::layout_installation;
    use crate::model::Arrangement;

    #[test]
    fn boxes_are_closed_and_face_outwards_at_any_tilt() {
        for tilt in [0.0f64, 5.0, -15.0] {
            let b = oriented_box("b", [10.0, 20.0, 30.0], (0.6, 0.8), tilt.to_radians(), 800.0, 300.0, 120.0, [0, 0, 0]);
            assert!((b.volume() - 800.0 * 300.0 * 120.0).abs() < 1e-3, "{}", b.volume());
        }
    }

    #[test]
    fn plan_and_solids_stand_where_the_columns_are() {
        let a = straight();
        let layout = layout_installation(&a, &installation(Arrangement::TwinCentral, 0.0, 300.0, 100.0)).unwrap();
        let g = lighting_geometry(&layout.columns, &spec()).unwrap();
        // 4 columns: a circle, two bracket lines and two luminaires each.
        assert_eq!(g.drawing.primitives.len(), 4 * 5);
        assert_eq!(g.solids.len(), 4 * 5);
        // Padding is the longer of the symbol radius and outreach + luminaire length: 2.3 m.
        assert!((g.origin_x_m - (1000.0 - 2.3)).abs() < 1e-9 && (g.origin_y_m - (2000.0 - 2.3)).abs() < 1e-9);
        assert!((g.drawing.extents.max.x - 304_600.0).abs() < 1e-6 && (g.drawing.extents.max.y - 4_600.0).abs() < 1e-6);
        match &g.drawing.primitives[0] {
            Primitive::Arc { centre, radius, .. } => {
                assert!((centre.x - 2300.0).abs() < 1e-6 && (centre.y - 2300.0).abs() < 1e-6);
                assert_eq!(*radius, DISPLAY_PLAN_SYMBOL_RADIUS_MM);
            }
            other => panic!("{other:?}"),
        }
        // The first bracket of a central column heading east points south, 1.5 m long.
        match &g.drawing.primitives[1] {
            Primitive::Line { from, to, .. } => assert!((to.x - from.x).abs() < 1e-6 && (to.y - from.y + 1500.0).abs() < 1e-6),
            other => panic!("{other:?}"),
        }
        // The column is 10 m high from level zero; every solid is closed and outward-facing.
        let top = g.solids[0].vertices.iter().map(|v| v[2]).fold(f64::NEG_INFINITY, f64::max);
        assert_eq!(top, 10_000.0);
        assert!(g.solids.iter().all(|m| m.volume() > 0.0));
        assert!(lighting_geometry(&[], &spec()).is_none());
    }
}
