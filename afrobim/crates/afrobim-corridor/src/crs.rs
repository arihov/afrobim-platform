//! The coordinate system a survey is in: a label kept with the surface, and enough of the
//! projection to say where on the Earth a coordinate is.
//!
//! Only **WGS 84 / UTM** zones are known: EPSG:32601 to 32660 (north) and EPSG:32701 to 32760
//! (south). Nothing is reprojected. Coordinates are stored as the file gives them; the system is a
//! record of what they mean, and [`CoordinateSystem::to_geographic`] is there so that an import can
//! report the latitude and longitude of the survey, which shows at once a wrong zone or swapped
//! northing and easting.
//!
//! The inverse projection is the Krüger series for the transverse Mercator as given by Karney
//! (2011, "Transverse Mercator with an accuracy of a few nanometers", J. Geodesy 85), truncated
//! after the third-order terms of the third flattening. That is good to well under a millimetre
//! inside a UTM zone. The constants are those of the WGS 84 ellipsoid and of UTM: scale 0.9996 on
//! the central meridian, false easting 500 000 m, false northing 10 000 000 m in the south.

use serde::{Deserialize, Serialize};

const WGS84_A: f64 = 6_378_137.0;
const WGS84_INVERSE_FLATTENING: f64 = 298.257_223_563;
const UTM_SCALE: f64 = 0.9996;
const UTM_FALSE_EASTING: f64 = 500_000.0;
const UTM_FALSE_NORTHING_SOUTH: f64 = 10_000_000.0;

/// A projected coordinate system, by EPSG code and name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinateSystem {
    pub epsg: u32,
    pub name: String,
}

impl CoordinateSystem {
    /// WGS 84 / UTM zone `zone` (1 to 60), north or south of the equator.
    pub fn utm_wgs84(zone: u8, south: bool) -> Option<CoordinateSystem> {
        if !(1..=60).contains(&zone) {
            return None;
        }
        Some(CoordinateSystem {
            epsg: if south { 32700 } else { 32600 } + u32::from(zone),
            name: format!("WGS 84 / UTM zone {zone}{}", if south { 'S' } else { 'N' }),
        })
    }

    /// From an EPSG code. Only the WGS 84 / UTM codes are known.
    pub fn from_epsg(epsg: u32) -> Option<CoordinateSystem> {
        match epsg {
            32601..=32660 => CoordinateSystem::utm_wgs84((epsg - 32600) as u8, false),
            32701..=32760 => CoordinateSystem::utm_wgs84((epsg - 32700) as u8, true),
            _ => None,
        }
    }

    /// From what a person types: `EPSG:32736`, `32736`, `WGS 84 / UTM zone 36S`, `WGS84/UTM36S`,
    /// `UTM 36 S`, `utm36s`. Case, spaces and punctuation do not matter. A zone with no `N` or `S`
    /// is not accepted: the hemisphere changes the northing by ten thousand kilometres.
    pub fn parse(text: &str) -> Option<CoordinateSystem> {
        let compact: String = text.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase();
        if let Some(code) = compact.strip_prefix("EPSG").or(Some(compact.as_str())).and_then(|d| d.parse::<u32>().ok()) {
            return CoordinateSystem::from_epsg(code);
        }
        let rest = compact.split("UTM").nth(1)?;
        let rest = rest.strip_prefix("ZONE").unwrap_or(rest);
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let south = match &rest[digits.len()..] {
            "S" | "SOUTH" => true,
            "N" | "NORTH" => false,
            _ => return None,
        };
        // Whatever stands before "UTM" must be WGS 84 or nothing.
        let datum = compact.split("UTM").next().unwrap_or("");
        if !matches!(datum, "" | "WGS84") {
            return None;
        }
        CoordinateSystem::utm_wgs84(digits.parse().ok()?, south)
    }

    /// `EPSG:32736`.
    pub fn code(&self) -> String {
        format!("EPSG:{}", self.epsg)
    }

    /// `WGS 84 / UTM zone 36S (EPSG:32736)`.
    pub fn label(&self) -> String {
        format!("{} ({})", self.name, self.code())
    }

    /// The UTM zone and whether it is the southern one, when this is a WGS 84 / UTM system.
    pub fn utm_zone(&self) -> Option<(u8, bool)> {
        match self.epsg {
            32601..=32660 => Some(((self.epsg - 32600) as u8, false)),
            32701..=32760 => Some(((self.epsg - 32700) as u8, true)),
            _ => None,
        }
    }

