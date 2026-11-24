//! The slippy-map tile grid and the tile providers.
//!
//! Web map tiles are 256-pixel squares of the spherical Web Mercator projection (EPSG:3857) on
//! WGS 84 latitude and longitude. At zoom `z` the world is `2^z` tiles across, numbered from the
//! north-west corner. A tile's edges are meridians and parallels, so its four corners and any
//! point inside it come from closed-form formulas: no projection library is needed. Placing a
//! tile in the project means moving a grid of its points from WGS 84 to the project system with
//! the kernel's transformation plans.

use serde::{Deserialize, Serialize};

/// Latitude limit of the Web Mercator square.
pub const MAX_LAT: f64 = 85.051_128_779_806_59;
/// Equatorial ground size of one pixel at zoom 0, metres (2 pi a / 256).
pub const M_PER_PX_Z0: f64 = 156_543.033_928_041;

/// One tile of one provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TileKey {
    pub provider: String,
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

impl TileKey {
    pub fn new(provider: &str, z: u8, x: u32, y: u32) -> TileKey {
        TileKey { provider: provider.to_string(), z, x, y }
    }

    /// The tile one level up that contains this one, and where this one sits in it (the
    /// fraction of the parent's width: left, top, size).
    pub fn parent(&self) -> Option<(TileKey, [f64; 3])> {
        if self.z == 0 {
            return None;
        }
        let p = TileKey { provider: self.provider.clone(), z: self.z - 1, x: self.x / 2, y: self.y / 2 };
        Some((p, [(self.x % 2) as f64 * 0.5, (self.y % 2) as f64 * 0.5, 0.5]))
    }
}

/// Fractional tile coordinates of a longitude and latitude at zoom `z`.
pub fn lonlat_to_tile(lon: f64, lat: f64, z: u8) -> (f64, f64) {
    let n = 2f64.powi(z as i32);
    let lat = lat.clamp(-MAX_LAT, MAX_LAT).to_radians();
    let x = (lon + 180.0) / 360.0 * n;
    let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * n;
    (x, y)
}

/// Longitude and latitude of fractional tile coordinates at zoom `z`.
pub fn tile_to_lonlat(x: f64, y: f64, z: u8) -> (f64, f64) {
    let n = 2f64.powi(z as i32);
    let lon = x / n * 360.0 - 180.0;
    let lat = (std::f64::consts::PI * (1.0 - 2.0 * y / n)).sinh().atan().to_degrees();
    (lon, lat)
}

/// The zoom whose ground pixel is closest to (and not coarser than about 1.4 times)
/// `metres_per_px` at latitude `lat`, within `min..=max`.
pub fn zoom_for(metres_per_px: f64, lat: f64, min: u8, max: u8) -> u8 {
    if metres_per_px.is_nan() || metres_per_px <= 0.0 {
        return max;
    }
    let ground = M_PER_PX_Z0 * lat.to_radians().cos().max(0.01);
    let z = (ground / metres_per_px).log2().round();
    (z.max(min as f64).min(max as f64)) as u8
}

/// The tiles at zoom `z` that cover a longitude and latitude box, as inclusive x and y ranges,
/// clamped to the world.
pub fn tiles_covering(lon_min: f64, lat_min: f64, lon_max: f64, lat_max: f64, z: u8) -> (std::ops::RangeInclusive<u32>, std::ops::RangeInclusive<u32>) {
    let n = 1u32 << z;
    let (x0, y0) = lonlat_to_tile(lon_min, lat_max, z);
    let (x1, y1) = lonlat_to_tile(lon_max, lat_min, z);
    let c = |v: f64| (v.floor().max(0.0) as u32).min(n - 1);
    (c(x0)..=c(x1), c(y0)..=c(y1))
}

/// The Bing quadkey of a tile.
pub fn quadkey(z: u8, x: u32, y: u32) -> String {
    (1..=z)
        .rev()
        .map(|i| {
            let mask = 1 << (i - 1);
            let d = (x & mask != 0) as u8 + 2 * (y & mask != 0) as u8;
            (b'0' + d) as char
        })
        .collect()
}

