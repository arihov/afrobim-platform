// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The drawn pictogram artwork library. Port of C# `PictogramLibrary`.
//!
//! App.1 §2.1.4 aircraft, App.3 §5 service/place pictograms and the §5 road and traffic plates
//! (the one-way arrow, the no-through-road stem, the extra-lane start/end plates, the priority
//! arrow pair, police control and the tourist-attraction rosette), plus the warning-family road
//! geometry silhouettes. Every glyph is authored in a normalised `[0,1]×[0,1]` unit box with y
//! up and the symbol upright (so "ahead" points +Y); [`try_build`] then scales it to the
//! requested sw size, translates it to the caller's box origin and, for directional symbols
//! such as the aircraft, rotates it about the box centre.
//!
//! Glyphs are built as solid silhouettes (filled shapes plus the occasional stroked bar)
//! rather than stroked outlines. Negative space (a kiosk window, a tent door, the fuel
//! display) is drawn in white so it reads against a dark glyph on a white sub-panel. The
//! first-aid cross carries its own red per §5.
//!
//! These are indicative shapes constructed from the sign descriptions, not artwork traced
//! from a standard: their proportions are the C# engine's own and are reproduced exactly.
//! A symbol with no glyph here returns `None`, so the caller keeps a positioned placeholder.

use std::f64::consts::PI;

use afrobim_draw::Pt2;

use crate::model::Colour;
use crate::prim::Prim;

/// Negative space inside a glyph.
const NEG: Colour = Colour::White;

/// The symbol ids the library draws, one per glyph (C# `PictogramLibrary.SupportedIds`).
/// [`try_build`] also accepts the aliases listed in its match.
pub const SUPPORTED_IDS: [&str; 34] = [
    "aircraft",
    "telephone",
    "first-aid",
    "pump",
    "wrench",
    "cup",
    "fork-knife",
    "bed",
    "table-tree",
    "tent",
    "bus",
    "pedestrian",
    "hump",
    // Warning-family geometric silhouettes.
    "bend",
    "hairpin",
    "double-bend",
    "crossroads",
    "side-road",
    "t-junction",
    "y-junction",
    "traffic-signal",
    "narrow-both",
    "narrow-one",
    "two-way",
    "other-danger",
    "uneven-road",
    "bicycle",
    // App.3 §5 road and traffic plates (I03, I06, I07, I09, I10).
    "one-way",
    "no-through-road",
    "lane-start",
    "lane-end",
    "priority",
    // App.3 §5 figurative plates (I08 police control, I51 tourist attraction).
    "police",
    "tourist",
];

/// A glyph primitive in the unit box.
enum Unit {
    Fill(Vec<(f64, f64)>, Colour),
    Line {
        from: (f64, f64),
        to: (f64, f64),
        width: f64,
        colour: Colour,
    },
}

