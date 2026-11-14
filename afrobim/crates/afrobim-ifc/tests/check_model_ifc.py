#!/usr/bin/env python3
"""Independent check of a model export (`export_ifc_model`) of afrobim-ifc, using IfcOpenShell.

    python3 check_model_ifc.py model.ifc [expected.json]

Without `expected.json` the generic checks run on any model export: the file validates against
the IFC4X3_ADD2 schema (attribute types, cardinalities and the EXPRESS WHERE rules), it is in
millimetres, the spatial structure is project > site > road, every element is either contained in
the road or aggregated in an assembly (never both), parts are placed relative to their assembly,
IfcOpenShell's geometry kernel builds every post and foundation, and every alignment is
aggregated by the project, ends in a zero-length segment and is continuous.

With `expected.json` (written by the Rust integration test from its hand-built site model) every
product is also compared with what the model says: class, world position and rotation read back
from the placement chain, the volume of its solids (from the swept-solid parameters and from the
mesh IfcOpenShell's geometry kernel builds), the bounding box and centroid against the solids the
3D view draws, materials, property sets, and the alignment against its elements.

IfcOpenShell is a test-time tool only (`pip install ifcopenshell pytest`); the Rust crate does not
use it. Exit status 0 means every check passed.
"""

import json
import math
import sys

import ifcopenshell
import ifcopenshell.util.element
import ifcopenshell.util.placement
import ifcopenshell.util.unit
import ifcopenshell.validate

try:  # a build of IfcOpenShell without its geometry kernel still validates and reads placements
    import ifcopenshell.api.alignment
    import ifcopenshell.geom
    import ifcopenshell.ifcopenshell_wrapper as wrapper

    KERNEL_ERROR = None
except ImportError as error:
    KERNEL_ERROR = error

POSITION_TOL = 1e-3  # mm, placements read back from the file
SHAPE_TOL = 1.0  # mm, bounding box and centroid of a meshed solid
VOLUME_TOL = 1e-3  # relative, volume of a meshed solid (0.1 %)

failures = []
passed = 0
worst = {"position_mm": 0.0, "volume_rel": 0.0, "shape_mm": 0.0, "alignment_mm": 0.0}


def check(condition, message):
    global passed
    if condition:
        passed += 1
    else:
        failures.append(message)


def close(a, b, tol):
    return a is not None and b is not None and abs(a - b) <= tol


def apply(m, p):
    return tuple(m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + m[r][3] for r in range(3))


def pose_matrix(pose):
    x, y, z, deg = pose
    c, s = math.cos(math.radians(deg)), math.sin(math.radians(deg))
    return [[c, -s, 0.0, x], [s, c, 0.0, y], [0.0, 0.0, 1.0, z], [0.0, 0.0, 0.0, 1.0]]


def world_matrix(product):
    return ifcopenshell.util.placement.get_local_placement(product.ObjectPlacement).tolist()


def check_pose(label, product, pose):
    m, want = world_matrix(product), pose_matrix(pose)
    shift = max(abs(m[r][3] - want[r][3]) for r in range(3))
    turn = max(abs(m[r][c] - want[r][c]) for r in range(3) for c in range(3))
    worst["position_mm"] = max(worst["position_mm"], shift)
    check(shift <= POSITION_TOL, f"{label}: placed at {[m[r][3] for r in range(3)]}, expected {pose[:3]} ({shift} mm off)")
    check(turn <= 1e-9, f"{label}: axes differ from a turn of {pose[3]} degrees about +Z by {turn}")


