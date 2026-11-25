// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Gantries in a document, and the signs they carry.
//!
//! # Hosting
//!
//! A sign carried by a gantry has `sign.host == gantry.id`, and the gantry's definition lists it
//! in `signs` with its position. The definition is the record; the sign's `host`, `site` and the
//! [`PROP_GANTRY`] property are derived from it by [`sync_gantry`], in the manner of
//! `afrobim_supports::sync_support_sites`. Call [`sync_gantry`] after the gantry is edited or
//! moved, the alignment or its profile changes, or a carried sign changes size.
//!
//! A sign has either post supports or a gantry, not both: [`add_sign_to_gantry`] refuses a sign
//! that has posts. The supports crate does not know about gantries, so it will still add posts
//! to a gantry-hosted sign if asked; [`support_conflicts`] finds such signs.
//!
//! # Site placement of a carried sign
//!
//! For a post-mounted sign the supports crate reads `site.z` as the ground level under the sign.
//! A gantry-hosted sign has no ground point of its own: **its `site.z` is the level of its lower
//! edge**, and `x`, `y` are its lower-left corner on the boom's face towards the traffic it
//! serves (see [`crate::GantryDefinition::sign_setting`]). Its `station_m` is the gantry's and
//! its `offset_m` the offset of the sign's centre, so recomputing the sign's position from
//! station and offset alone would lose the skew and the level: use [`sync_gantry`].

use afrobim_alignment::Alignment;
use afrobim_kernel::{Document, Element, ElementId};
use afrobim_supports::{FOUNDATION_CATEGORY, POST_CATEGORY};

use crate::{Direction, GantryDefinition, GantryError, SignMount, SignPosition, GANTRY_CATEGORY, SIGN_CATEGORY};

/// The layer gantries are put on.
pub const GANTRY_LAYER: &str = "Gantries";
/// Property on a carried sign: the mark of the gantry that carries it.
pub const PROP_GANTRY: &str = "Carried by gantry";

/// What [`sync_gantry`] did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SyncReport {
    /// Signs whose placement was set (or cleared, without an alignment).
    pub signs_updated: usize,
    /// Marks of mounts dropped because their sign is no longer in the document.
    pub dropped: Vec<String>,
}

fn gantry(doc: &Document, id: ElementId) -> Result<(&Element, GantryDefinition), GantryError> {
    let e = doc.find(id).ok_or(GantryError::ElementNotFound(id.0))?;
    if e.category != GANTRY_CATEGORY {
        return Err(GantryError::NotAGantry(e.name.clone()));
    }
    Ok((e, GantryDefinition::from_value(&e.data)?))
}

/// The definition of a gantry element.
pub fn definition_of(doc: &Document, gantry_id: ElementId) -> Result<GantryDefinition, GantryError> {
    gantry(doc, gantry_id).map(|(_, d)| d)
}

fn has_post_supports(doc: &Document, sign: ElementId) -> bool {
    doc.elements().iter().any(|e| (e.category == POST_CATEGORY || e.category == FOUNDATION_CATEGORY) && e.host == Some(sign))
}

/// Adds a gantry to the document and returns its id.
///
/// The element goes on [`GANTRY_LAYER`] at the next free place on the sheet, sized to its
/// elevation. With the alignment it is placed on site at once (see [`sync_gantry`]); without
/// it the gantry is a sheet element until [`sync_gantry`] is called with one. Carried signs
/// are added afterwards with [`add_sign_to_gantry`]: a definition that already lists signs is
/// refused. Nothing is added when the definition is invalid.
pub fn add_gantry(doc: &mut Document, name: &str, definition: &GantryDefinition, alignment: Option<&Alignment>) -> Result<ElementId, GantryError> {
    definition.check()?;
    if !definition.signs.is_empty() {
        return Err(GantryError::InvalidDefinition("add the gantry first, then its signs with add_sign_to_gantry".into()));
    }
    if let Some(a) = alignment {
        definition.setting_out(a)?;
    }
    let extents = definition.drawing().extents;
    let name = doc.unique_name(name);
    let origin = doc.next_free_origin(extents.width(), extents.height());
    let id = doc.add(GANTRY_CATEGORY, &name, origin, extents.width(), extents.height(), GANTRY_LAYER, definition.to_value());
    sync_gantry(doc, id, alignment)?;
    Ok(id)
}

