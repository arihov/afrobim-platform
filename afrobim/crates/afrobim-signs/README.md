# afrobim-signs

The traffic-sign engine of AfroBIM: a Rust port of the SignCore C# engine. It reads a rule pack and
the Transport fonts, lays a sign out in stroke widths (sw) and returns a `Drawing` (see
`afrobim-draw`), with the lettering already converted to glyph outlines. The C# engine remains the
reference: the Rust engine is checked against its output primitive by primitive.

```rust
let engine = SignEngine::load(Path::new("packs/UK-TSM-2019.json"), Path::new("fonts"))?;
let sign = SignDefinition::stack("ADS-001", 60, vec![Destination::new("Kasese", Direction::Ahead)]);
let layout = engine.layout(&sign)?;                       // SignLayout: drawing, sw_mm, x_height_mm, diagnostics
let in_mm = layout.drawing.scaled_to_origin(layout.sw_mm);
```

## Main types and modules

| Item | What it does |
|---|---|
| `SignEngine` (`lib.rs`) | `load(pack, fonts_dir)` reads a rule pack (a JSON file such as `packs/UK-TSM-2019.json`, or a folder pack such as `packs/UG-TSM-2004`, whose `pack.xml` manifest lists XML files: `xml_pack.rs`) and the two Transport fonts the pack names; `layout(&SignDefinition)` returns a `SignLayout`; `catalog()`, `catalog_entry`, `definition_for`, `definition_for_speed` and `sign_size_mm` serve the standard-sign catalogue (`std_catalog.rs`) |
| `SignDefinition`, `SignKind`, `Destination`, `Direction`, `EmbeddedSign`, ... (`model.rs`) | The input. Kinds: stack-type, the three map-type signs (crossroads, roundabout, T-junction), direction, confirmatory, place identification, lane pre-selection, diversion, information, warning, regulatory prohibitory and mandatory, stop, give way, supplementary plate |
| `SignLayout` | The result: the drawing in sw, `sw_mm`, `x_height_mm`, and `diagnostics` (`Severity`), with `has_errors()` |
| `SignError` | `Pack`, `Font`, `MissingGlyph` (a legend with a character the font lacks is an error, never a zero-width default) and `Unsupported` |
| `pack`, `std_pack`, `ug_pack`, `xml_pack`, `dir_rules`, `map_rules`, `embedded` | The rule pack and its sections (direction and map rules, standard-sign data, Uganda-only sections such as standard sizes, roundels and triangles, embedded-sign rules) |
| `font`, `measurer`, `outliner`, `pictogram_library` | Font metrics and glyph outlines, text measurement, the drawn pictograms of destination symbols |
| `stack`, `direction`, `confirmatory`, `place`, `lane`, `diversion`, `map_plate`, `map_symbol`, `std_signs`, `std_plate`, `std_arrow`, `std_info`, `std_tsrgd`, `ug_engine`, `ug_guidance`, `ug_signs` | The layouts of each kind; the `ug_*` modules hold those only the Uganda pack takes |

## How it is tested

`tests/golden.rs` runs every fixture in `tests/golden/` (446 JSON files at the time of writing:
Uganda 242, standard signs 95, map-type 34, embedded 33, direction 25, and a few others). Each
was written by the C# `GoldenDump` tool from a sign definition laid out with the same pack and
fonts, and the Rust engine must reproduce it: sizes, extents, diagnostics, and primitives in
order with the same kinds, colours and text, every coordinate and width within 1e-6 sw and every
glyph contour point within 1e-3 sw. The other files in `tests/` (`engine`, `engine_dir`,
`map_engine`, `std_signs`, `ug_pack`, `embedded`) test the layouts, the pack loaders and the
refusals directly, and `src/map_symbol_tests.rs` tests the map symbols. The tests that read the
pack and the fonts need `packs/` and `fonts/` (a source checkout has them; the Transport fonts are
third-party files and are not part of AfroBIM).

## Limits

- Embedded warning or regulatory signs on guidance signs are `SignError::Unsupported` under the
  Uganda pack. Warning triangles and regulatory roundels embedded on UK direction and map-type
  signs are drawn as the standard frame only, as the C# engine draws them: the inner symbol is
  not drawn.
- A sign kind the engine does not lay out returns `Unsupported`; it is never approximated.
- The engine implements the rules in the pack. It does not decide whether a sign is warranted
  or correct for a site; that is the engineer's.
- The golden fixtures are only as good as the C# engine that wrote them; there is no independent
  check against a published sign drawing here.
