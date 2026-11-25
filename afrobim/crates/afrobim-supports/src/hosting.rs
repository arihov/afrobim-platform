// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Creating, finding and removing the supports of a sign in a document.
//!
//! # Hosting
//!
//! Each post is hosted by its sign (`post.host == sign.id`) and each foundation by its post
//! (`foundation.host == post.id`), as the kernel's `Element::host` documents.
//!
//! # Site placement
//!
//! A sign's `SitePlacement` is read here as **the point on the ground vertically below the
//! sign's lower-left corner**: `x`, `y` in plan, `z` the ground level there, `rotation_deg`
//! turning the sign's frame (local +X along its width, the face looking towards local −Y)
//! counter-clockwise from site +X. The height of the face above that point is the
//! arrangement's mounting height. If the application decides that a sign's `z` is the level of
//! its lower edge instead, subtract the mounting height before calling these functions.
//!
//! A point at `u` along the sign's width and `v` behind its face is therefore at
//!
//! ```text
//! x = sign.x + u·cos θ − v·sin θ
//! y = sign.y + u·sin θ + v·cos θ        θ = rotation_deg
//! ```
//!
//! Supports take the sign's rotation and ground level `z`. Their own placement is the corner of
//! their plan bounding box (the origin of their solids' frame), so that a post of plan size
//! `w × d` whose axis is at (`u`, `v`) is placed at (`u − w/2`, `v − d/2`) in the sign's frame.
//! Supports do not copy the sign's alignment, station or offset: those describe the sign's own
//! point, and a support recomputed from them would land on the sign's corner. After a sign is
//! moved, call [`sync_support_sites`].

use afrobim_kernel::{Document, Element, ElementId, Pt, SitePlacement};

use crate::{FoundationDefinition, PostDefinition, SupportArrangement, SupportError, FOUNDATION_CATEGORY, POST_CATEGORY};

/// The layer supports are put on.
pub const SUPPORTS_LAYER: &str = "Supports";

/// Property on every support: the mark of the sign it carries.
pub const PROP_SIGN: &str = "Supports sign";
/// Property on every support: its number along the sign, from 1 at the left.
pub const PROP_INDEX: &str = "Support number";
/// Property on a post: the position of its axis along the sign's width from the sign's
/// lower-left corner, mm. [`sync_support_sites`] reads it back.
pub const PROP_POSITION_ALONG_SIGN_MM: &str = "Position along sign (mm)";
/// Property on a post: the distance of its axis behind the sign face, mm.
pub const PROP_SETBACK_MM: &str = "Setback behind face (mm)";
/// Property on a post: the height of the sign's lower edge above ground, mm.
pub const PROP_MOUNTING_HEIGHT_MM: &str = "Mounting height (mm)";

fn is_support(e: &Element) -> bool {
    e.category == POST_CATEGORY || e.category == FOUNDATION_CATEGORY
}

/// The site placement of a plan point (`u` along the host sign's width, `v` behind its face,
/// mm) given the sign's placement.
fn offset_site(sign: &SitePlacement, u: f64, v: f64) -> SitePlacement {
    let (sin, cos) = sign.rotation_deg.to_radians().sin_cos();
    SitePlacement {
        x: sign.x + u * cos - v * sin,
        y: sign.y + u * sin + v * cos,
        z: sign.z,
        rotation_deg: sign.rotation_deg,
        alignment: None,
        station_m: None,
        offset_m: None,
    }
}