/// Replaces a gantry's definition (after an edit in the property panel, say) and syncs. The
/// document is unchanged when the new definition is invalid or cannot be set out.
pub fn set_definition(doc: &mut Document, gantry_id: ElementId, definition: &GantryDefinition, alignment: Option<&Alignment>) -> Result<SyncReport, GantryError> {
    gantry(doc, gantry_id)?;
    definition.check()?;
    if let Some(a) = alignment {
        definition.setting_out(a)?;
    }
    doc.find_mut(gantry_id).expect("checked").data = definition.to_value();
    sync_gantry(doc, gantry_id, alignment)
}

/// Brings the gantry element and its carried signs up to date with the gantry's definition.
///
/// * Mounts whose sign has left the document are dropped (and reported).
/// * Each mount's mark, width and height are refreshed from its sign element.
/// * The element's sheet size is set to its elevation.
/// * With `alignment`: the gantry's `site` and reference level are set from the alignment and
///   its profile, and every carried sign gets `host`, [`PROP_GANTRY`] and its `site`.
/// * Without it: the gantry and its signs lose their site placement (as supports do when their
///   sign has none); hosting is kept.
///
/// Fails, changing nothing, when the alignment is not the gantry's, the station is off it, or
/// the refreshed definition is invalid.
pub fn sync_gantry(doc: &mut Document, gantry_id: ElementId, alignment: Option<&Alignment>) -> Result<SyncReport, GantryError> {
    let (element, mut def) = gantry(doc, gantry_id)?;
    let gantry_name = element.name.clone();
    let mut report = SyncReport::default();
    let mut kept = Vec::new();
    for mut m in std::mem::take(&mut def.signs) {
        match doc.find(m.sign) {
            Some(s) => {
                (m.mark, m.width_mm, m.height_mm) = (s.name.clone(), s.width, s.height);
                kept.push(m);
            }
            None => report.dropped.push(m.mark),
        }
    }
    def.signs = kept;
    def.check()?;
    let setting = match alignment {
        Some(a) => Some(def.setting_out(a)?),
        None => None,
    };
    def.reference_level_m = setting.as_ref().filter(|s| s.level_known).map(|s| s.reference_level_mm / 1000.0);
    let mut sign_sites = Vec::new();
    for m in &def.signs {
        let site = match &setting {
            Some(so) => Some(def.sign_setting(so, m)?.site),
            None => None,
        };
        sign_sites.push((m.sign, site));
    }
    let extents = def.drawing().extents;
    let e = doc.find_mut(gantry_id).expect("found above");
    e.data = def.to_value();
    (e.width, e.height) = (extents.width(), extents.height());
    e.site = setting.as_ref().map(|so| def.site(so));
    report.signs_updated = sign_sites.len();
    for (id, site) in sign_sites {
        let s = doc.find_mut(id).expect("kept mounts exist");
        s.host = Some(gantry_id);
        s.site = site;
        s.properties.insert(PROP_GANTRY.into(), gantry_name.clone());
    }
    Ok(report)
}

/// Hosts a sign on a gantry.
///
/// `position` is where the centre of the face sits along the boom, `bottom_above_boom_underside_mm`
/// the height of the sign's lower edge above the boom's underside (negative below it), and
/// `serves` the traffic it faces; `None` takes the direction of the lane under it.
///
/// Refused, changing nothing, when: the element is not a sign; it has post supports; it is
/// already hosted (by this gantry or anything else); the lane does not exist; no lane is under
/// it and `serves` is not given.
pub fn add_sign_to_gantry(
    doc: &mut Document,
    gantry_id: ElementId,
    sign_id: ElementId,
    position: SignPosition,
    bottom_above_boom_underside_mm: f64,
    serves: Option<Direction>,
    alignment: Option<&Alignment>,
) -> Result<SyncReport, GantryError> {
    let (_, mut def) = gantry(doc, gantry_id)?;
    let sign = doc.find(sign_id).ok_or(GantryError::ElementNotFound(sign_id.0))?;
    if sign.category != SIGN_CATEGORY {
        return Err(GantryError::NotASign(sign.name.clone()));
    }
    if has_post_supports(doc, sign_id) {
        return Err(GantryError::SignHasPostSupports(sign.name.clone()));
    }
    if let Some(host) = sign.host {
        let host_name = doc.find(host).map_or_else(|| format!("element {}", host.0), |h| h.name.clone());
        return Err(GantryError::AlreadyHosted(sign.name.clone(), host_name));
    }
    let mount = SignMount { sign: sign_id, mark: sign.name.clone(), width_mm: sign.width, height_mm: sign.height, position, bottom_above_boom_underside_mm, serves };
    def.sign_centre_offset_m(&mount)?;
    def.sign_direction(&mount)?;
    def.signs.push(mount);
    set_definition(doc, gantry_id, &def, alignment)
}