/// Builds real geometry for a known pictogram, or returns `None` when no artwork exists for
/// `symbol_id`. `ink` is the foreground colour the glyph is drawn in (unless the glyph fixes
/// its own, e.g. the red cross). The glyph fills a `size`-square box whose bottom-left corner
/// is `box_min`, turned `rotation_deg` anticlockwise about the box centre.
pub(crate) fn try_build(
    symbol_id: &str,
    size: f64,
    box_min: Pt2,
    rotation_deg: f64,
    ink: Colour,
) -> Option<Vec<Prim>> {
    let mut p = Vec::new();
    match symbol_id.trim().to_lowercase().as_str() {
        "aircraft" | "plane" | "aerodrome" | "airport" => aircraft(&mut p, ink),
        "telephone" | "phone" | "telephone-kiosk" => telephone(&mut p, ink),
        "first-aid" | "red-cross" | "hospital-first-aid" => first_aid(&mut p),
        "pump" | "fuel" | "filling-station" => pump(&mut p, ink),
        "wrench" | "spanner" | "workshop" => wrench(&mut p, ink),
        "cup" | "refreshments" => cup(&mut p, ink),
        "fork-knife" | "restaurant" | "catering" => fork_knife(&mut p, ink),
        "bed" | "hotel" | "accommodation" => bed(&mut p, ink),
        "table-tree" | "rest-area" => table_tree(&mut p, ink),
        "tent" | "camp-site" | "campsite" => tent(&mut p, ink),
        "bus" | "bus-stop" | "bus-station" => bus(&mut p, ink),
        "pedestrian" | "crossing" | "pedestrian-crossing" => pedestrian(&mut p, ink),
        "hump" | "road-hump" => hump(&mut p, ink),
        "bend" => bend(&mut p, ink),
        "hairpin" => hairpin(&mut p, ink),
        "double-bend" => double_bend(&mut p, ink),
        "crossroads" => crossroads(&mut p, ink),
        "side-road" => side_road(&mut p, ink),
        "t-junction" => t_junction(&mut p, ink),
        "y-junction" => y_junction(&mut p, ink),
        "traffic-signal" => traffic_signal(&mut p, ink),
        "narrow-both" => narrow_both(&mut p, ink),
        "narrow-one" => narrow_one(&mut p, ink),
        "two-way" => two_way(&mut p, ink),
        "other-danger" => other_danger(&mut p, ink),
        "uneven-road" => uneven_road(&mut p, ink),
        "bicycle" | "cycle" => bicycle(&mut p, ink),
        "one-way" => one_way(&mut p, ink),
        "no-through-road" | "dead-end" | "cul-de-sac" => no_through_road(&mut p, ink),
        "priority" | "priority-over-oncoming" => priority(&mut p, ink),
        "lane-start" | "extra-lane-start" => lane_start(&mut p, ink),
        "lane-end" | "extra-lane-end" => lane_end(&mut p, ink),
        "police" | "police-control" => police(&mut p, ink),
        "tourist" | "tourist-attraction" => tourist(&mut p, ink),
        _ => return None,
    }
    Some(render(p, size, box_min.x, box_min.y, rotation_deg))
}

// ─── glyph constructions (unit box, y up, upright) ────────────────────────────

fn aircraft(p: &mut Vec<Unit>, ink: Colour) {
    // Top view: fuselage + swept main plane + swept tailplane + nose.
    rect(p, ink, 0.44, 0.10, 0.12, 0.78); // fuselage

    // Swept wings.
    fill(
        p,
        ink,
        &[
            (0.50, 0.64),
            (0.96, 0.42),
            (0.96, 0.34),
            (0.50, 0.48),
            (0.04, 0.34),
            (0.04, 0.42),
        ],
    );
    // Tailplane.
    fill(
        p,
        ink,
        &[
            (0.50, 0.22),
            (0.74, 0.10),
            (0.74, 0.06),
            (0.50, 0.14),
            (0.26, 0.06),
            (0.26, 0.10),
        ],
    );
    fill(p, ink, &[(0.44, 0.86), (0.56, 0.86), (0.50, 0.96)]); // nose cone
}

fn telephone(p: &mut Vec<Unit>, ink: Colour) {
    // Public call box / kiosk: roof, body with a window, plinth.
    rect(p, ink, 0.28, 0.76, 0.44, 0.10); // roof
    rect(p, ink, 0.32, 0.24, 0.36, 0.52); // body
    rect(p, NEG, 0.38, 0.42, 0.24, 0.28); // window
    rect(p, ink, 0.28, 0.16, 0.44, 0.08); // plinth
}

fn first_aid(p: &mut Vec<Unit>) {
    // Greek cross, fixed red per App.3 §5.
    let (arm, half) = (0.16, 0.5);
    fill(
        p,
        Colour::Red,
        &[
            (half - arm, 0.16),
            (half + arm, 0.16),
            (half + arm, half - arm),
            (0.84, half - arm),
            (0.84, half + arm),
            (half + arm, half + arm),
            (half + arm, 0.84),
            (half - arm, 0.84),
            (half - arm, half + arm),
            (0.16, half + arm),
            (0.16, half - arm),
            (half - arm, half - arm),
        ],
    );
}

fn pump(p: &mut Vec<Unit>, ink: Colour) {
    // Fuel pump: cabinet with display + a nozzle/hose to the right.
    rect(p, ink, 0.24, 0.18, 0.32, 0.56); // cabinet
    rect(p, ink, 0.24, 0.74, 0.32, 0.06); // cap
    rect(p, NEG, 0.31, 0.50, 0.18, 0.16); // display
    bar(p, ink, 0.56, 0.60, 0.74, 0.60, 0.05); // hose
    rect(p, ink, 0.72, 0.30, 0.08, 0.34); // nozzle body
    rect(p, ink, 0.70, 0.30, 0.12, 0.05); // nozzle foot
}

