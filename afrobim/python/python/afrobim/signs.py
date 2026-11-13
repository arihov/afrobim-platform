"""The traffic-signs engine: rule pack, sign definitions and layouts."""

from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any, Dict, Iterable, List, Mapping, Optional, Union

from . import _native

PathLike = Union[str, "os.PathLike[str]"]
Definition = Dict[str, Any]
"""A sign definition: a plain dict in the engine's JSON shape."""

PACK_FILE = "UK-TSM-2019.json"

DIRECTIONS = ("Ahead", "Left", "Right")
ROUTE_STATUSES = ("Unspecified", "Motorway", "Primary", "NonPrimary")
COLOURS = ("White", "Black", "Green", "Blue", "Brown", "Yellow", "Red", "Grey")
SIGN_KINDS = (
    "StackType",
    "MapTypeCrossroads",
    "MapTypeRoundabout",
    "MapTypeTee",
    "DirectionSign",
    "Confirmatory",
    "PlaceIdentification",
    "LanePreselection",
    "Diversion",
    "InformationSign",
    "WarningSign",
    "RegulatoryProhibitory",
    "RegulatoryMandatory",
    "Stop",
    "GiveWay",
    "SupplementaryPlate",
)


def _choice(value: str, allowed: Iterable[str], what: str) -> str:
    """The spelling the engine uses for `value`: case, spaces, hyphens and underscores are free."""
    allowed = tuple(allowed)
    squash = lambda s: "".join(ch for ch in str(s).lower() if ch.isalnum())  # noqa: E731
    for name in allowed:
        if squash(name) == squash(value):
            return name
    raise ValueError(f"{value!r} is not a {what}; expected one of: {', '.join(allowed)}")


def destination(
    name: str,
    direction: str = "Ahead",
    distance: Optional[Union[str, int, float]] = None,
    route_number: Optional[str] = None,
    route: Optional[str] = None,
    *,
    order: int = 0,
    symbol_id: Optional[str] = None,
    number_status: Optional[str] = None,
) -> Dict[str, Any]:
    """One destination of a sign.

    Args:
        name: The legend, e.g. ``"Kasese"``.
        direction: ``"Ahead"``, ``"Left"`` or ``"Right"`` (any letter case).
        distance: The distance legend as it appears on the sign, e.g. ``"12"`` or ``40``.
        route_number: As it appears on the sign, e.g. ``"A11"`` or ``"(M11)"``; brackets mean
            the route is reached by way of another one.
        route: Status of the route towards the destination, which sets its panel:
            ``"Motorway"``, ``"Primary"`` or ``"NonPrimary"``. Default: as the sign.
        order: Proximity rank within the direction; 0 is nearest and is drawn on top.
        symbol_id: A pictogram drawn with the destination.
        number_status: Status of the numbered route, which sets the patch behind its number.
            Default: inferred from the number.

    Returns:
        A dict in the engine's JSON shape, ready for a definition's ``destinations`` list.
    """
    return {
        "name": str(name),
        "direction": _choice(direction, DIRECTIONS, "direction"),
        "order": int(order),
        "symbol_id": symbol_id,
        "distance": None if distance is None else str(distance),
        "route_number": route_number,
        "route": _choice(route, ROUTE_STATUSES, "route status") if route else "Unspecified",
        "number_status": _choice(number_status, ROUTE_STATUSES, "route status") if number_status else "Unspecified",
    }


def normalise(definition: Mapping[str, Any]) -> Definition:
    """The definition with every field present, as the engine reads it.

    Missing optional fields take the engine's defaults. Raises `afrobim.SignError` when the
    dict cannot be read as a sign definition (a missing required field, an unknown kind...).
    """
    return json.loads(_native.normalise_definition(_dump(definition)))


