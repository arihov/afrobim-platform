"""Generates a DXF covering each entity type with ezdxf, and what ezdxf reports for it.

Usage: python3 -I ezdxf_gen.py OUTDIR VERSION
Writes OUTDIR/cross.dxf and OUTDIR/cross.json. VERSION is an ezdxf version name such as R12, R2000,
R2010 or R2018. With `--expect FILE.dxf OUT.json` it only writes the expected geometry of an existing file. The JSON holds, for every model space entity (blocks and dimensions exploded, as a
CAD program shows them), a dense polyline of its geometry flattened to 0.001 drawing units by
ezdxf, and the plain text of TEXT and MTEXT entities. The Rust test compares the reader's own
flattening of the same file against it.

The expected geometry comes from ezdxf's own entity code (ezdxf.disassemble, ezdxf.path), not from
anything in this crate, so a mistake in the reader cannot cancel against the same mistake here.
"""
import json
import math
import sys

import ezdxf
from ezdxf import disassemble, path as ezpath
from ezdxf.entities import Hatch, MText, Text
from ezdxf.math import Vec3



def generate(out, version):
    doc = ezdxf.new(version)
    doc.units = 6  # metres ($INSUNITS 6)
    msp = doc.modelspace()
    V = doc.dxfversion
    newer = V >= "AC1015"  # R2000 and later: LWPOLYLINE, ELLIPSE, SPLINE, MTEXT, HATCH

    for name, colour in (("ROAD", 1), ("EDGE", 3), ("TEXT", 5), ("BLK", 2)):
        doc.layers.add(name, color=colour)

    # Straight and curved basics.
    msp.add_line((0, 0), (10, 5), dxfattribs={"layer": "ROAD"})
    msp.add_line((-3.25, 7.5), (100.125, -2), dxfattribs={"layer": "EDGE"})
    msp.add_circle((50, 50), 12.5, dxfattribs={"layer": "ROAD"})
    msp.add_arc((0, 0), 20, 30, 120, dxfattribs={"layer": "ROAD"})
    msp.add_arc((10, 10), 5, 300, 60, dxfattribs={"layer": "ROAD"})  # sweeps through 0
    msp.add_point((7, 8))
    # Mirrored extrusion (0, 0, -1): x flips, arcs reverse.
    msp.add_arc((3, 4), 2, 0, 90, dxfattribs={"extrusion": (0, 0, -1)})
    msp.add_circle((3, 4), 2, dxfattribs={"extrusion": (0, 0, -1)})

    # Old style 2D polylines with bulges (all versions), 3D polyline.
    pl = msp.add_polyline2d([(0, 0, 0, 0, 0.5), (4, 0, 0, 0, -1), (4, 3, 0, 0, 0), (0, 3, 0, 0, 2)], format="xyseb", close=True)
    msp.add_polyline2d([(20, 0), (22, 0, 0, 0, 1), (22, 4)], format="xyb" if False else "xy")
    msp.add_polyline3d([(0, 0, 5), (3, 1, 6), (6, 0, 7)])

    # Solid / 3DFACE.
    msp.add_solid([(0, 0), (2, 0), (0, 1), (2, 1)])
    msp.add_solid([(5, 5), (7, 5), (6, 7)])
    msp.add_3dface([(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)])

    # Text.
    msp.add_text("Kerb 45° %%dX Straße", height=2.5, dxfattribs={"insert": (5, 6), "rotation": 30, "layer": "TEXT"})
    t = msp.add_text("Centred", height=1.5, dxfattribs={"layer": "TEXT"})
    t.set_placement((9, 9), align=ezdxf.enums.TextEntityAlignment.MIDDLE_CENTER)

    if newer:
        msp.add_lwpolyline([(0, 0, 0, 0, 0.5), (2, 0, 0, 0, 0), (2, 2, 0, 0, -0.25), (0, 2)], format="xyseb", close=True, dxfattribs={"layer": "ROAD"})
        msp.add_lwpolyline([(30, 0), (34, 0), (34, 3)], format="xy")
        msp.add_lwpolyline([(0, 0, 0, 0, 1), (6, 0)], format="xyseb", dxfattribs={"extrusion": (0, 0, -1)})
        msp.add_ellipse((10, 20), major_axis=(4, 0), ratio=0.5)
        msp.add_ellipse((10, 20), major_axis=(3, 3), ratio=0.3, start_param=0.4, end_param=2.5)
        msp.add_ellipse((0, 0), major_axis=(0, 3), ratio=0.5, start_param=0, end_param=math.pi)
        # Splines: fit-point, open B-spline, rational (quarter circle). (Periodic closed splines are not covered.)
        msp.add_cad_spline_control_frame(fit_points=[(0, 0), (3, 4), (7, 3), (10, 0)], dxfattribs={"layer": "EDGE"})  # control points and knots as AutoCAD stores them
        msp.add_open_spline([(0, 0), (1, 3), (3, 4), (5, 1), (8, 2)], degree=3)
        msp.add_rational_spline([(1, 0), (1, 1), (0, 1)], weights=[1, math.sqrt(0.5), 1], degree=2)
        # MTEXT.
        m = msp.add_mtext("Line one\\PLine {\\C1;two} %%d", dxfattribs={"insert": (12, 13), "char_height": 2, "layer": "TEXT"})
        m.dxf.attachment_point = 5
        m.dxf.rotation = 15
        # Hatches: polyline path with a bulge, edge path with line, arc, ellipse and spline edges.
        h = msp.add_hatch(color=1)
        h.paths.add_polyline_path([(0, 0, 0), (4, 0, 0.5), (4, 3), (0, 3)], is_closed=True)
        h2 = msp.add_hatch(color=2)
        ep = h2.paths.add_edge_path()
        ep.add_line((0, 0), (4, 0))
        ep.add_arc((4, 2), 2, 270, 90, ccw=True)
        ep.add_line((4, 4), (0, 4))
        ep.add_line((0, 4), (0, 0))
        h3 = msp.add_hatch(color=3)
        ep3 = h3.paths.add_edge_path()
        ep3.add_ellipse((20, 20), major_axis=(5, 0), ratio=0.4, start_angle=0, end_angle=360, ccw=True)
        h4 = msp.add_hatch(color=4)
        ep4 = h4.paths.add_edge_path()
        ep4.add_arc((40, 40), 3, 90, 270, ccw=False)  # clockwise edge: stored as 360 - angle, swapped
        ep4.add_line((40, 37), (40, 43))

    # Blocks: a box with a base point, a by-block coloured line, a nested block, a recursive-free chain.
    box = doc.blocks.new("BOX", base_point=(1, 1))
    box.add_line((1, 1), (3, 1))
    box.add_line((3, 1), (3, 2), dxfattribs={"color": 0})
    box.add_circle((2, 1.5), 0.25)
    box.add_arc((2, 1.5), 0.5, 10, 100)
    outer = doc.blocks.new("OUTER", base_point=(0, 0))
    outer.add_blockref("BOX", (10, 0), dxfattribs={"xscale": 2, "yscale": 2, "rotation": 45})
    if newer:
        pbox = doc.blocks.new("PBOX", base_point=(0, 0))
        pbox.add_lwpolyline([(0, 0), (2, 0, 0, 0, 0.4), (2, 2)], format="xyseb")
        msp.add_blockref("PBOX", (400, 100), dxfattribs={"xscale": 2, "yscale": 2, "rotation": 15})
    msp.add_blockref("BOX", (100, 50), dxfattribs={"xscale": 2, "yscale": 2, "rotation": 90, "layer": "BLK"})
    msp.add_blockref("BOX", (200, 0), dxfattribs={"xscale": -1.5, "yscale": 1.5, "rotation": 20})  # mirrored
    msp.add_blockref("BOX", (300, 0), dxfattribs={"xscale": 3, "yscale": 1, "rotation": 30})  # non-uniform
    msp.add_blockref("OUTER", (500, 500), dxfattribs={"rotation": 10})
    mi = msp.add_blockref("BOX", (0, 200), dxfattribs={"rotation": 90})
    mi.dxf.column_count = 3
    mi.dxf.row_count = 2
    mi.dxf.column_spacing = 10
    mi.dxf.row_spacing = 5

    # A linear dimension: ezdxf renders its drawing block (*D...), which a DXF reader draws.
    if V >= "AC1015":
        dim = msp.add_linear_dim(base=(0, 330), p1=(0, 300), p2=(25, 300), dxfattribs={"layer": "ROAD"})
        dim.render()

    doc.saveas(f"{out}/cross.dxf")

    return f"{out}/cross.dxf", f"{out}/cross.json"