fn wrench(p: &mut Vec<Unit>, ink: Colour) {
    // Open-end spanner, upright: hexagon head with a carved mouth + handle.
    hexagon(p, ink, 0.50, 0.68, 0.18);
    disc(p, NEG, 0.50, 0.68, 0.09); // socket
    fill(
        p,
        NEG,
        &[(0.40, 0.68), (0.60, 0.68), (0.56, 0.86), (0.44, 0.86)],
    ); // open jaw
    rect(p, ink, 0.45, 0.14, 0.10, 0.50); // handle
}

fn cup(p: &mut Vec<Unit>, ink: Colour) {
    // Steaming cup on a saucer.
    fill(
        p,
        ink,
        &[(0.30, 0.60), (0.70, 0.60), (0.63, 0.34), (0.37, 0.34)],
    ); // body
    disc(p, ink, 0.72, 0.50, 0.10); // handle knob
    disc(p, NEG, 0.72, 0.50, 0.05);
    rect(p, ink, 0.24, 0.24, 0.52, 0.05); // saucer
    bar(p, ink, 0.44, 0.66, 0.40, 0.82, 0.03); // steam
    bar(p, ink, 0.56, 0.66, 0.60, 0.82, 0.03);
}

fn fork_knife(p: &mut Vec<Unit>, ink: Colour) {
    // Fork (left) + table knife (right).
    rect(p, ink, 0.28, 0.16, 0.05, 0.46); // fork handle
    rect(p, ink, 0.21, 0.60, 0.19, 0.04); // fork yoke
    rect(p, ink, 0.21, 0.62, 0.04, 0.18); // tine L
    rect(p, ink, 0.29, 0.62, 0.04, 0.18); // tine M
    rect(p, ink, 0.37, 0.62, 0.04, 0.18); // tine R
    rect(p, ink, 0.66, 0.16, 0.06, 0.36); // knife handle
    fill(
        p,
        ink,
        &[(0.64, 0.52), (0.74, 0.52), (0.74, 0.84), (0.66, 0.84)],
    ); // blade
}

fn bed(p: &mut Vec<Unit>, ink: Colour) {
    // Side-view bed: headboard, mattress, pillow, guest.
    rect(p, ink, 0.12, 0.30, 0.06, 0.46); // headboard
    rect(p, ink, 0.12, 0.28, 0.74, 0.12); // base
    rect(p, ink, 0.80, 0.28, 0.06, 0.20); // foot
    rect(p, ink, 0.20, 0.40, 0.22, 0.12); // pillow
    fill(
        p,
        ink,
        &[(0.44, 0.40), (0.84, 0.40), (0.84, 0.52), (0.50, 0.52)],
    ); // cover
}

fn table_tree(p: &mut Vec<Unit>, ink: Colour) {
    // Picnic table (left) + tree (right).
    rect(p, ink, 0.10, 0.44, 0.42, 0.05); // table top
    rect(p, ink, 0.14, 0.20, 0.05, 0.24); // leg L
    rect(p, ink, 0.43, 0.20, 0.05, 0.24); // leg R
    rect(p, ink, 0.66, 0.18, 0.06, 0.26); // trunk
    disc(p, ink, 0.69, 0.56, 0.20); // canopy
}

fn tent(p: &mut Vec<Unit>, ink: Colour) {
    // Ridge tent with a door flap.
    fill(p, ink, &[(0.10, 0.18), (0.90, 0.18), (0.50, 0.82)]); // body
    fill(p, NEG, &[(0.42, 0.18), (0.58, 0.18), (0.50, 0.50)]); // door
}

fn bus(p: &mut Vec<Unit>, ink: Colour) {
    // Front view: roof, window band open, lower body, wheels.
    rect(p, ink, 0.28, 0.68, 0.44, 0.14); // roof
    rect(p, ink, 0.28, 0.22, 0.44, 0.36); // lower body
    rect(p, ink, 0.30, 0.58, 0.04, 0.10); // post L
    rect(p, ink, 0.66, 0.58, 0.04, 0.10); // post R
    disc(p, ink, 0.38, 0.20, 0.07); // wheel L
    disc(p, ink, 0.62, 0.20, 0.07); // wheel R
}