/// Takes a sign off a gantry: the mount is removed and the sign loses its host, its
/// [`PROP_GANTRY`] property and its site placement (which was the gantry's to give). The sign
/// itself stays in the document.
pub fn remove_sign_from_gantry(doc: &mut Document, gantry_id: ElementId, sign_id: ElementId) -> Result<(), GantryError> {
    let (_, mut def) = gantry(doc, gantry_id)?;
    let before = def.signs.len();
    def.signs.retain(|m| m.sign != sign_id);
    if def.signs.len() == before {
        let name = doc.find(sign_id).map_or_else(|| format!("element {}", sign_id.0), |s| s.name.clone());
        return Err(GantryError::NotCarried(name));
    }
    let extents = def.drawing().extents;
    let e = doc.find_mut(gantry_id).expect("found above");
    e.data = def.to_value();
    (e.width, e.height) = (extents.width(), extents.height());
    if let Some(s) = doc.find_mut(sign_id) {
        s.host = None;
        s.site = None;
        s.properties.remove(PROP_GANTRY);
    }
    Ok(())
}

/// Removes a gantry. Its signs stay in the document, unhosted and without site placement.
/// Returns the ids of the signs released.
pub fn remove_gantry(doc: &mut Document, gantry_id: ElementId) -> Result<Vec<ElementId>, GantryError> {
    let (_, def) = gantry(doc, gantry_id)?;
    let ids: Vec<ElementId> = def.signs.iter().map(|m| m.sign).collect();
    for id in &ids {
        if let Some(s) = doc.find_mut(*id) {
            s.host = None;
            s.site = None;
            s.properties.remove(PROP_GANTRY);
        }
    }
    doc.remove(gantry_id);
    Ok(ids)
}

/// The signs a gantry carries, in the order they were added.
pub fn signs_of(doc: &Document, gantry_id: ElementId) -> Vec<ElementId> {
    definition_of(doc, gantry_id).map(|d| d.signs.iter().map(|m| m.sign).collect()).unwrap_or_default()
}

/// The gantry that carries a sign, if any.
pub fn gantry_of(doc: &Document, sign_id: ElementId) -> Option<ElementId> {
    let host = doc.find(sign_id)?.host?;
    doc.find(host).filter(|h| h.category == GANTRY_CATEGORY).map(|h| h.id)
}