def sign(
    kind: str,
    code: str = "",
    speed_kph: int = 60,
    destinations: Iterable[Mapping[str, Any]] = (),
    *,
    background: str = "Green",
    **fields: Any,
) -> Definition:
    """A sign definition of any kind.

    Args:
        kind: One of `afrobim.SIGN_KINDS`, e.g. ``"DirectionSign"`` (letter case and hyphens
            are free: ``"direction-sign"`` works).
        code: The sign's mark, e.g. ``"DS-001"``.
        speed_kph: Design speed; it sets the x-height.
        destinations: Dicts from `afrobim.destination`.
        background: A colour name, one of `afrobim.COLOURS`.
        **fields: Any other field of the definition, e.g. ``scheme="Primary"``,
            ``catalog_key="UK06.1"``, ``symbol_id="504.1"``.

    For a sign the rule pack catalogues, `SignEngine.definition` gives a ready definition.
    """
    return normalise(
        {
            "code": code,
            "kind": _choice(kind, SIGN_KINDS, "sign kind"),
            "speed_kph": int(speed_kph),
            "destinations": [dict(d) for d in destinations],
            "background": _choice(background, COLOURS, "colour"),
            **fields,
        }
    )


def stack_sign(code: str, speed_kph: int, destinations: Iterable[Mapping[str, Any]] = ()) -> Definition:
    """A stack-type advance direction sign with the defaults the desktop app gives one:
    primary approach, green background.

    Args:
        code: The sign's mark, e.g. ``"ADS-001"``.
        speed_kph: Design speed; it sets the x-height.
        destinations: Dicts from `afrobim.destination`, one panel per direction.
    """
    return sign("StackType", code, speed_kph, destinations)


def display_rgb(colour: str) -> str:
    """The display colour of a named sign colour as ``"#RRGGBB"``.

    An approximation for screens: the standards specify colours by name and chromaticity.
    """
    return _native.display_rgb(_choice(colour, COLOURS, "colour"))


def primitives_to_svg(
    primitives: Iterable[Mapping[str, Any]],
    width: float,
    height: float,
    path: Optional[PathLike] = None,
) -> str:
    """An SVG of primitives in the shape `Layout.primitives` returns, for geometry edited or
    assembled in Python (several signs on one sheet, say).

    Args:
        primitives: The dicts to draw, in order; Y up.
        width, height: The canvas; its lower-left corner is the origin. The SVG's unit is the
            primitives' unit, labelled mm.
        path: Where to write the SVG, if anywhere.

    Raises:
        ValueError: a dict is not a primitive.
    """
    text = _native.svg_from_primitives(json.dumps([dict(p) for p in primitives]), float(width), float(height))
    if path is not None:
        _write_text(path, text)
    return text


def _dump(definition: Mapping[str, Any]) -> str:
    if not isinstance(definition, Mapping):
        raise TypeError(f"a sign definition is a dict, not {type(definition).__name__}")
    return json.dumps(dict(definition))


def _search(env_var: str, relative: Path) -> Optional[Path]:
    """`relative` named by the environment variable, else in a folder above the working
    directory or above this package (a source checkout). The desktop app looks the same way."""
    named = os.environ.get(env_var)
    if named and Path(named).exists():
        return Path(named)
    roots = [Path.cwd(), *Path.cwd().parents, *Path(__file__).resolve().parents]
    for root in roots:
        candidate = root / relative
        if candidate.exists():
            return candidate
    return None


def find_pack() -> Path:
    """The rule pack `SignEngine()` loads when none is given: the file named by
    ``AFROBIM_PACK``, else ``packs/UK-TSM-2019.json`` in a folder above the working directory.

    Raises `FileNotFoundError` when there is none.
    """
    found = _search("AFROBIM_PACK", Path("packs") / PACK_FILE)
    if found is None:
        raise FileNotFoundError(f"rule pack packs/{PACK_FILE} was not found (set AFROBIM_PACK to its path)")
    return found


def find_fonts() -> Path:
    """The fonts folder `SignEngine()` uses when none is given: the folder named by
    ``AFROBIM_FONTS``, else ``fonts/`` in a folder above the working directory.

    Raises `FileNotFoundError` when there is none.
    """
    found = _search("AFROBIM_FONTS", Path("fonts"))
    if found is None:
        raise FileNotFoundError(
            "the fonts folder was not found (set AFROBIM_FONTS to the folder holding the Transport fonts)"
        )
    return found