/// A tile provider.
#[derive(Debug, Clone, PartialEq)]
pub struct Provider {
    /// Stable identifier, stored in documents and used as the cache folder.
    pub id: &'static str,
    pub name: &'static str,
    /// URL with `{z}`, `{x}`, `{y}`, optionally `{s}` (a, b or c), `{-y}` (TMS rows), `{q}`
    /// (quadkey) and `{key}`.
    pub url: &'static str,
    pub attribution: &'static str,
    pub max_zoom: u8,
    /// True when the template has a `{key}` that must be filled.
    pub needs_key: bool,
    /// Whether the imagery is a photograph (shown with "satellite" wording) or a drawn map.
    pub imagery: bool,
}

/// The built-in providers. The first is the default. Those without `needs_key` work as they are;
/// their terms ask for the attribution to be shown and for tiles not to be fetched in bulk, which
/// the tile service respects (a few requests at a time, only what the view needs, cached on disk).
/// Keyed providers take the user's own key or token, kept for the session only. Every URL was
/// checked against the live service or the provider's API documentation on 2026-11-25. Google and
/// Bing are absent on purpose: Google's terms forbid fetching its tiles directly (its Map Tiles API
/// needs a session token, not a plain URL), and Bing Maps is retired in favour of Azure Maps.
pub const PROVIDERS: &[Provider] = &[
    // Free, no key.
    Provider { id: "esri-imagery", name: "Esri World Imagery (satellite)", url: "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}", attribution: "Imagery © Esri, Maxar, Earthstar Geographics, and the GIS User Community", max_zoom: 19, needs_key: false, imagery: true },
    Provider { id: "esri-street", name: "Esri World Street Map", url: "https://server.arcgisonline.com/ArcGIS/rest/services/World_Street_Map/MapServer/tile/{z}/{y}/{x}", attribution: "© Esri, HERE, Garmin, OpenStreetMap contributors and the GIS user community", max_zoom: 19, needs_key: false, imagery: false },
    Provider { id: "esri-topo", name: "Esri World Topographic", url: "https://server.arcgisonline.com/ArcGIS/rest/services/World_Topo_Map/MapServer/tile/{z}/{y}/{x}", attribution: "© Esri and contributors", max_zoom: 19, needs_key: false, imagery: false },
    Provider { id: "esri-relief", name: "Esri World Shaded Relief", url: "https://server.arcgisonline.com/ArcGIS/rest/services/World_Shaded_Relief/MapServer/tile/{z}/{y}/{x}", attribution: "© Esri, USGS, NOAA", max_zoom: 13, needs_key: false, imagery: false },
    Provider { id: "esri-light-gray", name: "Esri Light Gray Canvas", url: "https://server.arcgisonline.com/ArcGIS/rest/services/Canvas/World_Light_Gray_Base/MapServer/tile/{z}/{y}/{x}", attribution: "© Esri, HERE, Garmin, OpenStreetMap contributors", max_zoom: 16, needs_key: false, imagery: false },
    Provider { id: "esri-dark-gray", name: "Esri Dark Gray Canvas", url: "https://server.arcgisonline.com/ArcGIS/rest/services/Canvas/World_Dark_Gray_Base/MapServer/tile/{z}/{y}/{x}", attribution: "© Esri, HERE, Garmin, OpenStreetMap contributors", max_zoom: 16, needs_key: false, imagery: false },
    Provider { id: "osm", name: "OpenStreetMap", url: "https://tile.openstreetmap.org/{z}/{x}/{y}.png", attribution: "© OpenStreetMap contributors", max_zoom: 19, needs_key: false, imagery: false },
    Provider { id: "osm-hot", name: "OpenStreetMap Humanitarian (HOT)", url: "https://{s}.tile.openstreetmap.fr/hot/{z}/{x}/{y}.png", attribution: "© OpenStreetMap contributors, tiles by Humanitarian OpenStreetMap Team hosted by OpenStreetMap France", max_zoom: 19, needs_key: false, imagery: false },
    Provider { id: "opentopomap", name: "OpenTopoMap", url: "https://{s}.tile.opentopomap.org/{z}/{x}/{y}.png", attribution: "© OpenStreetMap contributors, SRTM | Style © OpenTopoMap (CC-BY-SA)", max_zoom: 17, needs_key: false, imagery: false },
    Provider { id: "carto-voyager", name: "CARTO Voyager", url: "https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}.png", attribution: "© OpenStreetMap contributors © CARTO", max_zoom: 19, needs_key: false, imagery: false },
    Provider { id: "carto-positron", name: "CARTO Positron (light)", url: "https://{s}.basemaps.cartocdn.com/light_all/{z}/{x}/{y}.png", attribution: "© OpenStreetMap contributors © CARTO", max_zoom: 19, needs_key: false, imagery: false },
    Provider { id: "carto-dark", name: "CARTO Dark Matter", url: "https://{s}.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png", attribution: "© OpenStreetMap contributors © CARTO", max_zoom: 19, needs_key: false, imagery: false },
    Provider { id: "s2cloudless", name: "Sentinel-2 cloudless 2016 (EOX, 10 m)", url: "https://tiles.maps.eox.at/wmts/1.0.0/s2cloudless_3857/default/g/{z}/{y}/{x}.jpg", attribution: "Sentinel-2 cloudless - https://s2maps.eu by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2016)", max_zoom: 15, needs_key: false, imagery: true },
    Provider { id: "s2cloudless-2020", name: "Sentinel-2 cloudless 2020 (EOX, 10 m, non-commercial)", url: "https://tiles.maps.eox.at/wmts/1.0.0/s2cloudless-2020_3857/default/g/{z}/{y}/{x}.jpg", attribution: "Sentinel-2 cloudless - https://s2maps.eu by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2020), CC BY-NC-SA 4.0", max_zoom: 15, needs_key: false, imagery: true },
    // Keyed: the user's own account.
    Provider { id: "mapbox-satellite", name: "Mapbox Satellite (needs a token)", url: "https://api.mapbox.com/v4/mapbox.satellite/{z}/{x}/{y}@2x.jpg90?access_token={key}", attribution: "© Mapbox © Maxar", max_zoom: 19, needs_key: true, imagery: true },
    Provider { id: "mapbox-streets", name: "Mapbox Streets (needs a token)", url: "https://api.mapbox.com/styles/v1/mapbox/streets-v12/tiles/256/{z}/{x}/{y}?access_token={key}", attribution: "© Mapbox © OpenStreetMap contributors", max_zoom: 19, needs_key: true, imagery: false },
    Provider { id: "mapbox-satellite-streets", name: "Mapbox Satellite Streets (needs a token)", url: "https://api.mapbox.com/styles/v1/mapbox/satellite-streets-v12/tiles/256/{z}/{x}/{y}?access_token={key}", attribution: "© Mapbox © OpenStreetMap contributors © Maxar", max_zoom: 19, needs_key: true, imagery: true },
    Provider { id: "maptiler-satellite", name: "MapTiler Satellite (needs a key)", url: "https://api.maptiler.com/maps/satellite/256/{z}/{x}/{y}.jpg?key={key}", attribution: "© MapTiler © OpenStreetMap contributors", max_zoom: 20, needs_key: true, imagery: true },
    Provider { id: "maptiler-hybrid", name: "MapTiler Hybrid (needs a key)", url: "https://api.maptiler.com/maps/hybrid/256/{z}/{x}/{y}.jpg?key={key}", attribution: "© MapTiler © OpenStreetMap contributors", max_zoom: 20, needs_key: true, imagery: true },
    Provider { id: "maptiler-streets", name: "MapTiler Streets (needs a key)", url: "https://api.maptiler.com/maps/streets-v2/256/{z}/{x}/{y}.png?key={key}", attribution: "© MapTiler © OpenStreetMap contributors", max_zoom: 20, needs_key: true, imagery: false },
    Provider { id: "maptiler-topo", name: "MapTiler Topo (needs a key)", url: "https://api.maptiler.com/maps/topo-v2/256/{z}/{x}/{y}.png?key={key}", attribution: "© MapTiler © OpenStreetMap contributors", max_zoom: 20, needs_key: true, imagery: false },
    Provider { id: "azure-imagery", name: "Azure Maps Imagery (Bing successor, needs a key)", url: "https://atlas.microsoft.com/map/tile?api-version=2024-04-01&tilesetId=microsoft.imagery&zoom={z}&x={x}&y={y}&tileSize=256&subscription-key={key}", attribution: "© Microsoft and its data providers (Azure Maps)", max_zoom: 19, needs_key: true, imagery: true },
    Provider { id: "azure-road", name: "Azure Maps Road (Bing successor, needs a key)", url: "https://atlas.microsoft.com/map/tile?api-version=2024-04-01&tilesetId=microsoft.base.road&zoom={z}&x={x}&y={y}&tileSize=256&subscription-key={key}", attribution: "© Microsoft and its data providers (Azure Maps)", max_zoom: 19, needs_key: true, imagery: false },
    Provider { id: "thunderforest-landscape", name: "Thunderforest Landscape (needs a key)", url: "https://api.thunderforest.com/landscape/{z}/{x}/{y}.png?apikey={key}", attribution: "Maps © Thunderforest, Data © OpenStreetMap contributors", max_zoom: 19, needs_key: true, imagery: false },
    Provider { id: "thunderforest-outdoors", name: "Thunderforest Outdoors (needs a key)", url: "https://api.thunderforest.com/outdoors/{z}/{x}/{y}.png?apikey={key}", attribution: "Maps © Thunderforest, Data © OpenStreetMap contributors", max_zoom: 19, needs_key: true, imagery: false },
    Provider { id: "stadia-satellite", name: "Stadia Alidade Satellite (needs a key)", url: "https://tiles.stadiamaps.com/tiles/alidade_satellite/{z}/{x}/{y}.jpg?api_key={key}", attribution: "© CNES, Airbus DS, PlanetObserver (Contains Copernicus Data) | © Stadia Maps © OpenMapTiles © OpenStreetMap contributors", max_zoom: 20, needs_key: true, imagery: true },
    Provider { id: "stadia-outdoors", name: "Stadia Outdoors (needs a key)", url: "https://tiles.stadiamaps.com/tiles/outdoors/{z}/{x}/{y}.png?api_key={key}", attribution: "© Stadia Maps © OpenMapTiles © OpenStreetMap contributors", max_zoom: 20, needs_key: true, imagery: false },
];