    /// Latitude and longitude in degrees (WGS 84, north and east positive) of an easting and
    /// northing in metres.
    pub fn to_geographic(&self, easting_m: f64, northing_m: f64) -> Option<(f64, f64)> {
        let (zone, south) = self.utm_zone()?;
        let central_meridian = (f64::from(zone) * 6.0 - 183.0).to_radians();
        let f = 1.0 / WGS84_INVERSE_FLATTENING;
        let n = f / (2.0 - f);
        let (n2, n3) = (n * n, n * n * n);
        let radius = WGS84_A / (1.0 + n) * (1.0 + n2 / 4.0 + n2 * n2 / 64.0);
        let beta = [n / 2.0 - 2.0 * n2 / 3.0 + 37.0 * n3 / 96.0, n2 / 48.0 + n3 / 15.0, 17.0 * n3 / 480.0];
        let delta = [2.0 * n - 2.0 * n2 / 3.0 - 2.0 * n3, 7.0 * n2 / 3.0 - 8.0 * n3 / 5.0, 56.0 * n3 / 15.0];

        let xi = (northing_m - if south { UTM_FALSE_NORTHING_SOUTH } else { 0.0 }) / (UTM_SCALE * radius);
        let eta = (easting_m - UTM_FALSE_EASTING) / (UTM_SCALE * radius);
        let (mut xi1, mut eta1) = (xi, eta);
        for (j, b) in beta.iter().enumerate() {
            let k = 2.0 * (j + 1) as f64;
            xi1 -= b * (k * xi).sin() * (k * eta).cosh();
            eta1 -= b * (k * xi).cos() * (k * eta).sinh();
        }
        let chi = (xi1.sin() / eta1.cosh()).asin();
        let mut latitude = chi;
        for (j, d) in delta.iter().enumerate() {
            latitude += d * (2.0 * (j + 1) as f64 * chi).sin();
        }
        let longitude = central_meridian + eta1.sinh().atan2(xi1.cos());
        Some((latitude.to_degrees(), longitude.to_degrees()))
    }

