# AfroBIM platform: audit and feature plan (2026-11-23)

Scope: `arihov/afrobim-platform` at `main` (`d3d3deb`), read in full at the crate level, with spot
checks of the findings that matter most. ACP Studio (`ACP MASTER PLAN/acp-studio`, live at
`studio.africacoffepark.com`) was read for features worth bringing across. The first item of the
plan (georeferenced images and an online background map) is built on the branch
`claude/satellite-basemap` together with this document.

Severity: **High** (risk of lost work, exposure or a wrong result), **Med** (a real weakness
with a workaround), **Low** (hygiene). Effort: **S** (days), **M** (one to two weeks), **L**
(more).

## 1. Summary

The engineering core is sound. There are 34 crates and about 300 000 lines of Rust, with no
`unsafe` code. The dependency set is lean and the test suite is large: 2 429 tests passed on macOS
on this branch. The four that failed are the known headless file-picker tests in
`u4_dialogs_test` (section 2.5). Parsers carry depth and count limits, and the CRS module refuses
to invent datum shifts.

The risks sit around the core:

1. **Work can be lost.** A save overwrites the file in place. There is no autosave and no crash
   recovery.
2. **The MCP server writes where it is told.** An assistant driving it can overwrite any
   `.afrobim` or `.dxf` file on the machine.
3. **CI is thinner than the suite.** It runs on Linux only, it skips the format cross-checks, and
   it has no lint, format or dependency audit.
4. **The app crate is a 76 000-line binary.** Nothing outside it (the Python package, the MCP
   server) can reuse its glue.
5. **Gaps for a road platform:**
   - imagery (now built);
   - GIS exchange (GeoJSON, shapefile, KML);
   - swept paths;
   - design-check reporting by link;
   - collaboration.

## 2. Audit

### 2.1 Architecture

| Finding | Evidence | Sev | Recommendation |
|---|---|---|---|
| The kernel stays small and generic, as the first plan required. `Module` has three methods, and element data is untyped JSON that each module parses | `afrobim-kernel/src/module.rs:38`, `document.rs:60` | positive | Keep it |
| The engines depend on each other as a mesh. For example, `cost` depends on 10 discipline crates and `ifc` on 10 | `Cargo.toml` of each crate | Low | Keep the shared types in a `afrobim-model` crate if the mesh grows |
| The app is one binary crate: 75 850 lines, 124 modules, files named by round (`civil_build2.rs`, `sdb_ui2.rs`), and 343 tests in 16 files named `a1_`, `u2_`, `u4_` | `crates/afrobim-app/src` | Med | Split it into a library plus a thin binary, rename the files by feature, and move the tests to `tests/` |
| The file format has been `FORMAT_VERSION = 1` through six rounds of schema growth, with no migrations | `document.rs:11` | Med | Version each category's data and add a migration step on load |
| Undo keeps 100 full snapshots of every element. Kafunjo is 2.2 MB, so undo can reach hundreds of MB | `app.rs` undo stack | Med | Diff-based undo, or keep heavy TIN and point data out of the snapshots |
| A background job cannot be cancelled; Cancel only discards the result. The fingerprint serialises the whole document | `background.rs:23`, `:56` | Low | Pass a cancel token into the engines; fingerprint with a revision counter |
| Duplicates: two TIN engines (`corridor/tin.rs`, `afrobim-terrain`), two Delaunay libraries, LandXML readers in four crates, three point-file readers, two `crs.rs`, two traverses, `roxmltree` 0.20 and 0.21 | grep | Med | One TIN, one LandXML crate, one point reader, one CRS module |

### 2.2 Code quality

- **Panic sites in production code.** About 150 `unwrap`, 132 `expect` and 10 `panic!` in about
  201 000 lines, roughly 1.4 per 1 000 lines. The most are in `lighting` (43 unwrap), `app`
  (12 / 19) and `survey` (19, guarded). Low.
