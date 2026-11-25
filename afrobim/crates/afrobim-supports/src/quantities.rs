// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Take-off for posts and foundations, by geometry only.
//!
//! Every figure here is the entered dimensions put through a formula stated on the field. No
//! figure comes from a display mesh, a section table or a rate.

use afrobim_kernel::Element;

use crate::{
    positive, FoundationDefinition, PostDefinition, SupportError, FOUNDATION_CATEGORY, POST_CATEGORY, STEEL_DENSITY_KG_M3_NOMINAL,
};

/// One post measured.
#[derive(Debug, Clone, PartialEq)]
pub struct PostQuantities {
    pub mark: String,
    /// [`crate::Section::label`]: a description of the entered dimensions.
    pub section: String,
    pub material: String,
    pub finish: String,
    /// Total length including the buried part.
    pub length_mm: f64,
    pub embedment_mm: f64,
    /// Area of material in the cross-section ([`crate::Section::area_mm2`]); a hollow section
    /// excludes its void.
    pub section_area_mm2: f64,
    /// Section area × total length.
    pub volume_m3: f64,
    /// Outside outline of the section × total length. Outside faces only: the bore of a hollow
    /// section and the two cut ends are not included.
    pub coating_area_m2: f64,
    /// The same over the length above ground only.
    pub coating_area_above_ground_m2: f64,
    /// The density the mass was computed with, as supplied by the caller.
    pub density_kg_m3: f64,
    /// Volume × density.
    pub mass_kg: f64,
}

impl PostQuantities {
    /// Measures one post. Fails when the definition has Error notes or the density is not
    /// positive.
    pub fn measure(mark: &str, def: &PostDefinition, density_kg_m3: f64) -> Result<PostQuantities, SupportError> {
        def.check()?;
        if !positive(density_kg_m3) {
            return Err(SupportError::InvalidOption(format!("the density must be greater than zero (got {density_kg_m3})")));
        }
        let area = def.section.area_mm2();
        let perimeter = def.section.outer_perimeter_mm();
        let volume_m3 = area * def.length_mm / 1.0e9;
        Ok(PostQuantities {
            mark: mark.to_string(),
            section: def.section.label(),
            material: def.material.clone(),
            finish: def.finish.clone(),
            length_mm: def.length_mm,
            embedment_mm: def.embedment_mm,
            section_area_mm2: area,
            volume_m3,
            coating_area_m2: perimeter * def.length_mm / 1.0e6,
            coating_area_above_ground_m2: perimeter * def.above_ground_mm() / 1.0e6,
            density_kg_m3,
            mass_kg: volume_m3 * density_kg_m3,
        })
    }
}

/// One foundation measured.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundationQuantities {
    pub mark: String,
    /// [`crate::FoundationShape::label`].
    pub shape: String,
    pub concrete: String,
    pub top_below_ground_mm: f64,
    /// Zero when there is no blinding.
    pub blinding_mm: f64,
    /// Plan area × depth, before the socket is taken out.
    pub gross_volume_m3: f64,
    /// Volume of the socket void; zero without one.
    pub socket_void_m3: f64,
    /// Gross volume − socket void. Not reduced for a post cast directly into the concrete.
    pub concrete_m3: f64,
    /// Plan area of the foundation × blinding thickness.
    pub blinding_m3: f64,
    /// The working space the excavation was computed with, as supplied by the caller.
    pub working_space_mm: f64,
    /// A plain vertical-sided prism: the plan outline grown by the working space on every side
    /// × (top below ground + depth + blinding). No batter, bulking or overbreak.
    pub excavation_m3: f64,
    /// Plan perimeter × depth: the vertical faces of the concrete. This is the formwork area if
    /// the sides are formed; whether they are formed or cast against the ground is for the
    /// engineer to say.
    pub side_area_m2: f64,
}