class Layout:
    """A laid-out sign: its size, the checks the engine raised and its geometry.

    Made by `SignEngine.layout`. The engine works in stroke widths (sw); `sw_mm` is one stroke
    width in millimetres for this sign, so every length is available in either unit.
    """

    def __init__(self, native: "_native.Layout") -> None:
        self._native = native

    @property
    def width_mm(self) -> float:
        """Overall width in millimetres."""
        return self._native.width_mm

    @property
    def height_mm(self) -> float:
        """Overall height in millimetres."""
        return self._native.height_mm

    @property
    def size_mm(self) -> "tuple[float, float]":
        """``(width_mm, height_mm)``."""
        return (self.width_mm, self.height_mm)

    @property
    def width_sw(self) -> float:
        """Overall width in stroke widths."""
        return self._native.width_sw

    @property
    def height_sw(self) -> float:
        """Overall height in stroke widths."""
        return self._native.height_sw

    @property
    def area_m2(self) -> float:
        """Area of the bounding rectangle in square metres."""
        return self.width_mm * self.height_mm / 1.0e6

    @property
    def sw_mm(self) -> float:
        """One stroke width in millimetres (a quarter of the x-height)."""
        return self._native.sw_mm

    @property
    def x_height_mm(self) -> float:
        """The x-height of the lettering in millimetres."""
        return self._native.x_height_mm

    @property
    def diagnostics(self) -> List[Dict[str, str]]:
        """What the engine found, as dicts with ``severity`` (``"Info"``, ``"Warning"`` or
        ``"Error"``), ``code``, ``message`` and ``source`` (the clause of the standard)."""
        return json.loads(self._native.diagnostics_json())

    @property
    def has_errors(self) -> bool:
        """True when a diagnostic of severity ``"Error"`` was raised."""
        return self._native.has_errors

    def primitives(self, units: str = "mm") -> List[Dict[str, Any]]:
        """The sign's geometry in drawing order, Y up, lower-left corner on the origin.

        Args:
            units: ``"mm"`` (default) or ``"sw"`` (stroke widths, as the engine laid it out).

        Returns:
            Dicts keyed by ``type``; points are ``[x, y]`` pairs:

            - ``Fill``: ``outline`` (a closed outline), ``colour``.
            - ``Glyphs``: ``contours`` (closed outlines, filled even-odd so counters stay open),
              ``text``, ``ink`` (``None`` means white).
            - ``Line``: ``from``, ``to``, ``width`` (a square-ended band), ``colour``
              (``None`` means black).
            - ``Arc``: ``centre``, ``radius``, ``start_deg``, ``end_deg`` (anticlockwise from
              +X), ``colour``.
            - ``Symbol``: ``origin``, ``id``, ``size``, ``rotation_deg``: a placeholder for
              artwork not resolved to geometry.
        """
        unit = _choice(units, ("mm", "sw"), "unit")
        return json.loads(self._native.primitives_json(unit == "mm"))

    def to_dxf(self, path: Optional[PathLike] = None) -> str:
        """The sign as DXF R12 text in millimetres, written to `path` when one is given.

        The text is byte for byte what the desktop app and the C# reference writer produce.
        """
        text = self._native.to_dxf()
        if path is not None:
            _write_text(path, text)
        return text

    def to_svg(self, path: Optional[PathLike] = None) -> str:
        """The sign as an SVG in millimetres, written to `path` when one is given.

        One element per primitive, in drawing order, with ``class`` set to ``fill``,
        ``glyphs``, ``line``, ``arc`` or ``symbol``. Colours are screen approximations: use the
        picture for reports and notebooks, and the DXF for fabrication.
        """
        text = self._native.to_svg()
        if path is not None:
            _write_text(path, text)
        return text

    def _repr_svg_(self) -> str:
        """Lets a Jupyter notebook show the sign."""
        return self._native.to_svg()

    def __repr__(self) -> str:
        return (
            f"<afrobim.Layout {self.width_mm:.0f} x {self.height_mm:.0f} mm, "
            f"x-height {self.x_height_mm:g} mm, {len(self.diagnostics)} diagnostic(s)>"
        )


