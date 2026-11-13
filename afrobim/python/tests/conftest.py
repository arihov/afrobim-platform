"""Shared fixtures. The tests run against the rule pack and fonts of the checkout."""

import json
from pathlib import Path

import pytest

import afrobim

# tests/ -> python/ -> afrobim/ -> the repository root
REPO = Path(__file__).resolve().parents[3]
PACK = REPO / "packs" / "UK-TSM-2019.json"
FONTS = REPO / "fonts"
SIGN_FIXTURES = REPO / "afrobim" / "crates" / "afrobim-signs" / "tests" / "golden"
DXF_FIXTURES = REPO / "afrobim" / "crates" / "afrobim-dxf" / "tests" / "golden"


@pytest.fixture(scope="session")
def engine() -> afrobim.SignEngine:
    return afrobim.SignEngine(PACK, FONTS)


@pytest.fixture(scope="session")
def seed_fixture() -> dict:
    """The engine's golden fixture of the app's seed sign, checked against the C# engine."""
    return json.loads((SIGN_FIXTURES / "seed_ahead_right_60.json").read_text(encoding="utf-8"))


@pytest.fixture()
def seed() -> dict:
    """The app's seed sign: Kasese ahead, Masheruka to the right, 60 km/h."""
    return afrobim.stack_sign(
        "ADS-001",
        60,
        [afrobim.destination("Kasese", "ahead"), afrobim.destination("Masheruka", "right")],
    )
