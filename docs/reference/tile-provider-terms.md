# Map tile provider terms (AfroBIM background map)

Checked on 25 November 2026 from the providers' public terms pages. This is an engineering reading of the terms, not legal advice: get the licence position confirmed before AfroBIM is sold. Where a page was silent, unreachable or unclear, the table uses the restrictive value, and the note says so.

The provider table is `afrobim/crates/afrobim-raster/src/tiles.rs` (`PROVIDERS`). Its fields `commercial` and `capture_allowed` come from this document.

## Meaning of the columns

- **Commercial, no key**: may a commercial product show these tiles to its users with no account?
- **Commercial, own key**: may it show them with the user's own key on a plan that covers commercial use?
- **Save (MAPCAPTURE)**: may the tiles be saved as an image file? "No" means display only. A GeoTIFF-like image plus world file is a saved copy under every provider below.
- Code value: `Allowed`, `OwnKeyRequired` or `NonCommercialOnly`; `capture_allowed` true or false.

## Summary

| Provider (ids) | Commercial, no key | Commercial, own key | Save (MAPCAPTURE) | Code value |
|---|---|---|---|---|
| OpenStreetMap standard (`osm`) | Yes, with limits and no guarantee | n/a | No (bulk download and offline use are banned) | `Allowed`, capture false |
| OpenTopoMap (`opentopomap`) | Yes (CC BY-SA), no mass downloads | n/a | No (mass download discouraged, share-alike on copies) | `Allowed`, capture false |
| OSM Humanitarian, OSM France (`osm-hot`) | Unclear: sister service is non-commercial | n/a | No | `NonCommercialOnly`, capture false |
| Esri imagery, street, topo, relief, grays (`esri-*`) | Unclear (keyless endpoint) | A paid ArcGIS account is the clear route; not wired in | No | `NonCommercialOnly`, capture false |
| CARTO Voyager, Positron, Dark Matter (`carto-*`) | No: free use needs a CARTO key | Yes, up to 1 million requests a month, then paid | No | `NonCommercialOnly`, capture false |
| EOX Sentinel-2 cloudless 2016 and 2020 (`s2cloudless`, `s2cloudless-2020`) | No: EOX commercial licence needed | Not applicable | No | `NonCommercialOnly`, capture false |
| Mapbox (`mapbox-*`) | n/a | Yes, own token | No (unclear, so restrictive) | `OwnKeyRequired`, capture false |
| MapTiler (`maptiler-*`) | n/a | Yes, on a plan that allows it; free plan must show the MapTiler logo | No (unclear, so restrictive) | `OwnKeyRequired`, capture false |
| Azure Maps (`azure-*`) | n/a | Yes, own subscription | No (raw tiles may not be saved) | `OwnKeyRequired`, capture false |
| Thunderforest (`thunderforest-*`) | n/a | Yes, own key; bulk download needs a paid plan | No (bulk download restricted) | `OwnKeyRequired`, capture false |
| Stadia Maps (`stadia-*`) | n/a | Free tier is non-commercial or evaluation; a paid plan is needed | No (satellite may not be bulk downloaded) | `OwnKeyRequired`, capture false |
| Custom tile URL | The user's own source | The user's own source | Allowed by AfroBIM; the user answers for the terms | not a built-in |

Result: no built-in provider allows MAPCAPTURE. A capture needs a custom tile URL whose licence the owner holds (for example an own tile server, a licensed orthophoto service, or a provider agreement that permits saving).

## Per provider

### OpenStreetMap standard (`osm`)
- Terms: https://operations.osmfoundation.org/policies/tiles/ (checked 2026-11-25)
- Commercial: the policy does not forbid it, but warns that commercial services may lose access at any time; best-effort, no guarantee. It names commercial providers as alternatives.
- Saving: bulk downloading, pre-seeding, "download for offline use" and prefetching are prohibited. Tiles must be cached as the headers say (at least 7 days if unreadable). A generic user agent is blocked; AfroBIM sends `AfroBIM/<version>`.
- Attribution: `© OpenStreetMap contributors`, clearly visible on the map.