- **Clippy.** About 58 warnings on a sample of 9 crates, mostly style
  (`neg_cmp_op_on_partial_ord`, too many arguments). There is no `clippy.toml`, `rustfmt.toml`,
  `deny.toml`, `rust-toolchain` or workspace `[lints]`. Low.
- **Large files.** `app.rs` 3 218, `sheets_ui.rs` 2 482, `cost_ui.rs` 2 239, `pipes_ui.rs` 1 866,
  `landxml_ext.rs` 1 863. No single file is extreme; the crate as a whole is.

### 2.3 Security and data safety

| Finding | Evidence (checked) | Sev | Recommendation |
|---|---|---|---|
| Saving is `std::fs::write` over the file. A crash or power cut mid-save leaves a corrupt project. There is no autosave and no panic hook | `afrobim-kernel/src/document.rs:409`; no `set_hook` in `main.rs` | High | Write to a temporary file, `fsync`, then rename. Add a timed autosave to a recovery file and a panic hook that writes one |
| Third-party material is committed: the Transport fonts (`fonts/tranh___.TTF`, `tranm___.TTF`, commit `a5bbe7b`) and 23 manual PDFs and a help file under `Manuals/` (LFS). Tagged releases bundle the fonts | `git ls-files` | High if the repo or a release is ever shared; Low while private | Keep the repo private. Default `bundle_fonts` to false. Purge from history before any sharing |
| No licence | repo root | Med | Choose one before any binary leaves your hands |
| MCP server: `resolve` accepts any absolute path with the right extension. Writes are not confined to a project root, and only `export_dxf` refuses symlinks | `afrobim-mcp/src/paths.rs:33`, `:76`, `:122` | Med | Confine every path to a configured root and refuse symlinks on every write |
| The ODA File Converter is run with no timeout. It runs automatically for a DWG named inside a received `.afrobim` when the document opens | `afrobim-dxf/src/reader/dwg.rs:158`, `underlay.rs` | Med | Add a timeout. Ask before converting a DWG that a document names |
| The DWG cache in the shared temporary folder is trusted if `drawing.dxf` exists, and is never cleaned | `dwg.rs:149` | Low | Use a per-user cache folder with a size-capped clean-up |
| A DXF proxy count sizes an allocation (`with_capacity(declared)`). IFC and LandXML are read whole (a 233 MB LandXML used 2.2 GB) | `proxy.rs:94`, `ifc/import/mod.rs:274`, `landxml.rs:117` | Low/Med | Clamp counts to the input length; stream the XML; set file-size limits |
| Signing: macOS ad hoc, not notarised; Windows `.msi` unsigned | `packaging/macos/build-dmg.sh:75` | Med | Developer ID and `notarytool`; Authenticode |
| No dependency audit and no Dependabot | `.github/` | Med | Add `cargo deny check` (advisories, licences) to CI |

Network use: none before this branch. The background map is now the only network feature. It is
off until switched on, HTTPS only, with timeouts and a size limit, and a generic user agent.

### 2.4 Gaps against the roadmap

**Promised but not built:**

- Phase 3.1, a standalone CLI.
- Phase 3.4, scripting inside the app.
- Phase 4.1, auto-update.
- Phase 4.2, licensing and trial mode.
- Phase 4.3, a WASM build.
- S2 is partial: site photos are file paths only, with no inspection or non-conformance records.
- From round 4, still open:
  - EN 13201-3;
  - BCF 3.0;
  - IFC boolean geometry.
- The Python package and the MCP server expose draw, kernel, signs and dxf only. No civil engine
  is reachable from them.

**Missing for a road platform** (grep found nothing for each):

- GeoJSON, shapefile, KML/KMZ, WMS/WMTS;
- PDF underlay;
- DWG export;
- swept-path analysis;
- multi-user work, file locking, cloud sync;
- autosave.

Raster imagery and a basemap were missing; they are built on this branch (section 4).

