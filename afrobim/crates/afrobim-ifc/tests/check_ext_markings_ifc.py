#!/usr/bin/env python3
"""Independent check of the second marking set (category `marking_ext`) in a model export.

    python3 check_ext_markings_ifc.py ext.ifc [expected.json]

Without `expected.json` the generic checks of `check_roads_ifc.py` run: the file opens, is
IFC4X3_ADD2, and passes `ifcopenshell.validate` (schema and, with pytest installed, the EXPRESS
WHERE rules); the spatial structure; the class and PredefinedType of every product against the
mapping table; every standard property and quantity set against the IFC4X3 templates; and the
geometry iterator builds a shape for every product that has one.

With `expected.json` (written by the Rust test) each marking is looked up by name and checked:
its class and PredefinedType, `ObjectType` for a USERDEFINED type, the pavement it adheres to (or
its containment in the road when none does), the placeholder properties, and whether
`Pset_RoadMarkingCommon` is present. IfcOpenShell is a test-time tool only. Exit status 0 means
every check passed.
"""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import check_model_ifc as base  # noqa: E402
import check_roads_ifc as roads  # noqa: E402
import ifcopenshell  # noqa: E402
import ifcopenshell.util.element  # noqa: E402

check = base.check


def check_markings(f, expected):
    by_name = {p.Name: p for p in f.by_type("IfcProduct")}
    pavement = by_name.get(expected["pavement"])
    for m in expected["markings"]:
        name = m["name"]
        p = by_name.get(name)
        check(p is not None, f"{name}: not in the file")
        if p is None:
            continue
        check(p.is_a() == m["class"], f"{name}: class {p.is_a()}, expected {m['class']}")
        check(roads.predefined(p) == m["predefined"], f"{name}: PredefinedType {roads.predefined(p)}, expected {m['predefined']}")
        check(p.ObjectType == m["object_type"], f"{name}: ObjectType {p.ObjectType!r}, expected {m['object_type']!r}")
        if m["adheres_to"]:
            hosts = [rel.RelatingElement.Name for rel in p.AdheresToElement]
            check(hosts == [m["adheres_to"]], f"{name}: adheres to {hosts}")
            check(list(p.ContainedInStructure) == [], f"{name}: an adhering marking is also contained")
        else:
            check([r.RelatingStructure.is_a() for r in p.ContainedInStructure] == ["IfcRoad"], f"{name}: should be contained in the road")
        psets = ifcopenshell.util.element.get_psets(p)
        own = psets.get("AfroBIM_RoadMarkingExt")
        check(own is not None, f"{name}: no AfroBIM_RoadMarkingExt")
        if own is not None:
            check(own.get("TypeId") == m["type_id"], f"{name}: TypeId {own.get('TypeId')!r}")
            check(own.get("PlaceholderBlock") is m["placeholder"], f"{name}: PlaceholderBlock {own.get('PlaceholderBlock')!r}, expected {m['placeholder']}")
            status = own.get("ShapeStatus", "")
            check(m["placeholder"] == ("not its true shape" in status), f"{name}: ShapeStatus {status!r}")
            if m["placeholder"]:
                check("placeholder" in (p.Description or "").lower(), f"{name}: the description does not say it is a placeholder")
        check(("Pset_RoadMarkingCommon" in psets) == m["common"], f"{name}: Pset_RoadMarkingCommon present is {'Pset_RoadMarkingCommon' in psets}, expected {m['common']}")
        if m["adheres_to"] and "Qto_SurfaceFeatureBaseQuantities" in psets:
            check(set(psets["Qto_SurfaceFeatureBaseQuantities"]) - {"id"} <= {"Area", "Length"}, f"{name}: unexpected quantities")
    if pavement is not None:
        features = sorted(sf.Name for rel in f.by_type("IfcRelAdheresToElement") if rel.RelatingElement == pavement for sf in rel.RelatedSurfaceFeatures)
        want = sorted(m["name"] for m in expected["markings"] if m["adheres_to"])
        check(features == want, f"pavement {expected['pavement']}: features {features}, expected {want}")
    kinds = {}
    for p in f.by_type("IfcSurfaceFeature"):
        kinds[p.PredefinedType] = kinds.get(p.PredefinedType, 0) + 1
    print("IfcSurfaceFeature by PredefinedType: " + ", ".join(f"{n} {c}" for n, c in sorted(kinds.items())))


def main(argv):
    if len(argv) not in (2, 3):
        print(__doc__)
        return 2
    print(f"ifcopenshell {ifcopenshell.version}")
    f = ifcopenshell.open(argv[1])
    print(f"opened {argv[1]}: {len(list(f))} instances")
    roads.check_generic(f)
    if len(argv) == 3:
        with open(argv[2], encoding="utf-8") as handle:
            check_markings(f, json.load(handle))
    if base.failures:
        print(f"FAILED: {len(base.failures)} check(s) failed, {base.passed} passed")
        for failure in base.failures:
            print(f"  - {failure}")
        return 1
    print(f"OK: {base.passed} checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