### OpenTopoMap (`opentopomap`)
- Terms: https://opentopomap.org/about (checked 2026-11-25)
- Commercial: the FAQ says CC BY-SA permits commercial use with attribution and share-alike. Do not overload the server (no mass downloads). Volunteer service, no availability guarantee; the maintainers ask to be told about larger uses.
- Saving: mass downloads discouraged, and saved copies would carry share-alike, so capture is off.
- Attribution (official, German): `Kartendaten: © OpenStreetMap-Mitwirkende, SRTM | Kartendarstellung: © OpenTopoMap (CC-BY-SA)`. AfroBIM draws the English equivalent: `© OpenStreetMap contributors, SRTM | Style © OpenTopoMap (CC-BY-SA)`.

### OSM Humanitarian, OSM France (`osm-hot`)
- Terms: https://openmaps.fr/tile-usage-policy.html (checked 2026-11-25). This is the OpenMaps France policy, the closest published policy; the HOT tile host `tile.openstreetmap.fr` has no policy page of its own that could be read.
- Commercial: that policy says its free service is for non-commercial projects and offers a paid service. Treated as non-commercial.
- Saving: bulk download and prefetching prohibited.
- Attribution: `© OpenStreetMap contributors, tiles by Humanitarian OpenStreetMap Team hosted by OpenStreetMap France`.

### Esri (`esri-imagery`, `esri-street`, `esri-topo`, `esri-relief`, `esri-light-gray`, `esri-dark-gray`)
- Terms: https://developers.arcgis.com/documentation/mapping-apis-and-services/deployment/basemap-attribution/ and the ArcGIS Online item terms of each service, under the Esri master agreement (https://www.esri.com/en-us/legal/terms/full-master-agreement). Checked 2026-11-25; the pages could not be read in full by the tool, so the reading rests on search results.
- Commercial: the keyless `server.arcgisonline.com` endpoint has no clear grant for a commercial product. Esri's route for products is an ArcGIS Location Platform account and key. Treated as non-commercial.
- Saving: the standard layer is not meant for exporting tiles for offline use (Esri points to a separate export layer). Capture off.
- Attribution: required in every application. Texts used are those in the table in `tiles.rs` (for imagery: `Imagery © Esri, Maxar, Earthstar Geographics, and the GIS User Community`); the data-source line of each service can change, so check the service's current copyright text.

### CARTO (`carto-voyager`, `carto-positron`, `carto-dark`)
- Terms: https://carto.com/legal/basemap-terms/ (checked 2026-11-25)
- Commercial: allowed only with the customer's own CARTO key: free to 1 million commercial tile requests a month, then a paid plan. The built-in URLs carry no key, which the terms do not authorise (they may be watermarked), so the entries are non-commercial until a key is added.
- Saving: device cache limited to 30 days; server caching and bulk download prohibited.
- Attribution: `© OpenStreetMap, © CARTO`, prominent and conspicuous.

### EOX Sentinel-2 cloudless (`s2cloudless` 2016, `s2cloudless-2020`)
- Terms: https://cloudless.eox.at/documentation/license (checked 2026-11-25)
- Commercial: the page shows one licence for the whole product: commercial use needs an EOX Commercial licence. The older split (2016 under CC BY 4.0, later years under CC BY-NC-SA 4.0) could not be confirmed on the current page, so both are treated as non-commercial.
- Saving: only under the licence.
- Attribution: `EOxCloudless https://cloudless.eox.at by EOX IT Services GmbH (Contains modified Copernicus Sentinel data <year>)`, year 2016 or 2020.

