#!/usr/bin/env python3
"""Independent check of an IFC file written by afrobim-ifc, using IfcOpenShell.

    python3 check_ifc.py model.ifc [expected.json]

Without `expected.json` the generic checks run: the file parses, it validates against the
IFC4X3_ADD2 schema (attribute types, cardinalities and the EXPRESS WHERE rules), the project is in
millimetres, and every sign has a tessellated, coloured Body inside the site.

With `expected.json` (written by the Rust integration test from its hand-built document) every
element is also compared with what the document says: class, names, placement, property sets,
quantities, the bounding box of the plate, and the area, position and colour of the artwork.

IfcOpenShell is a test-time tool only (`pip install ifcopenshell`); the Rust crate does not use it.
Exit status 0 means every check passed.
"""

import json
import sys

import ifcopenshell
import ifcopenshell.util.element
import ifcopenshell.util.placement
import ifcopenshell.util.unit
import ifcopenshell.validate

TOL = 1e-5  # mm; the file carries micro-millimetres

failures = []
passed = 0


def check(condition, message):
    global passed
    if condition:
        passed += 1
    else:
        failures.append(message)


def close(a, b, tol=TOL):
    return abs(a - b) <= tol


def transform(matrix, point):
    x, y, z = point
    return tuple(matrix[r][0] * x + matrix[r][1] * y + matrix[r][2] * z + matrix[r][3] for r in range(3))


def bbox(points):
    return tuple(min(p[k] for p in points) for k in range(3)), tuple(max(p[k] for p in points) for k in range(3))


def triangles_of(face_set):
    pts = face_set.Coordinates.CoordList
    return [tuple(pts[i - 1] for i in tri) for tri in face_set.CoordIndex]


def normal_area(tri):
    (ax, ay, az), (bx, by, bz), (cx, cy, cz) = tri
    ux, uy, uz = bx - ax, by - ay, bz - az
    vx, vy, vz = cx - ax, cy - ay, cz - az
    n = (uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx)
    return n, 0.5 * (n[0] ** 2 + n[1] ** 2 + n[2] ** 2) ** 0.5


def style_rgb(item):
    """The shading colour of a representation item, 0-255, or None."""
    for styled in item.StyledByItem:
        for style in styled.Styles:
            if style.is_a("IfcSurfaceStyle"):
                for part in style.Styles:
                    if part.is_a("IfcSurfaceStyleShading"):
                        c = part.SurfaceColour
                        return tuple(round(v * 255) for v in (c.Red, c.Green, c.Blue))
    return None


def body_of(product):
    if product.Representation is None:
        return None
    bodies = [r for r in product.Representation.Representations if r.RepresentationIdentifier == "Body"]
    return bodies[0] if len(bodies) == 1 else None


def check_generic(f):
    schema = getattr(f, "schema_identifier", None) or f.schema
    check(schema == "IFC4X3_ADD2", f"schema is {schema}, expected IFC4X3_ADD2")

    # The EXPRESS WHERE and global rules need pytest (IfcOpenShell's rule executor imports it).
    # Without it only attribute types and cardinalities are validated, and the output says so.
    logger = ifcopenshell.validate.json_logger()
    try:
        ifcopenshell.validate.validate(f, logger, express_rules=True)
        scope = "schema + EXPRESS rules"
    except ModuleNotFoundError as error:
        logger = ifcopenshell.validate.json_logger()
        ifcopenshell.validate.validate(f, logger, express_rules=False)
        scope = f"schema only; EXPRESS rules NOT checked ({error}; pip install pytest)"
    for statement in logger.statements:
        failures.append(f"schema validation: {statement.get('message', statement)}")
    check(not logger.statements, "schema validation reported problems")
    print(f"ifcopenshell.validate ({scope}): {len(logger.statements)} issue(s)")

    projects = f.by_type("IfcProject")
    check(len(projects) == 1, "exactly one IfcProject")
    check(close(ifcopenshell.util.unit.calculate_unit_scale(f), 0.001, 1e-12), "the length unit is not the millimetre")
    units = {u.UnitType: u for u in projects[0].UnitsInContext.Units}
    length = units.get("LENGTHUNIT")
    check(length is not None and length.is_a("IfcSIUnit") and length.Prefix == "MILLI" and length.Name == "METRE", "LENGTHUNIT is not MILLI METRE")
    area = units.get("AREAUNIT")
    check(area is not None and area.Prefix is None and area.Name == "SQUARE_METRE", "AREAUNIT is not SQUARE_METRE")
    angle = units.get("PLANEANGLEUNIT")
    check(angle is not None and angle.Name == "RADIAN", "PLANEANGLEUNIT is not RADIAN")

    contexts = [c for c in f.by_type("IfcGeometricRepresentationContext") if not c.is_a("IfcGeometricRepresentationSubContext")]
    check(len(contexts) == 1 and contexts[0].ContextType == "Model" and contexts[0].CoordinateSpaceDimension == 3, "one 3D Model context")
    subs = f.by_type("IfcGeometricRepresentationSubContext")
    check(len(subs) == 1 and subs[0].ContextIdentifier == "Body" and subs[0].TargetView == "MODEL_VIEW", "one Body subcontext")

    sites = f.by_type("IfcSite")
    check(len(sites) == 1, "exactly one IfcSite")
    check(ifcopenshell.util.element.get_aggregate(sites[0]) == projects[0], "the site is not aggregated by the project")

    ids = [r.GlobalId for r in f.by_type("IfcRoot")]
    check(len(set(ids)) == len(ids), "GlobalIds are not unique")

    for product in f.by_type("IfcElement"):
        name = product.Name
        check(ifcopenshell.util.element.get_container(product) == sites[0], f"{name}: not contained in the site")
        check(product.ObjectPlacement.PlacementRelTo == sites[0].ObjectPlacement, f"{name}: not placed relative to the site")
        body = body_of(product)
        if body is None:
            continue
        check(body.RepresentationType == "Tessellation", f"{name}: Body is {body.RepresentationType}")
        check(body.ContextOfItems == subs[0], f"{name}: Body is not in the Body subcontext")
        check(all(i.is_a("IfcTriangulatedFaceSet") for i in body.Items), f"{name}: Body has items that are not triangulated face sets")
        check(all(style_rgb(i) is not None for i in body.Items), f"{name}: a face set has no surface colour")
        check(sum(1 for i in body.Items if i.Closed) == 1, f"{name}: expected exactly one closed face set (the plate)")
    for sign in f.by_type("IfcSign"):
        check(sign.PredefinedType == "PICTORAL", f"{sign.Name}: PredefinedType is {sign.PredefinedType}")
    return sites[0]