impl FoundationQuantities {
    /// Measures one foundation. Fails when the definition has Error notes or the working space
    /// is negative.
    pub fn measure(mark: &str, def: &FoundationDefinition, working_space_mm: f64) -> Result<FoundationQuantities, SupportError> {
        def.check()?;
        if !(working_space_mm.is_finite() && working_space_mm >= 0.0) {
            return Err(SupportError::InvalidOption(format!("the working space must not be negative (got {working_space_mm})")));
        }
        let depth = def.shape.depth_mm();
        let gross = def.shape.plan_area_mm2() * depth / 1.0e9;
        let socket = def.socket.as_ref().map_or(0.0, |s| s.volume_mm3() / 1.0e9);
        Ok(FoundationQuantities {
            mark: mark.to_string(),
            shape: def.shape.label(),
            concrete: def.concrete.clone(),
            top_below_ground_mm: def.top_below_ground_mm,
            blinding_mm: def.blinding_thickness_mm(),
            gross_volume_m3: gross,
            socket_void_m3: socket,
            concrete_m3: gross - socket,
            blinding_m3: def.shape.plan_area_mm2() * def.blinding_thickness_mm() / 1.0e9,
            working_space_mm,
            excavation_m3: def.shape.plan_area_with_margin_mm2(working_space_mm) * def.formation_depth_mm() / 1.0e9,
            side_area_m2: def.shape.plan_perimeter_mm() * depth / 1.0e6,
        })
    }
}

/// The values a take-off needs from the caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuantityOptions {
    /// Density for post mass, kg/m³. Defaults to [`STEEL_DENSITY_KG_M3_NOMINAL`]; pass the
    /// specified density of the actual material to override it.
    pub density_kg_m3: f64,
    /// Working space added on every side of a foundation for the excavation volume, mm.
    /// Defaults to zero.
    pub working_space_mm: f64,
}

impl Default for QuantityOptions {
    fn default() -> Self {
        QuantityOptions { density_kg_m3: STEEL_DENSITY_KG_M3_NOMINAL, working_space_mm: 0.0 }
    }
}

/// A post or foundation that could not be measured, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct SkippedElement {
    pub mark: String,
    pub reason: String,
}

/// The take-off of every post and foundation in a list of elements.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SupportQuantities {
    pub posts: Vec<PostQuantities>,
    pub foundations: Vec<FoundationQuantities>,
    /// Posts and foundations whose definition is unreadable or has Error notes. They are in no
    /// total; a schedule with anything here is incomplete.
    pub skipped: Vec<SkippedElement>,
}

impl SupportQuantities {
    /// Measures the posts and foundations among `elements`, in order. Elements of other
    /// categories are ignored. Fails only when an option is invalid.
    pub fn measure<'a>(elements: impl IntoIterator<Item = &'a Element>, options: &QuantityOptions) -> Result<SupportQuantities, SupportError> {
        if !positive(options.density_kg_m3) {
            return Err(SupportError::InvalidOption(format!("the density must be greater than zero (got {})", options.density_kg_m3)));
        }
        if !(options.working_space_mm.is_finite() && options.working_space_mm >= 0.0) {
            return Err(SupportError::InvalidOption(format!("the working space must not be negative (got {})", options.working_space_mm)));
        }
        let mut out = SupportQuantities::default();
        for e in elements {
            let skip = |err: SupportError| SkippedElement { mark: e.name.clone(), reason: err.to_string() };
            match e.category.as_str() {
                POST_CATEGORY => {
                    match PostDefinition::from_value(&e.data).and_then(|d| PostQuantities::measure(&e.name, &d, options.density_kg_m3)) {
                        Ok(q) => out.posts.push(q),
                        Err(err) => out.skipped.push(skip(err)),
                    }
                }
                FOUNDATION_CATEGORY => {
                    match FoundationDefinition::from_value(&e.data)
                        .and_then(|d| FoundationQuantities::measure(&e.name, &d, options.working_space_mm))
                    {
                        Ok(q) => out.foundations.push(q),
                        Err(err) => out.skipped.push(skip(err)),
                    }
                }
                _ => {}
            }
        }
        Ok(out)
    }

    pub fn post_length_m(&self) -> f64 {
        self.posts.iter().map(|p| p.length_mm).sum::<f64>() / 1000.0
    }
    pub fn post_volume_m3(&self) -> f64 {
        self.posts.iter().map(|p| p.volume_m3).sum()
    }
    pub fn post_coating_area_m2(&self) -> f64 {
        self.posts.iter().map(|p| p.coating_area_m2).sum()
    }
    pub fn post_coating_area_above_ground_m2(&self) -> f64 {
        self.posts.iter().map(|p| p.coating_area_above_ground_m2).sum()
    }
    pub fn post_mass_kg(&self) -> f64 {
        self.posts.iter().map(|p| p.mass_kg).sum()
    }
    pub fn concrete_m3(&self) -> f64 {
        self.foundations.iter().map(|f| f.concrete_m3).sum()
    }
    pub fn blinding_m3(&self) -> f64 {
        self.foundations.iter().map(|f| f.blinding_m3).sum()
    }
    pub fn excavation_m3(&self) -> f64 {
        self.foundations.iter().map(|f| f.excavation_m3).sum()
    }
    pub fn side_area_m2(&self) -> f64 {
        self.foundations.iter().map(|f| f.side_area_m2).sum()
    }
}

