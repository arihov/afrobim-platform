#!/usr/bin/env python3
"""World bounding boxes (mm) of every shaped product of an IFC file, by IfcOpenShell's kernel,
as JSON {GlobalId: [[xmin, ymin, zmin], [xmax, ymax, zmax]]}. Used by tests/import_linear.rs to
compare the importer's reading of IfcLinearPlacement with the toolkit's.

    python3 check_import_linear.py file.ifc out.json
"""
import json
import sys

import numpy as np

import ifcopenshell
import ifcopenshell.geom
import ifcopenshell.util.unit

f = ifcopenshell.open(sys.argv[1])
scale_to_mm = 1000.0  # the iterator returns metres
settings = ifcopenshell.geom.settings()
settings.set("use-world-coords", True)
out = {}
it = ifcopenshell.geom.iterator(settings, f, 1)
if it.initialize():
    while True:
        shape = it.get()
        v = np.array(shape.geometry.verts).reshape(-1, 3) * scale_to_mm
        if len(v):
            out[shape.guid] = [v.min(axis=0).tolist(), v.max(axis=0).tolist()]
        if not it.next():
            break
json.dump(out, open(sys.argv[2], "w"))
print(f"{len(out)} shaped products, IfcOpenShell {ifcopenshell.version}")