def mesh_volume_centroid(points, triangles):
    """Volume and centroid of a closed, outward-wound triangle mesh (signed tetrahedra)."""
    volume, cx, cy, cz = 0.0, 0.0, 0.0, 0.0
    for i, j, k in triangles:
        a, b, c = points[i], points[j], points[k]
        v = (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6.0
        volume += v
        cx += v * (a[0] + b[0] + c[0]) / 4.0
        cy += v * (a[1] + b[1] + c[1]) / 4.0
        cz += v * (a[2] + b[2] + c[2]) / 4.0
    return volume, ((cx / volume, cy / volume, cz / volume) if volume else (0.0, 0.0, 0.0))


def bbox(points):
    return tuple(min(p[k] for p in points) for k in range(3)), tuple(max(p[k] for p in points) for k in range(3))


def profile_area(profile):
    """Area of a parameterized profile from its attributes, mm²."""
    kind = profile.is_a()
    if kind == "IfcCircleHollowProfileDef":
        return math.pi * (profile.Radius**2 - (profile.Radius - profile.WallThickness) ** 2)
    if kind == "IfcCircleProfileDef":
        return math.pi * profile.Radius**2
    if kind == "IfcRectangleProfileDef":
        return profile.XDim * profile.YDim
    if kind == "IfcRectangleHollowProfileDef":
        t, ro, ri = profile.WallThickness, profile.OuterFilletRadius or 0.0, profile.InnerFilletRadius or 0.0
        outer = profile.XDim * profile.YDim - (4.0 - math.pi) * ro * ro
        inner = (profile.XDim - 2 * t) * (profile.YDim - 2 * t) - (4.0 - math.pi) * ri * ri
        return outer - inner
    if kind == "IfcIShapeProfileDef":
        return 2 * profile.OverallWidth * profile.FlangeThickness + (profile.OverallDepth - 2 * profile.FlangeThickness) * profile.WebThickness
    if kind == "IfcUShapeProfileDef":
        return 2 * profile.FlangeWidth * profile.FlangeThickness + (profile.Depth - 2 * profile.FlangeThickness) * profile.WebThickness
    raise ValueError(f"no area formula for {kind}")


def body_of(product):
    if product.Representation is None:
        return None
    bodies = [r for r in product.Representation.Representations if r.RepresentationIdentifier == "Body"]
    return bodies[0] if len(bodies) == 1 else None


def parameter_volume(body):
    """Volume of a Body from its parameters: swept solids by area × depth, face sets by their triangles. mm³."""
    total = 0.0
    for item in body.Items:
        if item.is_a("IfcExtrudedAreaSolid"):
            check(tuple(item.ExtrudedDirection.DirectionRatios) == (0.0, 0.0, 1.0), f"#{item.id()}: not extruded along +Z")
            total += profile_area(item.SweptArea) * item.Depth
        elif item.is_a("IfcTriangulatedFaceSet") and item.Closed:
            pts = item.Coordinates.CoordList
            total += mesh_volume_centroid(pts, [tuple(i - 1 for i in tri) for tri in item.CoordIndex])[0]
    return total


def geometry_settings():
    if KERNEL_ERROR is not None:
        raise KERNEL_ERROR
    settings = ifcopenshell.geom.settings()
    settings.set("use-world-coords", True)
    # Fine enough that a meshed circle is within 0.01 % of the circle.
    settings.set("mesher-linear-deflection", 1e-5)
    settings.set("mesher-angular-deflection", 0.02)
    return settings


def kernel_mesh(settings, product):
    """Points (mm, world) and triangles of the mesh IfcOpenShell's geometry kernel builds."""
    shape = ifcopenshell.geom.create_shape(settings, product)
    v, f = shape.geometry.verts, shape.geometry.faces
    points = [(v[i] * 1000.0, v[i + 1] * 1000.0, v[i + 2] * 1000.0) for i in range(0, len(v), 3)]
    return points, [(f[i], f[i + 1], f[i + 2]) for i in range(0, len(f), 3)]


def check_generic(f):
    schema = getattr(f, "schema_identifier", None) or f.schema
    check(schema == "IFC4X3_ADD2", f"schema is {schema}, expected IFC4X3_ADD2")

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
    check(units["AREAUNIT"].Name == "SQUARE_METRE" and units["AREAUNIT"].Prefix is None, "AREAUNIT is not SQUARE_METRE")
    check(units["PLANEANGLEUNIT"].Name == "RADIAN", "PLANEANGLEUNIT is not RADIAN")
    if f.by_type("IfcQuantityVolume"):
        volume = units.get("VOLUMEUNIT")
        check(volume is not None and volume.Name == "CUBIC_METRE" and volume.Prefix is None, "volume quantities without a CUBIC_METRE unit")

    ids = [r.GlobalId for r in f.by_type("IfcRoot")]
    check(len(set(ids)) == len(ids), "GlobalIds are not unique")

    sites = f.by_type("IfcSite")
    check(len(sites) == 1 and ifcopenshell.util.element.get_aggregate(sites[0]) == projects[0], "one IfcSite aggregated by the project")
    roads = f.by_type("IfcRoad")
    check(len(roads) <= 1, "more than one IfcRoad")
    container = sites[0]
    if roads:
        container = roads[0]
        check(ifcopenshell.util.element.get_aggregate(container) == sites[0], "the road is not aggregated by the site")
        check(container.ObjectPlacement.PlacementRelTo == sites[0].ObjectPlacement, "the road is not placed relative to the site")

    # Spatial containment (IfcElementAssembly, IFC 4.3; bSI rule SPS007): an element is either
    # contained in the spatial structure or an aggregated part of another element, never both.
    # Relative placement of parts (bSI rule OJP001): relative to the placement of the whole.
    for element in f.by_type("IfcElement"):
        name = element.Name
        whole = ifcopenshell.util.element.get_aggregate(element)
        contained = [r.RelatingStructure for r in element.ContainedInStructure]
        if whole is None:
            check(contained == [container], f"{name}: not contained in the {container.is_a()}")
            check(element.ObjectPlacement.PlacementRelTo == container.ObjectPlacement, f"{name}: not placed relative to the {container.is_a()}")
        else:
            check(whole.is_a("IfcElementAssembly"), f"{name}: aggregated by a {whole.is_a()}")
            check(contained == [], f"{name}: both aggregated and contained")
            check(element.ObjectPlacement.PlacementRelTo == whole.ObjectPlacement, f"{name}: not placed relative to its assembly")
    for assembly in f.by_type("IfcElementAssembly"):
        parts = ifcopenshell.util.element.get_parts(assembly)
        check(assembly.PredefinedType == "SIGNALASSEMBLY", f"{assembly.Name}: PredefinedType {assembly.PredefinedType}")
        check(sum(1 for p in parts if p.is_a("IfcSign")) == 1, f"{assembly.Name}: an assembly must hold exactly one sign")
        check(all(p.is_a("IfcSign") or p.is_a("IfcMember") or p.is_a("IfcFooting") for p in parts), f"{assembly.Name}: unexpected part classes")
        check(assembly.Representation is None, f"{assembly.Name}: the assembly has a representation of its own")
    for member in f.by_type("IfcMember"):
        check(member.PredefinedType == "POST", f"{member.Name}: PredefinedType {member.PredefinedType}")
    for footing in f.by_type("IfcFooting"):
        check(footing.PredefinedType == "PAD_FOOTING", f"{footing.Name}: PredefinedType {footing.PredefinedType}")
    for pset in f.by_type("IfcPropertySet"):
        standard = {"Pset_SignCommon", "Pset_MemberCommon", "Pset_Stationing"}
        check(not pset.Name.startswith("Pset_") or pset.Name in standard, f"property set {pset.Name} uses the reserved Pset_ prefix")
    for qto in f.by_type("IfcElementQuantity"):
        standard = {"Qto_SignBaseQuantities", "Qto_MemberBaseQuantities", "Qto_FootingBaseQuantities"}
        check(not qto.Name.startswith("Qto_") or qto.Name in standard, f"quantity set {qto.Name} uses the reserved Qto_ prefix")

    # Every post and foundation must build in the geometry kernel, and a swept solid must mesh
    # to the volume its parameters give.
    try:
        settings = geometry_settings()
    except ImportError as error:
        settings = None
        print(f"geometry kernel: NOT AVAILABLE ({error}); volumes checked from the swept-solid parameters only")
    built = 0
    for product in f.by_type("IfcMember") + f.by_type("IfcFooting"):
        body = body_of(product)
        if body is None:
            continue
        check(body.RepresentationType in ("SweptSolid", "Tessellation"), f"{product.Name}: Body is {body.RepresentationType}")
        kinds = {"SweptSolid": "IfcExtrudedAreaSolid", "Tessellation": "IfcTriangulatedFaceSet"}
        check(all(i.is_a(kinds[body.RepresentationType]) for i in body.Items), f"{product.Name}: Body items do not match {body.RepresentationType}")
        if settings is None:
            continue
        points, triangles = kernel_mesh(settings, product)
        volume = mesh_volume_centroid(points, triangles)[0]
        want = parameter_volume(body)
        rel = abs(volume - want) / want
        worst["volume_rel"] = max(worst["volume_rel"], rel)
        check(rel <= VOLUME_TOL, f"{product.Name}: the geometry kernel's volume {volume} mm3 differs from the parameters' {want} mm3")
        built += 1
    if settings is not None:
        print(f"geometry kernel: built {built} post and foundation shape(s)")

    for alignment in f.by_type("IfcAlignment"):
        check_alignment_generic(f, alignment, projects[0])
    return settings


def horizontal_segments(alignment):
    layouts = [o for rel in alignment.IsNestedBy for o in rel.RelatedObjects if o.is_a("IfcAlignmentHorizontal")]
    if len(layouts) != 1:
        return None
    return [s.DesignParameters for rel in layouts[0].IsNestedBy for s in rel.RelatedObjects]


def integrate(x, y, heading, k0, k1, length, steps=4000):
    """End point and heading of a segment whose curvature runs linearly from k0 to k1 (Simpson's rule)."""
    if length == 0.0:
        return x, y, heading

    def theta(s):
        return heading + k0 * s + (k1 - k0) * s * s / (2.0 * length)

    h = length / steps
    sx = math.cos(theta(0.0)) + math.cos(theta(length))
    sy = math.sin(theta(0.0)) + math.sin(theta(length))
    for i in range(1, steps):
        w = 4.0 if i % 2 else 2.0
        sx += w * math.cos(theta(i * h))
        sy += w * math.sin(theta(i * h))
    return x + sx * h / 3.0, y + sy * h / 3.0, theta(length)


def check_alignment_generic(f, alignment, project):
    name = alignment.Name
    # IfcAlignment (IFC 4.3), "Alignment Aggregation To Project"; bSI rule ALB004.
    check(ifcopenshell.util.element.get_aggregate(alignment) == project, f"{name}: not aggregated by the project")
    check(not alignment.ContainedInStructure, f"{name}: contained in a spatial element")
    nested = [o for rel in alignment.IsNestedBy for o in rel.RelatedObjects]
    check(all(o.is_a("IfcAlignmentHorizontal") or o.is_a("IfcReferent") for o in nested), f"{name}: nests something other than a layout or a referent")
    check(any(o.is_a("IfcReferent") for o in nested), f"{name}: no stationing referent")
    representations = alignment.Representation.Representations
    check(len(representations) == 1 and representations[0].RepresentationIdentifier == "Axis" and representations[0].RepresentationType == "Curve2D", f"{name}: not one Axis / Curve2D representation")
    check(all(i.is_a("IfcIndexedPolyCurve") or i.is_a("IfcPolyline") for i in representations[0].Items), f"{name}: the Axis is not a polyline")
    segments = horizontal_segments(alignment)
    check(segments is not None and len(segments) >= 2, f"{name}: no horizontal layout")
    if not segments:
        return
    # bSI rule ALB015: the layout ends with a segment of length 0.
    check(segments[-1].SegmentLength == 0.0, f"{name}: the last horizontal segment is not of zero length")
    check(all(s.SegmentLength > 0.0 for s in segments[:-1]), f"{name}: a zero-length segment before the end")
    # Continuity (IfcAlignmentHorizontalSegment, IFC 4.3): the end computed from each segment's
    # start, direction, radii and length must be the start of the next one. Radius 0 is a
    # straight; a positive radius turns counter-clockwise.
    for i, (s, following) in enumerate(zip(segments, segments[1:])):
        k0 = 0.0 if s.StartRadiusOfCurvature == 0.0 else 1.0 / s.StartRadiusOfCurvature
        k1 = 0.0 if s.EndRadiusOfCurvature == 0.0 else 1.0 / s.EndRadiusOfCurvature
        kind = s.PredefinedType
        check(kind == ("LINE" if k0 == 0.0 and k1 == 0.0 else "CIRCULARARC" if k0 == k1 else "CLOTHOID"), f"{name} segment {i + 1}: type {kind} with radii {s.StartRadiusOfCurvature}, {s.EndRadiusOfCurvature}")
        x, y, heading = integrate(s.StartPoint.Coordinates[0], s.StartPoint.Coordinates[1], s.StartDirection, k0, k1, s.SegmentLength)
        gap = math.hypot(x - following.StartPoint.Coordinates[0], y - following.StartPoint.Coordinates[1])
        bend = abs(math.remainder(heading - following.StartDirection, 2.0 * math.pi))
        worst["alignment_mm"] = max(worst["alignment_mm"], gap)
        check(gap <= 0.01, f"{name} segment {i + 1}: its computed end is {gap} mm from the start of the next segment")
        check(bend <= 1e-9, f"{name} segment {i + 1}: its computed end direction differs from the next segment's by {bend} rad")
    # The polyline starts and ends where the layout does and passes through every segment start.
    points = representations[0].Items[0].Points.CoordList
    for i, s in enumerate(segments):
        nearest = min(math.hypot(p[0] - s.StartPoint.Coordinates[0], p[1] - s.StartPoint.Coordinates[1]) for p in points)
        check(nearest <= 1e-3, f"{name}: the Axis polyline misses the start of segment {i + 1} by {nearest} mm")


def check_alignment_expected(f, expected, settings):
    e = expected["alignment"]
    alignment = f.by_guid(e["global_id"])
    name = e["name"]
    check(alignment.is_a("IfcAlignment") and alignment.Name == name, f"{name}: not the expected IfcAlignment")
    segments = horizontal_segments(alignment)
    check(len(segments) == len(e["segments"]) + 1, f"{name}: {len(segments)} horizontal segments, expected {len(e['segments'])} and the zero-length one")
    for i, (s, want) in enumerate(zip(segments, e["segments"])):
        label = f"{name} segment {i + 1}"
        radius = lambda k: 0.0 if k == 0.0 else 1000.0 / k
        check(s.PredefinedType == want["type"], f"{label}: type {s.PredefinedType}, expected {want['type']}")
        check(close(s.StartPoint.Coordinates[0], want["x_m"] * 1000.0, 1e-6) and close(s.StartPoint.Coordinates[1], want["y_m"] * 1000.0, 1e-6), f"{label}: start {s.StartPoint.Coordinates}")
        check(abs(math.remainder(s.StartDirection - want["heading_rad"], 2.0 * math.pi)) <= 1e-12, f"{label}: direction {s.StartDirection}, expected {want['heading_rad']}")
        check(close(s.SegmentLength, want["length_m"] * 1000.0, 1e-6), f"{label}: length {s.SegmentLength}")
        check(close(s.StartRadiusOfCurvature, radius(want["curvature_start"]), 1e-5), f"{label}: start radius {s.StartRadiusOfCurvature}")
        check(close(s.EndRadiusOfCurvature, radius(want["curvature_end"]), 1e-5), f"{label}: end radius {s.EndRadiusOfCurvature}")

    referents = [o for rel in alignment.IsNestedBy for o in rel.RelatedObjects if o.is_a("IfcReferent")]
    check(len(referents) == 1 and referents[0].PredefinedType == "STATION", f"{name}: expected one STATION referent")
    station = ifcopenshell.util.element.get_psets(referents[0]).get("Pset_Stationing", {}).get("Station")
    check(close(station, e["start_station_m"] * 1000.0, 1e-6), f"{name}: Pset_Stationing.Station is {station}")
    positioned = sorted(p.Name for rel in alignment.Positions for p in rel.RelatedProducts)
    check(positioned == sorted(e["positions"]), f"{name}: positions {positioned}, expected {sorted(e['positions'])}")

    # The Axis polyline: AfroBIM divides every curve and spiral into equal chords of at most 1 m
    # and writes a straight as its two ends, so the distance along of every vertex is known.
    points = alignment.Representation.Representations[0].Items[0].Points.CoordList
    distances, start = [0.0], 0.0
    for want in e["segments"]:
        n = 1 if want["type"] == "LINE" else max(1, math.ceil(want["length_m"] / 1.0))
        distances += [start + want["length_m"] * i / n for i in range(1, n + 1)]
        start += want["length_m"]
    check(len(points) == len(distances), f"{name}: the Axis has {len(points)} points, expected {len(distances)}")
    check(close(start, e["length_m"], 1e-9), f"{name}: the segments add up to {start} m, expected {e['length_m']}")

    # Second opinion on the curve itself: let IfcOpenShell derive the geometric representation
    # (IfcCompositeCurve of IfcCurveSegment with IfcLine, IfcCircle and IfcClothoid parents) from
    # the exported horizontal segments, evaluate it with its geometry kernel at those distances,
    # and compare with the polyline AfroBIM computed from its own formulae.
    try:
        if KERNEL_ERROR is not None:
            raise KERNEL_ERROR
        alignment.Representation = None
        ifcopenshell.api.alignment.create_representation(f, alignment)
        curve = alignment.Representation.Representations[0].Items[0]
        kinds = [s.ParentCurve.is_a() for s in curve.Segments]
        mapping = {"LINE": "IfcLine", "CIRCULARARC": "IfcCircle", "CLOTHOID": "IfcClothoid"}
        check(kinds == [mapping[w["type"]] for w in e["segments"]] + ["IfcLine"], f"{name}: IfcOpenShell maps the segments to {kinds}")
        plain = ifcopenshell.geom.settings()
        function = wrapper.map_shape(plain, curve)
        evaluator = wrapper.function_item_evaluator(plain, function)
        check(close(function.end(), e["length_m"], 1e-9), f"{name}: IfcOpenShell's curve is {function.end()} m long, expected {e['length_m']}")
        off = 0.0
        for d, p in zip(distances, points):
            m = evaluator.evaluate(d)
            off = max(off, math.hypot(m[0][3] * 1000.0 - p[0], m[1][3] * 1000.0 - p[1]))
        worst["alignment_mm"] = max(worst["alignment_mm"], off)
        check(off <= 0.01, f"{name}: the Axis polyline is up to {off} mm from the curve IfcOpenShell builds from the horizontal segments")
        print(f"alignment {name}: {len(points)} Axis points within {off:.6f} mm of the composite curve IfcOpenShell derives from the {len(segments)} horizontal segments")
    except (ImportError, AttributeError, RuntimeError) as error:
        print(f"alignment {name}: IfcOpenShell curve cross-check NOT RUN ({type(error).__name__}: {error}); continuity was checked by numerical integration only")


def check_expected(f, expected, settings):
    project = f.by_type("IfcProject")[0]
    check(project.Name == expected["project_name"], f"project name is {project.Name!r}")
    for cls, count in expected["counts"].items():
        have = len(f.by_type(cls, include_subtypes=False))
        check(have == count, f"{have} {cls}, expected {count}")

    assemblies = {}
    for a in expected["assemblies"]:
        assembly = f.by_guid(a["global_id"])
        assemblies[a["sign"]] = assembly
        check(assembly.is_a("IfcElementAssembly") and assembly.Name == a["sign"], f"{a['sign']}: not the expected assembly")
        check(len(ifcopenshell.util.element.get_parts(assembly)) == a["parts"], f"{a['sign']}: {len(ifcopenshell.util.element.get_parts(assembly))} parts, expected {a['parts']}")
        check_pose(f"assembly {a['sign']}", assembly, a["pose"])

    for e in expected["products"]:
        name = e["name"]
        try:
            product = f.by_guid(e["global_id"])
        except RuntimeError:
            failures.append(f"{name}: no object with GlobalId {e['global_id']}")
            continue
        check(product.is_a() == e["class"], f"{name}: is a {product.is_a()}, expected {e['class']}")
        check(product.Name == name and product.Tag == e["tag"], f"{name}: Name {product.Name!r}, Tag {product.Tag!r}")
        check(product.PredefinedType == e["predefined_type"], f"{name}: PredefinedType {product.PredefinedType}")
        check_pose(name, product, e["pose"])
        whole = ifcopenshell.util.element.get_aggregate(product)
        check(whole == (assemblies[e["assembly_of"]] if e["assembly_of"] else None), f"{name}: aggregated by {whole}")
        material = ifcopenshell.util.element.get_material(product)
        check((material.Name if material else None) == e["material"], f"{name}: material {material}, expected {e['material']!r}")
        body = body_of(product)
        check((body.RepresentationType if body else None) == e["body"], f"{name}: Body {body.RepresentationType if body else None}, expected {e['body']}")
        if e["volume_mm3"] is None or body is None:
            continue
        # Volume from the parameters in the file: exact to rounding.
        volume = parameter_volume(body)
        check(close(volume, e["volume_mm3"], 1e-9 * e["volume_mm3"]), f"{name}: volume from the parameters {volume} mm3, expected {e['volume_mm3']}")
        if settings is None:
            continue
        # Volume, bounding box and centroid of what the geometry kernel builds, against the
        # volume the model gives and against the solids the 3D view draws, moved to the site.
        points, triangles = kernel_mesh(settings, product)
        volume, centroid = mesh_volume_centroid(points, triangles)
        rel = abs(volume - e["volume_mm3"]) / e["volume_mm3"]
        worst["volume_rel"] = max(worst["volume_rel"], rel)
        check(rel <= VOLUME_TOL, f"{name}: kernel volume {volume} mm3, expected {e['volume_mm3']} ({rel:.2e} off)")
        m = pose_matrix(e["pose"])
        view_points, view_volume, view_centroid = [], 0.0, [0.0, 0.0, 0.0]
        for solid in e["solids"]:
            moved = [apply(m, p) for p in solid["vertices"]]
            view_points += moved
            v, c = mesh_volume_centroid(moved, solid["triangles"])
            view_volume += v
            view_centroid = [view_centroid[k] + v * c[k] for k in range(3)]
        view_centroid = [c / view_volume for c in view_centroid]
        lo, hi = bbox(points)
        want_lo, want_hi = bbox(view_points)
        off = max(max(abs(lo[k] - want_lo[k]), abs(hi[k] - want_hi[k])) for k in range(3))
        drift = max(abs(centroid[k] - view_centroid[k]) for k in range(3))
        worst["shape_mm"] = max(worst["shape_mm"], off, drift)
        check(off <= SHAPE_TOL, f"{name}: kernel bounding box {lo}..{hi}, the 3D view's solids span {want_lo}..{want_hi}")
        check(drift <= SHAPE_TOL, f"{name}: kernel centroid {centroid}, the 3D view's solids have {view_centroid}")

    # The two known solids, with their numbers written out here.
    cylinder = next(p for p in f.by_type("IfcFooting") if p.Name == "ADS-101-F1")
    block = next(p for p in f.by_type("IfcFooting") if p.Name == "INF-103-F1")
    known = [(cylinder, math.pi / 4.0 * 600.0**2 * 1200.0, "cylinder D600 x 1200"), (block, 800.0 * 800.0 * 1000.0, "block 800 x 800 x 1000")]
    for product, want, what in known:
        by_parameters = parameter_volume(body_of(product))
        line = f"{product.Name} ({what}): expected {want:.1f} mm3, parameters give {by_parameters:.1f}"
        check(abs(by_parameters - want) / want <= VOLUME_TOL, f"{product.Name}: {what} has {by_parameters} mm3 by its parameters, expected {want}")
        if settings is not None:
            by_kernel = mesh_volume_centroid(*kernel_mesh(settings, product))[0]
            check(abs(by_kernel - want) / want <= VOLUME_TOL, f"{product.Name}: {what} has {by_kernel} mm3 in the geometry kernel, expected {want}")
            line += f", geometry kernel {by_kernel:.1f} ({abs(by_kernel - want) / want:.2e} off)"
        print(line)

    # Property sets that carry the site position and the design basis.
    s = expected["placed_sign"]
    sign = next(p for p in f.by_type("IfcSign") if p.Name == s["name"])
    for holder in (sign, assemblies[s["name"]]):
        site = ifcopenshell.util.element.get_psets(holder).get("AfroBIM_SitePlacement", {})
        ok = all(close(site.get(k), s[j], 1e-6) for k, j in (("X", "x"), ("Y", "y"), ("Z", "z"), ("RotationDeg", "rotation_deg"), ("StationM", "station_m"), ("OffsetM", "offset_m")))
        check(ok and site.get("Alignment") == s["alignment"], f"{holder.is_a()} {s['name']}: AfroBIM_SitePlacement is {site}")
    for part in ifcopenshell.util.element.get_parts(assemblies[s["name"]]):
        if part.is_a("IfcSign"):
            continue
        psets = ifcopenshell.util.element.get_psets(part)
        own = psets.get("AfroBIM_Post" if part.is_a("IfcMember") else "AfroBIM_Foundation", {})
        check(own.get("DesignBasis") == expected["design_basis"], f"{part.Name}: DesignBasis is {own.get('DesignBasis')!r}")
        check(own.get("SignMark") == s["name"], f"{part.Name}: SignMark is {own.get('SignMark')!r}")
        check("AfroBIM_SitePlacement" in psets and "StationM" not in psets["AfroBIM_SitePlacement"], f"{part.Name}: a support must carry its own position and no station")
    basis = ifcopenshell.util.element.get_psets(assemblies[s["name"]]).get("AfroBIM_SignAssembly", {}).get("DesignBasis")
    check(basis == expected["design_basis"], f"assembly {s['name']}: DesignBasis is {basis!r}")

    check_alignment_expected(f, expected, settings)


def main(argv):
    if len(argv) not in (2, 3):
        print(__doc__)
        return 2
    print(f"ifcopenshell {ifcopenshell.version}")
    f = ifcopenshell.open(argv[1])
    print(f"opened {argv[1]}: {len(list(f))} instances")
    settings = check_generic(f)
    if len(argv) == 3:
        with open(argv[2], encoding="utf-8") as handle:
            check_expected(f, json.load(handle), settings)
    counts = {c: len(f.by_type(c)) for c in ("IfcSign", "IfcMember", "IfcFooting", "IfcElementAssembly", "IfcAlignment", "IfcExtrudedAreaSolid", "IfcTriangulatedFaceSet")}
    print(", ".join(f"{n} {c}" for c, n in counts.items()))
    print(
        f"largest deviations: placement {worst['position_mm']:.2e} mm (limit {POSITION_TOL}), kernel volume {worst['volume_rel']:.2e} (limit {VOLUME_TOL}), "
        f"kernel bounding box / centroid {worst['shape_mm']:.2e} mm (limit {SHAPE_TOL}), alignment {worst['alignment_mm']:.2e} mm (limit 0.01)"
    )
    if failures:
        print(f"FAILED: {len(failures)} check(s) failed, {passed} passed")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print(f"OK: {passed} checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