fn pedestrian(p: &mut Vec<Unit>, ink: Colour) {
    // Walking person: head, torso, striding legs, swinging arms.
    disc(p, ink, 0.50, 0.82, 0.09); // head
    rect(p, ink, 0.45, 0.44, 0.10, 0.30); // torso
    bar(p, ink, 0.50, 0.46, 0.34, 0.12, 0.09); // leg back
    bar(p, ink, 0.50, 0.46, 0.68, 0.14, 0.09); // leg front
    bar(p, ink, 0.50, 0.68, 0.30, 0.52, 0.07); // arm back
    bar(p, ink, 0.50, 0.68, 0.72, 0.56, 0.07); // arm front
}

fn hump(p: &mut Vec<Unit>, ink: Colour) {
    // Road hump: a solid dome on a baseline.
    let mut pts = vec![(0.14, 0.28)];
    for i in 0..=24 {
        let a = PI * f64::from(i) / 24.0; // 0..180°
        pts.push((0.50 + 0.36 * a.cos(), 0.28 + 0.44 * a.sin()));
    }
    pts.push((0.86, 0.28));
    fill(p, ink, &pts);
    rect(p, ink, 0.10, 0.22, 0.80, 0.06); // carriageway
}

// ─── Warning-family road-geometry silhouettes ─────────────────────────
// These are standard/UK-convention indicative shapes (Vol.2 §Warning signs
// follows the same symbol family), authored as simple filled road/arrow
// silhouettes rather than traced from the Uganda plate artwork. Asymmetric
// directional glyphs are drawn pointing LEFT; the warning engine mirrors the
// whole glyph when a .2 (Right) plate is laid out.

fn bend(p: &mut Vec<Unit>, ink: Colour) {
    // Single bend to the left: up-then-left arrow.
    rect(p, ink, 0.46, 0.08, 0.12, 0.48); // vertical leg
    rect(p, ink, 0.22, 0.46, 0.36, 0.12); // horizontal leg
    fill(p, ink, &[(0.28, 0.64), (0.28, 0.34), (0.06, 0.49)]); // arrowhead (left)
}

fn hairpin(p: &mut Vec<Unit>, ink: Colour) {
    // Hairpin bend: up, over to the left, back down, pointing down.
    rect(p, ink, 0.54, 0.12, 0.10, 0.58); // right leg
    rect(p, ink, 0.28, 0.60, 0.36, 0.10); // top leg
    rect(p, ink, 0.28, 0.38, 0.10, 0.32); // left leg
    fill(p, ink, &[(0.20, 0.44), (0.46, 0.44), (0.33, 0.22)]); // arrowhead (down)
}

fn double_bend(p: &mut Vec<Unit>, ink: Colour) {
    // Reverse (double) bend: S-path left then right, arrowhead up.
    rect(p, ink, 0.46, 0.06, 0.10, 0.28); // bottom stem
    rect(p, ink, 0.28, 0.28, 0.28, 0.10); // lower arm (left)
    rect(p, ink, 0.28, 0.28, 0.10, 0.34); // middle riser
    rect(p, ink, 0.28, 0.56, 0.44, 0.10); // upper arm (right)
    rect(p, ink, 0.62, 0.56, 0.10, 0.30); // top riser
    fill(p, ink, &[(0.54, 0.80), (0.80, 0.80), (0.67, 0.98)]); // arrowhead (up)
}

fn crossroads(p: &mut Vec<Unit>, ink: Colour) {
    // Crossroads: symmetric plus.
    rect(p, ink, 0.43, 0.10, 0.14, 0.80); // vertical road
    rect(p, ink, 0.10, 0.43, 0.80, 0.14); // horizontal road
}

fn side_road(p: &mut Vec<Unit>, ink: Colour) {
    // Minor road joining from the LEFT (base glyph; engine mirrors for .2).
    rect(p, ink, 0.46, 0.08, 0.12, 0.84); // main road
    rect(p, ink, 0.14, 0.44, 0.36, 0.12); // left stub
}

