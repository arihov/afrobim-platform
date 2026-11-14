#!/usr/bin/env python3
"""Compares the AfroBIM IFC importer with IfcOpenShell's geometry iterator.

    python3 -I check_import.py <file.ifc> <ours.json> <result.json> [--map]

`ours.json` is a list of the importer's products: step_id, global_id, entity, status, bounds
([min, max] in world mm or null), triangles, volume_mm3 (signed, null if open). IfcOpenShell runs
with world coordinates and without opening subtractions (the importer does not subtract openings).
Writes one JSON object with the comparison to <result.json> (IfcOpenShell prints diagnostics on stdout, so the result does not go there). Exit code 0 even for mismatches: the Rust test decides.
"""
import json
import sys

import ifcopenshell
import ifcopenshell.geom as geom
import ifcopenshell.util.geolocation as geolocation
import ifcopenshell.util.unit as unit


def bbox(verts):
    xs, ys, zs = verts[0::3], verts[1::3], verts[2::3]
    return [min(xs), min(ys), min(zs)], [max(xs), max(ys), max(zs)]


def volume(verts, faces):
    v = 0.0
    for i in range(0, len(faces), 3):
        a, b, c = (faces[i + k] * 3 for k in range(3))
        ax, ay, az = verts[a:a + 3]
        bx, by, bz = verts[b:b + 3]
        cx, cy, cz = verts[c:c + 3]
        v += (ax * (by * cz - bz * cy) - ay * (bx * cz - bz * cx) + az * (bx * cy - by * cx)) / 6.0
    return v


def main():
    path, ours_path = sys.argv[1], sys.argv[2]
    f = ifcopenshell.open(path)
    ours = {p["step_id"]: p for p in json.load(open(ours_path))}
    s = geom.settings()
    s.set("use-world-coords", True)
    s.set("disable-opening-subtractions", True)
    scale_to_mm = 1000.0  # the iterator returns metres
    to_map = None
    if "--map" in sys.argv[4:]:
        # Local engineering coordinates -> map coordinates with IfcOpenShell's own helmert
        # transformation (util.geolocation), in the unit of the project.
        h = geolocation.get_helmert_transformation_parameters(f)
        us = unit.calculate_unit_scale(f)

        def to_map(x_m, y_m, z_m):
            e, n, hh = geolocation.xyz2enh(x_m / us, y_m / us, z_m / us, h.e, h.n, h.h, h.xaa, h.xao, h.scale, h.factor_x, h.factor_y, h.factor_z)
            return e * us, n * us, hh * us
    theirs = {}
    it = geom.iterator(s, f, 2)
    errors = []
    if it.initialize():
        while True:
            sh = it.get()
            try:
                v = list(sh.geometry.verts)
                fc = list(sh.geometry.faces)
                if v and to_map:
                    v = [c for i in range(0, len(v), 3) for c in to_map(v[i], v[i + 1], v[i + 2])]
                if v:
                    lo, hi = bbox(v)
                    theirs[sh.id] = {
                        "entity": sh.product.is_a(),
                        "bounds": [[x * scale_to_mm for x in lo], [x * scale_to_mm for x in hi]],
                        "triangles": len(fc) // 3,
                        "volume_mm3": volume(v, fc) * 1e9,
                    }
            except Exception as e:  # keep going, report
                errors.append(f"{sh.id}: {e}")
            if not it.next():
                break
    products_ifcopenshell = {}
    for p in f.by_type("IfcProduct"):
        products_ifcopenshell[p.is_a()] = products_ifcopenshell.get(p.is_a(), 0) + 1
    result = {
        "file": path,
        "schema": f.schema,
        "unit_scale": unit.calculate_unit_scale(f),
        "products_ifcopenshell": products_ifcopenshell,
        "geometry_ifcopenshell": len(theirs),
        "iterator_errors": errors,
        "compared": [],
        "ours_only": [],
        "theirs_only": [],
    }
    for sid, t in theirs.items():
        o = ours.get(sid)
        if o is None or not o.get("bounds"):
            result["theirs_only"].append({"step_id": sid, "entity": t["entity"], "ours_status": o["status"] if o else "missing", "ours_unread": o.get("unread", []) if o else []})
            continue
        dev = max(abs(a - b) for a, b in zip(o["bounds"][0] + o["bounds"][1], t["bounds"][0] + t["bounds"][1]))
        ext = max(h - l for l, h in zip(t["bounds"][0], t["bounds"][1]))
        entry = {
            "step_id": sid,
            "entity": t["entity"],
            "status": o["status"],
            "max_bbox_deviation_mm": dev,
            "extent_mm": ext,
            "ours_triangles": o["triangles"],
            "theirs_triangles": t["triangles"],
        }
        if o.get("volume_mm3") is not None and t["volume_mm3"] is not None and abs(t["volume_mm3"]) > 1e-3:
            entry["volume_ratio"] = o["volume_mm3"] / t["volume_mm3"]
        result["compared"].append(entry)
    for sid, o in ours.items():
        if o.get("bounds") and sid not in theirs:
            result["ours_only"].append({"step_id": sid, "entity": o["entity"], "status": o["status"]})
    with open(sys.argv[3], "w") as out:
        json.dump(result, out)


main()