/// A provider by its identifier.
pub fn provider(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// The URL of a tile from a template.
pub fn expand_url(template: &str, z: u8, x: u32, y: u32, key: &str) -> String {
    let n = 1u32 << z;
    let s = ["a", "b", "c"][((x + y) % 3) as usize];
    template
        .replace("{z}", &z.to_string())
        .replace("{x}", &x.to_string())
        .replace("{-y}", &(n - 1 - y).to_string())
        .replace("{y}", &y.to_string())
        .replace("{s}", s)
        .replace("{q}", &quadkey(z, x, y))
        .replace("{key}", key)
}

/// Checks a custom template: HTTPS (or a local file URL is not accepted), and `{z}`, `{x}` and a
/// row placeholder, or a quadkey.
pub fn check_template(template: &str) -> Result<(), String> {
    let t = template.trim();
    if !t.starts_with("https://") {
        return Err("The tile URL must start with https://.".into());
    }
    let rows = t.contains("{y}") || t.contains("{-y}");
    if !(t.contains("{q}") || (t.contains("{z}") && t.contains("{x}") && rows)) {
        return Err("The tile URL needs {z}, {x} and {y} (or {-y} for TMS rows, or {q} for a quadkey).".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_places_land_in_their_tiles() {
        // Kampala (0.3476 N, 32.5825 E) at zoom 10 is tile 604, 511: just north of the equator,
        // which is the boundary between rows 511 and 512.
        let (x, y) = lonlat_to_tile(32.5825, 0.3476, 10);
        assert_eq!((x.floor() as u32, y.floor() as u32), (604, 511));
        // Null Island is the corner shared by the four tiles of zoom 1.
        let (x, y) = lonlat_to_tile(0.0, 0.0, 1);
        assert!((x - 1.0).abs() < 1e-12 && (y - 1.0).abs() < 1e-12);
        // The north-west corner of the world.
        let (lon, lat) = tile_to_lonlat(0.0, 0.0, 3);
        assert!((lon + 180.0).abs() < 1e-12 && (lat - MAX_LAT).abs() < 1e-9);
    }

    #[test]
    fn tile_coordinates_round_trip() {
        for &(lon, lat) in &[(32.5825, 0.3476), (30.0, -1.5), (-70.0, 45.0)] {
            let (x, y) = lonlat_to_tile(lon, lat, 17);
            let (lo, la) = tile_to_lonlat(x, y, 17);
            assert!((lo - lon).abs() < 1e-9 && (la - lat).abs() < 1e-9);
        }
    }

    #[test]
    fn the_zoom_matches_the_ground_pixel() {
        // At the equator zoom 17 is about 1.19 m per pixel.
        assert_eq!(zoom_for(1.19, 0.0, 0, 19), 17);
        assert_eq!(zoom_for(0.3, 0.0, 0, 19), 19);
        assert_eq!(zoom_for(0.01, 0.0, 0, 19), 19);
        assert_eq!(zoom_for(1e6, 0.0, 2, 19), 2);
        assert_eq!(zoom_for(0.0, 0.0, 0, 18), 18);
    }

    #[test]
    fn a_box_is_covered_by_the_tiles_it_touches() {
        let (xs, ys) = tiles_covering(32.58, 0.34, 32.59, 0.35, 15);
        let (x0, y0) = lonlat_to_tile(32.58, 0.35, 15);
        assert_eq!(*xs.start(), x0.floor() as u32);
        assert_eq!(*ys.start(), y0.floor() as u32);
        assert!(xs.end() >= xs.start() && ys.end() >= ys.start());
        let (xs, ys) = tiles_covering(-200.0, -90.0, 200.0, 90.0, 1);
        assert_eq!((xs, ys), (0..=1, 0..=1));
    }

    #[test]
    fn urls_fill_every_placeholder() {
        assert_eq!(expand_url(PROVIDERS[0].url, 15, 19500, 16380, ""), "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/15/16380/19500");
        assert_eq!(expand_url("https://t/{z}/{x}/{-y}.png?k={key}", 2, 1, 0, "K"), "https://t/2/1/3.png?k=K");
        assert_eq!(quadkey(3, 3, 5), "213");
        assert_eq!(expand_url("https://{s}.t/{q}", 3, 3, 5, ""), "https://c.t/213");
    }

    #[test]
    fn templates_must_be_https_and_complete() {
        assert!(check_template("https://a/{z}/{x}/{y}.png").is_ok());
        assert!(check_template("https://a/{q}.jpg").is_ok());
        assert!(check_template("http://a/{z}/{x}/{y}.png").is_err());
        assert!(check_template("https://a/{z}/{x}.png").is_err());
        assert!(PROVIDERS.iter().all(|p| check_template(p.url).is_ok()));
        // Every provider has a unique id, a key placeholder exactly when it needs a key, and an
        // attribution to show.
        let mut ids: Vec<&str> = PROVIDERS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PROVIDERS.len());
        for p in PROVIDERS {
            assert_eq!(p.url.contains("{key}"), p.needs_key, "{}", p.id);
            assert!(!p.attribution.is_empty() && (1..=22).contains(&p.max_zoom), "{}", p.id);
        }
    }

    #[test]
    fn a_tile_knows_its_parent_quadrant() {
        let (p, q) = TileKey::new("osm", 5, 11, 6).parent().unwrap();
        assert_eq!((p.z, p.x, p.y), (4, 5, 3));
        assert_eq!(q, [0.5, 0.0, 0.5]);
        assert!(TileKey::new("osm", 0, 0, 0).parent().is_none());
    }
}
