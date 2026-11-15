#!/usr/bin/env python3
"""Independent check of a model export that holds the road modules' elements, using IfcOpenShell.

    python3 check_roads_ifc.py roads.ifc [expected.json]

Without `expected.json` the generic checks run on any export:

* the file opens, is IFC4X3_ADD2, and passes `ifcopenshell.validate` (schema and, with pytest
  installed, the EXPRESS WHERE rules);
* the spatial structure is project > site > road; every element is exactly one of: contained in
  the road (or, for terrain, in the site), an aggregated part placed relative to its whole, or a
  surface feature that adheres to an element and is contained nowhere;
* the class and PredefinedType of every product is one the crate's mapping table gives;
* every property set and quantity set whose name starts with `Pset_` or `Qto_` exists in the
  IFC4X3 property-set templates IfcOpenShell ships, applies to the class (and predefined type) it
  is on, uses only property names the template has, and each property has the value type the
  template names; custom sets use the `AfroBIM_` prefix;
* IfcOpenShell's geometry iterator builds a shape for every product that has a representation.

With `expected.json` (written by the Rust integration test from the document it exported) the
world bounding box the geometry kernel computes is compared with the module's own solids moved
to where the 3D view draws them, and relationships, placements and quantities are compared too.

IfcOpenShell is a test-time tool only (`pip install ifcopenshell pytest`). Exit status 0 means
every check passed.
"""

import json
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import check_model_ifc as base  # noqa: E402  (helpers shared with the model checks)
import ifcopenshell  # noqa: E402
import ifcopenshell.util.element  # noqa: E402
import ifcopenshell.util.pset  # noqa: E402
import ifcopenshell.validate  # noqa: E402

try:
    import ifcopenshell.geom  # noqa: E402
except ImportError:  # a build without the geometry kernel
    pass

check = base.check
failures = base.failures

# The mapping table of the README: class -> allowed predefined types.
MAPPING = {
    "IfcSurfaceFeature": {"LINEMARKING", "SYMBOLMARKING", "HATCHMARKING", "PAVEMENTSURFACEMARKING", "USERDEFINED"},
    "IfcPipeSegment": {"GUTTER", "CULVERT"},
    "IfcElementAssembly": {"SIGNALASSEMBLY", "MAST"},
    "IfcColumn": {"COLUMN"},
    "IfcBeam": {"BEAM"},
    "IfcFooting": {"PAD_FOOTING"},
    "IfcMember": {"POST", "NOTDEFINED"},
    "IfcLightFixture": {"NOTDEFINED"},
    "IfcRailing": {"GUARDRAIL"},
    "IfcPavement": {"FLEXIBLE", "NOTDEFINED"},
    "IfcCourse": {"PAVEMENT"},
    "IfcBuildingElementProxy": {"USERDEFINED"},
    "IfcGeographicElement": {"TERRAIN"},
    "IfcSign": {"PICTORAL"},
}

QUANTITY_CLASSES = {"Q_LENGTH": "IfcQuantityLength", "Q_AREA": "IfcQuantityArea", "Q_VOLUME": "IfcQuantityVolume", "Q_COUNT": "IfcQuantityCount", "Q_WEIGHT": "IfcQuantityWeight"}

worst = {"bbox_mm": 0.0, "position_mm": 0.0, "volume_rel": 0.0}


def validate(f):
    logger = ifcopenshell.validate.json_logger()
    try:
        ifcopenshell.validate.validate(f, logger, express_rules=True)
        scope = "schema + EXPRESS rules"
    except ModuleNotFoundError as error:
        logger = ifcopenshell.validate.json_logger()
        ifcopenshell.validate.validate(f, logger, express_rules=False)
        scope = f"schema only; EXPRESS rules NOT checked ({error}; pip install pytest)"
    for statement in logger.statements:
        failures.append(f"validation: {statement.get('message', statement)}")
    check(not logger.statements, "validation reported problems")
    print(f"ifcopenshell.validate ({scope}): {len(logger.statements)} issue(s)")


def predefined(product):
    return getattr(product, "PredefinedType", None)


