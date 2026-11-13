"""The AfroBIM document: signs placed in model space, saved as ``.afrobim`` files."""

from __future__ import annotations

import json
from typing import Any, Dict, Iterator, List, Mapping, Optional

from . import _native
from .signs import Layout, PathLike, SignEngine, _dump, _write_text


class Document:
    """A set of elements in model space (millimetres, Y up), as the desktop app edits it.

    Signs are stored exactly as the app stores them: category ``"sign"``, layer ``"Signs"``,
    the element's ``name`` is the sign's mark and its ``data`` is the sign definition. A file
    saved here opens in the app, and the other way round.

    Elements are identified by an integer id, which `add_sign` returns and which stays the same
    for the life of the document.
    """

    def __init__(self) -> None:
        self._native = _native.Doc()

    @classmethod
    def _wrap(cls, native: "_native.Doc") -> "Document":
        doc = cls.__new__(cls)
        doc._native = native
        return doc

    # ── files ────────────────────────────────────────────────────────────────

    @classmethod
    def load(cls, path: PathLike) -> "Document":
        """Reads a ``.afrobim`` file.

        Raises:
            OSError: the file could not be read.
            afrobim.DocumentError: it is not an AfroBIM document, or a newer version.
        """
        return cls._wrap(_native.Doc.load(path))

    @classmethod
    def from_json(cls, text: str) -> "Document":
        """Reads a document from the text of a ``.afrobim`` file."""
        return cls._wrap(_native.Doc.from_json(text))

    def save(self, path: PathLike) -> None:
        """Writes the document as a ``.afrobim`` file (versioned JSON)."""
        self._native.save(path)

    def to_json(self) -> str:
        """The text `save` writes."""
        return self._native.to_json()

    # ── contents ─────────────────────────────────────────────────────────────

    @property
    def standard(self) -> str:
        """The design standard the document was authored against: the id of the rule pack of
        the first sign added, unless set by hand."""
        return self._native.standard

    @standard.setter
    def standard(self, value: str) -> None:
        self._native.standard = str(value)

    @property
    def elements(self) -> List[Dict[str, Any]]:
        """Every element in draw order, as dicts in the file's shape: ``id``, ``category``,
        ``name``, ``origin`` (``{"x", "y"}``, the lower-left corner in mm), ``width``,
        ``height``, ``layer``, ``data`` (for a sign, its definition) and ``properties``.

        The dicts are copies: change the document through its methods.
        """
        return json.loads(self._native.elements_json())

    def element(self, id: int) -> Dict[str, Any]:
        """One element as a dict. Raises `KeyError` when there is no such id."""
        return json.loads(self._native.element_json(id))

    @property
    def layers(self) -> List[Dict[str, Any]]:
        """The layers, as dicts with ``name``, ``visible`` and ``locked``."""
        return json.loads(self._native.layers_json())

    @property
    def extents(self) -> Optional["tuple[float, float, float, float]"]:
        """``(min_x, min_y, max_x, max_y)`` of every element in mm, or ``None`` when empty."""
        return self._native.extents()

    def __len__(self) -> int:
        return self._native.len()

    def __iter__(self) -> Iterator[Dict[str, Any]]:
        return iter(self.elements)

    def __repr__(self) -> str:
        return f"<afrobim.Document {len(self)} element(s), standard {self.standard or 'not set'}>"

    # ── signs ────────────────────────────────────────────────────────────────

    def add_sign(
        self,
        engine: SignEngine,
        definition: Mapping[str, Any],
        x: Optional[float] = None,
        y: Optional[float] = None,
        mark: Optional[str] = None,
    ) -> int:
        """Lays a sign out for its size and adds it to the document.

        Args:
            engine: The engine to size the sign with.
            definition: The sign definition dict.
            x, y: Lower-left corner in mm. Leave both out and the sign goes where automatic
                arrangement puts it: to the right of the last element, on a new row below once
                the row is 12 m wide.
            mark: The element's name, e.g. ``"ADS-001"``. Default: the definition's ``code``,
                else the mark the desktop app gives a new sign of that kind. A mark already in
                the document is made unique (``ADS-001`` becomes ``ADS-002``).

        Returns:
            The new element's id.

        Raises:
            afrobim.SignError: the sign cannot be laid out; nothing is added.
        """
        if (x is None) != (y is None):
            raise ValueError("give both x and y, or neither")
        at = None if x is None or y is None else (float(x), float(y))
        return self._native.add_sign(engine._native, _dump(definition), at, mark)

    def update_sign(self, engine: SignEngine, id: int, definition: Mapping[str, Any]) -> None:
        """Replaces a sign's definition and refreshes its stored size; its position stays."""
        self._native.update_sign(engine._native, id, _dump(definition))

    def layout(self, engine: SignEngine, id: int) -> Layout:
        """Lays out the sign stored in an element."""
        return Layout(self._native.layout(engine._native, id))

    def rename(self, id: int, name: str) -> None:
        """Changes an element's mark. The name is taken as given: see `unique_name`."""
        self._native.rename(id, name)

    def unique_name(self, name: str) -> str:
        """A mark not yet used in the document: ``ADS-001`` → ``ADS-002``, ``R61`` → ``R61-2``."""
        return self._native.unique_name(name)

    # ── editing ──────────────────────────────────────────────────────────────

    def remove(self, id: int) -> bool:
        """Removes an element. Returns False when there was no such id."""
        return self._native.remove(id)

    def move(self, id: int, dx: float, dy: float) -> None:
        """Moves an element by ``(dx, dy)`` mm. Raises `KeyError` when there is no such id."""
        self._native.translate(id, float(dx), float(dy))

    def move_to(self, id: int, x: float, y: float) -> None:
        """Puts an element's lower-left corner at ``(x, y)`` mm."""
        self._native.move_to(id, float(x), float(y))

    def arrange(self) -> None:
        """Lays every element out again in rows, in document order, 500 mm apart."""
        self._native.arrange()

    # ── properties ───────────────────────────────────────────────────────────

    def properties(self, id: int) -> Dict[str, str]:
        """An element's free-form property set (material, drawing reference, cost code...)."""
        return json.loads(self._native.properties_json(id))

    def set_property(self, id: int, key: str, value: Optional[str]) -> None:
        """Sets one property of an element; ``None`` removes it. Values are stored as text."""
        self._native.set_property(id, str(key), None if value is None else str(value))

    def set_properties(self, id: int, properties: Mapping[str, Optional[str]]) -> None:
        """Sets several properties of an element at once; a ``None`` value removes its key."""
        for key, value in properties.items():
            self.set_property(id, key, value)

    # ── export ───────────────────────────────────────────────────────────────

    def export_dxf(self, engine: SignEngine, path: Optional[PathLike] = None) -> str:
        """Every sign at its position, in millimetres, as one DXF R12 file.

        Returns the DXF text and writes it to `path` when one is given. Raises
        `afrobim.SignError`, naming the sign, when one cannot be laid out.
        """
        text = self._native.export_dxf(engine._native)
        if path is not None:
            _write_text(path, text)
        return text