### 2.5 CI and release

- **`ci.yml`** runs on Ubuntu only, on push and on pull requests. Windows and macOS run only when
  started by hand.
  - It runs no clippy, no fmt check, no audit and no MSRV check.
  - ifcopenshell, ezdxf, xmllint and poppler are not installed, so the cross-check tests skip and
    pass.
- **macOS failures that Linux CI hides:** `u4_dialogs_test` (4 tests) panics when a command opens
  an rfd file picker headless. This happens on untouched `main` too (TASKS Found 16).
- **`installers.yml`** builds Linux, Windows `.msi` and universal macOS `.dmg` packages on a tag,
  with SHA256SUMS. The macOS app is never launched in CI.
- **Hygiene:** `build1.log`, `testbuild.log` and a golden `batch.log` are tracked, and the local
  `target/` is 13 GB.

## 3. What ACP Studio has that AfroBIM does not

ACP Studio is a FastAPI and three.js build: hand-written tile maths, JSON stores, an SRTM terrain
blend, and site generators for the park. The table lists the features that carry over to a road
platform. Effort is for the AfroBIM port.

| ACP Studio feature | Where | AfroBIM today | Port |
|---|---|---|---|
| Satellite basemap: Esri, OSM, Google and Mapbox tiles with attribution, draped on the DTM | `viewer/src/basemap.js`, `game/sat.js` | **Built in plan, this branch** (draping in 3D still to do) | Done, then M for the 3D drape |
| Georeferenced orthophotos: KMZ GroundOverlay boxes, delivered PNG with `.pgw` and `.prj` that nothing read | `scripts/make_hd_tiles.py`, `data/overlay_box.json` | **Built (world file, GeoTIFF, `.prj`)**; KMZ to do | S for KMZ |
| HD ortho tiling with the nearest tile first; ortho patches | `make_hd_tiles.py`, `game/hdground.js`, `acp/orthopatch.py` | Decimated to one texture | M |
| Auto-DTM for a new project: SRTM via OpenTopoData, falling back to a plane | `acp/projects.py` | None | S |
| Design checks with red warnings: AASHTO and MoWT minimum radius, width, kerb return against the SU-9 vehicle | `acp/aashto.py`, `network.py` | Partly: sight distance, alignment checks | S |
| Road network conditioning and connectivity check | `acp/roadnet.py` | None | S |
| KMZ export for Google Earth | `acp/kmz_export.py` | None | S |
| glTF/GLB export with a shifted origin and split parts | `gltf_export.py`, `tools/glbpack` | None | M |
| Area-of-interest picker and geocoder (Nominatim through a throttled proxy) | `aoimap.js`, `/api/geocode` | None | S |
| Building footprints and trees from the orthophoto | `acp/buildings.py`, `acp/landscape.py` | None | M |
| Flythrough camera paths, sun position, section plane | `flyover.js`, `core/flightpath.js`, `main.js` | 4D playback frames only | M |
| Drone telemetry and 360 photo explorer | `_360_Viewer_Build/tools` | Photos stored as paths | M |
| Lane-graph traffic and routing | `traffic.js`, `game/route.js` | None | M |
| Already covered in AfroBIM | corridor, profile, junctions, earthworks, undo, lighting layout, schedules | Covered | None |

Not relevant: the aerodrome, terminal, stadium and sports generators, the games, and the APS
upload.

Findings on the ACP side:

- The live site resolves only under the misspelled domain (`africacoffepark.com`).
- The public explorer still shows a Google Maps screenshot that is labelled internal only.

## 4. Built on this branch: georeferenced images and the background map

**The `afrobim-raster` crate** (new; 25 tests):

- reads world files and GeoTIFF tags into one corner-based affine;
- matches ESRI `.prj` files by name (Arc 1960 / UTM 36S is found as EPSG:21036);
- decodes large orthophotos chunk by chunk, using internal overviews when present;
- provides the tile maths, the providers, and a tile service with a disk cache and a fetcher
  trait.

