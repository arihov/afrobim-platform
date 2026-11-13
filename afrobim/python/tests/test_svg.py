"""The SVG writer is new (there is no reference file), so it is checked structurally: the right
elements in the right order, the right fill rules and colours, and geometry that lands where
the primitives say after the flip to SVG's Y-down axis."""

import re
import xml.etree.ElementTree as ET

import pytest

import afrobim

SVG = "{http://www.w3.org/2000/svg}"


def parse(text: str) -> ET.Element:
    root = ET.fromstring(text)
    assert root.tag == SVG + "svg"
    return root


def rings(d: str):
    """The closed subpaths of a path's `d` as lists of (x, y)."""
    out = []
    for sub in re.findall(r"M([^Z]*)Z", d):
        out.append([tuple(float(v) for v in pair.split(",")) for pair in re.findall(r"[-\d.]+,[-\d.]+", sub)])
    return out


def test_the_root_carries_the_size_in_millimetres(engine, seed):
    layout = engine.layout(seed)
    root = parse(layout.to_svg())
    assert root.get("width") == "2111.875mm" and root.get("height") == "1125mm"
    assert root.get("viewBox") == "0 0 2111.875 1125"
    assert root.get("data-units") == "mm"
    assert float(root.get("data-sw-mm")) == layout.sw_mm
    assert float(root.get("data-x-height-mm")) == layout.x_height_mm


def test_one_element_per_primitive_in_drawing_order(engine, seed):
    layout = engine.layout(seed)
    prims = layout.primitives()
    elements = list(parse(layout.to_svg()))
    assert [e.get("class") for e in elements] == [p["type"].lower() for p in prims]
    assert all(e.tag == SVG + "path" for e in elements)

    for element, prim in zip(elements, prims):
        if prim["type"] == "Fill":
            assert element.get("fill") == afrobim.display_rgb(prim["colour"])
            assert element.get("fill-rule") is None
            assert len(rings(element.get("d"))) == 1
        else:
            # Lettering: every contour is a subpath, filled even-odd so counters stay open.
            assert element.get("fill-rule") == "evenodd"
            assert element.get("fill") == afrobim.display_rgb(prim["ink"] or "White")
            assert len(rings(element.get("d"))) == len(prim["contours"])
            assert element.find(SVG + "title").text == prim["text"]


def test_geometry_is_flipped_to_y_down_and_stays_on_the_canvas(engine, seed):
    layout = engine.layout(seed)
    prims = layout.primitives()
    elements = list(parse(layout.to_svg()))
    w, h = layout.size_mm

    # The face covers the canvas exactly.
    face = rings(elements[0].get("d"))[0]
    assert (min(x for x, _ in face), max(x for x, _ in face)) == pytest.approx((0, w))
    assert (min(y for _, y in face), max(y for _, y in face)) == pytest.approx((0, h))

    for element, prim in zip(elements, prims):
        outlines = [prim["outline"]] if prim["type"] == "Fill" else prim["contours"]
        for ring, outline in zip(rings(element.get("d")), outlines):
            assert len(ring) == len(outline)
            for (sx, sy), (x, y) in zip(ring, outline):
                assert sx == pytest.approx(x, abs=1e-3)
                assert sy == pytest.approx(h - y, abs=1e-3)
                assert -1e-3 <= sx <= w + 1e-3 and -1e-3 <= sy <= h + 1e-3


def test_to_svg_writes_the_file_it_returns(engine, seed, tmp_path):
    layout = engine.layout(seed)
    out = tmp_path / "sign.svg"
    text = layout.to_svg(out)
    assert out.read_text(encoding="utf-8") == text
    assert layout._repr_svg_() == text  # what a notebook shows


def test_every_catalog_sign_gives_well_formed_svg(engine):
    for entry in engine.catalog():
        layout = engine.layout(engine.definition(entry["key"]))
        root = parse(layout.to_svg())
        assert len(list(root)) == len(layout.primitives()), entry["key"]


