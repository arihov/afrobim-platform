# AfroBIM for Python

The AfroBIM sign engine, DXF and SVG export and `.afrobim` documents, from a script or a
notebook. It is the same Rust code the desktop app runs, so a sign laid out here has the same
dimensions, the same DXF and the same file format.

```python
import afrobim

engine = afrobim.SignEngine()
sign = afrobim.stack_sign("ADS-001", 60, [
    afrobim.destination("Kasese", "ahead"),
    afrobim.destination("Masheruka", "right"),
])
layout = engine.layout(sign)
print(f"{layout.width_mm:.0f} x {layout.height_mm:.0f} mm")   # 2112 x 1125 mm
```

## Install from source

You need Python 3.9 or later and Rust 1.85 or later ([rustup.rs](https://rustup.rs)). The
package is built with [maturin](https://www.maturin.rs); the wheel is `abi3`, so one build
serves every Python version from 3.9 on.

macOS and Linux:

```sh
cd afrobim/python
python3 -m venv .venv
source .venv/bin/activate
pip install maturin
maturin develop --release        # builds and installs into the active environment
```

Windows (PowerShell; Rust needs the MSVC build tools, which `rustup` offers to install):

```powershell
cd afrobim\python
py -m venv .venv
.venv\Scripts\Activate.ps1
pip install maturin
maturin develop --release
```

`maturin develop` installs in place: edits to the Python files take effect at once, and you run
it again after changing Rust. To make a wheel to install elsewhere or hand to a colleague on the
same operating system:

```sh
maturin build --release --out dist
pip install dist/afrobim-0.1.0-*.whl
```

On macOS add `--target universal2-apple-darwin` for one wheel that runs on both Intel and Apple
silicon (after `rustup target add x86_64-apple-darwin aarch64-apple-darwin`).

### The rule pack and the fonts

The engine needs a rule pack (`packs/UK-TSM-2019.json`) and the Transport fonts it names
(`fonts/`). **Neither is inside the wheel.** The Transport fonts are third-party files and are
not part of AfroBIM: do not redistribute them without a licence that allows it.

`afrobim.SignEngine()` finds them the way the desktop app does:

1. `AFROBIM_PACK` (the JSON file) and `AFROBIM_FONTS` (the folder), when set;
2. otherwise `packs/UK-TSM-2019.json` and `fonts/` in the working directory or any folder above
   it, so a script run anywhere inside a checkout just works.

Or name them: `afrobim.SignEngine(pack="D:/standards/UK-TSM-2019.json", fonts="D:/fonts")`.

## Quick start

```python
import afrobim

# 1. Load the engine (UK Traffic Signs Manual, Chapter 7).
engine = afrobim.SignEngine()
print(engine.pack_id, "-", engine.pack_name)

# 2. Build a sign. A definition is a plain dict: print it, edit it, store it.
sign = afrobim.stack_sign("ADS-001", 60, [
    afrobim.destination("Kasese", "ahead"),
    afrobim.destination("Masheruka", "right", distance=12, route_number="A109", route="primary"),
])

# 3. Lay it out and read its size.
layout = engine.layout(sign)
print(f"{layout.width_mm:.0f} x {layout.height_mm:.0f} mm, x-height {layout.x_height_mm:g} mm, "
      f"{layout.area_m2:.2f} m2")
for d in layout.diagnostics:
    print(d["severity"], d["code"], d["message"])

# 4. Export.
layout.to_dxf("ADS-001.dxf")     # DXF R12 in millimetres, for CAD and fabrication
layout.to_svg("ADS-001.svg")     # a picture for reports

# 5. A sign from the pack's catalog of standard signs.
for entry in engine.catalog()[:5]:
    print(entry["key"], entry["name"])
direction = engine.definition("UK06.1", speed_kph=80)

# 6. Build a document the desktop app opens.
doc = afrobim.Document()
first = doc.add_sign(engine, sign)                        # placed automatically
second = doc.add_sign(engine, direction, x=4000, y=0)     # or where you say, in mm
doc.set_property(first, "Material", "Class RA2")
doc.move(second, 0, -1500)
doc.save("scheme.afrobim")
doc.export_dxf(engine, "scheme.dxf")                      # every sign at its position

# Later, or after editing in the app:
doc = afrobim.Document.load("scheme.afrobim")
for e in doc.elements:
    print(e["name"], e["data"]["kind"], f'{e["width"]:.0f} x {e["height"]:.0f} mm', e["properties"])
```

In a Jupyter notebook a `Layout` displays itself as the sign's picture.

A schedule of signs from a spreadsheet is a loop:

```python
import csv

doc = afrobim.Document()
with open("schedule.csv", newline="") as f:           # columns: mark, speed, ahead, left, right
    for row in csv.DictReader(f):
        dests = [afrobim.destination(row[d], d) for d in ("ahead", "left", "right") if row[d]]
        doc.add_sign(engine, afrobim.stack_sign(row["mark"], int(row["speed"]), dests))
doc.save("schedule.afrobim")
```

## API in brief

Lengths are millimetres unless a name says `sw` (stroke widths, the unit the standard's layout
rules are written in). Model space has Y up.

### Sign definitions

A definition is a `dict` in the engine's JSON shape: `code`, `kind`, `speed_kph`,
`approach_class`, `destinations`, `background`, `class`, `scheme`, `variant`, `symbol_id`,
`catalog_key` and a few more. It is exactly what a `.afrobim` file stores in an element's `data`.

| | |
|---|---|
| `afrobim.destination(name, direction="Ahead", distance=None, route_number=None, route=None, *, order=0, symbol_id=None, number_status=None)` | One destination. `direction`: ahead, left, right. `route`: motorway, primary, non-primary. |
| `afrobim.stack_sign(code, speed_kph, destinations=[])` | A stack-type advance direction sign with the app's defaults. |
| `afrobim.sign(kind, code="", speed_kph=60, destinations=[], *, background="Green", **fields)` | Any kind in `afrobim.SIGN_KINDS`; `fields` sets anything else. |
| `afrobim.normalise(definition)` | The definition with every field filled in; raises `SignError` if it is not valid. |
| `afrobim.SIGN_KINDS`, `DIRECTIONS`, `ROUTE_STATUSES`, `COLOURS` | The names the engine accepts. |

Names are forgiving about letter case and hyphens (`"non-primary"`, `"direction-sign"`).

### `afrobim.SignEngine(pack=None, fonts=None)`

| | |
|---|---|
| `pack_id`, `pack_name`, `pack_path`, `fonts_path` | What was loaded. |
| `catalog()` | The pack's standard signs: a list of dicts with `key`, `name`, `kind`, `class`, `diagram`, `sizes`... |
| `definition(key, speed_kph=None)` | The definition a catalog entry seeds. `KeyError` if there is no such key. |
| `layout(definition)` | Lays the sign out: returns a `Layout`. |
| `sign_size_mm(definition)`, `size_bands_kph()` | The tabled size of a warning or regulatory sign, and the speed bands. |

`afrobim.find_pack()` and `afrobim.find_fonts()` return what `SignEngine()` would load.

### `Layout`

| | |
|---|---|
| `width_mm`, `height_mm`, `size_mm`, `area_m2` | Overall size. |
| `width_sw`, `height_sw`, `sw_mm`, `x_height_mm` | Size in stroke widths; one stroke width and the x-height in mm. |
| `diagnostics`, `has_errors` | The engine's findings: dicts with `severity`, `code`, `message`, `source` (the clause). |
| `primitives(units="mm")` | The geometry, `"mm"` or `"sw"`: dicts with `type` `Fill`, `Glyphs`, `Line`, `Arc` or `Symbol`; points are `[x, y]`. |
| `to_dxf(path=None)` | DXF R12 text in mm; written to `path` when given. Byte for byte the desktop app's export. |
| `to_svg(path=None)` | SVG text in mm; written to `path` when given. |

`afrobim.primitives_to_svg(primitives, width, height, path=None)` draws primitives you have
assembled or edited yourself. `afrobim.display_rgb("Green")` gives the screen colour used.

### `afrobim.Document`

Signs are stored as the desktop app stores them (category `sign`, layer `Signs`, the element's
`name` is the mark, its `data` the definition), so files move freely between the two. Elements
are identified by the integer id `add_sign` returns.

| | |
|---|---|
| `Document()`, `Document.load(path)`, `Document.from_json(text)` | New, or read. |
| `save(path)`, `to_json()` | Write a `.afrobim` file. |
| `add_sign(engine, definition, x=None, y=None, mark=None)` | Sizes the sign and adds it; returns its id. No position: automatic arrangement. The mark is made unique. |
| `update_sign(engine, id, definition)`, `rename(id, name)`, `unique_name(name)` | Change a sign. |
| `elements`, `element(id)`, `len(doc)`, `layers`, `extents`, `standard` | Read the document. Elements are dicts in the file's shape. |
| `remove(id)`, `move(id, dx, dy)`, `move_to(id, x, y)`, `arrange()` | Edit. |
| `properties(id)`, `set_property(id, key, value)`, `set_properties(id, {...})` | The free-form property set; `None` removes a key. |
| `layout(engine, id)` | The `Layout` of a stored sign. |
| `export_dxf(engine, path=None)` | Every sign at its position, in mm, as one DXF. |

### Errors

| | |
|---|---|
| `afrobim.SignError` (a `ValueError`) | The engine's own message: an invalid definition, a character the font lacks, an unreadable pack or font. |
| `afrobim.UnsupportedError` (a `SignError`) | The sign needs something the engine does not lay out yet. |
| `afrobim.DocumentError` (a `ValueError`) | Not an AfroBIM document, or a newer version. |
| `FileNotFoundError` | No pack or fonts folder was given and none was found. |
| `KeyError` | No such catalog key or element id. |

## Tests

```sh
pip install pytest
pytest
```

The tests use the pack and fonts of the checkout. They check the seed sign against the known
result of the C# engine (2112 x 1125 mm), the DXF against the reference writer's file byte for
byte, every catalog entry, and a saved document against one written by the Rust kernel itself
(`tests/kernel_reference`, a small Rust program built on the fly; that test is skipped when
`cargo` is not on the path).

## How it is put together

`src/` is a PyO3 extension crate (`afrobim._native`) over `afrobim-signs`, `afrobim-dxf` and
`afrobim-kernel`; `python/afrobim/` is the typed Python package users import. This crate is
deliberately not a member of the Cargo workspace in `../Cargo.toml`, so the rest of the platform
builds on a machine without Python.