**`IMAGE` command and window** (`imagery.rs`, 5 tests):

- An image is stored by path and hash, with its own placement and system. It is placed in the
  project through the kernel's datum transformations.
- Placement by hand is available for an image that carries none.
- It is drawn as a textured grid that is split and clipped in f64 before the f32 cast.
- Drag and drop works for `.tif`, `.png` and `.jpg`.

**`BASEMAP` command** (`basemap.rs`, 7 tests):

- Providers: Esri World Imagery (the default), OSM, Esri Topo, OpenTopoMap, Mapbox (token) or a
  custom HTTPS template.
- The map is off until switched on and needs the project CRS.
- Each tile is placed as a grid through the plan from WGS 84. While a tile loads, its parent tile
  stands in.
- The attribution is drawn on the canvas. The key is kept for the session only.

**Checked on real data.** The tests above do not cover this; it was a screenshot run with the
release build.

- The ACP Park orthophoto (35 MB PNG with `.pgw` and an ESRI `.prj`) is placed in an Arc 1960 /
  UTM 36S project.
- The delivered road DXF overlays the roads in the image.
- With Esri World Imagery on, the tiles are moved through EPSG:1284 (stated accuracy 6 m) and
  continue the roads of the orthophoto with no visible offset at the view scale.
- All 20 tiles arrived in about 3 s.

**Follow-ups:**

- drape images and the map on the TIN in the 3D view;
- KMZ GroundOverlay import;
- two-point or N-point georeferencing with residuals (reusing `SiteCalibration`);
- WMS/WMTS;
- streaming of full-resolution orthophotos;
- a labels overlay (Esri reference layers).

### 4.1 Map workflow modelled on CADEarth and Spatial Manager (added 2026-11-24)

The owner asked for the map to work like the AutoCAD plugins CADEarth and Spatial Manager. The table lists their capabilities against AfroBIM. **MVP** marks what is built on this branch; the rest is planned.

| Capability | Status | Notes |
|---|---|---|
| Set the drawing's coordinate system, Uganda defaults | **MVP** | CRS command has one-click WGS 84 / UTM 35N, 35S, 36N, 36S and Arc 1960 / UTM 35N, 35S, 36N, 36S. Every preset is tested to resolve and to have a plan to WGS 84. |
| Live georeferenced background map that follows pan and zoom, on and off | **MVP** | BASEMAP. Satellite, Street and Terrain quick switch (Esri World Imagery, OpenStreetMap, OpenTopoMap), plus custom HTTPS templates and keyed providers. |
| Transparency and draw order | **MVP** | Opacity on the map and on each image; images can be sent back or brought forward. The online map always sits under every image. |
| Capture the map at a chosen resolution as a georeferenced raster | **MVP** | MAPCAPTURE: the current view, resampled into the project system at a chosen ground resolution, written as PNG + .pgw + .prj (+ attribution note) and attached as an image underlay. Capped at 400 tiles per capture (AfroBIM's own limit; whether each provider's terms allow saving tiles is an open question for the owner). |
| Export geometry to Google Earth KML | **MVP** | MAPEXPORT: selection or everything to KML (one folder per layer) or GeoJSON, reprojected to WGS 84. Arcs cut into chords within 20 mm; lettering left out. |
| Import GIS data (GeoJSON, KML, GPX, CSV lon/lat) with reprojection | Engine built, app import next | The readers are in `afrobim-raster::vector` with tests. Bringing the features in as a read-only overlay in the project system is the next step. |
| Import SHP (with DBF attributes and .prj) | Planned | Needs a shapefile reader; reproject through the .prj like images. |
| KMZ import and export | Planned | Needs a zip dependency; KMZ GroundOverlay import also gives image underlays from Google Earth. |
| Terrain: DEM to surface and contours | Planned | Read SRTM / Copernicus GLO-30 GeoTIFF heights (float TIFF, which the image decoder refuses on purpose), sample on a grid in the project system, build a TIN with the existing surface engine, contour from it. Vertical datum (EGM96 / EGM2008 against local levels) must be stated on the surface. |
| Point and attribute query on the map (lat/lon of a pick) | Partly there | The status bar already shows lat/lon and WGS 84 for the cursor. |
| Search a place by name | Planned | Needs a geocoding service and a decision on which provider's terms are acceptable. |