/// Signs that are carried by a gantry and also have post supports, with the gantry's id: a
/// conflict to put right (remove the supports, or take the sign off the gantry).
pub fn support_conflicts(doc: &Document) -> Vec<(ElementId, ElementId)> {
    doc.elements().iter().filter(|e| e.category == SIGN_CATEGORY).filter_map(|e| Some((e.id, gantry_of(doc, e.id)?))).filter(|(s, _)| has_post_supports(doc, *s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;
    use afrobim_alignment::{AlignmentBuilder, Profile, Pvi};
    use afrobim_kernel::Pt;
    use afrobim_supports::{add_supports, FoundationDefinition, FoundationShape, PostDefinition, Section, SupportArrangement};
    use serde_json::json;

    /// Heading north from (1000, 2000) m; profile level 50 m at station 0 rising 2 %.
    fn north() -> Alignment {
        let mut a = AlignmentBuilder::new("A1", 0.0, 1000.0, 2000.0, std::f64::consts::FRAC_PI_2).line(500.0).build().unwrap();
        a.profile = Some(Profile::new("P", vec![Pvi::point(0.0, 50.0), Pvi::point(500.0, 60.0)]).unwrap());
        a
    }

    fn sign(doc: &mut Document, name: &str) -> ElementId {
        doc.add(SIGN_CATEGORY, name, Pt::new(0.0, 0.0), 3000.0, 2000.0, "Signs", json!({}))
    }

    fn posts(doc: &mut Document, sign: ElementId) {
        let post = PostDefinition {
            section: Section::Chs { outside_diameter_mm: 100.0, wall_mm: 10.0 },
            length_mm: 1.0,
            embedment_mm: 800.0,
            material: "as specified".into(),
            finish: "as specified".into(),
            placeholder: false,
        };
        let foundation = FoundationDefinition {
            shape: FoundationShape::Block { length_mm: 600.0, width_mm: 600.0, depth_mm: 900.0 },
            top_below_ground_mm: 0.0,
            concrete: "as specified".into(),
            blinding_mm: None,
            socket: None,
            placeholder: false,
        };
        add_supports(doc, sign, &SupportArrangement::new(2000.0, 2, 300.0), &post, &foundation).unwrap();
    }

    #[test]
    fn add_gantry_places_the_element_on_sheet_and_site() {
        let mut doc = Document::default();
        let id = add_gantry(&mut doc, "G1", &portal(), Some(&north())).unwrap();
        let e = doc.find(id).unwrap();
        assert_eq!((e.category.as_str(), e.layer.as_str(), e.name.as_str()), ("gantry", "Gantries", "G1"));
        // Elevation: 21 000 wide (foundations), 1700 below the road to boom top 6800: 8500 high.
        assert!(close(e.width, 21_000.0) && close(e.height, 8500.0));
        let site = e.site.clone().unwrap();
        assert!(close(site.x, 990_700.0) && close(site.y, 2_100_000.0) && close(site.z, 52_000.0));
        assert_eq!(definition_of(&doc, id).unwrap().reference_level_m, Some(52.0));
        // Without an alignment: a sheet element only.
        let id2 = add_gantry(&mut doc, "G1", &cantilever(), None).unwrap();
        let e2 = doc.find(id2).unwrap();
        // The mark is made unique.
        assert!(e2.site.is_none() && e2.name != "G1");
        // Invalid or mis-set-out definitions add nothing.
        let n = doc.elements().len();
        let mut bad = portal();
        bad.legs.clear();
        assert!(add_gantry(&mut doc, "X", &bad, None).is_err());
        let mut off = portal();
        off.station_m = 9000.0;
        assert_eq!(add_gantry(&mut doc, "X", &off, Some(&north())), Err(GantryError::StationOffAlignment(9000.0)));
        assert_eq!(doc.elements().len(), n);
    }

    #[test]
    fn hosting_a_sign_sets_host_site_and_property() {
        let mut doc = Document::default();
        let a = north();
        let g = add_gantry(&mut doc, "G1", &portal(), Some(&a)).unwrap();
        let s1 = sign(&mut doc, "ADS-1");
        let s2 = sign(&mut doc, "ADS-2");
        add_sign_to_gantry(&mut doc, g, s1, SignPosition::Lane { index: 2 }, -150.0, None, Some(&a)).unwrap();
        let r = add_sign_to_gantry(&mut doc, g, s2, SignPosition::Offset { offset_m: 1.825 }, 200.0, None, Some(&a)).unwrap();
        assert_eq!(r.signs_updated, 2);
        assert_eq!(signs_of(&doc, g), [s1, s2]);
        assert_eq!((gantry_of(&doc, s1), gantry_of(&doc, g)), (Some(g), None));
        let e1 = doc.find(s1).unwrap();
        assert_eq!(e1.host, Some(g));
        assert_eq!(e1.properties[PROP_GANTRY], "G1");
        // As the geometry tests: lane 2 centre −1.825 m, up-station traffic, lower edge 57 850.
        let site = e1.site.clone().unwrap();
        assert!(close(site.x, 996_675.0) && close(site.y, 2_099_750.0) && close(site.z, 57_850.0) && close(site.rotation_deg, 0.0));
        // The second sign is over lane 3 (down-station): turned to face southbound... northward.
        let site2 = doc.find(s2).unwrap().site.clone().unwrap();
        assert!(close(site2.rotation_deg, 180.0) && close(site2.x, 1_003_325.0) && close(site2.y, 2_100_250.0) && close(site2.z, 58_200.0));
        // The gantry's definition records both, with the signs' sizes, and its elevation grew to
        // the top of the higher sign: 6000 + 200 + 2000 = 8200, + 1700 below the road.
        let def = definition_of(&doc, g).unwrap();
        assert_eq!((def.signs[0].mark.as_str(), def.signs[0].width_mm, def.signs[0].height_mm), ("ADS-1", 3000.0, 2000.0));
        assert!(close(doc.find(g).unwrap().height, 9900.0));
    }

    #[test]
    fn signs_follow_when_the_gantry_moves() {
        let mut doc = Document::default();
        let a = north();
        let g = add_gantry(&mut doc, "G1", &portal(), Some(&a)).unwrap();
        let s = sign(&mut doc, "ADS-1");
        add_sign_to_gantry(&mut doc, g, s, SignPosition::Lane { index: 2 }, -150.0, None, Some(&a)).unwrap();
        // Move the gantry 150 m up-station, raise the boom 500 and skew it 30°.
        let mut def = definition_of(&doc, g).unwrap();
        def.station_m = 250.0;
        def.boom.underside_above_reference_mm += 500.0;
        def.skew_deg = 30.0;
        set_definition(&mut doc, g, &def, Some(&a)).unwrap();
        let site = doc.find(s).unwrap().site.clone().unwrap();
        // Level: profile 50 + 0.02 × 250 = 55 m; + 6500 − 150.
        assert!(close(site.z, 55_000.0 + 6350.0));
        assert!(close(site.rotation_deg, 30.0));
        assert_eq!(site.station_m, Some(250.0));
        // Corner: u = −1825 / cos 30° − 1500 along the line at 30°, 250 back from it.
        let (c, sn) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
        let u = -1825.0 / c - 1500.0;
        assert!(close(site.x, 1_000_000.0 + u * c + 250.0 * sn));
        assert!(close(site.y, 2_250_000.0 + u * sn - 250.0 * c));
        assert!(close(doc.find(g).unwrap().site.clone().unwrap().z, 55_000.0));
        // The profile changes: only a sync is needed.
        let mut a2 = a.clone();
        a2.profile = Some(Profile::new("P", vec![Pvi::point(0.0, 70.0), Pvi::point(500.0, 70.0)]).unwrap());
        sync_gantry(&mut doc, g, Some(&a2)).unwrap();
        assert!(close(doc.find(s).unwrap().site.clone().unwrap().z, 76_350.0));
        // The sign changes size and mark: the mount is refreshed.
        let e = doc.find_mut(s).unwrap();
        (e.width, e.name) = (4000.0, "ADS-9".into());
        sync_gantry(&mut doc, g, Some(&a2)).unwrap();
        let m = definition_of(&doc, g).unwrap().signs[0].clone();
        assert_eq!((m.width_mm, m.mark.as_str()), (4000.0, "ADS-9"));
        // Without an alignment the placements are cleared but the hosting stays.
        sync_gantry(&mut doc, g, None).unwrap();
        assert!(doc.find(s).unwrap().site.is_none() && doc.find(g).unwrap().site.is_none());
        assert_eq!(doc.find(s).unwrap().host, Some(g));
        // A wrong alignment changes nothing.
        let other = AlignmentBuilder::new("B7", 0.0, 0.0, 0.0, 0.0).line(500.0).build().unwrap();
        assert!(matches!(sync_gantry(&mut doc, g, Some(&other)), Err(GantryError::WrongAlignment { .. })));
    }

    #[test]
    fn a_sign_has_posts_or_a_gantry_not_both() {
        let mut doc = Document::default();
        let g = add_gantry(&mut doc, "G1", &portal(), None).unwrap();
        let posted = sign(&mut doc, "POSTED");
        posts(&mut doc, posted);
        let before = doc.clone();
        assert_eq!(
            add_sign_to_gantry(&mut doc, g, posted, SignPosition::Lane { index: 1 }, 0.0, None, None),
            Err(GantryError::SignHasPostSupports("POSTED".into()))
        );
        assert_eq!(doc, before);
        assert!(support_conflicts(&doc).is_empty());
        // The other way round cannot be refused here (the supports crate adds the posts), but
        // it is found.
        let s = sign(&mut doc, "ADS-1");
        add_sign_to_gantry(&mut doc, g, s, SignPosition::Lane { index: 1 }, 0.0, None, None).unwrap();
        posts(&mut doc, s);
        assert_eq!(support_conflicts(&doc), [(s, g)]);
    }

    #[test]
    fn refusals_change_nothing() {
        let mut doc = Document::default();
        let g = add_gantry(&mut doc, "G1", &portal(), None).unwrap();
        let g2 = add_gantry(&mut doc, "G2", &cantilever(), None).unwrap();
        let s = sign(&mut doc, "ADS-1");
        let other = doc.add("marking", "M1", Pt::new(0.0, 0.0), 10.0, 10.0, "0", json!({}));
        let before = doc.clone();
        let add = |doc: &mut Document, g, s, p, serves| add_sign_to_gantry(doc, g, s, p, 0.0, serves, None);
        assert_eq!(add(&mut doc, g, other, SignPosition::Lane { index: 1 }, None), Err(GantryError::NotASign("M1".into())));
        assert_eq!(add(&mut doc, s, s, SignPosition::Lane { index: 1 }, None), Err(GantryError::NotAGantry("ADS-1".into())));
        assert_eq!(add(&mut doc, g, ElementId(999), SignPosition::Lane { index: 1 }, None), Err(GantryError::ElementNotFound(999)));
        assert!(matches!(add(&mut doc, g, s, SignPosition::Lane { index: 7 }, None), Err(GantryError::InvalidPosition(_))));
        // Over the verge with no lane: the direction must be stated.
        assert!(matches!(add(&mut doc, g, s, SignPosition::Offset { offset_m: 8.5 }, None), Err(GantryError::InvalidPosition(_))));
        assert_eq!(doc, before);
        add(&mut doc, g, s, SignPosition::Offset { offset_m: 8.0 }, Some(Direction::UpStation)).unwrap();
        // Already hosted: not by the same gantry again, nor by another.
        assert_eq!(add(&mut doc, g, s, SignPosition::Lane { index: 1 }, None), Err(GantryError::AlreadyHosted("ADS-1".into(), "G1".into())));
        assert_eq!(add(&mut doc, g2, s, SignPosition::Offset { offset_m: -5.0 }, None), Err(GantryError::AlreadyHosted("ADS-1".into(), "G1".into())));
        assert_eq!(remove_sign_from_gantry(&mut doc, g2, s), Err(GantryError::NotCarried("ADS-1".into())));
    }

    #[test]
    fn removing_signs_and_gantries() {
        let mut doc = Document::default();
        let a = north();
        let g = add_gantry(&mut doc, "G1", &portal(), Some(&a)).unwrap();
        let s1 = sign(&mut doc, "ADS-1");
        let s2 = sign(&mut doc, "ADS-2");
        let s3 = sign(&mut doc, "ADS-3");
        for (s, lane) in [(s1, 1), (s2, 2), (s3, 3)] {
            add_sign_to_gantry(&mut doc, g, s, SignPosition::Lane { index: lane }, 0.0, None, Some(&a)).unwrap();
        }
        remove_sign_from_gantry(&mut doc, g, s2).unwrap();
        let e = doc.find(s2).unwrap();
        assert!(e.host.is_none() && e.site.is_none() && !e.properties.contains_key(PROP_GANTRY));
        assert_eq!(signs_of(&doc, g), [s1, s3]);
        // A sign erased from the document is dropped at the next sync and reported.
        doc.remove(s3);
        let r = sync_gantry(&mut doc, g, Some(&a)).unwrap();
        assert_eq!((r.signs_updated, r.dropped), (1, vec!["ADS-3".to_string()]));
        assert_eq!(signs_of(&doc, g), [s1]);
        // Removing the gantry releases its signs.
        assert_eq!(remove_gantry(&mut doc, g).unwrap(), [s1]);
        assert!(doc.find(g).is_none());
        let e = doc.find(s1).unwrap();
        assert!(e.host.is_none() && e.site.is_none());
        assert!(signs_of(&doc, g).is_empty());
    }
}
