"""The sign engine through Python: loading, definitions, layout, DXF and errors."""

import shutil

import pytest

import afrobim
from afrobim import signs
from conftest import DXF_FIXTURES, FONTS, PACK, REPO


def test_the_engine_names_its_pack(engine):
    assert engine.pack_id == "UK-TSM-2019"
    assert engine.pack_name == "UK Traffic Signs Manual (Chapter 7)"
    assert engine.pack_path == PACK
    assert engine.fonts_path == FONTS


# ── finding the pack and fonts ───────────────────────────────────────────────


def test_with_no_arguments_the_engine_searches_upward_from_the_working_directory(monkeypatch):
    monkeypatch.delenv("AFROBIM_PACK", raising=False)
    monkeypatch.delenv("AFROBIM_FONTS", raising=False)
    monkeypatch.chdir(REPO / "afrobim" / "python" / "tests")
    assert afrobim.find_pack() == PACK
    assert afrobim.find_fonts() == FONTS
    assert afrobim.SignEngine().pack_id == "UK-TSM-2019"


def test_the_environment_variables_win(monkeypatch, tmp_path):
    pack = tmp_path / "my-pack.json"
    shutil.copy(PACK, pack)
    fonts = tmp_path / "my-fonts"
    shutil.copytree(FONTS, fonts)
    monkeypatch.setenv("AFROBIM_PACK", str(pack))
    monkeypatch.setenv("AFROBIM_FONTS", str(fonts))
    monkeypatch.chdir(REPO)
    engine = afrobim.SignEngine()
    assert (engine.pack_path, engine.fonts_path) == (pack, fonts)
    assert engine.pack_id == "UK-TSM-2019"


def test_nothing_to_find_is_a_file_not_found_error(monkeypatch, tmp_path):
    monkeypatch.delenv("AFROBIM_PACK", raising=False)
    monkeypatch.delenv("AFROBIM_FONTS", raising=False)
    monkeypatch.chdir(tmp_path)
    # A source checkout is also found from the package's own location; take that away.
    monkeypatch.setattr(signs, "__file__", str(tmp_path / "afrobim" / "signs.py"))
    with pytest.raises(FileNotFoundError, match="AFROBIM_PACK"):
        afrobim.SignEngine()
    with pytest.raises(FileNotFoundError, match="AFROBIM_FONTS"):
        afrobim.SignEngine(pack=PACK)


# ── definitions ──────────────────────────────────────────────────────────────


def test_stack_sign_is_the_engines_own_definition(seed, seed_fixture):
    assert seed == seed_fixture["definition"]


def test_destination_spells_values_as_the_engine_does():
    d = afrobim.destination("Gulu", "LEFT", 7, "(M11)", "non-primary", order=1)
    assert d == {
        "name": "Gulu",
        "direction": "Left",
        "order": 1,
        "symbol_id": None,
        "distance": "7",
        "route_number": "(M11)",
        "route": "NonPrimary",
        "number_status": "Unspecified",
    }
    with pytest.raises(ValueError, match="direction"):
        afrobim.destination("Gulu", "north")


def test_sign_builds_any_kind_and_fills_the_defaults():
    d = afrobim.sign("direction-sign", "DS-001", 80, [afrobim.destination("Kasese")], scheme="Primary", catalog_key="UK06.1")
    assert (d["kind"], d["scheme"], d["catalog_key"], d["speed_kph"]) == ("DirectionSign", "Primary", "UK06.1", 80)
    assert d["approach_class"] == "Primary" and d["variant"] == "Secondary" and d["location"] is None
    with pytest.raises(ValueError, match="sign kind"):
        afrobim.sign("billboard")


def test_normalise_accepts_a_partial_dict(seed):
    partial = {
        "code": "ADS-001",
        "kind": "StackType",
        "speed_kph": 60,
        "background": "Green",
        "destinations": [{"name": "Kasese", "direction": "Ahead"}, {"name": "Masheruka", "direction": "Right"}],
    }
    assert afrobim.normalise(partial) == seed


# ── layout ───────────────────────────────────────────────────────────────────