fn t_junction(p: &mut Vec<Unit>, ink: Colour) {
    // T-junction ahead: stem meeting a top bar (symmetric).
    rect(p, ink, 0.44, 0.12, 0.12, 0.58); // stem
    rect(p, ink, 0.10, 0.62, 0.80, 0.12); // top bar
}

fn y_junction(p: &mut Vec<Unit>, ink: Colour) {
    // Fork / Y-junction: a stem splitting into two diverging branches.
    rect(p, ink, 0.44, 0.08, 0.12, 0.34); // stem (PolyFill)
    bar(p, ink, 0.50, 0.42, 0.24, 0.86, 0.12); // left branch
    bar(p, ink, 0.50, 0.42, 0.76, 0.86, 0.12); // right branch
}

fn traffic_signal(p: &mut Vec<Unit>, ink: Colour) {
    // Traffic signals ahead: a lamp housing with three lenses and a post.
    rect(p, ink, 0.38, 0.14, 0.24, 0.66); // housing
    disc(p, NEG, 0.50, 0.68, 0.07); // top lens
    disc(p, NEG, 0.50, 0.50, 0.07); // middle lens
    disc(p, NEG, 0.50, 0.30, 0.07); // bottom lens
    rect(p, ink, 0.47, 0.06, 0.06, 0.10); // post
}

fn narrow_both(p: &mut Vec<Unit>, ink: Colour) {
    // Road narrows on both sides: a symmetric tapering carriageway.
    fill(
        p,
        ink,
        &[(0.14, 0.10), (0.86, 0.10), (0.62, 0.90), (0.38, 0.90)],
    );
}

fn narrow_one(p: &mut Vec<Unit>, ink: Colour) {
    // Road narrows on one side (base = left; engine mirrors for .2): straight right
    // edge, left edge tapering inward toward the top.
    fill(
        p,
        ink,
        &[(0.16, 0.10), (0.84, 0.10), (0.84, 0.90), (0.48, 0.90)],
    );
}

fn two_way(p: &mut Vec<Unit>, ink: Colour) {
    // Two-way traffic: opposing up and down arrows side by side.
    bar(p, ink, 0.36, 0.14, 0.36, 0.72, 0.12); // up shaft
    fill(p, ink, &[(0.24, 0.70), (0.48, 0.70), (0.36, 0.92)]); // up head
    bar(p, ink, 0.64, 0.28, 0.64, 0.86, 0.12); // down shaft
    fill(p, ink, &[(0.52, 0.30), (0.76, 0.30), (0.64, 0.08)]); // down head
}

fn other_danger(p: &mut Vec<Unit>, ink: Colour) {
    // Other dangers: exclamation mark.
    fill(
        p,
        ink,
        &[(0.42, 0.88), (0.58, 0.88), (0.55, 0.34), (0.45, 0.34)],
    ); // body
    disc(p, ink, 0.50, 0.18, 0.08); // dot
}

fn uneven_road(p: &mut Vec<Unit>, ink: Colour) {
    // Uneven road: two bumps on a carriageway.
    rect(p, ink, 0.10, 0.16, 0.80, 0.06); // carriageway
    disc(p, ink, 0.34, 0.36, 0.14); // bump 1
    disc(p, ink, 0.62, 0.36, 0.14); // bump 2
}

fn bicycle(p: &mut Vec<Unit>, ink: Colour) {
    // Cyclists / cycle crossing: two wheels and a frame.
    disc(p, ink, 0.26, 0.32, 0.16); // rear wheel
    disc(p, ink, 0.74, 0.32, 0.16); // front wheel
    bar(p, ink, 0.26, 0.32, 0.50, 0.58, 0.05); // rear stay
    bar(p, ink, 0.50, 0.58, 0.74, 0.32, 0.05); // front fork
    bar(p, ink, 0.50, 0.58, 0.58, 0.32, 0.05); // seat stay
    bar(p, ink, 0.42, 0.66, 0.60, 0.66, 0.05); // bars/saddle
}

// ─── App.3 §5 road & traffic plates (I03, I06, I07, I08, I09, I10, I51) ──
// I07 one-way draws one figure straight onto the coloured ground in the plate's
// lettering colour as RELATIVE proportions in the unit box (travel is upward), NOT
// measured mm or traced raster artwork; its arrowhead is kept near the standard
// arrow's 90° point (head length ≈ half the base).