### Mapbox (`mapbox-satellite`, `mapbox-streets`, `mapbox-satellite-streets`)
- Terms: https://www.mapbox.com/legal/tos and https://www.mapbox.com/legal/product-terms (checked 2026-11-25; the Product Terms PDF itself was not readable by the tool).
- Commercial: own token required; some application types need a separate commercial licence. Terms on offline storage not confirmed, so saving is off.
- Attribution: `© Mapbox © OpenStreetMap contributors` (satellite also `© Maxar`); the notices may not be removed.

### MapTiler (`maptiler-*`)
- Terms: https://www.maptiler.com/terms/ (checked 2026-11-25)
- Commercial: own key within the plan's quota; no commercial derivative works unless a licence allows them. The free plan must show the MapTiler logo, which AfroBIM does not draw, so a paid plan is needed in practice. Saving is not addressed in the main terms, so it is off.
- Attribution: `© MapTiler © OpenStreetMap contributors`, always visible and readable.

### Azure Maps (`azure-imagery`, `azure-road`)
- Terms: https://www.microsoft.com/licensing/terms/productoffering/MicrosoftAzure/MCA (Azure Maps product terms, checked 2026-11-25)
- Commercial: own subscription. Raw tiles may not be saved "under any circumstances"; caching only to reduce latency and within the header validity.
- Attribution: text should come from the Get Map Attribution API; AfroBIM shows a fixed `© Microsoft and its data providers (Azure Maps)`. Screenshots need the Microsoft reprint statement.

### Thunderforest (`thunderforest-landscape`, `thunderforest-outdoors`)
- Terms: https://www.thunderforest.com/terms/ (checked 2026-11-25)
- Commercial: "permitted and encouraged"; the free plan has limits. On-device offline caching is allowed; bulk download or pre-caching needs a paid plan, and proxies and redistribution are banned.
- Attribution: `Maps © Thunderforest, Data © OpenStreetMap contributors`, with links on screen.

### Stadia Maps (`stadia-satellite`, `stadia-outdoors`)
- Terms: https://stadiamaps.com/terms-of-service/ (checked 2026-11-25)
- Commercial: the free tier is for non-commercial or evaluation use only; a paid subscription is needed. Offline caching is limited (100 MB per device on a mobile app); satellite imagery may not be bulk downloaded.
- Attribution: `© Stadia Maps © OpenMapTiles © OpenStreetMap contributors`; the satellite style adds the imagery credits in `tiles.rs`.

## Behaviour in AfroBIM

- Default for a new document: OpenStreetMap (commercial use allowed, no key).
- Quick map types: Satellite uses MapTiler (recommended, owner decision 2026-11-25), Mapbox, Azure or Stadia (own key; none works without one), Street uses OpenStreetMap or a keyed provider, Terrain uses OpenTopoMap or a keyed provider. With a key typed in, the first keyed provider of the type is chosen, which is MapTiler in all three types, so one MapTiler key covers Satellite, Street and Terrain. Customers get the key from cloud.maptiler.com (the free plan is non-commercial; client work needs a paid plan).
- The list groups providers as Free (commercial use allowed), Needs your own key, and Evaluation only (not for commercial work). A warning shows while an evaluation provider is selected.
- MAPCAPTURE refuses every provider with `capture_allowed = false` and says why. The attribution is always drawn on the map and written to the capture's `.attribution.txt`.

## Open points for the owner

1. Is a keyless OpenStreetMap standard tile server acceptable as the Street default of a paid product, given the no-guarantee policy? A keyed provider (own MapTiler or Mapbox plan) is safer for support.
2. Which paid satellite source will AfroBIM recommend to customers (Mapbox, MapTiler, Azure Maps, or an Esri ArcGIS Location Platform key)? Esri keyed access is not yet wired in.
3. Ask Esri, EOX and CARTO in writing if their keyless or free layers may be used. A CARTO key and an EOX commercial licence would move those providers to allowed.
4. If customers need saved basemaps for drawings, a provider agreement that allows saving, or a licensed orthophoto, is needed; MAPCAPTURE works with a custom URL for that.
