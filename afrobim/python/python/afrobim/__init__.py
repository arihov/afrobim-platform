"""AfroBIM for Python: traffic signs laid out to the standard, DXF and SVG export, and
``.afrobim`` documents the desktop app opens.

Quick start::

    import afrobim

    engine = afrobim.SignEngine()          # UK-TSM-2019 from a checkout, or AFROBIM_PACK / AFROBIM_FONTS
    sign = afrobim.stack_sign("ADS-001", 60, [
        afrobim.destination("Kasese", "ahead"),
        afrobim.destination("Masheruka", "right"),
    ])
    layout = engine.layout(sign)
    print(layout.width_mm, layout.height_mm, layout.x_height_mm)
    layout.to_dxf("ADS-001.dxf")
    layout.to_svg("ADS-001.svg")

    doc = afrobim.Document()
    doc.add_sign(engine, sign)
    doc.save("scheme.afrobim")

A sign definition is a plain ``dict`` in the engine's JSON shape (the same dict a ``.afrobim``
file stores), so it can be built by hand, read from a spreadsheet or edited in place.

The engine needs a rule pack (``packs/UK-TSM-2019.json``) and the Transport fonts. Neither is
part of this package: the fonts are third-party files and must not be redistributed without a
licence that allows it.
"""

from ._native import FILE_EXTENSION, DocumentError, SignError, UnsupportedError
from .document import Document
from .signs import (
    COLOURS,
    DIRECTIONS,
    ROUTE_STATUSES,
    SIGN_KINDS,
    Layout,
    SignEngine,
    destination,
    display_rgb,
    find_fonts,
    find_pack,
    normalise,
    primitives_to_svg,
    sign,
    stack_sign,
)

__version__ = "0.1.0"

__all__ = [
    "COLOURS",
    "DIRECTIONS",
    "FILE_EXTENSION",
    "ROUTE_STATUSES",
    "SIGN_KINDS",
    "Document",
    "DocumentError",
    "Layout",
    "SignEngine",
    "SignError",
    "UnsupportedError",
    "destination",
    "display_rgb",
    "find_fonts",
    "find_pack",
    "normalise",
    "primitives_to_svg",
    "sign",
    "stack_sign",
]