fn one_way(p: &mut Vec<Unit>, ink: Colour) {
    // I07 one-way route: a single bold arrow pointing ahead (up).
    rect(p, ink, 0.43, 0.08, 0.14, 0.52); // shaft
    fill(p, ink, &[(0.20, 0.56), (0.80, 0.56), (0.50, 0.94)]); // head
}

// The remaining App.3 §5 plates. These six are constructed from the §5 sign descriptions
// (the I03/I06/I08/I09/I10/I51 figures), not traced from the standard's artwork, so their
// proportions are approximate until checked against the figures.

fn no_through_road(p: &mut Vec<Unit>, ink: Colour) {
    // I03: the road (ink stem) closed off by a red cross-bar at its far end.
    rect(p, ink, 0.40, 0.12, 0.20, 0.62); // road stem
    rect(p, Colour::Red, 0.18, 0.72, 0.64, 0.16); // red terminal bar
}

fn priority(p: &mut Vec<Unit>, ink: Colour) {
    // I06: the driver's own direction (large ink arrow, left, pointing ahead) has
    // priority over oncoming traffic (smaller red arrow, right, pointing back).
    rect(p, ink, 0.24, 0.12, 0.14, 0.54); // own shaft
    fill(p, ink, &[(0.14, 0.62), (0.48, 0.62), (0.31, 0.90)]); // own head
    rect(p, Colour::Red, 0.64, 0.42, 0.10, 0.46); // oncoming shaft
    fill(p, Colour::Red, &[(0.56, 0.46), (0.82, 0.46), (0.69, 0.22)]); // oncoming head
}

fn lane_start(p: &mut Vec<Unit>, ink: Colour) {
    // I09: a through lane (left) with an extra lane opening off it to the right.
    lane_arrow(p, ink, 0.30, 0.10); // through lane
    bar(p, ink, 0.34, 0.18, 0.66, 0.42, 0.09); // taper out
    lane_arrow(p, ink, 0.70, 0.40); // added lane
}

fn lane_end(p: &mut Vec<Unit>, ink: Colour) {
    // I10: the right-hand lane runs out and merges into the through lane.
    lane_arrow(p, ink, 0.30, 0.10); // through lane
    rect(p, ink, 0.655, 0.10, 0.09, 0.30); // ending lane
    bar(p, ink, 0.70, 0.38, 0.36, 0.64, 0.09); // taper in
}

fn police(p: &mut Vec<Unit>, ink: Colour) {
    // I08: officer on traffic duty — peaked cap, head, torso, one arm raised to stop.
    rect(p, ink, 0.40, 0.84, 0.20, 0.06); // cap crown
    rect(p, ink, 0.36, 0.82, 0.30, 0.03); // cap peak
    disc(p, ink, 0.50, 0.75, 0.08); // head
    fill(
        p,
        ink,
        &[(0.36, 0.64), (0.64, 0.64), (0.60, 0.36), (0.40, 0.36)],
    ); // tunic
    bar(p, ink, 0.60, 0.62, 0.80, 0.88, 0.07); // raised arm
    bar(p, ink, 0.40, 0.62, 0.26, 0.42, 0.07); // lowered arm
    bar(p, ink, 0.45, 0.38, 0.42, 0.10, 0.08); // leg L
    bar(p, ink, 0.55, 0.38, 0.58, 0.10, 0.08); // leg R
}

fn tourist(p: &mut Vec<Unit>, ink: Colour) {
    // I51: the tourist-attraction rosette — eight petals round an open centre.
    for i in 0..8 {
        let a = PI * f64::from(i) / 4.0;
        disc(p, ink, 0.50 + 0.26 * a.cos(), 0.50 + 0.26 * a.sin(), 0.11);
    }
    disc(p, ink, 0.50, 0.50, 0.20); // hub
    disc(p, NEG, 0.50, 0.50, 0.09); // open eye
}

/// An upright lane arrow: shaft from `y0` to a head at the top.
fn lane_arrow(p: &mut Vec<Unit>, ink: Colour, cx: f64, y0: f64) {
    rect(p, ink, cx - 0.045, y0, 0.09, 0.66 - y0); // shaft
    fill(p, ink, &[(cx - 0.12, 0.64), (cx + 0.12, 0.64), (cx, 0.88)]); // head
}