pub const POSTS_CSV_HEADER: &str = "mark,section,material,finish,length_mm,embedment_mm,section_area_mm2,volume_m3,coating_area_m2,coating_area_above_ground_m2,density_kg_m3,mass_kg";

/// Post schedule, one row per post, then a `TOTAL` row carrying the number of posts in the
/// second column and the summed length, volume, coating areas and mass. Lines end with `\n`.
///
/// Rounding: mm and mm² to at most 1 decimal, volume to 6 decimals (a post is a few litres of
/// steel), areas to 3, mass to 2. Totals are sums of the unrounded values, rounded once.
pub fn posts_to_csv(rows: &[PostQuantities]) -> String {
    let mut out = String::from(POSTS_CSV_HEADER);
    out.push('\n');
    for r in rows {
        let fields = [
            csv(&r.mark),
            csv(&r.section),
            csv(&r.material),
            csv(&r.finish),
            fixed(r.length_mm, 0, 1),
            fixed(r.embedment_mm, 0, 1),
            fixed(r.section_area_mm2, 0, 1),
            fixed(r.volume_m3, 6, 6),
            fixed(r.coating_area_m2, 3, 3),
            fixed(r.coating_area_above_ground_m2, 3, 3),
            fixed(r.density_kg_m3, 0, 1),
            fixed(r.mass_kg, 2, 2),
        ];
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out.push_str(&format!(
        "TOTAL,{},,,{},,,{},{},{},,{}\n",
        rows.len(),
        fixed(rows.iter().map(|r| r.length_mm).sum::<f64>(), 0, 1),
        fixed(rows.iter().map(|r| r.volume_m3).sum::<f64>(), 6, 6),
        fixed(rows.iter().map(|r| r.coating_area_m2).sum::<f64>(), 3, 3),
        fixed(rows.iter().map(|r| r.coating_area_above_ground_m2).sum::<f64>(), 3, 3),
        fixed(rows.iter().map(|r| r.mass_kg).sum::<f64>(), 2, 2),
    ));
    out
}

pub const FOUNDATIONS_CSV_HEADER: &str = "mark,shape,concrete,top_below_ground_mm,blinding_mm,gross_volume_m3,socket_void_m3,concrete_m3,blinding_m3,working_space_mm,excavation_m3,side_area_m2";

/// Foundation schedule, one row per foundation, then a `TOTAL` row carrying the number of
/// foundations in the second column and the summed volumes and side area. Lines end with `\n`.
/// Volumes and areas are to 3 decimals, mm to at most 1.
pub fn foundations_to_csv(rows: &[FoundationQuantities]) -> String {
    let mut out = String::from(FOUNDATIONS_CSV_HEADER);
    out.push('\n');
    for r in rows {
        let fields = [
            csv(&r.mark),
            csv(&r.shape),
            csv(&r.concrete),
            fixed(r.top_below_ground_mm, 0, 1),
            fixed(r.blinding_mm, 0, 1),
            fixed(r.gross_volume_m3, 3, 3),
            fixed(r.socket_void_m3, 3, 3),
            fixed(r.concrete_m3, 3, 3),
            fixed(r.blinding_m3, 3, 3),
            fixed(r.working_space_mm, 0, 1),
            fixed(r.excavation_m3, 3, 3),
            fixed(r.side_area_m2, 3, 3),
        ];
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    let sum = |f: fn(&FoundationQuantities) -> f64| fixed(rows.iter().map(f).sum::<f64>(), 3, 3);
    out.push_str(&format!(
        "TOTAL,{},,,,{},{},{},{},,{},{}\n",
        rows.len(),
        sum(|r| r.gross_volume_m3),
        sum(|r| r.socket_void_m3),
        sum(|r| r.concrete_m3),
        sum(|r| r.blinding_m3),
        sum(|r| r.excavation_m3),
        sum(|r| r.side_area_m2),
    ));
    out
}

pub const SUMMARY_CSV_HEADER: &str = "item,unit,quantity";

/// The totals as one bill-style list. Lines end with `\n`. A final `not measured` row counts
/// the skipped elements when there are any.
pub fn summary_to_csv(q: &SupportQuantities) -> String {
    let mut out = String::from(SUMMARY_CSV_HEADER);
    out.push('\n');
    let mut row = |item: &str, unit: &str, quantity: String| out.push_str(&format!("{item},{unit},{quantity}\n"));
    row("posts", "nr", q.posts.len().to_string());
    row("post length", "m", fixed(q.post_length_m(), 3, 3));
    row("post material volume", "m3", fixed(q.post_volume_m3(), 6, 6));
    row("post coating area (outside only)", "m2", fixed(q.post_coating_area_m2(), 3, 3));
    row("post mass", "kg", fixed(q.post_mass_kg(), 2, 2));
    row("foundations", "nr", q.foundations.len().to_string());
    row("foundation concrete", "m3", fixed(q.concrete_m3(), 3, 3));
    row("blinding", "m3", fixed(q.blinding_m3(), 3, 3));
    row("excavation", "m3", fixed(q.excavation_m3(), 3, 3));
    row("foundation side face area", "m2", fixed(q.side_area_m2(), 3, 3));
    if !q.skipped.is_empty() {
        row("not measured (invalid definition)", "nr", q.skipped.len().to_string());
    }
    out
}

/// `v` rounded to `max_decimals`, with trailing zeros removed down to `min_decimals`.
pub(crate) fn fixed(v: f64, min_decimals: usize, max_decimals: usize) -> String {
    let mut s = format!("{v:.max_decimals$}");
    if let Some(dot) = s.find('.') {
        while s.len() > dot + 1 + min_decimals && s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s.starts_with('-') && s.chars().all(|c| matches!(c, '-' | '0' | '.')) {
        s.remove(0);
    }
    s
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
    use crate::{FoundationShape, Section, Socket};
    use afrobim_kernel::{Document, Pt};
    use std::f64::consts::PI;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * b.abs().max(1.0)
    }

    fn tube() -> PostDefinition {
        PostDefinition {
            section: Section::Chs { outside_diameter_mm: 100.0, wall_mm: 10.0 },
            length_mm: 4000.0,
            embedment_mm: 1000.0,
            material: "as specified".into(),
            finish: "as specified".into(),
            placeholder: false,
        }
    }

    fn block() -> FoundationDefinition {
        FoundationDefinition {
            shape: FoundationShape::Block { length_mm: 600.0, width_mm: 500.0, depth_mm: 900.0 },
            top_below_ground_mm: 100.0,
            concrete: "as specified".into(),
            blinding_mm: Some(50.0),
            socket: Some(Socket::Round { diameter_mm: 150.0, depth_mm: 500.0 }),
            placeholder: false,
        }
    }

    #[test]
    fn tube_post_by_hand() {
        let q = PostQuantities::measure("P1", &tube(), STEEL_DENSITY_KG_M3_NOMINAL).unwrap();
        // A = π/4 · (100² − 80²) = 900π mm² = 2827.4334 mm².
        assert!(close(q.section_area_mm2, 900.0 * PI));
        // V = 900π × 4000 = 3 600 000π mm³ = 11 309 733.55 mm³ = 0.01130973355 m³.
        assert!(close(q.volume_m3, 0.011309733552923255));
        // Outside surface = π × 100 × 4000 = 400 000π mm² = 1.2566370614 m².
        assert!(close(q.coating_area_m2, 1.2566370614359172));
        // Above ground only (3000 mm): 300 000π mm² = 0.9424777961 m².
        assert!(close(q.coating_area_above_ground_m2, 0.9424777960769379));
        // Mass = 0.01130973355 m³ × 7850 kg/m³ = 88.78140839 kg.
        assert!(close(q.mass_kg, 88.78140839044755));
        assert_eq!(q.density_kg_m3, 7850.0);
        // A different density is used as given: × 2700 = 30.53628059 kg.
        let al = PostQuantities::measure("P1", &tube(), 2700.0).unwrap();
        assert!(close(al.mass_kg, 30.536280592892787));
        assert!(matches!(PostQuantities::measure("P1", &tube(), 0.0), Err(SupportError::InvalidOption(_))));
        let mut bad = tube();
        bad.embedment_mm = 4000.0;
        assert!(matches!(PostQuantities::measure("P1", &bad, 7850.0), Err(SupportError::InvalidDefinition(_))));
    }

    #[test]
    fn rectangular_hollow_post_by_hand() {
        let def = PostDefinition {
            section: Section::Rhs { width_mm: 100.0, depth_mm: 50.0, wall_mm: 5.0, outer_corner_radius_mm: 0.0, inner_corner_radius_mm: 0.0 },
            length_mm: 3000.0,
            embedment_mm: 600.0,
            ..tube()
        };
        let q = PostQuantities::measure("P1", &def, 7850.0).unwrap();
        // A = 100 × 50 − 90 × 40 = 1400 mm². V = 1400 × 3000 = 4 200 000 mm³ = 0.0042 m³.
        assert!(close(q.section_area_mm2, 1400.0));
        assert!(close(q.volume_m3, 0.0042));
        // Mass = 0.0042 × 7850 = 32.97 kg. Outside = 300 × 3000 mm² = 0.9 m²; above ground 0.72 m².
        assert!(close(q.mass_kg, 32.97));
        assert!(close(q.coating_area_m2, 0.9));
        assert!(close(q.coating_area_above_ground_m2, 0.72));
    }

    #[test]
    fn block_foundation_less_its_socket_by_hand() {
        let q = FoundationQuantities::measure("F1", &block(), 150.0).unwrap();
        // Gross: 0.6 × 0.5 × 0.9 = 0.27 m³.
        assert!(close(q.gross_volume_m3, 0.27));
        // Socket: π/4 × 150² × 500 = 2 812 500π mm³ = 8 835 729.34 mm³ = 0.00883572934 m³.
        assert!(close(q.socket_void_m3, 0.008835729338221293));
        // Concrete: 0.27 − 0.00883572934 = 0.26116427066 m³.
        assert!(close(q.concrete_m3, 0.2611642706617787));
        // Blinding: 0.6 × 0.5 × 0.05 = 0.015 m³.
        assert!(close(q.blinding_m3, 0.015));
        // Excavation: (600 + 300)(500 + 300) mm² × (100 + 900 + 50) mm = 0.9 × 0.8 × 1.05 = 0.756 m³.
        assert!(close(q.excavation_m3, 0.756));
        // Side faces: 2 × (600 + 500) × 900 mm² = 1.98 m².
        assert!(close(q.side_area_m2, 1.98));
        // No working space: 0.6 × 0.5 × 1.05 = 0.315 m³.
        assert!(close(FoundationQuantities::measure("F1", &block(), 0.0).unwrap().excavation_m3, 0.315));
        assert!(matches!(FoundationQuantities::measure("F1", &block(), -1.0), Err(SupportError::InvalidOption(_))));
        // A rectangular pocket 200 × 150 × 400 = 0.012 m³: concrete 0.258 m³.
        let pocket = FoundationDefinition { socket: Some(Socket::Rect { length_mm: 200.0, width_mm: 150.0, depth_mm: 400.0 }), ..block() };
        assert!(close(FoundationQuantities::measure("F1", &pocket, 0.0).unwrap().concrete_m3, 0.258));
    }

    #[test]
    fn cylinder_foundation_by_hand() {
        let def = FoundationDefinition {
            shape: FoundationShape::Cylinder { diameter_mm: 600.0, depth_mm: 1200.0 },
            top_below_ground_mm: 0.0,
            blinding_mm: None,
            socket: None,
            ..block()
        };
        let q = FoundationQuantities::measure("F1", &def, 100.0).unwrap();
        // π/4 × 0.6² × 1.2 = 0.108π = 0.3392920066 m³.
        assert!(close(q.concrete_m3, 0.108 * PI));
        assert!(close(q.gross_volume_m3, q.concrete_m3));
        assert_eq!((q.socket_void_m3, q.blinding_m3, q.blinding_mm), (0.0, 0.0, 0.0));
        // Excavation: π/4 × (0.6 + 0.2)² × 1.2 = 0.192π = 0.6031857895 m³.
        assert!(close(q.excavation_m3, 0.192 * PI));
        // Side: π × 0.6 × 1.2 = 0.72π = 2.2619467106 m².
        assert!(close(q.side_area_m2, 0.72 * PI));
    }

    fn document() -> Document {
        let mut d = Document::default();
        d.add("sign", "ADS-001", Pt::new(0.0, 0.0), 2400.0, 1200.0, "Signs", serde_json::json!({}));
        d.add(POST_CATEGORY, "ADS-001-P1", Pt::new(0.0, 0.0), 100.0, 4000.0, "Supports", tube().to_value());
        d.add(POST_CATEGORY, "ADS-001-P2", Pt::new(0.0, 0.0), 100.0, 4000.0, "Supports", tube().to_value());
        d.add(FOUNDATION_CATEGORY, "ADS-001-F1", Pt::new(0.0, 0.0), 600.0, 950.0, "Supports", block().to_value());
        d.add(FOUNDATION_CATEGORY, "ADS-001-F2", Pt::new(0.0, 0.0), 600.0, 950.0, "Supports", block().to_value());
        d
    }

    #[test]
    fn summary_over_a_document_and_csv_content() {
        let d = document();
        let q = SupportQuantities::measure(d.elements(), &QuantityOptions { working_space_mm: 150.0, ..QuantityOptions::default() }).unwrap();
        assert_eq!((q.posts.len(), q.foundations.len(), q.skipped.len()), (2, 2, 0));

        // Per post (see tube_post_by_hand): 2827.4 mm², 0.011310 m³, 1.257 m², 0.942 m², 88.78 kg.
        // Two posts: 8000 mm, 0.022619467 → 0.022619 m³, 2.513274 → 2.513 m², 1.884956 → 1.885 m²,
        // 177.562817 → 177.56 kg.
        assert_eq!(
            posts_to_csv(&q.posts),
            "mark,section,material,finish,length_mm,embedment_mm,section_area_mm2,volume_m3,coating_area_m2,coating_area_above_ground_m2,density_kg_m3,mass_kg\n\
             ADS-001-P1,CHS D100 t10,as specified,as specified,4000,1000,2827.4,0.011310,1.257,0.942,7850,88.78\n\
             ADS-001-P2,CHS D100 t10,as specified,as specified,4000,1000,2827.4,0.011310,1.257,0.942,7850,88.78\n\
             TOTAL,2,,,8000,,,0.022619,2.513,1.885,,177.56\n"
        );
        // Per foundation (see block_foundation_less_its_socket_by_hand): gross 0.270, socket
        // 0.008836 → 0.009, concrete 0.261164 → 0.261, blinding 0.015, excavation 0.756, sides 1.980.
        // Two: 0.540, 0.017671 → 0.018, 0.522329 → 0.522, 0.030, 1.512, 3.960.
        assert_eq!(
            foundations_to_csv(&q.foundations),
            "mark,shape,concrete,top_below_ground_mm,blinding_mm,gross_volume_m3,socket_void_m3,concrete_m3,blinding_m3,working_space_mm,excavation_m3,side_area_m2\n\
             ADS-001-F1,Block 600x500x900,as specified,100,50,0.270,0.009,0.261,0.015,150,0.756,1.980\n\
             ADS-001-F2,Block 600x500x900,as specified,100,50,0.270,0.009,0.261,0.015,150,0.756,1.980\n\
             TOTAL,2,,,,0.540,0.018,0.522,0.030,,1.512,3.960\n"
        );
        assert_eq!(
            summary_to_csv(&q),
            "item,unit,quantity\n\
             posts,nr,2\n\
             post length,m,8.000\n\
             post material volume,m3,0.022619\n\
             post coating area (outside only),m2,2.513\n\
             post mass,kg,177.56\n\
             foundations,nr,2\n\
             foundation concrete,m3,0.522\n\
             blinding,m3,0.030\n\
             excavation,m3,1.512\n\
             foundation side face area,m2,3.960\n"
        );
        assert!(posts_to_csv(&q.posts).starts_with(POSTS_CSV_HEADER));
        assert!(foundations_to_csv(&q.foundations).starts_with(FOUNDATIONS_CSV_HEADER));
    }

    #[test]
    fn invalid_elements_are_skipped_and_reported_not_silently_dropped() {
        let mut d = document();
        let mut bad = tube();
        bad.section = Section::Chs { outside_diameter_mm: 100.0, wall_mm: 50.0 };
        d.add(POST_CATEGORY, "BAD-P1", Pt::new(0.0, 0.0), 100.0, 4000.0, "Supports", bad.to_value());
        d.add(FOUNDATION_CATEGORY, "BAD-F1", Pt::new(0.0, 0.0), 1.0, 1.0, "Supports", serde_json::json!({"nonsense": true}));
        let q = SupportQuantities::measure(d.elements(), &QuantityOptions::default()).unwrap();
        assert_eq!((q.posts.len(), q.foundations.len()), (2, 2));
        assert_eq!(q.skipped.iter().map(|s| s.mark.as_str()).collect::<Vec<_>>(), ["BAD-P1", "BAD-F1"]);
        assert!(q.skipped[0].reason.contains("half the diameter"));
        assert!(summary_to_csv(&q).ends_with("not measured (invalid definition),nr,2\n"));
        // Invalid options are refused outright.
        let bad_density = QuantityOptions { density_kg_m3: -1.0, ..QuantityOptions::default() };
        assert!(SupportQuantities::measure(d.elements(), &bad_density).is_err());
        let bad_space = QuantityOptions { working_space_mm: f64::NAN, ..QuantityOptions::default() };
        assert!(SupportQuantities::measure(d.elements(), &bad_space).is_err());
    }

    #[test]
    fn empty_schedules_still_have_a_total_and_text_is_quoted() {
        assert_eq!(posts_to_csv(&[]), format!("{POSTS_CSV_HEADER}\nTOTAL,0,,,0,,,0.000000,0.000,0.000,,0.00\n"));
        assert_eq!(foundations_to_csv(&[]), format!("{FOUNDATIONS_CSV_HEADER}\nTOTAL,0,,,,0.000,0.000,0.000,0.000,,0.000,0.000\n"));
        let mut def = tube();
        def.material = "tube, \"grade as specified\"".into();
        let q = PostQuantities::measure("A,1", &def, 7850.0).unwrap();
        let text = posts_to_csv(&[q]);
        assert!(text.lines().nth(1).unwrap().starts_with("\"A,1\",CHS D100 t10,\"tube, \"\"grade as specified\"\"\",as specified,4000,"));
    }

    #[test]
    fn fixed_rounds_and_trims() {
        assert_eq!(fixed(2827.4333882, 0, 1), "2827.4");
        assert_eq!(fixed(4000.0, 0, 1), "4000");
        assert_eq!(fixed(0.27, 3, 3), "0.270");
        assert_eq!(fixed(88.9, 0, 1), "88.9");
        assert_eq!(fixed(0.0, 2, 2), "0.00");
        assert_eq!(fixed(-0.0001, 3, 3), "0.000");
        assert_eq!(fixed(-1.26, 0, 1), "-1.3");
        assert_eq!(fixed(100.0, 0, 0), "100");
    }

    #[test]
    fn defaults_are_the_nominal_density_and_no_working_space() {
        let o = QuantityOptions::default();
        assert_eq!((o.density_kg_m3, o.working_space_mm), (7850.0, 0.0));
        assert_eq!(STEEL_DENSITY_KG_M3_NOMINAL, 7850.0);
    }
}
