//! One direction's destinations laid out as UK TSM Chapter 7 destination blocks, with route
//! numbers, route-number patches and panels (§3.2–3.3, §3.5–3.7). Port of C#
//! `DestinationColumn`.
//!
//! - A **block** is a run of destinations sharing a route number and route status (§3.7.1).
//!   Its route number goes on the same line as a single name, alongside two names (a patch is
//!   centred on them and raised 0.5 sw, Figure 3-12; plain tiles are centred, Figure 3-16), or
//!   below three or more names, ranged left. It also goes below when the block carries
//!   distances (§3.9.4).
//! - A **patch** backs a route number of higher status than the ground it sits on (§3.2.3):
//!   blue for a motorway number on green or white, green for a primary number on white. White
//!   patches are never used (§3.2.6).
//! - A **panel** backs destinations reached by turning onto a route of a different status from
//!   the sign (§3.2.3–3.2.6). Consecutive blocks with the same panel colour share one panel;
//!   panels are never nested (§3.3.1).
//! - A dark patch or panel on a dark ground gets a 0.5 sw white border (§3.3.1), and spacings
//!   are measured to the outside of that border (§3.7.5).
//!
//! Lengths are in sw. Offsets are measured down from the column's top edge.

use std::cell::Cell;

use afrobim_draw::Pt2;

use crate::measurer::TextTileMeasurer;
use crate::model::{Colour, Destination, FontFace, RouteStatus};
use crate::pack::{BlockRules, PanelRules, RulePack};
use crate::prim::{rect, Prim, TextRun};
use crate::route_numbers::{self, RouteGap, RouteNumberRun};
use crate::symbol;
use crate::SignError;

/// How the top or bottom edge of a block or column is formed, which picks the spacing to its
/// neighbour (Figure 3-17).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edge {
    Tiles,
    Patch,
}

#[derive(Debug, Clone, PartialEq)]
enum Item {
    Rect {
        x: f64,
        down: f64,
        w: f64,
        h: f64,
        fill: Colour,
    },
    Text {
        x: f64,
        down: f64,
        text: String,
        face: FontFace,
        ink: Colour,
    },
    Symbol {
        x: f64,
        down: f64,
        symbol_id: String,
        size: f64,
        rotation: f64,
        ink: Colour,
    },
}

impl Item {
    fn shifted(mut self, dx: f64, d_down: f64) -> Item {
        match &mut self {
            Item::Rect { x, down, .. }
            | Item::Text { x, down, .. }
            | Item::Symbol { x, down, .. } => {
                *x += dx;
                *down += d_down;
            }
        }
        self
    }
}

const TILE_HEIGHT: f64 = 8.0;

/// §3.5.1 / Figure 3-12: a symbol or patch sits 0.5 sw above centre.
const SYMBOL_RAISE: f64 = 0.5;

#[derive(Debug, Clone)]
pub(crate) struct DestinationColumn {
    items: Vec<Item>,
    pub width: f64,
    pub height: f64,
    #[allow(dead_code)] // read by the map-type layout in C#; kept so the column is complete
    pub top: Edge,
    pub bottom: Edge,
    /// True when the column is plain tiles: no patch, panel or symbol line.
    pub all_tiles: bool,
    /// True when the column ends in a tiled name line with a descender, which map-type signs
    /// keep 1.5 sw clear of the point of a vertical arm (Chapter 7 Figure 5-2).
    #[allow(dead_code)]
    pub bottom_descender: bool,
    /// Number of destination name lines in the column.
    pub name_lines: usize,
    /// Notes for the caller to raise as diagnostics, e.g. route numbers that cannot be drawn
    /// because the pack has no `blocks` section.
    pub unsupported: Vec<String>,
}

/// A laid-out block, panel or run of blocks, with its own top at offset 0.
struct Piece {
    items: Vec<Item>,
    width: f64,
    height: f64,
    top: Edge,
    bottom: Edge,
    all_tiles: bool,
    name_lines: usize,
    bottom_descender: bool,
}