/// Adds the posts and foundations of a sign and returns their ids: the posts left to right,
/// then their foundations in the same order.
///
/// * One post per position of `arrangement` across the sign's width (`sign.width`), each with
///   one foundation centred on its axis.
/// * The post definition is used as given except for `length_mm`, which is set to the
///   arrangement's length: embedment + mounting height + sign height + top offset.
/// * Names come from the sign's mark: `ADS-001-P1`, `ADS-001-F1`, numbered from the left.
/// * Layer [`SUPPORTS_LAYER`]; `host` and site placement as the module documentation describes.
/// * On the sheet the supports are laid out as an exploded elevation to the right of the sign:
///   the posts in a row, then the foundations, [`Document::ARRANGE_GAP`] apart, all drawn to
///   one ground line `mounting height` below the sign's lower edge (so a post's top is level
///   with the sign's top, plus the top offset).
///
/// Nothing is added when anything is invalid. A sign that already has supports is refused; use
/// [`replace_supports`]. This function does not judge whether the supports are adequate.
pub fn add_supports(
    doc: &mut Document,
    sign_id: ElementId,
    arrangement: &SupportArrangement,
    post: &PostDefinition,
    foundation: &FoundationDefinition,
) -> Result<Vec<ElementId>, SupportError> {
    let sign = doc.find(sign_id).ok_or(SupportError::ElementNotFound(sign_id.0))?.clone();
    if !supports_of(doc, sign_id).is_empty() {
        return Err(SupportError::AlreadySupported(sign.name));
    }
    arrangement.check(sign.width, sign.height)?;
    let mut post = post.clone();
    post.length_mm = arrangement.post_length_mm(sign.height, post.embedment_mm);
    post.check()?;
    foundation.check()?;

    let positions = arrangement.post_positions(sign.width);
    let (post_w, post_d) = post.section.bounding_mm();
    let (found_l, found_w) = foundation.shape.plan_mm();
    let found_h = foundation.shape.depth_mm() + foundation.blinding_thickness_mm();
    let gap = Document::ARRANGE_GAP;
    let ground_y = sign.origin.y - arrangement.mounting_height_mm;
    let mut cursor = sign.origin.x + sign.width + gap;

    let mut post_ids = Vec::new();
    for (i, &u) in positions.iter().enumerate() {
        let name = doc.unique_name(&format!("{}-P{}", sign.name, i + 1));
        let origin = Pt::new(cursor, ground_y - post.embedment_mm);
        cursor += post_w + gap;
        let id = doc.add(POST_CATEGORY, &name, origin, post_w, post.length_mm, SUPPORTS_LAYER, post.to_value());
        let e = doc.find_mut(id).expect("just added");
        e.host = Some(sign_id);
        e.site = sign.site.as_ref().map(|s| offset_site(s, u - post_w / 2.0, arrangement.setback_mm - post_d / 2.0));
        e.properties.insert(PROP_SIGN.into(), sign.name.clone());
        e.properties.insert(PROP_INDEX.into(), (i + 1).to_string());
        e.properties.insert(PROP_POSITION_ALONG_SIGN_MM.into(), u.to_string());
        e.properties.insert(PROP_SETBACK_MM.into(), arrangement.setback_mm.to_string());
        e.properties.insert(PROP_MOUNTING_HEIGHT_MM.into(), arrangement.mounting_height_mm.to_string());
        post_ids.push(id);
    }
    let mut foundation_ids = Vec::new();
    // One foundation under each post, or a single one on the sign's centre line hosted by the
    // first post. A foundation records its own position along the sign.
    let foundation_positions: Vec<f64> = if arrangement.shared_foundation { vec![sign.width / 2.0] } else { positions.clone() };
    for (i, &u) in foundation_positions.iter().enumerate() {
        let name = doc.unique_name(&format!("{}-F{}", sign.name, i + 1));
        let origin = Pt::new(cursor, ground_y - foundation.formation_depth_mm());
        cursor += found_l + gap;
        let id = doc.add(FOUNDATION_CATEGORY, &name, origin, found_l, found_h, SUPPORTS_LAYER, foundation.to_value());
        let e = doc.find_mut(id).expect("just added");
        e.host = Some(post_ids[i]);
        e.site = sign.site.as_ref().map(|s| offset_site(s, u - found_l / 2.0, arrangement.setback_mm - found_w / 2.0));
        e.properties.insert(PROP_SIGN.into(), sign.name.clone());
        e.properties.insert(PROP_INDEX.into(), (i + 1).to_string());
        e.properties.insert(PROP_POSITION_ALONG_SIGN_MM.into(), u.to_string());
        foundation_ids.push(id);
    }
    post_ids.extend(foundation_ids);
    Ok(post_ids)
}

