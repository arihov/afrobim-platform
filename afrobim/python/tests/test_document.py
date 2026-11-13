"""Documents: building, editing, the file format the desktop app reads, and DXF export."""

import json
import os
import shutil
import subprocess
from pathlib import Path

import pytest

import afrobim
from conftest import FONTS, PACK

HERE = Path(__file__).resolve().parent

ELEMENT_FIELDS = {"id", "category", "name", "origin", "width", "height", "layer", "data", "properties"}


def build(engine, seed) -> afrobim.Document:
    """Three signs, as `tests/kernel_reference` builds them with the Rust kernel."""
    doc = afrobim.Document()
    blank = dict(seed, code="")  # a sign fresh from the app's palette has no code of its own
    doc.add_sign(engine, blank, mark="ADS-001")
    doc.add_sign(engine, blank, mark="ADS-001")
    third = doc.add_sign(engine, engine.definition("UK06.1", speed_kph=80), x=5000, y=-3000)
    doc.set_properties(third, {"Material": "Class RA2", "Drawing": "TS-101"})
    return doc


def test_add_sign_files_the_sign_as_the_desktop_app_does(engine, seed):
    doc = afrobim.Document()
    assert len(doc) == 0 and doc.elements == [] and doc.extents is None and doc.standard == ""

    first = doc.add_sign(engine, seed)
    assert first == 1 and len(doc) == 1
    e = doc.element(first)
    assert set(e) == ELEMENT_FIELDS
    assert (e["category"], e["layer"], e["name"]) == ("sign", "Signs", "ADS-001")
    assert e["origin"] == {"x": 0.0, "y": 0.0}
    # Sized by laying the sign out: the known 2112 x 1125 mm.
    layout = engine.layout(seed)
    assert (e["width"], e["height"]) == (layout.width_mm, layout.height_mm)
    assert (round(e["width"]), round(e["height"])) == (2112, 1125)
    # The definition is stored whole, so the app can edit it.
    assert e["data"] == seed
    assert e["properties"] == {}
    assert doc.standard == "UK-TSM-2019"
    assert [l["name"] for l in doc.layers] == ["0", "Signs"]
    assert doc.elements == [e] and list(doc) == [e]


def test_marks_are_made_unique_and_signs_are_placed_in_a_row(engine, seed):
    doc = build(engine, seed)
    a, b, c = doc.elements
    assert [a["name"], b["name"], c["name"]] == ["ADS-001", "ADS-002", "UK06.1"]
    assert [a["id"], b["id"], c["id"]] == [1, 2, 3]
    # The second sign goes 500 mm to the right of the first; the third where it was put.
    assert b["origin"] == {"x": a["width"] + 500.0, "y": 0.0}
    assert c["origin"] == {"x": 5000.0, "y": -3000.0}
    assert doc.unique_name("ADS-001") == "ADS-003"
    # With no mark and no code, a sign takes the mark the app gives its kind.
    lane = afrobim.sign("LanePreselection", "", 80, [afrobim.destination("Left", "left"), afrobim.destination("Ahead", order=1)])
    assert doc.element(doc.add_sign(engine, lane))["name"] == "LP-001"
    with pytest.raises(ValueError, match="both x and y"):
        doc.add_sign(engine, seed, x=100)


def test_save_and_load_round_trip(engine, seed, tmp_path):
    doc = build(engine, seed)
    path = tmp_path / f"scheme.{afrobim.FILE_EXTENSION}"
    doc.save(path)

    back = afrobim.Document.load(path)
    assert back.elements == doc.elements
    assert back.layers == doc.layers
    assert back.standard == doc.standard == "UK-TSM-2019"
    assert back.to_json() == doc.to_json() == path.read_text(encoding="utf-8")
    assert afrobim.Document.from_json(doc.to_json()).elements == doc.elements
    # Ids continue after the highest stored one.
    assert back.add_sign(engine, seed) == 4
    # Every stored sign still lays out to its stored size.
    for e in doc.elements:
        layout = back.layout(engine, e["id"])
        assert (layout.width_mm, layout.height_mm) == (e["width"], e["height"])


def test_the_file_has_the_fields_the_desktop_app_reads(engine, seed, tmp_path):
    doc = build(engine, seed)
    path = tmp_path / "scheme.afrobim"
    doc.save(path)
    file = json.loads(path.read_text(encoding="utf-8"))

    assert set(file) == {"format", "version", "units", "standard", "layers", "elements"}
    assert (file["format"], file["version"], file["units"], file["standard"]) == ("afrobim", 1, "mm", "UK-TSM-2019")
    assert file["layers"] == [
        {"name": "0", "visible": True, "locked": False},
        {"name": "Signs", "visible": True, "locked": False},
    ]
    assert len(file["elements"]) == 3
    for e in file["elements"]:
        assert set(e) == ELEMENT_FIELDS
        assert (e["category"], e["layer"]) == ("sign", "Signs")
        assert set(e["origin"]) == {"x", "y"}
        # `data` is a complete sign definition in the engine's JSON shape.
        assert afrobim.normalise(e["data"]) == e["data"]
    assert file["elements"][2]["properties"] == {"Drawing": "TS-101", "Material": "Class RA2"}


def test_the_file_is_the_one_the_rust_kernel_writes(engine, seed, tmp_path):
    """`tests/kernel_reference` builds the same document with `afrobim_kernel::Document`
    directly, following the desktop app's glue. The two files must be identical."""
    cargo = shutil.which("cargo")
    if cargo is None:
        pytest.skip("cargo is not on PATH")
    reference = tmp_path / "reference.afrobim"
    env = dict(os.environ)
    # Share the extension's build folder so the engine is not compiled a second time from scratch.
    env.setdefault("CARGO_TARGET_DIR", str(HERE.parent / "target"))
    subprocess.run(
        [cargo, "run", "--quiet", "--locked", "--manifest-path", str(HERE / "kernel_reference" / "Cargo.toml"), "--", str(PACK), str(FONTS), str(reference)],
        check=True,
        env=env,
    )
    ours = tmp_path / "ours.afrobim"
    build(engine, seed).save(ours)
    assert ours.read_bytes() == reference.read_bytes()