def test_the_seed_sign_is_2112_by_1125_mm(engine, seed):
    """The known result of the C# engine: 67.58 x 36 sw at 31.25 mm per stroke width."""
    layout = engine.layout(seed)
    assert (round(layout.width_mm), round(layout.height_mm)) == (2112, 1125)
    assert layout.width_mm == pytest.approx(2111.875)
    assert layout.height_mm == pytest.approx(1125.0)
    assert (layout.width_sw, layout.height_sw) == pytest.approx((67.58, 36.0))
    assert layout.sw_mm == 31.25
    assert layout.x_height_mm == 125.0
    assert layout.size_mm == (layout.width_mm, layout.height_mm)
    assert layout.area_m2 == pytest.approx(2.111875 * 1.125)
    assert layout.diagnostics == []
    assert not layout.has_errors
    assert "2112 x 1125 mm" in repr(layout)


def test_primitives_in_stroke_widths_are_the_golden_fixture(engine, seed, seed_fixture):
    assert engine.layout(seed).primitives(units="sw") == seed_fixture["primitives"]


def test_primitives_in_millimetres_are_the_same_geometry_scaled(engine, seed):
    layout = engine.layout(seed)
    sw, mm = layout.primitives("sw"), layout.primitives()
    assert [p["type"] for p in mm] == ["Fill", "Fill", "Fill", "Glyphs", "Fill", "Fill", "Glyphs"]
    assert [p["type"] for p in mm] == [p["type"] for p in sw]
    assert [p["text"] for p in mm if p["type"] == "Glyphs"] == ["Kasese", "Masheruka"]
    # The face: the full plate, lower-left on the origin.
    xs = [x for x, _ in mm[0]["outline"]]
    ys = [y for _, y in mm[0]["outline"]]
    assert (min(xs), min(ys)) == (0.0, 0.0)
    assert (max(xs), max(ys)) == pytest.approx((layout.width_mm, layout.height_mm))
    for a, b in zip(sw[1]["outline"], mm[1]["outline"]):
        assert b == pytest.approx([a[0] * layout.sw_mm, a[1] * layout.sw_mm])
    with pytest.raises(ValueError, match="unit"):
        layout.primitives("inches")


def test_diagnostics_are_dicts(engine, seed):
    # A T-junction has no ahead arm: the engine lays the sign out and says so.
    tee = afrobim.sign("MapTypeTee", "ADS-9", 60, seed["destinations"], scheme="Primary", catalog_key="UK03.1")
    found = engine.layout(tee).diagnostics
    assert found, "expected a diagnostic for an ahead destination on a T-junction"
    for d in found:
        assert set(d) == {"severity", "code", "message", "source"}
        assert d["severity"] in ("Info", "Warning", "Error")
        assert d["code"] and d["message"]


# ── DXF ──────────────────────────────────────────────────────────────────────


def test_to_dxf_is_the_reference_writers_file_byte_for_byte(engine, seed_fixture, tmp_path):
    reference = (DXF_FIXTURES / "seed_ahead_right_60.dxf").read_bytes()
    layout = engine.layout(seed_fixture["definition"])
    assert layout.to_dxf().encode("utf-8") == reference

    out = tmp_path / "ADS-001.dxf"
    assert layout.to_dxf(out).encode("utf-8") == reference
    assert out.read_bytes() == reference


# ── catalog ──────────────────────────────────────────────────────────────────


def test_the_catalog_lists_the_packs_signs(engine):
    catalog = engine.catalog()
    assert len(catalog) >= 30
    keys = [c["key"] for c in catalog]
    assert len(set(keys)) == len(keys)
    for key in ("UK01.1", "UK06.1", "UK07.1"):
        assert key in keys
    assert {"key", "name", "family", "series", "scheme", "kind", "class", "symbol_id", "diagram", "citation", "sizes"} <= set(catalog[0])
    assert {c["class"] for c in catalog} >= {"Guidance", "Warning", "Regulatory"}


def test_definition_seeds_a_catalog_sign(engine):
    d = engine.definition("uk06.1")  # case is not significant
    assert (d["kind"], d["catalog_key"], d["code"]) == ("DirectionSign", "UK06.1", "UK06.1")
    assert engine.definition("UK06.1", speed_kph=100)["speed_kph"] == 100
    assert afrobim.normalise(d) == d
    with pytest.raises(KeyError, match="NOPE"):
        engine.definition("NOPE")


