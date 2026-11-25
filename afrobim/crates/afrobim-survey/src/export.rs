// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Export of network points to `afrobim-cogo` point structures.

use std::collections::{BTreeMap, HashSet};

use afrobim_cogo::{CogoPoint, PointStore};

use crate::error::{Result, SurveyError};
use crate::model::Network;

/// Points of a network that have easting and northing, as COGO points. A point name that is a whole
/// number (and not used twice) becomes the point number; other names get the next free numbers
/// starting at `first_free_number` (an input) and keep their name in `name`. Returns the points and
/// the map from survey name to COGO number. Coordinates are copied exactly.
pub fn to_cogo_points(net: &Network, first_free_number: u32) -> (Vec<CogoPoint>, BTreeMap<String, u32>) {
    let mut used: HashSet<u32> = HashSet::new();
    let mut map = BTreeMap::new();
    let mut numbered = Vec::new();
    for p in net.points.iter().filter(|p| p.e.is_some() && p.n.is_some()) {
        if let Ok(k) = p.name.parse::<u32>() {
            if used.insert(k) {
                map.insert(p.name.clone(), k);
                numbered.push((p, k));
                continue;
            }
        }
        numbered.push((p, 0));
    }
    let mut next = first_free_number;
    let mut out = Vec::new();
    for (p, mut k) in numbered {
        if k == 0 {
            while used.contains(&next) {
                next += 1;
            }
            k = next;
            used.insert(k);
            map.insert(p.name.clone(), k);
        }
        let mut c = CogoPoint::new(k, p.e.unwrap(), p.n.unwrap(), p.z, &p.description);
        c.name = Some(p.name.clone());
        c.source.file = Some("afrobim-survey".into());
        c.source.note = Some(if p.computed { "computed by radiation or adjustment".into() } else if p.control { "control".into() } else { "given".into() });
        out.push(c);
    }
    (out, map)
}

/// Add the points of a network to a COGO point store.
pub fn into_store(net: &Network, store: &mut PointStore, first_free_number: u32) -> Result<BTreeMap<String, u32>> {
    let (pts, map) = to_cogo_points(net, first_free_number);
    for p in pts {
        store.add(p).map_err(SurveyError::from)?;
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SurveyPoint;

    #[test]
    fn numbers_names_and_exact_coordinates() {
        let mut net = Network::new("N");
        net.points.push(SurveyPoint::control("10", 500_000.123, 9_960_000.456, Some(100.5)));
        net.points.push(SurveyPoint::given("EP", 500_001.0, 9_960_001.0, None));
        net.points.push(SurveyPoint::given("11", 1.0, 2.0, None));
        let mut no_coords = SurveyPoint::given("X", 0.0, 0.0, None);
        no_coords.e = None;
        net.points.push(no_coords);
        let (pts, map) = to_cogo_points(&net, 10);
        assert_eq!(pts.len(), 3);
        assert_eq!(map["10"], 10);
        assert_eq!(map["11"], 11);
        assert_eq!(map["EP"], 12, "free numbers skip those taken by numeric names");
        assert_eq!(pts[0].northing, 9_960_000.456);
        assert_eq!(pts[1].name.as_deref(), Some("EP"));
        let mut store = PointStore::new();
        into_store(&net, &mut store, 100).unwrap();
        assert_eq!(store.len(), 3);
        assert_eq!(store.get(10).unwrap().easting, 500_000.123);
    }
}
