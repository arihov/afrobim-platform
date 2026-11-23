# afrobim-raster

Georeferenced imagery and web map tiles, as an engine with no user interface. The desktop app
uses it for the `IMAGE` (image underlays) and `BASEMAP` (online background map) commands.

## Contents

| Module | What it does |
| --- | --- |
| `georef` | The six-term affine from pixel corners to the image's own coordinates. Read from a world file (`.pgw`, `.jgw`, `.tfw`, `.wld`, short and long forms) or from the GeoTIFF tags (ModelTiepoint with ModelPixelScale, or ModelTransformation). The system comes from the GeoTIFF keys (ProjectedCSType 3072, GeographicType 2048) or a `.prj` beside the image; an ESRI `.prj` with no EPSG authority is matched by name against the kernel's registry. `resolve_crs` says whether an image is used as it is, moved through a transformation, or cannot be placed. |
| `decode` | The pixels, decimated while they are read: TIFF strip by strip or tile by tile (a strip with no kept row is skipped, an internal overview is used when it meets the requested size), PNG row by row, JPEG and the rest through `image` under an allocation limit. 16-bit imagery is stretched between the 2nd and 98th percentile per band. Floating-point rasters are refused. |
| `tiles` | The slippy-map tile grid on spherical Web Mercator: longitude and latitude to tile and back, the zoom for a ground pixel, the tiles covering a box, quadkeys, URL templates and the built-in providers with their attributions. |
| `cache` | `TileService`: worker threads, newest request first, de-duplication of requests in flight, a queue cap, `retain` to drop requests the view no longer needs, and a disk cache with a 30-day age and atomic writes. The fetcher is a trait; the HTTP one (rustls, HTTPS only, timeouts, an 8 MB limit, a generic user agent) is behind the `http` feature, so tests never touch the network. |

## Conventions

- Pixel coordinates are corner based: `(0, 0)` is the top-left corner of the top-left pixel. A
  world file (pixel centres) and a GeoTIFF with PixelIsPoint are moved half a pixel on reading.
- Nothing here reprojects pixels. A caller moves a grid of points from the image's system to the
  project's with the kernel's transformation plans and draws the texture over the grid.

## Limits

- Palette, YCbCr (JPEG-compressed) and band-separate TIFFs are refused with the GDAL line that
  converts them. GeoTIFF user-defined systems are not read.
- Decimation is nearest neighbour, not averaged.
- The built-in providers' terms (attribution, no bulk download) are the user's to follow; the tile
  service only fetches what the view needs, three at a time, and caches on disk.

## Tests

`cargo test -p afrobim-raster --features http`: world files (centre to corner, rotation, the ACP
orthophoto's own `.pgw`), ESRI `.prj` matching, GeoKeys, the decoder on generated PNG and TIFF
files (stripped, 16-bit, floating point), tile maths, URL templates, and the tile service with a
fake fetcher (disk cache, de-duplication, dropped requests, refused payloads).
