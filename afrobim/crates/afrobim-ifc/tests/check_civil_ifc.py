#!/usr/bin/env python3
"""Independent check of an export that holds civil objects (corridors, pipe networks, grading), using IfcOpenShell.

    python3 check_civil_ifc.py civil.ifc

* the file passes `ifcopenshell.validate` (schema and EXPRESS rules) and is IFC4X3_ADD2;
* the spatial structure is project > site > road, with every IfcRoadPart aggregated by the road;
* every civil product is a class and PredefinedType of the mapping table below, which lists only what
  was looked up in the IFC4X3_ADD2 schema;
* an IfcPavement aggregates its IfcCourse parts, each placed relative to it; an IfcDistributionSystem groups
  the pipe segments and chambers by an IfcRelAssignsToGroup; an earthworks cut is not contained in a spatial
  element (the schema forbids it for subtraction features);
* every standard property and quantity set is in the IFC4X3 templates, applies to the class it is on and
  uses the value types the template names (the same check as the road modules');
* the geometry iterator builds a shape for every product that has a representation, and a course's solid
  has the volume its quantity set states.

Exit status 0 means every check passed. IfcOpenShell is a test-time tool only.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import check_model_ifc as base  # noqa: E402
import check_roads_ifc as roads  # noqa: E402
import ifcopenshell  # noqa: E402
import ifcopenshell.util.element  # noqa: E402

check = base.check

CIVIL = {
    "IfcPavement": {"FLEXIBLE", "RIGID", "NOTDEFINED", "USERDEFINED"},
    "IfcCourse": {"ARMOUR", "BALLASTBED", "CORE", "FILTER", "PAVEMENT", "PROTECTION", "USERDEFINED", "NOTDEFINED"},
    "IfcKerb": {"USERDEFINED", "NOTDEFINED"},
    "IfcEarthworksCut": {"BASE_EXCAVATION", "CUT", "DREDGING", "EXCAVATION", "OVEREXCAVATION", "PAVEMENTMILLING", "STEPEXCAVATION", "TOPSOILREMOVAL", "TRENCH", "USERDEFINED", "NOTDEFINED"},
    "IfcEarthworksFill": {"BACKFILL", "COUNTERWEIGHT", "EMBANKMENT", "SLOPEFILL", "SUBGRADE", "SUBGRADEBED", "TRANSITIONSECTION", "USERDEFINED", "NOTDEFINED"},
    "IfcPipeSegment": {"CULVERT", "FLEXIBLESEGMENT", "GUTTER", "RIGIDSEGMENT", "SPOOL", "USERDEFINED", "NOTDEFINED"},
    "IfcDistributionChamberElement": {"FORMEDDUCT", "INSPECTIONCHAMBER", "INSPECTIONPIT", "MANHOLE", "METERCHAMBER", "SUMP", "TRENCH", "VALVECHAMBER", "USERDEFINED", "NOTDEFINED"},
    "IfcCivilElement": None,  # no PredefinedType
    "IfcGeographicElement": {"TERRAIN"},
}
SYSTEM_TYPES = {"DRAINAGE", "RAINWATER", "SEWAGE", "STORMWATER", "WASTEWATER", "USERDEFINED", "NOTDEFINED"}


def check_civil(f):
    schema = getattr(f, "schema_identifier", None) or f.schema
    check(schema == "IFC4X3_ADD2", f"schema is {schema}, expected IFC4X3_ADD2")
    projects, sites, roads_ = f.by_type("IfcProject"), f.by_type("IfcSite"), f.by_type("IfcRoad")
    check(len(projects) == 1 and len(sites) == 1 and len(roads_) == 1, "one project, one site, one road")
    road = roads_[0]
    for part in f.by_type("IfcRoadPart"):
        check(ifcopenshell.util.element.get_aggregate(part) == road, f"IfcRoadPart {part.Name}: not aggregated by the road")
        check(part.PredefinedType in {"CARRIAGEWAY", "ROADSEGMENT", "ROADSIDE", "SHOULDER", "INTERSECTION", "ROUNDABOUT", "USERDEFINED", "NOTDEFINED"}, f"IfcRoadPart {part.Name}: PredefinedType {part.PredefinedType}")
    counts = {}
    for e in f.by_type("IfcElement"):
        kind = e.is_a()
        if kind not in CIVIL and not kind.startswith("IfcAlignment"):
            continue
        counts[kind] = counts.get(kind, 0) + 1
        if CIVIL.get(kind):
            check(getattr(e, "PredefinedType", None) in CIVIL[kind], f"{kind} {e.Name}: PredefinedType {getattr(e, 'PredefinedType', None)}")
        if kind == "IfcEarthworksCut":
            check(not [r for r in f.by_type("IfcRelContainedInSpatialStructure") if e in r.RelatedElements], f"{e.Name}: a cut is contained in a spatial element")
            check(ifcopenshell.util.element.get_aggregate(e) is not None, f"{e.Name}: a cut belongs to nothing")
    for p in f.by_type("IfcPavement"):
        parts = ifcopenshell.util.element.get_parts(p)
        for c in parts:
            check(c.is_a("IfcCourse"), f"{p.Name}: part {c.Name} is a {c.is_a()}")
            check(c.ObjectPlacement.PlacementRelTo == p.ObjectPlacement, f"{c.Name}: not placed relative to its pavement")
    for s in f.by_type("IfcDistributionSystem"):
        check(s.PredefinedType in SYSTEM_TYPES, f"{s.Name}: system type {s.PredefinedType}")
        members = [o for r in f.by_type("IfcRelAssignsToGroup") if r.RelatingGroup == s for o in r.RelatedObjects]
        check(len(members) > 0, f"system {s.Name}: nothing assigned to it")
        for m in members:
            check(m.is_a("IfcPipeSegment") or m.is_a("IfcDistributionChamberElement"), f"system {s.Name}: member {m.is_a()}")
    return counts


def check_volumes(f, shapes):
    """The solid of a course or kerb has the volume its quantity set states."""
    done = 0
    for p in f.by_type("IfcCourse") + f.by_type("IfcKerb"):
        q = ifcopenshell.util.element.get_psets(p, qtos_only=True)
        volume = next((v.get("Volume") for v in q.values()), None)
        if volume is None or p.id() not in shapes:
            continue
        points, tris = shapes[p.id()]
        v, _ = base.mesh_volume_centroid(points, tris)
        got = abs(v) / 1e9
        check(abs(got - volume) <= 0.03 * max(volume, 1e-9) + 1e-6, f"{p.Name}: solid volume {got:.3f} m3, quantity set {volume:.3f} m3")
        done += 1
    print(f"course and kerb volumes compared with their solids: {done}")


def main(argv):
    if len(argv) != 2:
        print(__doc__)
        return 2
    print(f"ifcopenshell {ifcopenshell.version}")
    f = ifcopenshell.open(argv[1])
    print(f"opened {argv[1]}: {len(list(f))} instances, {os.path.getsize(argv[1]) / 1024:.0f} KiB")
    roads.validate(f)
    counts = check_civil(f)
    roads.check_standard_sets(f)
    # Products with a body: not the axis curves of the alignments.
    shaped = [p for p in f.by_type("IfcProduct") if p.Representation is not None and any(r.RepresentationIdentifier == "Body" for r in p.Representation.Representations)]
    try:
        shapes = roads.kernel_shapes(f)
        missing = [f"{p.is_a()} {p.Name}" for p in shaped if p.id() not in shapes]
        check(not missing, f"the geometry iterator built no shape for {missing[:5]}")
        print(f"geometry iterator: {len(shapes)} shape(s) built for {len(shaped)} product(s) with a representation")
        check_volumes(f, shapes)
    except ImportError as error:
        print(f"geometry kernel: NOT AVAILABLE ({error})")
    print("civil entities by class: " + ", ".join(f"{n} {c}" for n, c in sorted(counts.items())))
    if base.failures:
        print(f"FAILED: {len(base.failures)} check(s) failed, {base.passed} passed")
        for failure in base.failures:
            print(f"  - {failure}")
        return 1
    print(f"OK: {base.passed} checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