def test_move_remove_and_arrange(engine, seed):
    doc = build(engine, seed)
    doc.move(1, 100, -50)
    doc.move(1, 25.5, 0)
    assert doc.element(1)["origin"] == {"x": 125.5, "y": -50.0}
    doc.move_to(1, 0, 0)
    assert doc.element(1)["origin"] == {"x": 0.0, "y": 0.0}

    doc.arrange()
    a, b, c = doc.elements
    assert a["origin"] == {"x": 0.0, "y": 0.0}
    assert b["origin"] == {"x": a["width"] + 500.0, "y": 0.0}
    assert c["origin"] == {"x": b["origin"]["x"] + b["width"] + 500.0, "y": 0.0}
    assert doc.extents == (0.0, 0.0, c["origin"]["x"] + c["width"], max(a["height"], c["height"]))

    assert doc.remove(2) is True
    assert doc.remove(2) is False
    assert [e["id"] for e in doc.elements] == [1, 3]
    for call in (lambda: doc.move(2, 1, 1), lambda: doc.element(2), lambda: doc.properties(2), lambda: doc.set_property(2, "a", "b")):
        with pytest.raises(KeyError, match="no element with id 2"):
            call()


def test_properties(engine, seed):
    doc = afrobim.Document()
    id = doc.add_sign(engine, seed)
    assert doc.properties(id) == {}
    doc.set_property(id, "Material", "Class RA2")
    doc.set_property(id, "Posts", 2)  # stored as text, as the app stores every property
    assert doc.properties(id) == {"Material": "Class RA2", "Posts": "2"}
    doc.set_properties(id, {"Material": "Class RA3", "Posts": None, "Cost code": "1200.01"})
    assert doc.properties(id) == {"Cost code": "1200.01", "Material": "Class RA3"}
    assert doc.element(id)["properties"] == doc.properties(id)


def test_update_and_rename(engine, seed):
    doc = afrobim.Document()
    id = doc.add_sign(engine, seed, x=1000, y=2000)
    faster = dict(seed, speed_kph=100)
    doc.update_sign(engine, id, faster)
    e = doc.element(id)
    assert e["data"]["speed_kph"] == 100
    assert e["origin"] == {"x": 1000.0, "y": 2000.0}
    assert (e["width"], e["height"]) == engine.layout(faster).size_mm
    assert e["width"] > engine.layout(seed).width_mm
    doc.rename(id, "ADS-A12")
    assert doc.element(id)["name"] == "ADS-A12"


def test_a_sign_that_cannot_be_laid_out_is_not_added(engine, seed):
    doc = afrobim.Document()
    with pytest.raises(afrobim.UnsupportedError):
        doc.add_sign(engine, dict(seed, kind="WarningSign"))
    with pytest.raises(afrobim.SignError):
        doc.add_sign(engine, {"kind": "StackType"})
    assert len(doc) == 0 and doc.standard == ""


def test_export_dxf_writes_every_sign_at_its_position(engine, seed, tmp_path):
    doc = afrobim.Document()
    assert "POLYLINE" not in doc.export_dxf(engine)

    doc.add_sign(engine, seed)
    alone = engine.layout(seed).to_dxf()
    # One sign on the origin is the sign's own DXF.
    assert doc.export_dxf(engine) == alone

    other = engine.definition("UK06.1")
    second = doc.add_sign(engine, other, x=3000, y=-2000)
    out = tmp_path / "scheme.dxf"
    text = doc.export_dxf(engine, out)
    assert out.read_bytes() == text.encode("utf-8")
    assert text.count("POLYLINE") == alone.count("POLYLINE") + doc.layout(engine, second).to_dxf().count("POLYLINE")

    # Coordinates are millimetres in model space: together the two signs fill exactly the
    # document's extents (corners may be rounded, so compare bounds, not vertices).
    lines = text.split("\n")
    points = [(float(lines[i + 1]), float(lines[i + 3])) for i in range(len(lines) - 3) if lines[i].strip() == "10" and lines[i + 2].strip() == "20"]
    bounds = (min(x for x, _ in points), min(y for _, y in points), max(x for x, _ in points), max(y for _, y in points))
    e = doc.element(second)
    assert doc.extents == (0.0, -2000.0, 3000.0 + e["width"], engine.layout(seed).height_mm)
    assert bounds == pytest.approx(doc.extents, abs=1e-3)
    # The second sign's own geometry starts at its origin.
    assert min(x for x, y in points if y < 0) == pytest.approx(3000.0, abs=1e-3)

    doc.move(second, 0, 5000)
    assert doc.export_dxf(engine) != text


def test_bad_files(tmp_path):
    with pytest.raises(afrobim.DocumentError, match="not a valid AfroBIM document"):
        afrobim.Document.from_json("{}")
    newer = '{"format":"afrobim","version":99,"units":"mm","layers":[],"elements":[]}'
    with pytest.raises(afrobim.DocumentError, match="newer"):
        afrobim.Document.from_json(newer)
    bad = tmp_path / "bad.afrobim"
    bad.write_text("not json", encoding="utf-8")
    with pytest.raises(afrobim.DocumentError):
        afrobim.Document.load(bad)
    with pytest.raises(OSError):
        afrobim.Document.load(tmp_path / "missing.afrobim")