def check_geom_kernel(f, expected):
    """Lets the IfcOpenShell geometry kernel build every shape and compares its bounding box."""
    try:
        import ifcopenshell.geom
    except ImportError as error:  # a build without the geometry module
        print(f"geometry kernel: skipped ({error})")
        return
    settings = ifcopenshell.geom.settings()
    settings.set("use-world-coords", True)
    built = 0
    for e in expected["elements"]:
        product = f.by_guid(e["global_id"])
        shape = ifcopenshell.geom.create_shape(settings, product)
        v = shape.geometry.verts
        points = [(v[i] * 1000.0, v[i + 1] * 1000.0, v[i + 2] * 1000.0) for i in range(0, len(v), 3)]  # metres -> mm
        lo, hi = bbox(points)
        depth = max([g["layer"] + 1 for g in e["groups"]], default=0) * expected["offset"]
        want_lo = (e["origin"][0], -depth, e["origin"][1])
        want_hi = (e["origin"][0] + e["width"], expected["thickness"], e["origin"][1] + e["height"])
        ok = all(close(lo[k], want_lo[k], 1e-3) and close(hi[k], want_hi[k], 1e-3) for k in range(3))
        check(ok, f"{e['name']}: geometry kernel bounding box {lo}..{hi}, expected {want_lo}..{want_hi}")
        check(len(shape.geometry.faces) > 0, f"{e['name']}: geometry kernel produced no faces")
        built += 1
    print(f"geometry kernel: built {built} shape(s)")