/// Removes the sign's supports, if any, and adds new ones. When the new supports are invalid
/// the existing ones are kept and the error is returned.
pub fn replace_supports(
    doc: &mut Document,
    sign_id: ElementId,
    arrangement: &SupportArrangement,
    post: &PostDefinition,
    foundation: &FoundationDefinition,
) -> Result<Vec<ElementId>, SupportError> {
    let mut trial = doc.clone();
    remove_supports(&mut trial, sign_id);
    add_supports(&mut trial, sign_id, arrangement, post, foundation)?;
    remove_supports(doc, sign_id);
    add_supports(doc, sign_id, arrangement, post, foundation)
}

/// The posts and foundations that belong to a sign, in document order: every element of
/// category `post` or `foundation` whose chain of hosts leads to the sign.
pub fn supports_of(doc: &Document, sign_id: ElementId) -> Vec<ElementId> {
    let mut found: Vec<ElementId> = Vec::new();
    loop {
        let before = found.len();
        for e in doc.elements() {
            if is_support(e) && !found.contains(&e.id) && e.host.is_some_and(|h| h == sign_id || found.contains(&h)) {
                found.push(e.id);
            }
        }
        if found.len() == before {
            break;
        }
    }
    doc.elements().iter().filter(|e| found.contains(&e.id)).map(|e| e.id).collect()
}

/// Removes every support of the sign and returns the ids removed. The sign itself is untouched.
pub fn remove_supports(doc: &mut Document, sign_id: ElementId) -> Vec<ElementId> {
    let ids = supports_of(doc, sign_id);
    for id in &ids {
        doc.remove(*id);
    }
    ids
}

