"""Reads a DXF with ezdxf, audits it and prints a JSON summary of its entities.

Usage: python3 -I read_dxf.py FILE.dxf
"""
import json
import sys

import ezdxf
from ezdxf.lldxf.encoding import decode_dxf_unicode

doc = ezdxf.readfile(sys.argv[1])
auditor = doc.audit()
msp = doc.modelspace()
out = {"version": doc.dxfversion, "errors": [str(e.message) for e in auditor.errors], "fixes": len(auditor.fixes), "entities": []}
for e in msp:
    t = e.dxftype()
    d = {"type": t, "layer": e.dxf.layer, "color": e.dxf.get("color", 256)}
    if t == "TEXT":
        d.update(
            text=decode_dxf_unicode(e.dxf.text),
            height=e.dxf.height,
            rotation=e.dxf.get("rotation", 0.0),
            halign=e.dxf.get("halign", 0),
            width=e.dxf.get("width", 1.0),
            insert=list(e.dxf.insert)[:2],
            align_point=list(e.dxf.align_point)[:2] if e.dxf.hasattr("align_point") else None,
        )
    elif t == "LINE":
        d.update(start=list(e.dxf.start)[:2], end=list(e.dxf.end)[:2])
    elif t == "ARC":
        d.update(center=list(e.dxf.center)[:2], radius=e.dxf.radius, start=e.dxf.start_angle, end=e.dxf.end_angle)
    elif t == "SOLID":
        d.update(vertices=[list(e.dxf.get(f"vtx{i}"))[:2] for i in range(4)])
    elif t == "POLYLINE":
        d.update(points=[list(v.dxf.location)[:2] for v in e.vertices], closed=e.is_closed, start_width=e.dxf.get("default_start_width", 0.0))
    out["entities"].append(d)
print(json.dumps(out))