**Privacy:** a received document with the map switched on fetches tiles for its site when it opens, which tells the tile provider roughly where the project is. The map starts off in every new document; a future setting should ask before fetching tiles for a document that came from someone else.

### 4.2 Green BIM: what AfroBIM already gives, and what to build (added 2026-11-25)

Green BIM uses the model's own data to judge environmental impact before construction: energy, carbon and waste, alongside cost. Most of the data it needs is already in AfroBIM; the factors and the reports are not. The table checks each part of the idea against the code.

| Green BIM claim | AfroBIM today | Gap | Plan item |
|---|---|---|---|
| Spot construction problems before they happen (less rework) | Built: clash tests and coordination rules (`afrobim-coordination`), BCF issues in and out | None for roads; buildings are only reference models | Built |
| Fewer delays | Built: 4D programme (`afrobim-schedule`, `afrobim-progress`), MS Project and Primavera import | None | Built |
| Know the cost before building | Built: 5D bill, rates, estimate and cash flow (`afrobim-cost`); corridor quantities (`afrobim-corridor` quantities) | None | Built |
| Less material waste | Built: a waste allowance per quantity rule, as a percentage the user enters (`afrobim-green`); exact quantities and a cut and fill balance with mass haul (`afrobim-corridor`) | No report of surplus spoil or imported fill as separate figures | 9.15 (built) |
| Lower carbon footprint | Built: embodied carbon (A1-A3) and transport (A4) from the quantities, with the user's factor file (`afrobim-green`). No factor is shipped | Haul distance is typed per rule, not read from the mass-haul diagram; design options compare totals, not kgCO2e per km | 9.15 (built, with those exceptions) |
| Lower energy consumption | Built for street lighting: kWh and kgCO2e per year from the luminaire lines, with the user's burning hours, dimming and grid factor (`afrobim-green`). Not built for buildings | Building energy: sun path, solar gain, PV yield and a simulation export | 9.17 (built); 9.16 for buildings |
| Better material choices | Partly: pavement design compares layers by structural need (`afrobim-pavement`), not by carbon or cost together | Side-by-side options on cost, carbon and life | 9.15 |

**9.17 Street lighting energy and carbon** (new, size S): annual kWh and kgCO2e from the luminaire layout the lighting module already designs (wattage, burning hours, dimming profile), with the grid emission factor for Uganda stated and sourced. It is the quickest Green BIM result, because the layout exists today.

Order: 9.17 first (small, uses existing data), then 9.15 (carbon and waste from the quantities), then 9.16 (building energy). All three wait on the carbon factor source (open question 4).

## 5. Plan

The plan is ordered by risk first, then by value to a highway engineer in Uganda.

### Round 7: make it safe to rely on (hardening)