struct Context<'a> {
    rules: Option<&'a BlockRules>,
    panel: &'a PanelRules,
    measurer: &'a TextTileMeasurer<'a>,
    symbol_size: &'a dyn Fn(&str) -> f64,
    symbols_leading: bool,
    distance_gap: f64,
    /// The shared distance-column centre, in column coordinates, when distances are aligned;
    /// `None` for a column per block.
    distance_centre: Option<f64>,
    /// The widest distance in the column, when distances are aligned.
    distance_width: f64,
    /// The right end of the longest name line among blocks with distances, and the widest
    /// distance, both in column coordinates; collected while building.
    probe_clear_right: Cell<f64>,
    probe_width: Cell<f64>,
}

impl Context<'_> {
    fn block_spacing(&self, upper: Edge, lower: Edge) -> f64 {
        let Some(r) = self.rules else { return 0.0 }; // unreachable: no rules means a single block
        match (upper, lower) {
            (Edge::Tiles, Edge::Tiles) => r.block_spacing_tiles,
            (Edge::Tiles, Edge::Patch) => r.block_spacing_tiles_over_patch,
            (Edge::Patch, Edge::Tiles) => r.block_spacing_patch_over_tiles,
            (Edge::Patch, Edge::Patch) => r.block_spacing_patch_over_patch,
        }
    }

    /// A legend's tile width in the face it is drawn in.
    fn width(&self, text: &str, face: FontFace) -> Result<f64, SignError> {
        self.measurer.width(text, face)
    }
}

impl DestinationColumn {
    pub fn draw(
        &self,
        prims: &mut Vec<Prim>,
        left: f64,
        top: f64,
        measurer: &TextTileMeasurer,
    ) -> Result<(), SignError> {
        for item in &self.items {
            match item {
                Item::Rect {
                    x,
                    down,
                    w,
                    h,
                    fill,
                } => {
                    let x = left + x;
                    prims.push(rect(x, top - down - h, x + w, top - down, *fill));
                }
                Item::Text {
                    x,
                    down,
                    text,
                    face,
                    ink,
                } => {
                    // Item.down is the tile top; a TextRun's origin is the tile foot.
                    prims.push(Prim::Text(TextRun {
                        origin: Pt2::new(left + x, top - down - TILE_HEIGHT),
                        text: text.clone(),
                        face: *face,
                        x_height: 4.0,
                        ink: Some(*ink),
                    }));
                }
                Item::Symbol {
                    x,
                    down,
                    symbol_id,
                    size,
                    rotation,
                    ink,
                } => {
                    prims.extend(symbol::resolve_inked(
                        symbol_id,
                        *size,
                        Pt2::new(left + x, top - down - size),
                        *rotation,
                        measurer,
                        *ink,
                    )?);
                }
            }
        }
        Ok(())
    }

    // ─── building ─────────────────────────────────────────────────────────────────

    /// - `dests`: one direction's destinations, nearest first.
    /// - `ground`, `lettering`, `face`: the sign's ground colour, lettering colour and face.
    /// - `symbols_leading`: symbols go before the name (the arrow is on the right, §4.2.6);
    ///   otherwise after the name and route number.
    /// - `align_distances`: set the distances of every block in one centred column, clear of
    ///   the longest name line of any block that carries distances (§3.9.1–3.9.2). Otherwise
    ///   each block has its own distance column, as on a stack-type section.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        dests: &[&Destination],
        ground: Colour,
        lettering: Colour,
        face: FontFace,
        pack: &RulePack,
        measurer: &TextTileMeasurer,
        symbol_size: &dyn Fn(&str) -> f64,
        symbols_leading: bool,
        align_distances: bool,
    ) -> Result<DestinationColumn, SignError> {
        let p = &pack.panel;
        let dist_gap = pack
            .distances
            .as_ref()
            .map_or(p.word_spacing, |d| d.min_gap);
        let mut ctx = Context {
            rules: pack.blocks.as_ref(),
            panel: p,
            measurer,
            symbol_size,
            symbols_leading,
            distance_gap: dist_gap,
            distance_centre: None,
            distance_width: 0.0,
            probe_clear_right: Cell::new(0.0),
            probe_width: Cell::new(0.0),
        };
        let col = build_column(dests, ground, lettering, face, &ctx)?;
        if !align_distances || ctx.probe_width.get() <= 0.0 {
            return Ok(col);
        }

        // Second pass with the shared column centre the first pass measured.
        let centre = ctx.probe_clear_right.get() + dist_gap + ctx.probe_width.get() / 2.0;
        ctx.distance_centre = Some(centre);
        ctx.distance_width = ctx.probe_width.get();
        build_column(dests, ground, lettering, face, &ctx)
    }
}

fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_uppercase)
        .eq(b.chars().flat_map(char::to_uppercase))
}

fn has_text(s: &Option<String>) -> bool {
    s.as_ref().is_some_and(|s| !s.is_empty())
}

fn build_column(
    dests: &[&Destination],
    ground: Colour,
    lettering: Colour,
    face: FontFace,
    ctx: &Context,
) -> Result<DestinationColumn, SignError> {
    let mut col = DestinationColumn {
        items: Vec::new(),
        width: 0.0,
        height: 0.0,
        top: Edge::Tiles,
        bottom: Edge::Tiles,
        all_tiles: true,
        bottom_descender: false,
        name_lines: 0,
        unsupported: Vec::new(),
    };
    let rules = ctx.rules;

    if rules.is_none() && dests.iter().any(|d| has_text(&d.route_number)) {
        col.unsupported
            .push("route numbers (the rule pack has no 'blocks' section)".to_string());
    }

    // Blocks: consecutive destinations sharing route number and route status (§3.7.1).
    // Without block rules the whole direction is one plain block.
    let mut blocks: Vec<Vec<&Destination>> = Vec::new();
    for d in dests {
        match blocks.last_mut() {
            Some(last) if rules.is_none() => last.push(d),
            Some(last)
                if eq_ignore_case(
                    last[0].route_number.as_deref().unwrap_or(""),
                    d.route_number.as_deref().unwrap_or(""),
                ) && last[0].route == d.route =>
            {
                last.push(d)
            }
            _ => blocks.push(vec![d]),
        }
    }

    // Units: a panel holds consecutive blocks of the same panel colour; other blocks stand
    // alone on the sign ground.
    let mut units: Vec<(Option<Colour>, Vec<Vec<&Destination>>)> = Vec::new();
    for b in blocks {
        let panel = rules
            .and_then(|r| r.colour_for(b[0].route))
            .filter(|c| *c != ground);
        match units.last_mut() {
            Some((last_panel, unit_blocks)) if panel.is_some() && *last_panel == panel => {
                unit_blocks.push(b)
            }
            _ => units.push((panel, vec![b])),
        }
    }

    let mut down = 0.0;
    let mut prev_bottom: Option<Edge> = None;
    for (panel, unit_blocks) in &units {
        let piece = match panel {
            Some(pc) => build_panel(unit_blocks, *pc, ground, ctx)?,
            None => {
                let pieces = unit_blocks
                    .iter()
                    .map(|b| build_block(b, ground, lettering, face, ctx, 0.0))
                    .collect::<Result<Vec<_>, _>>()?;
                stack(pieces, ctx)
            }
        };

        match prev_bottom {
            Some(pb) => down += ctx.block_spacing(pb, piece.top),
            None => col.top = piece.top,
        }
        col.items
            .extend(piece.items.into_iter().map(|it| it.shifted(0.0, down)));
        col.width = col.width.max(piece.width);
        col.all_tiles &= piece.all_tiles;
        col.name_lines += piece.name_lines;
        col.bottom_descender = piece.bottom_descender;
        down += piece.height;
        prev_bottom = Some(piece.bottom);
    }
    col.height = down;
    col.bottom = prev_bottom.unwrap_or(Edge::Tiles);
    Ok(col)
}