def check_structure(f):
    schema = getattr(f, "schema_identifier", None) or f.schema
    check(schema == "IFC4X3_ADD2", f"schema is {schema}, expected IFC4X3_ADD2")
    projects = f.by_type("IfcProject")
    check(len(projects) == 1, "exactly one IfcProject")
    sites, roads = f.by_type("IfcSite"), f.by_type("IfcRoad")
    check(len(sites) == 1 and ifcopenshell.util.element.get_aggregate(sites[0]) == projects[0], "one IfcSite aggregated by the project")
    check(len(roads) == 1 and ifcopenshell.util.element.get_aggregate(roads[0]) == sites[0], "one IfcRoad aggregated by the site")
    site, road = sites[0], roads[0]
    check(not f.by_type("IfcRoadPart"), "the crate writes no IfcRoadPart")
    ids = [r.GlobalId for r in f.by_type("IfcRoot")]
    check(len(set(ids)) == len(ids), "GlobalIds are not unique")

    for e in f.by_type("IfcElement"):
        label = f"{e.is_a()} {e.Name}"
        kind = e.is_a()
        check(kind in MAPPING, f"{label}: a class outside the mapping table")
        pre = predefined(e)
        if kind in MAPPING:
            check(pre in MAPPING[kind], f"{label}: PredefinedType {pre} is not one of {sorted(MAPPING[kind])}")
        if pre == "USERDEFINED":
            check(bool(e.ObjectType), f"{label}: USERDEFINED without an ObjectType")
        whole = ifcopenshell.util.element.get_aggregate(e)
        contained = [r.RelatingStructure for r in e.ContainedInStructure]
        adhering = [r for r in e.AdheresToElement] if kind == "IfcSurfaceFeature" else []
        if whole is not None:
            check(contained == [] and not adhering, f"{label}: a part that is also contained or adheres")
            check(e.ObjectPlacement.PlacementRelTo == whole.ObjectPlacement, f"{label}: not placed relative to its whole ({whole.is_a()} {whole.Name})")
        elif adhering:
            check(contained == [], f"{label}: adheres to an element and is contained")
            check(e.is_a("IfcSurfaceFeature"), f"{label}: only surface features adhere")
        else:
            want = [site] if kind == "IfcGeographicElement" else [road]
            check(contained == want, f"{label}: contained in {[c.is_a() for c in contained]}, expected {[w.is_a() for w in want]}")
            check(e.ObjectPlacement.PlacementRelTo == want[0].ObjectPlacement, f"{label}: not placed relative to the {want[0].is_a()}")
    for e in f.by_type("IfcSurfaceFeature"):
        check(len(e.AdheresToElement) == 1, f"{e.Name}: an IfcSurfaceFeature must adhere to exactly one element")
    for assembly in f.by_type("IfcElementAssembly"):
        check(assembly.Representation is None, f"{assembly.Name}: the assembly has a representation of its own")
        check(len(ifcopenshell.util.element.get_parts(assembly)) >= 1, f"{assembly.Name}: an assembly without parts")
    for group in f.by_type("IfcGroup"):
        check(group.Representation is None if hasattr(group, "Representation") else True, f"{group.Name}: a group has no shape")
        members = [m for rel in group.IsGroupedBy for m in rel.RelatedObjects]
        check(len(members) >= 2, f"group {group.Name}: fewer than two members")


def template_for(name):
    return ifcopenshell.util.pset.get_template("IFC4X3").get_by_name(name)


def check_standard_sets(f):
    """Property and quantity sets with a standard prefix against the IFC4X3 templates."""
    checked = 0
    for rel in f.by_type("IfcRelDefinesByProperties"):
        definition = rel.RelatingPropertyDefinition
        name = definition.Name
        owners = list(rel.RelatedObjects)
        is_qto = definition.is_a("IfcElementQuantity")
        if not (name.startswith("Pset_") or name.startswith("Qto_")):
            check(name.startswith("AfroBIM_"), f"{name}: neither a standard set nor AfroBIM_")
            continue
        check(name.startswith("Qto_") == is_qto, f"{name}: prefix does not match the kind of set")
        template = template_for(name)
        check(template is not None, f"{name}: not in the IFC4X3 templates")
        if template is None:
            continue
        applicable = {a.strip() for a in (template.ApplicableEntity or "").split(",") if a.strip()}
        by_name = {p.Name: p for p in template.HasPropertyTemplates}
        for owner in owners:
            kind, pre = owner.is_a(), predefined(owner)
            ok = any(a == kind or a == f"{kind}/{pre}" or (owner.is_a(a.split("/")[0]) and "/" not in a) for a in applicable)
            check(ok, f"{name} on {kind} {owner.Name} ({pre}): the template applies to {sorted(applicable)}")
        members = definition.Quantities if is_qto else definition.HasProperties
        for m in members:
            t = by_name.get(m.Name)
            check(t is not None, f"{name}.{m.Name}: not a property of the template ({sorted(by_name)})")
            if t is None:
                continue
            if is_qto:
                check(m.is_a(QUANTITY_CLASSES[t.TemplateType]), f"{name}.{m.Name}: {m.is_a()} where the template has {t.TemplateType}")
            else:
                got = m.NominalValue.is_a() if m.NominalValue is not None else None
                check(got is not None and got.lower() == (t.PrimaryMeasureType or "").lower(), f"{name}.{m.Name}: value type {got}, the template has {t.PrimaryMeasureType}")
            checked += 1
    print(f"standard property and quantity sets checked against the IFC4X3 templates: {checked} properties")