def check_expected(f, expected):
    thickness, offset = expected["thickness"], expected["offset"]
    project = f.by_type("IfcProject")[0]
    check(project.Name == expected["project_name"], f"project name is {project.Name!r}")
    check(project.GlobalId == expected["project_global_id"], "project GlobalId differs")
    for cls, count in expected["counts"].items():
        check(len(f.by_type(cls, include_subtypes=False)) == count, f"{len(f.by_type(cls, include_subtypes=False))} {cls}, expected {count}")

    for e in expected["elements"]:
        name = e["name"]
        try:
            product = f.by_guid(e["global_id"])
        except RuntimeError:
            failures.append(f"{name}: no object with GlobalId {e['global_id']}")
            continue
        check(product.is_a() == e["class"], f"{name}: is a {product.is_a()}, expected {e['class']}")
        for attribute in ("Name", "Description", "ObjectType", "Tag", "PredefinedType"):
            want = e[{"Name": "name", "Description": "description", "ObjectType": "object_type", "Tag": "tag", "PredefinedType": "predefined_type"}[attribute]]
            check(getattr(product, attribute) == want, f"{name}: {attribute} is {getattr(product, attribute)!r}, expected {want!r}")

        # Placement: sheet X -> world X, sheet Y -> world Z, axes unrotated.
        m = ifcopenshell.util.placement.get_local_placement(product.ObjectPlacement).tolist()
        ox, oy = e["origin"]
        identity = all(close(m[r][c], 1.0 if r == c else 0.0, 1e-12) for r in range(3) for c in range(3))
        check(identity, f"{name}: the placement is rotated")
        check(close(m[0][3], ox) and close(m[1][3], 0.0) and close(m[2][3], oy), f"{name}: placed at {[m[r][3] for r in range(3)]}")

        # Property sets and quantities.
        psets = ifcopenshell.util.element.get_psets(product)
        got = {pset: {k: v for k, v in values.items() if k != "id"} for pset, values in psets.items()}
        check(sorted(got) == sorted(e["psets"]), f"{name}: property sets {sorted(got)}, expected {sorted(e['psets'])}")
        for pset, want in e["psets"].items():
            have = got.get(pset, {})
            check(sorted(have) == sorted(want), f"{name}: {pset} has {sorted(have)}, expected {sorted(want)}")
            for key, value in want.items():
                same = close(have.get(key), value, 1e-9) if isinstance(value, float) and isinstance(have.get(key), float) else have.get(key) == value
                check(same and type(have.get(key)) is type(value), f"{name}: {pset}.{key} is {have.get(key)!r}, expected {value!r}")
        for definition in (r.RelatingPropertyDefinition for r in product.IsDefinedBy if r.is_a("IfcRelDefinesByProperties")):
            if definition.is_a("IfcElementQuantity"):
                kinds = {q.Name: q.is_a() for q in definition.Quantities}
                check(all(k == ("IfcQuantityArea" if n == "GrossArea" else "IfcQuantityLength") for n, k in kinds.items()), f"{name}: quantity types {kinds}")

        # Geometry.
        body = body_of(product)
        check(body is not None, f"{name}: no Body representation")
        if body is None:
            continue
        plates = [i for i in body.Items if i.Closed]
        artwork = [i for i in body.Items if not i.Closed]
        check(len(plates) == 1, f"{name}: {len(plates)} plates")
        w, h = e["width"], e["height"]
        lo, hi = bbox([transform(m, p) for p in plates[0].Coordinates.CoordList])
        check(
            all(close(a, b) for a, b in zip(lo, (ox, 0.0, oy))) and all(close(a, b) for a, b in zip(hi, (ox + w, thickness, oy + h))),
            f"{name}: plate bounding box {lo}..{hi}, expected {w} x {thickness} x {h} at ({ox}, 0, {oy})",
        )
        check(len(plates[0].CoordIndex) == 12, f"{name}: the plate has {len(plates[0].CoordIndex)} triangles")
        check(style_rgb(plates[0]) == tuple(expected["plate_rgb"]), f"{name}: plate colour {style_rgb(plates[0])}")
        # Signed volume of the closed shell: outward normals give +w*t*h.
        volume = 0.0
        for a, b, c in triangles_of(plates[0]):
            volume += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6.0
        check(close(volume, w * thickness * h, 1e-3), f"{name}: plate volume {volume}, expected {w * thickness * h}")

        check(len(artwork) == len(e["groups"]), f"{name}: {len(artwork)} artwork face sets, expected {len(e['groups'])}")
        for face_set, group in zip(artwork, e["groups"]):
            label = f"{name} {group['colour']} layer {group['layer']}"
            check(style_rgb(face_set) == tuple(group["rgb"]), f"{label}: colour {style_rgb(face_set)}, expected {group['rgb']}")
            points = face_set.Coordinates.CoordList
            y = -offset * (group["layer"] + 1)
            check(all(close(p[1], y) for p in points), f"{label}: not in the plane y = {y}")
            check(all(-TOL <= p[0] <= w + TOL and -TOL <= p[2] <= h + TOL for p in points), f"{label}: outside the plate face")
            used = {i for tri in face_set.CoordIndex for i in tri}
            check(used == set(range(1, len(points) + 1)), f"{label}: unused or out-of-range points")
            total, facing_front = 0.0, True
            for tri in triangles_of(face_set):
                n, a = normal_area(tri)
                total += a
                facing_front = facing_front and n[1] < 0.0 and close(n[0], 0.0) and close(n[2], 0.0)
            check(facing_front, f"{label}: a triangle does not face -Y")
            check(close(total, group["area"], 1e-3), f"{label}: triangle area {total}, expected {group['area']}")

    check_geom_kernel(f, expected)


def main(argv):
    if len(argv) not in (2, 3):
        print(__doc__)
        return 2
    print(f"ifcopenshell {ifcopenshell.version}")
    f = ifcopenshell.open(argv[1])
    print(f"opened {argv[1]}: {len(list(f))} instances, schema {f.schema_identifier if hasattr(f, 'schema_identifier') else f.schema}")
    check_generic(f)
    if len(argv) == 3:
        with open(argv[2], encoding="utf-8") as handle:
            check_expected(f, json.load(handle))
    signs = f.by_type("IfcSign")
    print(f"{len(signs)} IfcSign, {len(f.by_type('IfcBuildingElementProxy'))} IfcBuildingElementProxy, {len(f.by_type('IfcTriangulatedFaceSet'))} face sets, {len(f.by_type('IfcPropertySet'))} property sets")
    if failures:
        print(f"FAILED: {len(failures)} check(s) failed, {passed} passed")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print(f"OK: {passed} checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