/// Stacks pieces (at least one) with the Figure 3-17 spacing between them.
fn stack(pieces: Vec<Piece>, ctx: &Context) -> Piece {
    let mut items = Vec::new();
    let (mut down, mut width) = (0.0, 0.0f64);
    let top = pieces[0].top;
    let all_tiles = pieces.iter().all(|x| x.all_tiles);
    let name_lines = pieces.iter().map(|x| x.name_lines).sum();
    let (mut bottom, mut bottom_descender) = (top, false);
    let mut prev: Option<Edge> = None;
    for piece in pieces {
        if let Some(prev_bottom) = prev {
            down += ctx.block_spacing(prev_bottom, piece.top);
        }
        items.extend(piece.items.into_iter().map(|it| it.shifted(0.0, down)));
        width = width.max(piece.width);
        down += piece.height;
        prev = Some(piece.bottom);
        bottom = piece.bottom;
        bottom_descender = piece.bottom_descender;
    }
    Piece {
        items,
        width,
        height: down,
        top,
        bottom,
        all_tiles,
        name_lines,
        bottom_descender,
    }
}

fn build_panel(
    blocks: &[Vec<&Destination>],
    panel: Colour,
    sign_ground: Colour,
    ctx: &Context,
) -> Result<Piece, SignError> {
    let r = ctx
        .rules
        .expect("a panel colour comes from the block rules");
    let (lettering, face) = lettering_on(panel);
    let border = if is_dark(panel) && is_dark(sign_ground) {
        r.edge_border
    } else {
        0.0
    };
    let pieces = blocks
        .iter()
        .map(|b| build_block(b, panel, lettering, face, ctx, border + r.panel_pad_side))
        .collect::<Result<Vec<_>, _>>()?;
    let content = stack(pieces, ctx);

    let pad_bottom = if content.bottom == Edge::Tiles {
        r.panel_pad_bottom_tiles
    } else {
        r.panel_pad_bottom_patch
    };
    let inner_w = r.panel_pad_side + content.width + r.panel_pad_side;
    let inner_h = r.panel_pad_top + content.height + pad_bottom;

    let mut items = Vec::new();
    if border > 0.0 {
        items.push(Item::Rect {
            x: 0.0,
            down: 0.0,
            w: inner_w + 2.0 * border,
            h: inner_h + 2.0 * border,
            fill: Colour::White,
        });
    }
    items.push(Item::Rect {
        x: border,
        down: border,
        w: inner_w,
        h: inner_h,
        fill: panel,
    });
    items.extend(
        content
            .items
            .into_iter()
            .map(|it| it.shifted(border + r.panel_pad_side, border + r.panel_pad_top)),
    );

    Ok(Piece {
        items,
        width: inner_w + 2.0 * border,
        height: inner_h + 2.0 * border,
        top: Edge::Patch,
        bottom: Edge::Patch,
        all_tiles: false,
        name_lines: content.name_lines,
        bottom_descender: false,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Placement {
    None,
    SameLine,
    Alongside,
    Below,
}

struct Line<'a> {
    dest: &'a Destination,
    tile_down: f64,
    run: f64,
    name_right: f64,
}

fn max_of(values: impl Iterator<Item = f64>) -> f64 {
    values.fold(f64::NEG_INFINITY, f64::max)
}

/// `offset_x`: how far the block sits right of the column's left edge.
fn build_block(
    dests: &[&Destination],
    ground: Colour,
    lettering: Colour,
    face: FontFace,
    ctx: &Context,
    offset_x: f64,
) -> Result<Piece, SignError> {
    let mut items: Vec<Item> = Vec::new();
    let r = ctx.rules;
    let word_gap = ctx.panel.word_spacing;
    let number: Option<String> = match (r, &dests[0].route_number) {
        (Some(_), Some(n)) if !n.is_empty() => Some(n.trim().to_string()),
        _ => None,
    };
    let has_distances = dests.iter().any(|d| has_text(&d.distance));
    let mut any_symbol = false;

    // Route number geometry, before placing names.
    let rn = match (&number, r) {
        (Some(number), Some(rules)) => Some(shape(
            number, dests[0], ground, lettering, face, rules, ctx,
        )?),
        _ => None,
    };
    let mode = if rn.is_none() {
        Placement::None
    } else if has_distances || dests.len() >= 3 {
        Placement::Below
    } else if dests.len() == 1 {
        Placement::SameLine
    } else {
        Placement::Alongside
    };

    // Name lines, butted (§3.7.1). A line holding a symbol grows to fit it with the symbol
    // raised 0.5 sw above the tile centre (§3.5.1).
    let mut lines: Vec<Line> = Vec::new();
    let mut down = 0.0;
    for d in dests {
        let name_w = ctx.width(&d.name, face)?;
        let sym = d
            .symbol_id
            .as_deref()
            .map_or(0.0, |sid| (ctx.symbol_size)(sid));
        let line_h = TILE_HEIGHT.max(if sym > 0.0 { sym + 1.0 } else { 0.0 });
        let tile_down = down + (line_h - TILE_HEIGHT) / 2.0;

        let mut x = 0.0;
        if sym > 0.0 && ctx.symbols_leading {
            items.push(symbol_for(d, x, tile_down, sym, lettering));
            x += sym + word_gap;
            any_symbol = true;
        }
        items.push(Item::Text {
            x,
            down: tile_down,
            text: d.name.clone(),
            face,
            ink: lettering,
        });
        x += name_w;
        let name_right = x;
        if let (Placement::SameLine, Some(rules), Some(rn)) = (mode, r, &rn) {
            x += rules.name_to_route_number + rn.outer_width;
        }
        if sym > 0.0 && !ctx.symbols_leading {
            x += word_gap;
            items.push(symbol_for(d, x, tile_down, sym, lettering));
            x += sym;
            any_symbol = true;
        }
        lines.push(Line {
            dest: d,
            tile_down,
            run: x,
            name_right,
        });
        down += line_h;
    }
    let names_height = down;
    let last_name = &dests[dests.len() - 1].name;

    // Route number.
    let (mut top, mut bottom) = (Edge::Tiles, Edge::Tiles);
    let (mut min_down, mut max_down) = (0.0, names_height);
    let mut width = max_of(lines.iter().map(|l| l.run));
    if let (Some(rn), Some(rules), Some(number)) = (&rn, r, &number) {
        let (tile_down, x);
        match mode {
            Placement::SameLine => {
                // §3.5.2: the route-number tiles align with the name's tiles.
                tile_down = lines[0].tile_down;
                x = lines[0].name_right + rules.name_to_route_number;
            }
            Placement::Alongside => {
                x = max_of(lines.iter().map(|l| l.name_right)) + rules.name_to_route_number;
                // Figure 3-12: a patch is centred on the names and raised 0.5 sw; plain tiles
                // are centred (Figure 3-16).
                tile_down = if rn.patched {
                    (names_height - rn.outer_height) / 2.0 - SYMBOL_RAISE + rn.tile_from_outer_top
                } else {
                    (names_height - TILE_HEIGHT) / 2.0
                };
                width = width.max(x + rn.outer_width);
            }
            _ => {
                // §3.7.2–3.7.3 line spacing, then ranged left.
                let spacing = if rn.patched {
                    if has_descender(last_name) {
                        rules.line_spacing_patch_descender
                    } else {
                        rules.line_spacing_patch
                    }
                } else if number.starts_with('(') {
                    rules.line_spacing_bracketed
                } else {
                    0.0
                };
                x = 0.0;
                tile_down = names_height + spacing + rn.tile_from_outer_top;
                width = width.max(rn.outer_width);
            }
        }
        items.extend(rn.items(x, tile_down, ctx)?);
        let outer_top = tile_down - rn.tile_from_outer_top;
        let outer_bottom = outer_top + rn.outer_height;
        if outer_top < min_down - 1e-9 {
            min_down = outer_top;
            top = Edge::Patch;
        }
        // A patch level with the tiles still forms the edge (Figure 3-17 spaces it as a patch).
        if outer_bottom > max_down - 1e-9 {
            max_down = max_down.max(outer_bottom);
            bottom = if rn.patched { Edge::Patch } else { Edge::Tiles };
        }
    }

    // Distances: a centred column clear of the longest name line (§3.9.1–3.9.2), the block's
    // own unless the column aligns them.
    if has_distances {
        let longest = max_of(lines.iter().map(|l| l.run));
        let mut dist_w = 0.0f64;
        for l in &lines {
            if let Some(dd) = l.dest.distance.as_deref().filter(|d| !d.is_empty()) {
                dist_w = dist_w.max(ctx.width(dd, face)?);
            }
        }
        ctx.probe_clear_right
            .set(ctx.probe_clear_right.get().max(offset_x + longest));
        ctx.probe_width.set(ctx.probe_width.get().max(dist_w));
        let centre = match ctx.distance_centre {
            Some(shared) => shared - offset_x,
            None => longest + ctx.distance_gap + dist_w / 2.0,
        };
        for l in &lines {
            if let Some(dd) = l.dest.distance.as_deref().filter(|d| !d.is_empty()) {
                items.push(Item::Text {
                    x: centre - ctx.width(dd, face)? / 2.0,
                    down: l.tile_down,
                    text: dd.to_string(),
                    face,
                    ink: lettering,
                });
            }
        }
        width = match ctx.distance_centre {
            None => width.max(longest + ctx.distance_gap + dist_w),
            Some(_) => width.max(centre + ctx.distance_width / 2.0),
        };
    }

    // The bottom edge is the last name's tiles unless a route number or patch is lower.
    let bottom_descender =
        bottom == Edge::Tiles && max_down <= names_height + 1e-9 && has_descender(last_name);

    Ok(Piece {
        items: items
            .into_iter()
            .map(|it| it.shifted(0.0, -min_down))
            .collect(),
        width,
        height: max_down - min_down,
        top,
        bottom,
        all_tiles: !any_symbol && !rn.as_ref().is_some_and(|rn| rn.patched),
        name_lines: dests.len(),
        bottom_descender,
    })
}

/// A route number's tiles, with or without a patch, ready to place.
struct RouteNumberShape {
    patched: bool,
    outer_width: f64,
    outer_height: f64,
    tile_from_outer_top: f64,
    runs: Vec<RouteNumberRun>,
    letter_gap: f64,
    word_gap: f64,
    face: FontFace,
    ink: Colour,
    /// The patch behind the tiles: white edge border width, inner size and colour.
    patch: Option<Patch>,
}

struct Patch {
    border: f64,
    pad_side: f64,
    inner_w: f64,
    inner_h: f64,
    fill: Colour,
}

impl RouteNumberShape {
    fn gap(&self, g: RouteGap) -> f64 {
        match g {
            RouteGap::Letter => self.letter_gap,
            RouteGap::Word => self.word_gap,
            RouteGap::None => 0.0,
        }
    }

    /// Items for a route number whose tiles start at `x` and whose tile top is at `tile_down`.
    fn items(&self, x: f64, tile_down: f64, ctx: &Context) -> Result<Vec<Item>, SignError> {
        let mut list = Vec::new();
        let mut x = x;
        if let Some(p) = &self.patch {
            let outer_top = tile_down - self.tile_from_outer_top;
            if p.border > 0.0 {
                list.push(Item::Rect {
                    x,
                    down: outer_top,
                    w: self.outer_width,
                    h: self.outer_height,
                    fill: Colour::White,
                });
            }
            list.push(Item::Rect {
                x: x + p.border,
                down: outer_top + p.border,
                w: p.inner_w,
                h: p.inner_h,
                fill: p.fill,
            });
            x = x + p.border + p.pad_side;
        }
        for run in &self.runs {
            x += self.gap(run.gap_before);
            list.push(Item::Text {
                x,
                down: tile_down,
                text: run.text.clone(),
                face: self.face,
                ink: self.ink,
            });
            x += ctx.width(&run.text, self.face)?;
        }
        Ok(list)
    }
}

fn shape(
    number: &str,
    d: &Destination,
    ground: Colour,
    lettering: Colour,
    face: FontFace,
    r: &BlockRules,
    ctx: &Context,
) -> Result<RouteNumberShape, SignError> {
    let runs = route_numbers::runs(number);
    let (letter_gap, word_gap) = (r.route_letter_gap, ctx.panel.word_spacing);
    // Route-number tiles are measured in the face they are drawn in: the block's own on its
    // ground, or the patch's (e.g. white Medium on a blue patch on a white sign).
    let tiles_width = |run_face: FontFace| -> Result<f64, SignError> {
        let mut total = 0.0;
        for run in &runs {
            let gap = match run.gap_before {
                RouteGap::Letter => letter_gap,
                RouteGap::Word => word_gap,
                RouteGap::None => 0.0,
            };
            total += gap + ctx.width(&run.text, run_face)?;
        }
        Ok(total)
    };

    let status = route_numbers::patch_status(d);
    let patch = r
        .colour_for(status)
        .filter(|_| rank(status) > ground_rank(ground, r));

    let Some(fill) = patch else {
        let tiles_w = tiles_width(face)?;
        let ink = r.ink_on(ground, lettering);
        return Ok(RouteNumberShape {
            patched: false,
            outer_width: tiles_w,
            outer_height: TILE_HEIGHT,
            tile_from_outer_top: 0.0,
            runs,
            letter_gap,
            word_gap,
            face,
            ink,
            patch: None,
        });
    };

    let (patch_lettering, patch_face) = lettering_on(fill);
    let border = if is_dark(fill) && is_dark(ground) {
        r.edge_border
    } else {
        0.0
    };
    let inner_w = r.patch_pad_side + tiles_width(patch_face)? + r.patch_pad_side;
    let inner_h = r.patch_rise + TILE_HEIGHT;
    let (outer_w, outer_h) = (inner_w + 2.0 * border, inner_h + 2.0 * border);
    let patch_ink = r.ink_on(fill, patch_lettering);
    let tile_from_top = border + r.patch_rise;

    Ok(RouteNumberShape {
        patched: true,
        outer_width: outer_w,
        outer_height: outer_h,
        tile_from_outer_top: tile_from_top,
        runs,
        letter_gap,
        word_gap,
        face: patch_face,
        ink: patch_ink,
        patch: Some(Patch {
            border,
            pad_side: r.patch_pad_side,
            inner_w,
            inner_h,
            fill,
        }),
    })
}

fn symbol_for(d: &Destination, x: f64, tile_down: f64, size: f64, ink: Colour) -> Item {
    let rot = symbol::clamp_not_below_horizontal(symbol::rotation_for(d.direction));
    let centre_down = tile_down + TILE_HEIGHT / 2.0 - SYMBOL_RAISE;
    Item::Symbol {
        x,
        down: centre_down - size / 2.0,
        symbol_id: d.symbol_id.clone().unwrap_or_default(),
        size,
        rotation: rot,
        ink,
    }
}

fn rank(s: RouteStatus) -> u8 {
    match s {
        RouteStatus::Motorway => 3,
        RouteStatus::Primary => 2,
        RouteStatus::NonPrimary => 1,
        RouteStatus::Unspecified => 0,
    }
}

/// The status a ground colour stands for; grounds that stand for no route status (brown,
/// yellow) rank with non-primary routes.
fn ground_rank(ground: Colour, r: &BlockRules) -> u8 {
    [
        RouteStatus::Motorway,
        RouteStatus::Primary,
        RouteStatus::NonPrimary,
    ]
    .into_iter()
    .find(|s| r.colour_for(*s) == Some(ground))
    .map_or(rank(RouteStatus::NonPrimary), rank)
}

fn is_dark(c: Colour) -> bool {
    !matches!(c, Colour::White | Colour::Yellow | Colour::Grey)
}

/// §2.1.1: white Transport Medium on dark grounds, black Transport Heavy on light.
fn lettering_on(ground: Colour) -> (Colour, FontFace) {
    if is_dark(ground) {
        (Colour::White, FontFace::TransportMedium)
    } else {
        (Colour::Black, FontFace::TransportHeavy)
    }
}

/// Lower-case letters with descenders on the Transport tiles (Figure 2-1).
fn has_descender(text: &str) -> bool {
    text.contains(['g', 'j', 'p', 'q', 'y'])
}