| # | Item | Sev | Effort |
|---|---|---|---|
| 7.1 | Atomic save (temporary file, fsync, rename) in the kernel and MCP; timed autosave to `<name>.afrobim.recover`; a panic hook that writes a recovery file; offer recovery on start | High | S |
| 7.2 | MCP: confine every path to a project root given at start-up; refuse symlinks on every write; a read-only mode | Med | S |
| 7.3 | ODA converter: a timeout, and a confirmation before converting a DWG named by a document; a per-user DWG cache with clean-up | Med | S |
| 7.4 | Parser limits: clamp declared counts to the input length (DXF proxy, point clouds); file-size limits on IFC and LandXML with a clear message; stream LandXML | Med | M |
| 7.5 | CI: clippy with `-D warnings` on new code, `cargo fmt --check`, `cargo deny`, a weekly macOS run, ezdxf, ifcopenshell, xmllint and poppler installed so the cross-checks run; actions pinned by SHA | Med | S |
| 7.6 | Make `u4_dialogs_test` pass headless: put file pickers behind a trait with a test stub | Med | S |
| 7.7 | Per-category data versions with migrations on load; bump `FORMAT_VERSION` | Med | M |
| 7.8 | Release hygiene: untrack the logs, `bundle_fonts` off by default, choose a licence, Developer ID signing and notarisation, Authenticode | Med | M |

### Round 8: structure (so the next rounds stay cheap)

| # | Item | Effort |
|---|---|---|
| 8.1 | Split `afrobim-app` into a library and a binary. Rename the round-named files by feature. Move the integration tests to `tests/` | M |
| 8.2 | One TIN engine (retire `corridor/tin.rs` for `afrobim-terrain`), one LandXML crate, one point-file reader, one CRS module | L |
| 8.3 | Diff-based undo; heavy data (TIN, clouds) kept out of the snapshots | M |
| 8.4 | Cancellable background jobs (a cancel token threaded into the engines); revision-counter fingerprints | M |
| 8.5 | Expose the civil engines (alignment, corridor, terrain, quantities) to the Python package and MCP, once 8.1 gives them a library to call | M |

### Round 9: new features for road work (ranked)