def test_lines_arcs_and_symbols():
    """Signs of the UK pack are all fills and lettering today; the other primitives are drawn
    through `primitives_to_svg`, which uses the same writer."""
    prims = [
        {"type": "Line", "from": [10, 20], "to": [90, 20], "width": 4, "colour": "Red"},
        {"type": "Line", "from": [10, 30], "to": [90, 30], "width": 0, "colour": None},
        {"type": "Arc", "centre": [50, 50], "radius": 20, "start_deg": 0, "end_deg": 90, "colour": "Blue"},
        {"type": "Arc", "centre": [50, 50], "radius": 20, "start_deg": 90, "end_deg": -180, "colour": None},
        {"type": "Arc", "centre": [50, 50], "radius": 10, "start_deg": 0, "end_deg": 360, "colour": None},
        {"type": "Symbol", "origin": [60, 70], "id": "fuel & <rest>", "size": 20, "rotation_deg": 0},
        {"type": "Glyphs", "contours": [[[0, 0], [5, 0], [5, 5]]], "text": "A<B", "ink": "Black"},
    ]
    root = parse(afrobim.primitives_to_svg(prims, 100, 100))
    assert root.get("viewBox") == "0 0 100 100"
    assert root.get("data-sw-mm") is None
    band, hairline, quarter, reflex, circle, symbol, glyphs = list(root)
    assert [e.get("class") for e in root] == ["line", "line", "arc", "arc", "arc", "symbol", "glyphs"]

    # A band: its width is the stroke, its ends square, Y flipped (20 up is 80 down).
    assert band.tag == SVG + "line"
    assert [band.get(k) for k in ("x1", "y1", "x2", "y2")] == ["10", "80", "90", "80"]
    assert (band.get("stroke"), band.get("stroke-width"), band.get("stroke-linecap")) == (afrobim.display_rgb("Red"), "4", "butt")
    # No width: a hairline; no colour: black.
    assert hairline.get("stroke") == afrobim.display_rgb("Black")
    assert hairline.get("vector-effect") == "non-scaling-stroke"

    # 0° to 90° anticlockwise: from (70, 50) to (50, 70), which is (70, 50) to (50, 30) on
    # screen, the short way round, anticlockwise (sweep flag 0).
    assert quarter.get("d") == "M70,50 A20,20 0 0 0 50,30"
    assert quarter.get("fill") == "none" and quarter.get("stroke") == afrobim.display_rgb("Blue")
    # 90° back to -180°: 270° clockwise, so the large arc with sweep flag 1.
    assert reflex.get("d") == "M50,30 A20,20 0 1 1 30,50"
    assert (circle.tag, circle.get("cx"), circle.get("cy"), circle.get("r")) == (SVG + "circle", "50", "50", "10")

    # A placeholder box over [60, 80] x [70, 90]: its top edge is at y = 100 - 90.
    assert symbol.tag == SVG + "rect"
    assert [symbol.get(k) for k in ("x", "y", "width", "height")] == ["60", "10", "20", "20"]
    assert symbol.get("data-symbol-id") == "fuel & <rest>"  # escaped in the file, read back whole
    assert glyphs.find(SVG + "title").text == "A<B"


def test_primitives_round_trip_through_the_svg_writer(engine, seed, tmp_path):
    layout = engine.layout(seed)
    out = tmp_path / "copy.svg"
    copy = afrobim.primitives_to_svg(layout.primitives(), *layout.size_mm, path=out)
    assert [e.attrib for e in parse(copy)] == [e.attrib for e in parse(layout.to_svg())]
    assert out.read_text(encoding="utf-8") == copy


def test_a_dict_that_is_not_a_primitive_is_a_value_error():
    with pytest.raises(ValueError, match="primitive 0: unknown type"):
        afrobim.primitives_to_svg([{"type": "Circle"}], 10, 10)
    with pytest.raises(ValueError, match="primitive 1: 'outline'"):
        afrobim.primitives_to_svg(
            [{"type": "Fill", "outline": [[0, 0], [1, 0], [1, 1]], "colour": "Green"}, {"type": "Fill", "outline": 3, "colour": "Green"}],
            10,
            10,
        )