// ─── unit-space primitive helpers ─────────────────────────────────────────────

fn fill(p: &mut Vec<Unit>, c: Colour, pts: &[(f64, f64)]) {
    p.push(Unit::Fill(pts.to_vec(), c));
}

fn rect(p: &mut Vec<Unit>, c: Colour, x: f64, y: f64, w: f64, h: f64) {
    fill(p, c, &[(x, y), (x + w, y), (x + w, y + h), (x, y + h)]);
}

fn bar(p: &mut Vec<Unit>, c: Colour, ax: f64, ay: f64, bx: f64, by: f64, w: f64) {
    p.push(Unit::Line {
        from: (ax, ay),
        to: (bx, by),
        width: w,
        colour: c,
    });
}

/// A disc as a 28-gon.
fn disc(p: &mut Vec<Unit>, c: Colour, cx: f64, cy: f64, r: f64) {
    const SEG: usize = 28;
    let pts: Vec<(f64, f64)> = (0..SEG)
        .map(|i| {
            let a = 2.0 * PI * i as f64 / SEG as f64;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect();
    fill(p, c, &pts);
}

fn hexagon(p: &mut Vec<Unit>, c: Colour, cx: f64, cy: f64, r: f64) {
    let pts: Vec<(f64, f64)> = (0..6)
        .map(|i| {
            let a = PI / 6.0 + PI * i as f64 / 3.0;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect();
    fill(p, c, &pts);
}

// ─── unit → stroke-width transform ────────────────────────────────────────────

fn render(unit: Vec<Unit>, size: f64, x0: f64, y0: f64, rot: f64) -> Vec<Prim> {
    let (cx, cy) = (x0 + size / 2.0, y0 + size / 2.0);
    let rad = rot * PI / 180.0;
    let (cos, sin) = (rad.cos(), rad.sin());
    let rotate = rot.abs() > 1e-9;

    let map = |q: (f64, f64)| {
        let x = x0 + q.0 * size;
        let y = y0 + q.1 * size;
        if !rotate {
            return Pt2::new(x, y);
        }
        let (dx, dy) = (x - cx, y - cy);
        Pt2::new(cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
    };

    unit.into_iter()
        .map(|u| match u {
            Unit::Line {
                from,
                to,
                width,
                colour,
            } => Prim::Line {
                from: map(from),
                to: map(to),
                width: width * size,
                colour: Some(colour),
            },
            Unit::Fill(outline, colour) => Prim::Fill {
                outline: outline.into_iter().map(map).collect(),
                colour,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(prims: &[Prim]) -> (f64, f64, f64, f64) {
        let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        let mut add = |p: &Pt2| b = (b.0.min(p.x), b.1.min(p.y), b.2.max(p.x), b.3.max(p.y));
        for prim in prims {
            match prim {
                Prim::Fill { outline, .. } => outline.iter().for_each(&mut add),
                Prim::Line { from, to, .. } => {
                    add(from);
                    add(to);
                }
                _ => panic!("a glyph is fills and lines only"),
            }
        }
        b
    }

    /// Every id of the C# `TryBuild` switch, aliases included.
    const ALL_IDS: [&str; 66] = [
        "aircraft",
        "plane",
        "aerodrome",
        "airport",
        "telephone",
        "phone",
        "telephone-kiosk",
        "first-aid",
        "red-cross",
        "hospital-first-aid",
        "pump",
        "fuel",
        "filling-station",
        "wrench",
        "spanner",
        "workshop",
        "cup",
        "refreshments",
        "fork-knife",
        "restaurant",
        "catering",
        "bed",
        "hotel",
        "accommodation",
        "table-tree",
        "rest-area",
        "tent",
        "camp-site",
        "campsite",
        "bus",
        "bus-stop",
        "bus-station",
        "pedestrian",
        "crossing",
        "pedestrian-crossing",
        "hump",
        "road-hump",
        "bend",
        "hairpin",
        "double-bend",
        "crossroads",
        "side-road",
        "t-junction",
        "y-junction",
        "traffic-signal",
        "narrow-both",
        "narrow-one",
        "two-way",
        "other-danger",
        "uneven-road",
        "bicycle",
        "cycle",
        "one-way",
        "no-through-road",
        "dead-end",
        "cul-de-sac",
        "priority",
        "priority-over-oncoming",
        "lane-start",
        "extra-lane-start",
        "lane-end",
        "extra-lane-end",
        "police",
        "police-control",
        "tourist",
        "tourist-attraction",
    ];

    #[test]
    fn every_id_draws_inside_its_box() {
        for id in ALL_IDS {
            let prims = try_build(id, 10.0, Pt2::new(3.0, 4.0), 0.0, Colour::Black)
                .unwrap_or_else(|| panic!("no glyph for '{id}'"));
            assert!(!prims.is_empty(), "{id}");
            let (x0, y0, x1, y1) = bounds(&prims);
            assert!(
                x0 >= 3.0 && y0 >= 4.0 && x1 <= 13.0 && y1 <= 14.0,
                "{id}: {x0} {y0} {x1} {y1}"
            );
        }
        for id in SUPPORTED_IDS {
            assert!(ALL_IDS.contains(&id), "{id}");
        }
    }

    #[test]
    fn ids_are_matched_trimmed_and_without_case() {
        let a = try_build("aircraft", 10.0, Pt2::new(0.0, 0.0), 0.0, Colour::Black);
        assert!(a.is_some());
        assert_eq!(
            try_build(" AirPort ", 10.0, Pt2::new(0.0, 0.0), 0.0, Colour::Black),
            a
        );
        assert_eq!(
            try_build("castle", 10.0, Pt2::new(0.0, 0.0), 0.0, Colour::Black),
            None
        );
        assert_eq!(
            try_build("parking-p", 10.0, Pt2::new(0.0, 0.0), 0.0, Colour::Black),
            None
        );
    }

    /// The nose cone's tip is at (0.50, 0.96) of the box: up when upright, at the left when
    /// turned +90° (the convention of the arrows: +90° points left).
    #[test]
    fn the_aircraft_turns_about_the_box_centre() {
        let nose = |rot: f64| {
            let prims =
                try_build("aircraft", 10.0, Pt2::new(0.0, 0.0), rot, Colour::White).unwrap();
            match prims.last().unwrap() {
                Prim::Fill { outline, colour } => {
                    assert_eq!(*colour, Colour::White);
                    outline[2]
                }
                _ => panic!("the nose cone is a fill"),
            }
        };
        let close = |a: Pt2, x: f64, y: f64| (a.x - x).abs() < 1e-9 && (a.y - y).abs() < 1e-9;
        assert!(close(nose(0.0), 5.0, 9.6));
        assert!(close(nose(90.0), 0.4, 5.0));
        assert!(close(nose(-90.0), 9.6, 5.0));
    }

    #[test]
    fn fixed_colours_and_negative_space_ignore_the_ink() {
        let cross = try_build("first-aid", 8.0, Pt2::new(0.0, 0.0), 0.0, Colour::Black).unwrap();
        assert!(matches!(
            cross.as_slice(),
            [Prim::Fill {
                colour: Colour::Red,
                ..
            }]
        ));
        let tent = try_build("tent", 8.0, Pt2::new(0.0, 0.0), 0.0, Colour::Black).unwrap();
        assert!(matches!(
            tent.as_slice(),
            [
                Prim::Fill {
                    colour: Colour::Black,
                    ..
                },
                Prim::Fill {
                    colour: Colour::White,
                    ..
                }
            ]
        ));
    }

    /// A bar is a stroked line whose width scales with the box.
    #[test]
    fn bars_scale_with_the_box() {
        let pump = try_build("pump", 20.0, Pt2::new(0.0, 0.0), 0.0, Colour::Black).unwrap();
        let hose = pump
            .iter()
            .find_map(|p| match p {
                Prim::Line {
                    from, to, width, ..
                } => Some((*from, *to, *width)),
                _ => None,
            })
            .unwrap();
        assert!((hose.0.x - 11.2).abs() < 1e-9 && (hose.1.x - 14.8).abs() < 1e-9);
        assert!((hose.2 - 1.0).abs() < 1e-9);
    }
}
