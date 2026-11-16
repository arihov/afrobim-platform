//! The coordinate system a survey is in. This is the kernel's [`afrobim_kernel::ProjectCrs`]: the
//! one coordinate system type of the application, the same one the project setting holds. The
//! corridor crate used to carry its own `{epsg, name}` type for WGS 84 / UTM only; it is now this
//! alias, so a survey can be in any system the kernel knows (WGS 84 and Arc 1960 UTM zones) or be
//! named only.
//!
//! Nothing is reprojected. Coordinates are stored as the file gives them; the system is a record of
//! what they mean, and `to_geographic` is there so that an import can report the latitude and
//! longitude of the survey, which shows at once a wrong zone or swapped northing and easting.
//!
//! Surfaces saved before this change hold `{"epsg": 32736, "name": "WGS 84 / UTM zone 36S"}`; the
//! kernel type reads that (see `ProjectCrs`'s deserialiser), so those documents open unchanged.

pub use afrobim_kernel::ProjectCrs as CoordinateSystem;

#[cfg(test)]
mod tests {
    use super::*;

    fn utm36s() -> CoordinateSystem {
        CoordinateSystem::utm_wgs84(36, true).unwrap()
    }

    #[test]
    fn utm_zones_have_their_epsg_codes_and_names() {
        let c = utm36s();
        assert_eq!((c.epsg, c.name.as_str()), (Some(32736), "WGS 84 / UTM zone 36S"));
        assert_eq!(c.label(), "WGS 84 / UTM zone 36S (EPSG:32736)");
        assert_eq!(CoordinateSystem::utm_wgs84(36, false).unwrap().epsg, Some(32636));
        assert_eq!(CoordinateSystem::from_epsg(32601).unwrap().name, "WGS 84 / UTM zone 1N");
        assert!(CoordinateSystem::utm_wgs84(0, true).is_none());
        // Known codes only: 4326 is geographic. Arc 1960 / UTM 36N is known now.
        assert!(CoordinateSystem::from_epsg(4326).is_none());
        assert_eq!(CoordinateSystem::from_epsg(21096).unwrap().name, "Arc 1960 / UTM zone 36N");
    }

    #[test]
    fn typed_names_are_understood() {
        for text in ["EPSG:32736", "epsg 32736", "32736", "WGS 84 / UTM zone 36S", "WGS 84/ UTM36S", "wgs84 utm 36 s", "UTM36S", "UTM zone 36 South"] {
            assert_eq!(CoordinateSystem::parse(text), Some(utm36s()), "{text}");
        }
        assert_eq!(CoordinateSystem::parse("UTM 36N").unwrap().epsg, Some(32636));
        assert_eq!(CoordinateSystem::parse("Arc 1960 / UTM zone 36S").unwrap().epsg, Some(21036));
        for text in ["UTM 36", "UTM 61S", "", "metres"] {
            assert_eq!(CoordinateSystem::parse(text), None, "{text}");
        }
    }

    #[test]
    fn a_surface_saved_with_the_old_type_reads_back() {
        let old: CoordinateSystem = serde_json::from_value(serde_json::json!({"epsg": 32736, "name": "WGS 84 / UTM zone 36S"})).unwrap();
        assert_eq!(old, utm36s());
    }

    #[test]
    fn swapped_columns_and_a_wrong_zone_are_doubted() {
        let c = utm36s();
        assert!(c.doubts((213_629.0, 9_885_072.0), (237_955.0, 9_892_378.0)).is_empty());
        let swapped = c.doubts((9_885_072.0, 213_629.0), (9_892_378.0, 237_955.0));
        assert_eq!(swapped.len(), 1);
        assert!(swapped[0].contains("check the column order"), "{swapped:?}");
        let far = c.doubts((880_000.0, 9_885_072.0), (890_000.0, 9_892_378.0));
        assert!(far.len() == 1 && far[0].contains("neighbouring zone"), "{far:?}");
    }
}