    /// What is odd about coordinates said to be in this system, as sentences. Empty when nothing
    /// is. `min` and `max` are the south-west and north-east corners `(easting, northing)` in metres.
    ///
    /// Checked for a UTM zone: eastings between 100 000 m and 900 000 m (a zone is at most about
    /// 668 km wide, centred on 500 000 m), northings between 0 and 10 000 000 m, and the extent
    /// within three degrees of the zone's central meridian.
    pub fn doubts(&self, min: (f64, f64), max: (f64, f64)) -> Vec<String> {
        let mut out = Vec::new();
        let Some((zone, south)) = self.utm_zone() else { return out };
        if min.0 < 100_000.0 || max.0 > 900_000.0 {
            out.push(format!(
                "Eastings run from {:.3} to {:.3} m. A UTM easting lies between about 166 000 and 834 000 m, so these are not eastings of {}: check the column order.",
                min.0, max.0, self.name
            ));
        }
        if min.1 < 0.0 || max.1 > 10_000_000.0 {
            out.push(format!(
                "Northings run from {:.3} to {:.3} m. A UTM northing lies between 0 and 10 000 000 m, so these are not northings of {}: check the column order.",
                min.1, max.1, self.name
            ));
        }
        if !out.is_empty() {
            return out;
        }
        let central = f64::from(zone) * 6.0 - 183.0;
        for (e, n) in [min, max] {
            if let Some((lat, lon)) = self.to_geographic(e, n) {
                if (lon - central).abs() > 3.0 + 1e-9 {
                    out.push(format!(
                        "The survey reaches longitude {lon:.4}, which is outside zone {zone} ({:.0} to {:.0} degrees east). It may belong to a neighbouring zone.",
                        central - 3.0,
                        central + 3.0
                    ));
                    break;
                }
                if south != (lat < 0.0) && lat.abs() > 0.5 {
                    out.push(format!("The survey reaches latitude {lat:.4}, which is not in the {} hemisphere.", if south { "southern" } else { "northern" }));
                    break;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utm36s() -> CoordinateSystem {
        CoordinateSystem::utm_wgs84(36, true).unwrap()
    }

    #[test]
    fn utm_zones_have_their_epsg_codes_and_names() {
        let c = utm36s();
        assert_eq!((c.epsg, c.name.as_str()), (32736, "WGS 84 / UTM zone 36S"));
        assert_eq!(c.label(), "WGS 84 / UTM zone 36S (EPSG:32736)");
        assert_eq!(CoordinateSystem::utm_wgs84(36, false).unwrap().epsg, 32636);
        assert_eq!(CoordinateSystem::from_epsg(32601).unwrap().name, "WGS 84 / UTM zone 1N");
        assert_eq!(CoordinateSystem::from_epsg(32760).unwrap().name, "WGS 84 / UTM zone 60S");
        assert!(CoordinateSystem::utm_wgs84(0, true).is_none());
        assert!(CoordinateSystem::utm_wgs84(61, true).is_none());
        // Known codes only: 4326 is geographic, 21096 is Arc 1960 / UTM 36N.
        assert!(CoordinateSystem::from_epsg(4326).is_none());
        assert!(CoordinateSystem::from_epsg(21096).is_none());
    }

    #[test]
    fn typed_names_are_understood() {
        for text in ["EPSG:32736", "epsg 32736", "32736", "WGS 84 / UTM zone 36S", "WGS 84/ UTM36S", "wgs84 utm 36 s", "UTM36S", "UTM zone 36 South"] {
            assert_eq!(CoordinateSystem::parse(text), Some(utm36s()), "{text}");
        }
        assert_eq!(CoordinateSystem::parse("UTM 36N").unwrap().epsg, 32636);
        // No hemisphere, another datum, nonsense.
        for text in ["UTM 36", "Arc 1960 / UTM zone 36S", "UTM 61S", "", "metres", "EPSG:4326"] {
            assert_eq!(CoordinateSystem::parse(text), None, "{text}");
        }
    }

    #[test]
    fn the_inverse_projection_matches_worked_values() {
        let c = utm36s();
        // The origin of the zone: the equator on the central meridian (33 degrees east).
        let (lat, lon) = c.to_geographic(500_000.0, 10_000_000.0).unwrap();
        assert!(lat.abs() < 1e-9 && (lon - 33.0).abs() < 1e-9, "{lat} {lon}");
        // One degree south along the central meridian: the meridian arc of the WGS 84 ellipsoid
        // from the equator to 1 degree is 110 574.4 m to the decimetre; times 0.9996 is
        // 110 530.17 m. A decimetre of northing is 0.000 000 9 degrees.
        let (lat, lon) = c.to_geographic(500_000.0, 10_000_000.0 - 110_530.17).unwrap();
        assert!((lat + 1.0).abs() < 1e-6 && (lon - 33.0).abs() < 1e-9, "{lat} {lon}");
        // One degree east along the equator: 0.9996 a (L + L^3 (1 + e'^2) / 6) with L one degree
        // in radians and e'^2 = 0.006 739 497, which is 111 280.650 m.
        let (lat, lon) = c.to_geographic(611_280.650, 10_000_000.0).unwrap();
        assert!(lat.abs() < 1e-9 && (lon - 34.0).abs() < 1e-6, "{lat} {lon}");
        // The same easting and a northing counted from the equator in the northern zone.
        let (lat, _) = CoordinateSystem::utm_wgs84(36, false).unwrap().to_geographic(500_000.0, 110_530.17).unwrap();
        assert!((lat - 1.0).abs() < 1e-6, "{lat}");
        // Zone 31 is centred on 3 degrees east.
        let (_, lon) = CoordinateSystem::utm_wgs84(31, false).unwrap().to_geographic(500_000.0, 0.0).unwrap();
        assert!((lon - 3.0).abs() < 1e-9, "{lon}");
    }

    #[test]
    fn swapped_columns_and_a_wrong_zone_are_doubted() {
        let c = utm36s();
        assert!(c.doubts((213_629.0, 9_885_072.0), (237_955.0, 9_892_378.0)).is_empty());
        // Northing and easting the wrong way round.
        let swapped = c.doubts((9_885_072.0, 213_629.0), (9_892_378.0, 237_955.0));
        assert_eq!(swapped.len(), 1);
        assert!(swapped[0].contains("check the column order"), "{swapped:?}");
        // The same numbers said to be north of the equator: about 89 degrees north.
        let north = CoordinateSystem::utm_wgs84(36, false).unwrap().doubts((213_629.0, 9_885_072.0), (237_955.0, 9_892_378.0));
        assert!(north.is_empty() || north[0].contains("longitude"), "{north:?}");
        // Far east of the zone.
        let far = c.doubts((880_000.0, 9_885_072.0), (890_000.0, 9_892_378.0));
        assert!(far.len() == 1 && far[0].contains("neighbouring zone"), "{far:?}");
    }
}