def test_every_catalog_entry_lays_out(engine):
    """Each entry lays out to a real size and exports, or raises the documented
    UnsupportedError. With the UK pack every entry lays out today."""
    unsupported = []
    for entry in engine.catalog():
        definition = engine.definition(entry["key"])
        try:
            layout = engine.layout(definition)
        except afrobim.UnsupportedError as e:
            assert str(e).startswith("Not supported:")
            unsupported.append(entry["key"])
            continue
        assert layout.width_mm > 0 and layout.height_mm > 0, entry["key"]
        assert layout.sw_mm > 0 and layout.x_height_mm > 0, entry["key"]
        assert layout.primitives(), entry["key"]
        assert layout.to_dxf().startswith("0\nSECTION"), entry["key"]
        assert layout.to_svg().startswith("<svg "), entry["key"]
    assert unsupported == []


def test_tabled_sizes_of_warning_signs(engine):
    warning = next(c for c in engine.catalog() if c["class"] == "Warning" and c["sizes"])
    size = engine.sign_size_mm(engine.definition(warning["key"]))
    assert size is not None and size > 0
    bands = engine.size_bands_kph()
    assert len(bands) == len(warning["sizes"]["by_band_mm"]) and bands[-1] is None
    assert engine.sign_size_mm(engine.definition("UK06.1")) is None


# ── errors ───────────────────────────────────────────────────────────────────


def test_the_exception_types():
    assert issubclass(afrobim.SignError, ValueError)
    assert issubclass(afrobim.UnsupportedError, afrobim.SignError)
    assert issubclass(afrobim.DocumentError, ValueError)
    assert afrobim.SignError.__module__ == "afrobim"


def test_an_invalid_definition_is_a_sign_error(engine, seed):
    with pytest.raises(afrobim.SignError, match="The sign definition is not valid: unknown variant `Billboard`"):
        engine.layout(dict(seed, kind="Billboard"))
    with pytest.raises(afrobim.SignError, match="missing field `kind`"):
        engine.layout({"code": "X"})
    with pytest.raises(TypeError):
        engine.layout("ADS-001")


def test_a_character_the_font_lacks_is_a_sign_error_with_the_engines_message(engine, seed):
    bad = dict(seed, destinations=[afrobim.destination("漢", "ahead")])
    with pytest.raises(afrobim.SignError, match=r"No glyph for character '漢' \(U\+6F22\)") as raised:
        engine.layout(bad)
    assert not isinstance(raised.value, afrobim.UnsupportedError)


def test_an_unsupported_sign_is_an_unsupported_error(uganda_engine, seed):
    # Embedded signs are not ported for the Uganda layouts, so the engine refuses the sign.
    embedded = dict(seed, embedded=[{"symbol_id": "height-limit", "type": "WarningTriangle"}])
    with pytest.raises(afrobim.UnsupportedError, match="^Not supported") as raised:
        uganda_engine.layout(embedded)
    assert isinstance(raised.value, ValueError)


def test_the_uganda_pack_loads_and_lays_out_its_catalogue(uganda_engine, seed):
    assert uganda_engine.pack_id == "UG-TSM-2004"
    assert len(uganda_engine.catalog()) == 105
    for entry in uganda_engine.catalog():
        layout = uganda_engine.layout(uganda_engine.definition(entry["key"]))
        assert layout.width_mm > 0 and layout.height_mm > 0, entry["key"]
    # The same definition lays out differently under the two standards.
    assert uganda_engine.layout(seed).size_mm != afrobim.SignEngine(PACK, FONTS).layout(seed).size_mm


def test_a_bad_pack_or_fonts_folder_is_a_sign_error(tmp_path):
    with pytest.raises(afrobim.SignError):
        afrobim.SignEngine(tmp_path / "missing.json", FONTS)
    broken = tmp_path / "broken.json"
    broken.write_text("{ not json", encoding="utf-8")
    with pytest.raises(afrobim.SignError):
        afrobim.SignEngine(broken, FONTS)
    with pytest.raises(afrobim.SignError):
        afrobim.SignEngine(PACK, tmp_path)  # no fonts in here