def _write_text(path: PathLike, text: str) -> None:
    """Writes the text as the Rust side does: UTF-8, line endings untouched."""
    Path(path).write_bytes(text.encode("utf-8"))


class SignEngine:
    """A rule pack and the fonts it names, ready to lay signs out.

    Args:
        pack: The rule pack JSON file. Default: the file named by ``AFROBIM_PACK``, else
            ``packs/UK-TSM-2019.json`` in a folder above the working directory.
        fonts: The folder holding the Transport fonts the pack names. Default: the folder named
            by ``AFROBIM_FONTS``, else ``fonts/`` in a folder above the working directory.

    Raises:
        FileNotFoundError: no pack or fonts folder was given and none was found.
        afrobim.SignError: the pack or a font could not be read.
    """

    def __init__(self, pack: Optional[PathLike] = None, fonts: Optional[PathLike] = None) -> None:
        pack_path = Path(pack) if pack is not None else find_pack()
        fonts_path = Path(fonts) if fonts is not None else find_fonts()
        self._native = _native.Engine(pack_path, fonts_path)

    @property
    def pack_id(self) -> str:
        """The rule pack's id, e.g. ``"UK-TSM-2019"``."""
        return self._native.pack_id

    @property
    def pack_name(self) -> str:
        """The rule pack's title."""
        return self._native.pack_name

    @property
    def pack_path(self) -> Path:
        """The rule pack file this engine loaded."""
        return Path(self._native.pack_path)

    @property
    def fonts_path(self) -> Path:
        """The fonts folder this engine loaded."""
        return Path(self._native.fonts_path)

    def catalog(self) -> List[Dict[str, Any]]:
        """Every standard sign the rule pack registers, in pack order.

        Each entry is a dict with ``key`` (e.g. ``"UK04.1"``), ``name``, ``family``, ``series``,
        ``scheme``, ``kind``, ``class``, ``symbol_id``, ``embedded_symbols``, ``template``,
        ``diagram``, ``citation`` and ``sizes`` (the size by speed band, when the pack tables
        one). Pass an entry's key to `definition`.
        """
        return json.loads(self._native.catalog_json())

    def definition(self, key: str, speed_kph: Optional[int] = None) -> Definition:
        """The sign a catalog entry seeds.

        Args:
            key: The entry's key, e.g. ``"UK06.1"`` (letter case is not significant).
            speed_kph: Design speed; default: the default speed of the sign's family.

        Raises:
            KeyError: the pack has no such entry.
        """
        return json.loads(self._native.definition_json(key, speed_kph))

    def layout(self, definition: Mapping[str, Any]) -> Layout:
        """Lays a sign out.

        Args:
            definition: A sign definition dict, from `afrobim.stack_sign`, `afrobim.sign`,
                `definition` or a document element's ``data``.

        Raises:
            afrobim.UnsupportedError: the sign needs something the engine does not do yet.
            afrobim.SignError: the definition is not valid, or a legend uses a character the
                font has no glyph for.
        """
        return Layout(self._native.layout(_dump(definition)))

    def sign_size_mm(self, definition: Mapping[str, Any]) -> Optional[float]:
        """The size the pack tables for a warning or regulatory sign at its design speed (a
        triangle's or octagon's height, a disc's diameter), or ``None`` when it tables none."""
        return self._native.sign_size_mm(_dump(definition))

    def size_bands_kph(self) -> List[Optional[int]]:
        """Upper bounds, in km/h, of the speed bands a catalog entry's ``sizes`` are listed by;
        the last band is open (``None``)."""
        return self._native.size_bands_kph()

    def __repr__(self) -> str:
        return f"<afrobim.SignEngine {self.pack_id}: {self.pack_name}>"