def kernel_shapes(f):
    """Product -> (points mm, triangles) for everything the geometry iterator builds."""
    settings = base.geometry_settings()
    shapes = {}
    iterator = ifcopenshell.geom.iterator(settings, f, max(1, (os.cpu_count() or 2) // 2))
    if iterator.initialize():
        while True:
            shape = iterator.get()
            v, faces = shape.geometry.verts, shape.geometry.faces
            points = [(v[i] * 1000.0, v[i + 1] * 1000.0, v[i + 2] * 1000.0) for i in range(0, len(v), 3)]
            shapes[shape.id] = (points, [(faces[i], faces[i + 1], faces[i + 2]) for i in range(0, len(faces), 3)])
            if not iterator.next():
                break
    return shapes


def all_parts(product):
    out = []
    for p in ifcopenshell.util.element.get_parts(product):
        out.append(p)
        out += all_parts(p)
    return out


def check_generic(f):
    validate(f)
    check_structure(f)
    check_standard_sets(f)
    shaped = [p for p in f.by_type("IfcProduct") if p.Representation is not None and not p.is_a("IfcAlignment")]
    try:
        shapes = kernel_shapes(f)
    except ImportError as error:
        print(f"geometry kernel: NOT AVAILABLE ({error})")
        return None
    missing = [f"{p.is_a()} {p.Name}" for p in shaped if p.id() not in shapes]
    check(not missing, f"the geometry iterator built no shape for {missing}")
    print(f"geometry iterator: {len(shapes)} shape(s) built for {len(shaped)} product(s) with a representation")
    return shapes


def find(f, kind, name):
    found = [p for p in f.by_type(kind) if p.Name == name]
    check(len(found) == 1, f"{kind} named {name!r}: found {len(found)}")
    return found[0] if found else None


def check_expected(f, expected, shapes):
    check(shapes is not None, "the geometry kernel is needed for the bounding box checks")
    for kind, count in expected["counts"].items():
        check(len(f.by_type(kind)) == count, f"{len(f.by_type(kind))} {kind}, expected {count}")
    for w in expected["world"]:
        product = find(f, w["class"], w["name"])
        if product is not None:
            base.check_pose(f"{w['class']} {w['name']}", product, w["pose"])

    # The kernel's world bounding box of what each element is made of against the module's solids.
    for g in expected.get("groups", []):
        roots = [find(f, k, n) for k, n in g["roots"]]
        products = []
        for r in roots:
            if r is None:
                continue
            products += [r] + (all_parts(r) if g["parts"] else [])
        points = []
        for p in products:
            if p.id() in shapes:
                points += shapes[p.id()][0]
        if not points:
            failures.append(f"{g['label']}: no geometry in the kernel")
            continue
        lo, hi = base.bbox(points)
        off = max(max(abs(lo[k] - g["lo"][k]), abs(hi[k] - g["hi"][k])) for k in range(3))
        worst["bbox_mm"] = max(worst["bbox_mm"], off)
        check(off <= 0.05, f"{g['label']}: kernel bounding box {lo}..{hi}, the module's solids span {g['lo']}..{g['hi']} ({off} mm off)")
        if "triangles" in g:
            total = sum(len(shapes[p.id()][1]) for p in products if p.id() in shapes)
            check(total == g["triangles"], f"{g['label']}: kernel has {total} triangles, the module's solids {g['triangles']}")

    # Markings: the painted area times the display thickness is the volume of the solid.
    thickness = expected["marking_thickness_mm"]
    for name in expected["markings_with_volume"]:
        m = find(f, "IfcSurfaceFeature", name)
        if m is None:
            continue
        area = ifcopenshell.util.element.get_psets(m, qtos_only=True)["Qto_SurfaceFeatureBaseQuantities"]["Area"]
        volume = base.mesh_volume_centroid(*shapes[m.id()])[0]
        rel = abs(volume - area * 1e6 * thickness) / volume
        worst["volume_rel"] = max(worst["volume_rel"], rel)
        check(rel <= 1e-3, f"{name}: painted area {area} m2 x {thickness} mm = {area * 1e6 * thickness} mm3, the kernel's volume is {volume}")
    # A gantry foundation is a box: its quantity is its volume.
    for name, want_m3 in expected["foundation_volumes"]:
        part = find(f, "IfcFooting", name)
        if part is None:
            continue
        gross = ifcopenshell.util.element.get_psets(part, qtos_only=True)["Qto_FootingBaseQuantities"]["GrossVolume"]
        volume = base.mesh_volume_centroid(*shapes[part.id()])[0] / 1e9
        rel = abs(volume - gross) / gross
        worst["volume_rel"] = max(worst["volume_rel"], rel)
        check(rel <= 1e-6 and abs(gross - want_m3) <= 1e-9, f"{name}: GrossVolume {gross} m3, kernel volume {volume} m3, expected {want_m3}")
    # Pavement courses: the module's compacted volume against the volume of the slab (chords on
    # the curve, so a little less than the exact figure).
    for name, rel_limit in expected["course_volumes"]:
        course = find(f, "IfcCourse", name)
        if course is None:
            continue
        quantity = ifcopenshell.util.element.get_psets(course, qtos_only=True)["Qto_CourseBaseQuantities"]["Volume"]
        volume = base.mesh_volume_centroid(*shapes[course.id()])[0] / 1e9
        rel = abs(volume - quantity) / quantity
        worst["volume_rel"] = max(worst["volume_rel"], rel)
        check(rel <= rel_limit, f"{name}: Volume {quantity} m3, the slab's volume is {volume} m3 ({rel:.2e} apart, limit {rel_limit})")
        print(f"{name}: Qto Volume {quantity:.4f} m3, slab {volume:.4f} m3, {rel:.2e} apart")

    # Relationships.
    adh = expected["adheres"]
    pavement = find(f, "IfcPavement", adh["pavement"])
    if pavement is not None:
        features = sorted(sf.Name for rel in pavement.HasSurfaceFeatures for sf in rel.RelatedSurfaceFeatures) if hasattr(pavement, "HasSurfaceFeatures") else []
        if not features:
            features = sorted(sf.Name for rel in f.by_type("IfcRelAdheresToElement") if rel.RelatingElement == pavement for sf in rel.RelatedSurfaceFeatures)
        check(features == sorted(adh["markings"]), f"pavement {adh['pavement']}: features {features}, expected {sorted(adh['markings'])}")
    for name in adh["contained"]:
        # No pavement carries it: a proxy (an IfcSurfaceFeature must adhere to an element), in the road.
        m = find(f, "IfcBuildingElementProxy", name)
        if m is not None:
            check([r.RelatingStructure.is_a() for r in m.ContainedInStructure] == ["IfcRoad"], f"{name}: should be contained in the road")
            check(m.ObjectType == "marking", f"{name}: ObjectType {m.ObjectType!r}")
    for g in expected["groups_ifc"]:
        group = find(f, "IfcGroup", g["name"])
        if group is not None:
            members = sorted(m.Name for rel in group.IsGroupedBy for m in rel.RelatedObjects)
            check(members == sorted(g["members"]), f"group {g['name']}: members {members}, expected {sorted(g['members'])}")
    for g in expected["aggregates"]:
        whole = find(f, g["class"], g["name"])
        if whole is not None:
            parts = sorted(p.Name for p in ifcopenshell.util.element.get_parts(whole))
            check(parts == sorted(g["parts"]), f"{g['name']}: parts {parts}, expected {sorted(g['parts'])}")
    positioned = {p.Name for rel in f.by_type("IfcRelPositions") for p in rel.RelatedProducts}
    for name in expected["positions"]:
        check(name in positioned, f"{name}: not positioned by the alignment")


def main(argv):
    if len(argv) not in (2, 3):
        print(__doc__)
        return 2
    print(f"ifcopenshell {ifcopenshell.version}")
    f = ifcopenshell.open(argv[1])
    print(f"opened {argv[1]}: {len(list(f))} instances, {os.path.getsize(argv[1]) / 1024:.0f} KiB")
    shapes = check_generic(f)
    if len(argv) == 3:
        with open(argv[2], encoding="utf-8") as handle:
            check_expected(f, json.load(handle), shapes)
    counts = {}
    for p in f.by_type("IfcProduct"):
        counts[p.is_a()] = counts.get(p.is_a(), 0) + 1
    print("entities by class: " + ", ".join(f"{n} {c}" for c, n in sorted(counts.items())))
    print(f"largest deviations: bounding box {worst['bbox_mm']:.2e} mm (limit 0.05), placement {base.worst['position_mm']:.2e} mm (limit {base.POSITION_TOL}), volume {worst['volume_rel']:.2e}")
    if base.failures:
        print(f"FAILED: {len(base.failures)} check(s) failed, {base.passed} passed")
        for failure in base.failures:
            print(f"  - {failure}")
        return 1
    print(f"OK: {base.passed} checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