def expect(SRC, DST):
    """What ezdxf says the file holds (re-read, so it is what the file says and not what was intended)."""
    doc2 = ezdxf.readfile(SRC)
    msp2 = doc2.modelspace()

    expected = []
    texts = []
    points = []
    for e in disassemble.recursive_decompose(msp2):
        kind = e.dxftype()
        if isinstance(e, (Text, MText)):
            continue
        if isinstance(e, Hatch):
            for p in ezpath.from_hatch(e):
                pts = [(v.x, v.y) for v in p.flattening(0.001)]
                expected.append({"kind": "HATCH", "closed": True, "pts": pts})
            continue
        prims = list(disassemble.to_primitives([e], max_flattening_distance=0.001))
        for pr in prims:
            if pr.is_empty:
                continue
            pts = [(v.x, v.y) for v in pr.vertices()]
            if kind == "POINT":
                points.append(pts[0])
                continue
            if len(pts) >= 2:
                expected.append({"kind": kind, "closed": False, "pts": pts})

    for e in msp2:
        if isinstance(e, Text):
            h, v = e.dxf.halign, e.dxf.valign
            at = e.dxf.align_point if (h or v) else e.dxf.insert
            texts.append({"text": e.plain_text(), "x": at.x, "y": at.y, "height": e.dxf.height, "rotation": e.dxf.get("rotation", 0.0)})
        elif isinstance(e, MText):
            ins = e.dxf.insert
            texts.append({"text": e.plain_text(split=False), "x": ins.x, "y": ins.y, "height": e.dxf.char_height, "rotation": e.dxf.get("rotation", 0.0)})

    json.dump({"version": doc2.dxfversion, "paths": expected, "texts": texts, "points": points}, open(DST, "w"))
    print(doc2.dxfversion, len(expected), "paths", len(texts), "texts")


if sys.argv[1] == "--expect":
    expect(sys.argv[2], sys.argv[3])
else:
    src, dst = generate(sys.argv[1], sys.argv[2])
    expect(src, dst)
