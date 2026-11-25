//! AfroBIM raster engine: georeferenced images and web map tiles.
//!
//! * [`georef`]: where an image sits. A six-term affine from pixel corners to the coordinates of
//!   the image's own system, read from a world file (`.pgw`, `.jgw`, `.tfw`, `.wld`) or from the
//!   GeoTIFF tags, and the system itself from the GeoTIFF keys or a `.prj` file.
//! * [`decode`]: the pixels, decimated while they are read so that a large orthophoto never sits
//!   whole in memory. TIFF is read strip by strip or tile by tile (and from an internal overview
//!   when the file has one), PNG row by row.
//! * [`tiles`]: the slippy-map tile grid (spherical Web Mercator), the providers and their URL
//!   templates.
//! * [`cache`]: a tile service with worker threads, a disk cache and de-duplication of requests in
//!   flight. The fetcher is a trait, so tests never touch the network.
//!
//! Nothing here reprojects pixels. A caller places an image by moving a grid of points from the
//! image's system to the project's through the kernel's transformation plans, and draws the
//! texture over that grid.

pub mod cache;
pub mod capture;
pub mod decode;
pub mod georef;
pub mod tiles;
pub mod vector;

pub use cache::{DiskCache, TileFetcher, TileResult, TileService};
pub use decode::{decode, Decoded, DecodeOptions};
pub use georef::{read_georef, resolve_crs, Affine, Georef};
pub use tiles::{CommercialUse, Provider, TileKey, PROVIDERS};