| # | Feature | Why it matters | Source | Effort |
|---|---|---|---|---|
| 9.1 | **Imagery, part 2**: drape images and the map on the ground surface in 3D; KMZ GroundOverlay import; N-point georeferencing with residuals; WMS/WMTS (UBOS, NFA and NEMA services) | Design over the real ground in 3D, and use of what clients deliver | ACP basemap drape and KMZ boxes | M |
| 9.2 | **Design-check report by link**: minimum radius against design speed, carriageway and shoulder widths, superelevation, kerb returns against the design vehicle, sight distance. Red, amber and green with the clause cited (MoWT Road Design Manual, AASHTO) | Reviews with UNRA and consultants are run against these | ACP `aashto.py`, `network.py` | S to M |
| 9.3 | **Swept-path analysis**: design vehicles (SU-9, bus, WB-15, Ugandan 3-axle and 7-axle trucks), drive along an alignment or a junction, envelope against kerbs and other objects | Junction and roundabout sign-off; no tool in the platform does it | New | M |
| 9.4 | **Early-stage terrain**: SRTM or Copernicus GLO-30 to a draft surface for an area of interest, labelled "not a survey"; blend into a survey when one arrives | Feasibility and route options before survey money is spent | ACP `projects.py`, `terrain.py` | S |
| 9.5 | **GIS exchange**: GeoJSON and shapefile in and out, and KMZ out (alignments, corridors, road reserve, parcels, chainage markers) for Google Earth and QGIS | Clients, RAP teams and supervisors live in Google Earth and QGIS | ACP `kmz_export.py` | S to M |
| 9.6 | **Land acquisition from imagery**: building footprints and trees from the orthophoto inside the road reserve, into the acquisition schedule (PAPs, structures, trees by species class) and 5D clearance quantities | RAP and compensation drive programme risk on Ugandan roads | ACP `buildings.py`, `landscape.py`; AfroBIM `cogo` acquisition schedule | M |
| 9.7 | **Area-of-interest start**: new project from a box drawn on the map, with a geocoder, the CRS proposed from the location (UTM 35/36/37, Arc 1960 or WGS 84) and the draft terrain from 9.4 | A new job is placed correctly in minutes | ACP `aoimap.js` | S |
| 9.8 | **glTF/GLB export** with a shifted origin and parts by discipline, for web viewers, Blender and Unreal | Client presentations; reuse of the venture-video pipeline | ACP `gltf_export.py` | M |
| 9.9 | **Network conditioning**: snap ends, find gaps and dead ends, report connectivity for a set of alignments | Clean networks before corridor and junction work | ACP `roadnet.py` | S |
| 9.10 | **Site evidence in Construction**: geotagged and 360 photos and drone telemetry placed on the map and tied to chainage; inspection and non-conformance records (finishing S2) | Supervision reports and IPC evidence | ACP 360 explorer | M |
| 9.11 | **Flythrough and sun study**: keyframed camera paths over the 3D model, rendered to frames (as 4D playback already does), and sun position by date | Stakeholder engagement | ACP `flyover.js` | M |
| 9.12 | **PDF underlay** (vector where possible) and **DWG export** through the ODA converter | Drawings arrive as PDF; deliverables are asked for in DWG | New | M |
| 9.13 | **Shared work**: file locks for data shortcuts, then a sync folder with conflict detection | More than one designer per project | New | L |
| 9.14 | Lane-graph traffic animation for junction presentations | Presentation only; lowest priority | ACP `traffic.js` | M |
| 9.15 | **Built, with exceptions** (haul is typed per rule, not read from the mass-haul diagram; no per-km view). **Green BIM, part 1: embodied carbon and waste from the quantities.** Carbon factors per material (cement and concrete by class, bitumen and asphalt, steel, aggregates, timber) applied to the BoQ the 5D module already produces; earthworks haul emissions from the mass-haul diagram; a waste allowance per item. Compare design options side by side (flexible against rigid pavement, gabion against concrete wall, cut and fill balance) in kgCO2e per km and per item, with every factor's source and edition shown | Clients and funders (World Bank, AfDB, EU) increasingly ask for a carbon figure at design stage; the cheapest saving is the one chosen before construction | New; builds on 5D quantities and mass haul | M |
| 9.17 | **Built** (the grid factor is typed by the user; none is shipped). **Green BIM: street lighting energy and carbon**: annual kWh and kgCO2e from the luminaire layout already designed, with a sourced grid emission factor for Uganda | The quickest Green BIM result; the layout exists today | New; builds on `afrobim-lighting` | S |
| 9.16 | **Green BIM, part 2: operational energy and daylight for buildings** (ACP park buildings, toll plazas, weighbridges, offices): sun path from the project location, solar gain on facades, roof area for solar PV yield, a simple energy estimate; export to gbXML or IFC for a full simulation tool | Lower running costs and a smaller footprint over the building's life | ACP buildings; sun position from 9.11 | M to L |

### Suggested order

Rounds 7.1 to 7.3 and 7.5 first: about a week, and they remove the ways of losing work. Then 9.1
to 9.5, which build on the imagery now in place, while 8.1 runs alongside because every later
feature is cheaper after it.

Green BIM part 1 (9.15) follows straight after 9.2, because it reuses the quantities and the
design-option comparison already in place and needs no new geometry. Its factors must come from a
named, dated source that the owner chooses (open question 4).

## 6. Open questions for the owner

1. Is the GitHub repo private, and will a build or the repo be shared? This decides how urgent the
   fonts and manuals purge and the licence choice are.
2. Which tile providers are acceptable commercially? Esri World Imagery is free for
   non-commercial and light use. For a commercial product, an ArcGIS, Mapbox or MapTiler account
   (the custom URL field supports it) is the safer route.
3. Which design vehicles and which manual editions should 9.2 and 9.3 cite first (MoWT Road
   Design Manual 2010, AASHTO 2018)?
4. Which carbon factor source should Green BIM use: the ICE database (Inventory of Carbon and
   Energy, Circular Ecology), manufacturers' EPDs where a Ugandan supplier publishes one, or a
   client's own figures? And at which boundary (cradle to gate A1 to A3, or including transport
   A4 and construction A5)?