/// Recomputes the site placement of a sign's supports from the sign's current placement, using
/// the position and setback recorded on each post when it was created. Supports lose their
/// placement when the sign has none. Returns the number of supports updated; posts whose
/// recorded position or definition cannot be read are left as they are, with their foundations.
pub fn sync_support_sites(doc: &mut Document, sign_id: ElementId) -> Result<usize, SupportError> {
    let sign_site = doc.find(sign_id).ok_or(SupportError::ElementNotFound(sign_id.0))?.site.clone();
    let number = |e: &Element, key: &str| e.properties.get(key).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite());
    let mut updates: Vec<(ElementId, Option<SitePlacement>)> = Vec::new();
    for post in doc.elements().iter().filter(|e| e.category == POST_CATEGORY && e.host == Some(sign_id)) {
        let (Some(u), Some(v), Ok(def)) =
            (number(post, PROP_POSITION_ALONG_SIGN_MM), number(post, PROP_SETBACK_MM), PostDefinition::from_value(&post.data))
        else {
            continue;
        };
        let (w, d) = def.section.bounding_mm();
        updates.push((post.id, sign_site.as_ref().map(|s| offset_site(s, u - w / 2.0, v - d / 2.0))));
        for f in doc.elements().iter().filter(|e| e.category == FOUNDATION_CATEGORY && e.host == Some(post.id)) {
            if let Ok(fd) = FoundationDefinition::from_value(&f.data) {
                let (l, w) = fd.shape.plan_mm();
                let u = number(f, PROP_POSITION_ALONG_SIGN_MM).unwrap_or(u);
                updates.push((f.id, sign_site.as_ref().map(|s| offset_site(s, u - l / 2.0, v - w / 2.0))));
            }
        }
    }
    let count = updates.len();
    for (id, site) in updates {
        if let Some(e) = doc.find_mut(id) {
            e.site = site;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FoundationModule, FoundationShape, PostModule, Section, Socket};
    use afrobim_kernel::{Module, ModuleRegistry};
    use serde_json::json;

    fn post() -> PostDefinition {
        PostDefinition {
            section: Section::Chs { outside_diameter_mm: 100.0, wall_mm: 10.0 },
            length_mm: 1.0, // replaced by the arrangement's length
            embedment_mm: 800.0,
            material: "as specified".into(),
            finish: "as specified".into(),
            placeholder: false,
        }
    }

    fn foundation() -> FoundationDefinition {
        FoundationDefinition {
            shape: FoundationShape::Block { length_mm: 600.0, width_mm: 400.0, depth_mm: 900.0 },
            top_below_ground_mm: 100.0,
            concrete: "as specified".into(),
            blinding_mm: Some(50.0),
            socket: Some(Socket::Round { diameter_mm: 150.0, depth_mm: 500.0 }),
            placeholder: false,
        }
    }

    /// A sign 2400 × 1200 at sheet (1000, 5000), element id 1.
    fn doc_with_sign() -> (Document, ElementId) {
        let mut d = Document::default();
        let id = d.add("sign", "ADS-001", Pt::new(1000.0, 5000.0), 2400.0, 1200.0, "Signs", json!({"kind": "StackType"}));
        (d, id)
    }

    fn site(x: f64, y: f64, z: f64, rotation_deg: f64) -> SitePlacement {
        SitePlacement { x, y, z, rotation_deg, alignment: Some("A1".into()), station_m: Some(1250.0), offset_m: Some(-4.5) }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn ids_names_hosts_layer_and_sizes() {
        let (mut d, sign) = doc_with_sign();
        let ids = add_supports(&mut d, sign, &SupportArrangement::new(2000.0, 2, 300.0), &post(), &foundation()).unwrap();
        assert_eq!(ids, [ElementId(2), ElementId(3), ElementId(4), ElementId(5)]);
        let e = |i: usize| d.find(ids[i]).unwrap();
        assert_eq!([e(0).name.as_str(), e(1).name.as_str(), e(2).name.as_str(), e(3).name.as_str()], ["ADS-001-P1", "ADS-001-P2", "ADS-001-F1", "ADS-001-F2"]);
        assert_eq!([e(0).category.as_str(), e(2).category.as_str()], ["post", "foundation"]);
        // Posts are hosted by the sign, each foundation by its own post.
        assert_eq!((e(0).host, e(1).host), (Some(sign), Some(sign)));
        assert_eq!((e(2).host, e(3).host), (Some(ids[0]), Some(ids[1])));
        assert!(ids.iter().all(|id| d.find(*id).unwrap().layer == "Supports"));
        assert!(d.layers().iter().any(|l| l.name == "Supports"));
        // Post length = 800 embedment + 2000 mounting + 1200 sign = 4000.
        let stored = PostDefinition::from_value(&e(0).data).unwrap();
        assert_eq!(stored.length_mm, 4000.0);
        assert_eq!((e(0).width, e(0).height), (100.0, 4000.0));
        // Foundation element: 600 long, 900 concrete + 50 blinding = 950 high.
        assert_eq!((e(2).width, e(2).height), (600.0, 950.0));
        assert_eq!(FoundationDefinition::from_value(&e(2).data).unwrap(), foundation());
        // Recorded for later: positions 300 and 2400 − 300 = 2100 along the sign.
        assert_eq!(e(0).properties[PROP_SIGN], "ADS-001");
        assert_eq!((e(0).properties[PROP_INDEX].as_str(), e(1).properties[PROP_INDEX].as_str()), ("1", "2"));
        assert_eq!((e(0).properties[PROP_POSITION_ALONG_SIGN_MM].as_str(), e(1).properties[PROP_POSITION_ALONG_SIGN_MM].as_str()), ("300", "2100"));
        assert_eq!(e(0).properties[PROP_MOUNTING_HEIGHT_MM], "2000");
        assert_eq!(e(3).properties[PROP_INDEX], "2");
        // No site on the sign, none on the supports.
        assert!(ids.iter().all(|id| d.find(*id).unwrap().site.is_none()));
        // The sign itself is unchanged.
        assert_eq!(d.find(sign).unwrap().host, None);
    }

    #[test]
    fn sheet_origins_form_an_exploded_elevation_beside_the_sign() {
        let (mut d, sign) = doc_with_sign();
        let ids = add_supports(&mut d, sign, &SupportArrangement::new(2000.0, 2, 300.0), &post(), &foundation()).unwrap();
        let o = |i: usize| d.find(ids[i]).unwrap().origin;
        // Sign spans x 1000..3400, lower edge y = 5000. Ground line y = 5000 − 2000 = 3000.
        // Posts: foot at 3000 − 800 = 2200 (top at 2200 + 4000 = 6200 = sign top 5000 + 1200).
        //   P1 x = 3400 + 500 = 3900; P2 x = 3900 + 100 + 500 = 4500.
        assert_eq!(o(0), Pt::new(3900.0, 2200.0));
        assert_eq!(o(1), Pt::new(4500.0, 2200.0));
        // Foundations: underside of blinding at 3000 − (100 + 900 + 50) = 1950.
        //   F1 x = 4500 + 100 + 500 = 5100; F2 x = 5100 + 600 + 500 = 6200.
        assert_eq!(o(2), Pt::new(5100.0, 1950.0));
        assert_eq!(o(3), Pt::new(6200.0, 1950.0));
        // Nothing overlaps the sign or another support.
        let all: Vec<_> = d.elements().iter().map(|e| e.bounds()).collect();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert!(!a.intersects(b));
            }
        }
    }

    #[test]
    fn site_placement_for_an_unrotated_sign() {
        let (mut d, sign) = doc_with_sign();
        d.find_mut(sign).unwrap().site = Some(site(500_000.0, 200_000.0, 1150.0, 0.0));
        let mut a = SupportArrangement::new(2000.0, 2, 300.0);
        a.setback_mm = 80.0;
        let ids = add_supports(&mut d, sign, &a, &post(), &foundation()).unwrap();
        let s = |i: usize| d.find(ids[i]).unwrap().site.clone().unwrap();
        // P1 axis at u = 300, v = 80. Its frame corner is half the 100 mm tube back from the
        // axis: (300 − 50, 80 − 50) = (250, 30) → site (500 250, 200 030).
        assert!(close(s(0).x, 500_250.0) && close(s(0).y, 200_030.0));
        // P2 axis at u = 2100: corner (2050, 30).
        assert!(close(s(1).x, 502_050.0) && close(s(1).y, 200_030.0));
        // F1 600 × 400 centred on the P1 axis: corner (300 − 300, 80 − 200) = (0, −120).
        assert!(close(s(2).x, 500_000.0) && close(s(2).y, 199_880.0));
        // F2: corner (2100 − 300, −120) = (1800, −120).
        assert!(close(s(3).x, 501_800.0) && close(s(3).y, 199_880.0));
        for i in 0..4 {
            assert_eq!((s(i).z, s(i).rotation_deg), (1150.0, 0.0));
            // The alignment reference describes the sign's own point and is not copied.
            assert_eq!((s(i).alignment.clone(), s(i).station_m, s(i).offset_m), (None, None, None));
        }
    }

    #[test]
    fn site_placement_for_a_sign_rotated_ninety_degrees() {
        let (mut d, sign) = doc_with_sign();
        d.find_mut(sign).unwrap().site = Some(site(1000.0, 2000.0, 0.0, 90.0));
        let ids = add_supports(&mut d, sign, &SupportArrangement::new(2000.0, 2, 300.0), &post(), &foundation()).unwrap();
        let s = |i: usize| d.find(ids[i]).unwrap().site.clone().unwrap();
        // θ = 90°: cos = 0, sin = 1, so (u, v) → (x − v, y + u). The sign's width runs along site +Y.
        // P1 corner (u, v) = (250, −50)   → (1000 + 50, 2000 + 250)  = (1050, 2250).
        // P2 corner (2050, −50)           → (1050, 4050).
        // F1 corner (0, −200)             → (1200, 2000).
        // F2 corner (1800, −200)          → (1200, 3800).
        let expect = [(1050.0, 2250.0), (1050.0, 4050.0), (1200.0, 2000.0), (1200.0, 3800.0)];
        for (i, (x, y)) in expect.iter().enumerate() {
            assert!(close(s(i).x, *x) && close(s(i).y, *y), "support {i}: {:?}", s(i));
            assert_eq!(s(i).rotation_deg, 90.0);
        }
        // Check through the axis too: the post's own frame puts its axis at local (50, 50);
        // turning that by 90° gives (−50, 50), so the P1 axis is at (1050 − 50, 2250 + 50)
        // = (1000, 2300): 300 mm along the sign's width from its corner, in the face plane.
        let (sin, cos) = 90f64.to_radians().sin_cos();
        let axis = (s(0).x + 50.0 * cos - 50.0 * sin, s(0).y + 50.0 * sin + 50.0 * cos);
        assert!(close(axis.0, 1000.0) && close(axis.1, 2300.0));
    }

    #[test]
    fn site_placement_for_a_sign_rotated_thirty_degrees_with_setback() {
        let (mut d, sign) = doc_with_sign();
        d.find_mut(sign).unwrap().site = Some(site(10_000.0, 20_000.0, 300.0, 30.0));
        let mut a = SupportArrangement::new(2000.0, 3, 200.0);
        a.setback_mm = 150.0;
        let ids = add_supports(&mut d, sign, &a, &post(), &foundation()).unwrap();
        assert_eq!(ids.len(), 6);
        // Three posts on 2400 with inset 200: u = 200, 1200, 2200. Take the middle one, P2.
        // Its corner is (u, v) = (1200 − 50, 150 − 50) = (1150, 100).
        // cos 30° = 0.8660254, sin 30° = 0.5:
        //   x = 10 000 + 1150 × 0.8660254 − 100 × 0.5       = 10 000 + 995.92921 − 50      = 10 945.92921
        //   y = 20 000 + 1150 × 0.5       + 100 × 0.8660254 = 20 000 + 575 + 86.60254      = 20 661.60254
        let p2 = d.find(ids[1]).unwrap().site.clone().unwrap();
        assert!((p2.x - 10945.92921).abs() < 1e-4 && (p2.y - 20661.60254).abs() < 1e-4, "{p2:?}");
        assert_eq!((p2.z, p2.rotation_deg), (300.0, 30.0));
        // Its foundation F2 (600 × 400) has its corner at (1200 − 300, 150 − 200) = (900, −50):
        //   x = 10 000 + 900 × 0.8660254 + 50 × 0.5         = 10 000 + 779.42286 + 25      = 10 804.42286
        //   y = 20 000 + 900 × 0.5       − 50 × 0.8660254   = 20 000 + 450 − 43.30127      = 20 406.69873
        let f2 = d.find(ids[4]).unwrap().site.clone().unwrap();
        assert!((f2.x - 10804.42286).abs() < 1e-4 && (f2.y - 20406.69873).abs() < 1e-4, "{f2:?}");
        // Both centres are the same point: the foundation is centred on the post axis.
        let (sin, cos) = 30f64.to_radians().sin_cos();
        let centre = |s: &SitePlacement, hx: f64, hy: f64| (s.x + hx * cos - hy * sin, s.y + hx * sin + hy * cos);
        let (pc, fc) = (centre(&p2, 50.0, 50.0), centre(&f2, 300.0, 200.0));
        assert!(close(pc.0, fc.0) && close(pc.1, fc.1));
    }

    #[test]
    fn moving_the_sign_and_syncing_moves_the_supports() {
        let (mut d, sign) = doc_with_sign();
        let ids = add_supports(&mut d, sign, &SupportArrangement::new(2000.0, 2, 300.0), &post(), &foundation()).unwrap();
        assert!(d.find(ids[0]).unwrap().site.is_none());
        d.find_mut(sign).unwrap().site = Some(site(1000.0, 2000.0, 0.0, 90.0));
        assert_eq!(sync_support_sites(&mut d, sign).unwrap(), 4);
        // Same hand values as the 90° test.
        let s = |i: usize| d.find(ids[i]).unwrap().site.clone().unwrap();
        assert!(close(s(1).x, 1050.0) && close(s(1).y, 4050.0));
        assert!(close(s(3).x, 1200.0) && close(s(3).y, 3800.0));
        d.find_mut(sign).unwrap().site = None;
        assert_eq!(sync_support_sites(&mut d, sign).unwrap(), 4);
        assert!(ids.iter().all(|id| d.find(*id).unwrap().site.is_none()));
        assert!(matches!(sync_support_sites(&mut d, ElementId(999)), Err(SupportError::ElementNotFound(999))));
    }

    #[test]
    fn supports_of_and_removal_leave_other_signs_alone() {
        let (mut d, a) = doc_with_sign();
        let b = d.add("sign", "ADS-002", Pt::new(1000.0, -3000.0), 900.0, 900.0, "Signs", json!({}));
        let of_a = add_supports(&mut d, a, &SupportArrangement::new(2000.0, 2, 300.0), &post(), &foundation()).unwrap();
        let of_b = add_supports(&mut d, b, &SupportArrangement::new(2100.0, 1, 0.0), &post(), &foundation()).unwrap();
        assert_eq!(of_b.iter().map(|id| d.find(*id).unwrap().name.clone()).collect::<Vec<_>>(), ["ADS-002-P1", "ADS-002-F1"]);
        // One post on a 900 wide sign: on the centre line.
        assert_eq!(d.find(of_b[0]).unwrap().properties[PROP_POSITION_ALONG_SIGN_MM], "450");
        assert_eq!(supports_of(&d, a), of_a);
        assert_eq!(supports_of(&d, b), of_b);
        // Something else hosted by the sign is not a support.
        let plate = d.add("sign", "ADS-001-SP", Pt::new(0.0, 0.0), 1.0, 1.0, "Signs", json!({}));
        d.find_mut(plate).unwrap().host = Some(a);
        assert_eq!(supports_of(&d, a), of_a);
        assert!(supports_of(&d, ElementId(999)).is_empty());

        assert_eq!(remove_supports(&mut d, a), of_a);
        assert!(supports_of(&d, a).is_empty());
        assert_eq!(supports_of(&d, b), of_b);
        assert!(d.find(a).is_some() && d.find(plate).is_some());
        assert!(remove_supports(&mut d, a).is_empty());
    }

    #[test]
    fn a_supported_sign_is_refused_and_can_be_replaced() {
        let (mut d, sign) = doc_with_sign();
        let two = SupportArrangement::new(2000.0, 2, 300.0);
        let first = add_supports(&mut d, sign, &two, &post(), &foundation()).unwrap();
        assert_eq!(add_supports(&mut d, sign, &two, &post(), &foundation()), Err(SupportError::AlreadySupported("ADS-001".into())));
        let three = SupportArrangement::new(2000.0, 3, 300.0);
        let second = replace_supports(&mut d, sign, &three, &post(), &foundation()).unwrap();
        assert_eq!(second.len(), 6);
        assert!(first.iter().all(|id| d.find(*id).is_none()));
        assert_eq!(d.find(second[2]).unwrap().name, "ADS-001-P3");
        assert_eq!(d.find(second[5]).unwrap().name, "ADS-001-F3");
        // A failed replacement keeps what was there.
        let bad = SupportArrangement::new(2000.0, 0, 300.0);
        assert!(replace_supports(&mut d, sign, &bad, &post(), &foundation()).is_err());
        assert_eq!(supports_of(&d, sign), second);
    }

    #[test]
    fn invalid_input_adds_nothing() {
        let (mut d, sign) = doc_with_sign();
        let ok = SupportArrangement::new(2000.0, 2, 300.0);
        assert_eq!(add_supports(&mut d, ElementId(42), &ok, &post(), &foundation()), Err(SupportError::ElementNotFound(42)));
        let none = SupportArrangement::new(2000.0, 0, 300.0);
        assert!(matches!(add_supports(&mut d, sign, &none, &post(), &foundation()), Err(SupportError::InvalidArrangement(_))));
        let mut bad_post = post();
        bad_post.section = Section::Chs { outside_diameter_mm: 100.0, wall_mm: 60.0 };
        assert!(matches!(add_supports(&mut d, sign, &ok, &bad_post, &foundation()), Err(SupportError::InvalidDefinition(_))));
        let mut bad_found = foundation();
        bad_found.shape = FoundationShape::Cylinder { diameter_mm: 0.0, depth_mm: 900.0 };
        assert!(matches!(add_supports(&mut d, sign, &ok, &post(), &bad_found), Err(SupportError::InvalidDefinition(_))));
        assert_eq!(d.elements().len(), 1);
    }

    #[test]
    fn definitions_survive_a_document_saved_and_reloaded() {
        let (mut d, sign) = doc_with_sign();
        d.find_mut(sign).unwrap().site = Some(site(10_000.0, 20_000.0, 300.0, 30.0));
        let rhs = PostDefinition {
            section: Section::Rhs { width_mm: 100.0, depth_mm: 50.0, wall_mm: 5.0, outer_corner_radius_mm: 10.0, inner_corner_radius_mm: 5.0 },
            ..post()
        };
        let ids = add_supports(&mut d, sign, &SupportArrangement::new(2000.0, 2, 300.0), &rhs, &foundation()).unwrap();
        let other = d.add("sign", "ADS-002", Pt::new(0.0, -9000.0), 900.0, 900.0, "Signs", json!({}));
        let more = add_supports(&mut d, other, &SupportArrangement::new(2000.0, 1, 0.0), &PostDefinition::placeholder(), &FoundationDefinition::placeholder()).unwrap();

        let path = std::env::temp_dir().join(format!("afrobim-supports-roundtrip-{}.afrobim", std::process::id()));
        d.save(&path).unwrap();
        let back = Document::load(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(back.elements(), d.elements());
        let stored = PostDefinition::from_value(&back.find(ids[0]).unwrap().data).unwrap();
        assert_eq!(stored, PostDefinition { length_mm: 4000.0, ..rhs });
        assert_eq!(FoundationDefinition::from_value(&back.find(ids[2]).unwrap().data).unwrap(), foundation());
        // The placeholder flag survives, so the note is still raised after reloading.
        let placeholder_post = PostDefinition::from_value(&back.find(more[0]).unwrap().data).unwrap();
        assert!(placeholder_post.placeholder);
        assert!(FoundationDefinition::from_value(&back.find(more[1]).unwrap().data).unwrap().placeholder);
        assert_eq!(supports_of(&back, sign), ids);
        assert_eq!(back.find(ids[3]).unwrap().host, Some(ids[1]));
        assert_eq!(back.find(ids[1]).unwrap().site, d.find(ids[1]).unwrap().site);

        // The reloaded elements generate through a registry exactly as the originals do.
        let mut registry = ModuleRegistry::default();
        registry.register(Box::new(PostModule::default()));
        registry.register(Box::new(FoundationModule::default()));
        for id in ids.iter().chain(&more) {
            let (before, after) = (registry.generate(d.find(*id).unwrap()).unwrap(), registry.generate(back.find(*id).unwrap()).unwrap());
            assert_eq!(before, after);
            assert!(!after.solids.is_empty());
        }
        let g = registry.generate(back.find(more[0]).unwrap()).unwrap();
        assert!(g.notes.iter().any(|n| n.code == crate::NOTE_PLACEHOLDER));
        assert_eq!(PostModule::default().category(), "post");
        assert_eq!(FoundationModule::default().category(), "foundation");
    }

    #[test]
    fn generated_sizes_agree_with_the_stored_element_sizes() {
        let (mut d, sign) = doc_with_sign();
        let ids = add_supports(&mut d, sign, &SupportArrangement::new(2000.0, 2, 300.0), &post(), &foundation()).unwrap();
        for id in ids {
            let e = d.find(id).unwrap();
            let g = if e.category == "post" { PostModule::default().generate(e) } else { FoundationModule::default().generate(e) }.unwrap();
            assert_eq!((g.drawing.extents.width(), g.drawing.extents.height()), (e.width, e.height));
            assert_eq!((g.drawing.extents.min.x, g.drawing.extents.min.y), (0.0, 0.0));
        }
    }
}
