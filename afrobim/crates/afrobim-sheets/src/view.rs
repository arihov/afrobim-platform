// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! A view on a sheet: what it shows and where it sits.

use serde::{Deserialize, Serialize};

use crate::detail::{compose_detail, DetailView};
use crate::geom::{Fraction, Rect};
use crate::item::{layers, ComposedSheet};
use crate::model::{Ctx, Model};
use crate::paper::Scale;
use crate::plan::{compose_plan, PlanView};
use crate::profile::{compose_profile, ProfileView};
use crate::sections::{compose_sections, SectionsView};
use crate::table::{derived_values_table, draw_notes, draw_table_in, Notes, Table};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ViewKind {
    Plan(PlanView),
    Profile(ProfileView),
    Sections(SectionsView),
    Table(Table),
    Detail(DetailView),
    Notes(Notes),
}

impl ViewKind {
    /// The drawing scale of the view, for the title block's scale field; `None` for tables and notes.
    pub fn scale(&self) -> Option<Scale> {
        match self {
            ViewKind::Plan(p) => Some(p.scale),
            ViewKind::Profile(p) => Some(p.h_scale),
            ViewKind::Sections(s) => Some(s.scale),
            ViewKind::Detail(d) if !d.fit => Some(d.scale),
            _ => None,
        }
    }
}

/// A view and the share of the drawing area it takes (fractions from the area's lower left).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewDef {
    pub area: Fraction,
    pub kind: ViewKind,
}

impl ViewDef {
    /// A view that fills the whole drawing area.
    pub fn full(kind: ViewKind) -> ViewDef {
        ViewDef { area: Fraction::FULL, kind }
    }

    /// A view in part of the drawing area, from `x0, y0` to `x1, y1` as fractions.
    pub fn in_area(x0: f64, y0: f64, x1: f64, y1: f64, kind: ViewKind) -> ViewDef {
        ViewDef { area: Fraction { x0, y0, x1, y1 }, kind }
    }
}

/// Composes one view into its part of `drawing_area`.
pub fn compose_view(sheet: &mut ComposedSheet, ctx: &Ctx, def: &ViewDef, drawing_area: &Rect) {
    let gap = 1.0;
    let rect = drawing_area.fraction(&def.area).inset(gap);
    match &def.kind {
        ViewKind::Plan(v) => compose_plan(sheet, ctx, v, &rect),
        ViewKind::Profile(v) => compose_profile(sheet, ctx, v, &rect),
        ViewKind::Sections(v) => {
            compose_sections(sheet, ctx, v, &rect);
        }
        ViewKind::Table(t) => {
            draw_table_in(sheet, t, &rect.inset(2.0), ctx.styles);
        }
        ViewKind::Detail(v) => {
            compose_detail(sheet, ctx, v, &rect);
        }
        ViewKind::Notes(n) => {
            let pen = ctx.styles.pens.pen(layers::VIEWPORT);
            sheet.rect(layers::VIEWPORT, &rect, pen.weight_mm, pen.rgb);
            draw_notes(sheet, n, &rect.inset(3.0), ctx.styles);
        }
    }
}

/// The values modules report for the elements of `category` (all categories when `None`), as a
/// table of element, quantity and value. `skip_keys` leaves out derived keys (plan origins and
/// the like) by exact name. Elements whose module cannot generate are skipped.
pub fn module_quantities(model: &Model, title: &str, category: Option<&str>, skip_keys: &[&str]) -> Table {
    let mut rows: Vec<(String, String, String)> = Vec::new();
    for e in model.doc.elements() {
        if category.is_some_and(|c| e.category != c) || model.registry.get(&e.category).is_none() {
            continue;
        }
        if let Ok(g) = model.registry.generate(e) {
            for (k, v) in &g.derived {
                if !skip_keys.contains(&k.as_str()) {
                    rows.push((e.name.clone(), k.clone(), v.clone()));
                }
            }
        }
    }
    derived_values_table(title, &rows)
}
