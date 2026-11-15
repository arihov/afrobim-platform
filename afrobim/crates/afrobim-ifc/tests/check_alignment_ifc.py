#!/usr/bin/env python3
"""Checks an AfroBIM alignment export against IfcOpenShell.

    python3 check_alignment_ifc.py linear.ifc absolute.ifc expectations.json

`linear.ifc` is the model exported with AlignmentExport::Linear, `absolute.ifc` the same model
exported with AlignmentExport::Geometry (identical placements, written absolutely), and the JSON
what the Rust side worked out from the alignment crate alone. The script

  * validates both files against the IFC4X3_ADD2 schema and EXPRESS rules (0 issues expected),
  * checks the structure the IFC 4.3 documentation asks of an alignment: aggregation by the
    project and not contained, nesting of the layouts, a zero-length last segment in every layout
    and every curve, transition codes, the representations and their identifiers and contexts,
    the stationing referents, the property sets of the referents against the IFC4X3 templates,
  * evaluates the horizontal and vertical curves with IfcOpenShell's kernel at the sampled
    distances and compares position, direction and level with the alignment crate,
  * resolves displayed stations to distances along with ifcopenshell.api.alignment,
  * resolves the world placement of every product in both files and compares them, and the
    kernel's world bounding boxes of every shaped product, and the CartesianPosition fallback of
    every linear placement.

Exit status 1 when a check fails. The figures reached are printed and written to the JSON given
as a fourth argument.
"""
import json
import math
import sys

import numpy as np

import ifcopenshell
import ifcopenshell.api.alignment as al
import ifcopenshell.geom
import ifcopenshell.ifcopenshell_wrapper as w
import ifcopenshell.util.element
import ifcopenshell.util.placement
import ifcopenshell.util.pset
import ifcopenshell.validate

failures = []
figures = {}


def check(ok, message):
    if not ok:
        failures.append(message)
        print("FAIL:", message)


def validate(f, label):
    logger = ifcopenshell.validate.json_logger()
    ifcopenshell.validate.validate(f, logger, express_rules=True)
    figures[f"issues_{label}"] = len(logger.statements)
    for s in logger.statements[:10]:
        print("  issue:", s.get("message", s)[:240])
    check(len(logger.statements) == 0, f"{label}: {len(logger.statements)} validation issue(s)")


# ----------------------------------------------------------------------------- structure


def structure(f, expect):
    project = f.by_type("IfcProject")[0]
    aligned = f.by_type("IfcAlignment")
    check(len(aligned) == len(expect["alignments"]), f"{len(aligned)} alignments, expected {len(expect['alignments'])}")
    templates = ifcopenshell.util.pset.get_template("IFC4X3")
    for a, e in zip(aligned, expect["alignments"]):
        name = e["name"]
        # "Alignment Aggregation To Project": IfcRelAggregates to the project, not contained.
        agg = [r for r in a.Decomposes if r.is_a("IfcRelAggregates") and r.RelatingObject == project]
        check(len(agg) == 1, f"{name}: aggregated by the project {len(agg)} time(s)")
        check(not a.ContainedInStructure, f"{name}: must not be contained in a spatial element")
        # "Alignment Spatial Reference": the road references it.
        refs = [r for r in a.ReferencedInStructures]
        check(len(refs) == 1 and refs[0].RelatingStructure.is_a("IfcRoad"), f"{name}: referenced in the road")
        # "Alignment Layouts": one IfcRelNests holds the layouts, in the order horizontal, vertical.
        nests = list(a.IsNestedBy)
        layouts = [o for n in nests for o in n.RelatedObjects if o.is_a("IfcAlignmentHorizontal") or o.is_a("IfcAlignmentVertical")]
        want = ["IfcAlignmentHorizontal"] + (["IfcAlignmentVertical"] if e["vertical"] else [])
        check([x.is_a() for x in layouts] == want, f"{name}: layouts {[x.is_a() for x in layouts]}")
        check(not any(o.is_a("IfcAlignmentCant") for n in nests for o in n.RelatedObjects), f"{name}: no IfcAlignmentCant for a road")
        # Segments: zero-length last, counts, parameters against the alignment crate.
        for layout, key in zip(layouts, ("horizontal", "vertical")):
            segs = [s for n in layout.IsNestedBy for s in n.RelatedObjects]
            check(len(segs) == e[key]["segments"] + 1, f"{name}: {key} has {len(segs)} segments, expected {e[key]['segments'] + 1}")
            last = segs[-1].DesignParameters
            length = last.SegmentLength if key == "horizontal" else last.HorizontalLength
            check(length == 0.0, f"{name}: the last {key} segment has length {length}, not 0")
            for s in segs:
                check(s.Representation is not None, f"{name}: a {key} segment has no representation")
                rep = s.Representation.Representations[0]
                check(rep.RepresentationIdentifier == "Axis" and rep.RepresentationType == "Segment", f"{name}: segment representation {rep.RepresentationIdentifier}/{rep.RepresentationType}")
                check(rep.Items[0].is_a("IfcCurveSegment"), f"{name}: segment representation item {rep.Items[0].is_a()}")
                check(s.ObjectPlacement == a.ObjectPlacement, f"{name}: a segment is placed with the alignment")
            # design parameters against the crate's elements
            if key == "horizontal":
                kinds = [s.DesignParameters.PredefinedType for s in segs[:-1]]
                check(kinds == e["horizontal"]["kinds"], f"{name}: horizontal kinds {kinds}")
                for s, ex in zip(segs[:-1], e["horizontal"]["segments_detail"]):
                    d = s.DesignParameters
                    check(abs(d.StartPoint.Coordinates[0] - ex["x_mm"]) < 1e-5 and abs(d.StartPoint.Coordinates[1] - ex["y_mm"]) < 1e-5, f"{name}: start point of {s.Name}")
                    dh = math.atan2(math.sin(d.StartDirection - ex["heading"]), math.cos(d.StartDirection - ex["heading"]))
                    check(abs(dh) < 1e-9, f"{name}: start direction of {s.Name} differs by {dh}")
                    check(abs(d.StartRadiusOfCurvature - ex["r0_mm"]) < 1e-3 and abs(d.EndRadiusOfCurvature - ex["r1_mm"]) < 1e-3, f"{name}: radii of {s.Name}")
                    check(abs(d.SegmentLength - ex["length_mm"]) < 1e-5, f"{name}: length of {s.Name}")
            else:
                kinds = [s.DesignParameters.PredefinedType for s in segs[:-1]]
                check(kinds == e["vertical"]["kinds"], f"{name}: vertical kinds {kinds}")
                for s, ex in zip(segs[:-1], e["vertical"]["segments_detail"]):
                    d = s.DesignParameters
                    check(abs(d.StartDistAlong - ex["d0_mm"]) < 1e-4 and abs(d.HorizontalLength - ex["length_mm"]) < 1e-4, f"{name}: extent of {s.Name}")
                    check(abs(d.StartHeight - ex["z0_mm"]) < 1e-4, f"{name}: start height of {s.Name}")
                    check(abs(d.StartGradient - ex["g0"]) < 1e-9 and abs(d.EndGradient - ex["g1"]) < 1e-9, f"{name}: gradients of {s.Name}")
                    if ex["radius_mm"] is None:
                        check(d.RadiusOfCurvature is None, f"{name}: a straight has a radius")
                    else:
                        check(d.RadiusOfCurvature is not None and abs(d.RadiusOfCurvature - ex["radius_mm"]) < 1e-3 * max(1.0, abs(ex["radius_mm"])) * 1e-3, f"{name}: radius of {s.Name} {d.RadiusOfCurvature} vs {ex['radius_mm']}")
        # Geometry: representations, identifiers, contexts, curve segments.
        reps = {(r.RepresentationIdentifier, r.RepresentationType): r for r in a.Representation.Representations}
        if e["vertical"]:
            check(set(reps) == {("Axis", "Curve3D"), ("FootPrint", "Curve2D")}, f"{name}: representations {sorted(reps)}")
            gc = reps[("Axis", "Curve3D")].Items[0]
            cc = reps[("FootPrint", "Curve2D")].Items[0]
            check(gc.is_a("IfcGradientCurve") and gc.BaseCurve == cc, f"{name}: the gradient curve is on the footprint curve")
            curves = [gc, cc]
        else:
            check(set(reps) == {("Axis", "Curve2D")}, f"{name}: representations {sorted(reps)}")
            curves = [reps[("Axis", "Curve2D")].Items[0]]
        for r in reps.values():
            check(r.ContextOfItems.ContextIdentifier == r.RepresentationIdentifier, f"{name}: context '{r.ContextOfItems.ContextIdentifier}' for representation '{r.RepresentationIdentifier}'")
        for c in curves:
            segs = list(c.Segments)
            check(len(segs) >= 2, f"{name}: {c.is_a()} has {len(segs)} segments")
            check(segs[-1].SegmentLength.wrappedValue == 0.0 and segs[-1].Transition == "DISCONTINUOUS", f"{name}: {c.is_a()}: last segment is not a zero-length DISCONTINUOUS one")
            body = [s.Transition for s in segs[:-1]]
            # IfcCompositeCurve CurveContinuous: no DISCONTINUOUS except the last of an open curve.
            check("DISCONTINUOUS" not in body, f"{name}: {c.is_a()}: a transition code inside the curve is DISCONTINUOUS: {body}")
            check(c.SelfIntersect is False or c.SelfIntersect is None or str(c.SelfIntersect) in ("False", "FALSE"), f"{name}: {c.is_a()} SelfIntersect {c.SelfIntersect}")
            check(c.is_a("IfcGradientCurve") or all(not s.ParentCurve.is_a("IfcPolynomialCurve") for s in segs), f"{name}: polynomial in the horizontal curve")
        # Referents: the first nested referent is the start station.
        ref_nests = [n for n in nests if any(o.is_a("IfcReferent") for o in n.RelatedObjects)]
        check(len(ref_nests) >= 1, f"{name}: no nested referents")
        first = ref_nests[0].RelatedObjects[0]
        check(first.PredefinedType == "STATION", f"{name}: the first referent is {first.PredefinedType}")
        sta = ifcopenshell.util.element.get_pset(first, "Pset_Stationing")
        check(abs(sta["Station"] - e["start_station_m"] * 1000.0) < 1e-6, f"{name}: start station {sta['Station']}")
        stationing = [o for o in ref_nests[0].RelatedObjects]
        check(all(o.PredefinedType == "STATION" for o in stationing), f"{name}: the stationing nest holds only STATION referents")
        check(len(stationing) == 1 + len(e["equations"]), f"{name}: {len(stationing)} stationing referents")
        # in order of distance along
        dists = [o.ObjectPlacement.RelativePlacement.Location.DistanceAlong.wrappedValue for o in stationing]
        check(dists == sorted(dists), f"{name}: stationing referents out of order {dists}")
        for o, eq in zip(stationing[1:], e["equations"]):
            ps = ifcopenshell.util.element.get_pset(o, "Pset_Stationing")
            check(abs(ps["Station"] - eq["ahead_mm"]) < 1e-5 and abs(ps["IncomingStation"] - eq["back_mm"]) < 1e-5, f"{name}: equation referent {o.Name}: {ps}")
            check(ps.get("HasIncreasingStation") == eq["has_increasing"], f"{name}: HasIncreasingStation of {o.Name}: {ps.get('HasIncreasingStation')} vs {eq['has_increasing']}")
            check(abs(dists[stationing.index(o)] - eq["distance_mm"]) < 1e-5, f"{name}: distance along of {o.Name}")
        # superelevation referents
        events = [o for n in nests for o in n.RelatedObjects if o.is_a("IfcReferent") and o.PredefinedType == "SUPERELEVATIONEVENT"]
        check(len(events) == len(e["superelevation"]), f"{name}: {len(events)} superelevation events, expected {len(e['superelevation'])}")
        for o, ex in zip(events, e["superelevation"]):
            ps = ifcopenshell.util.element.get_pset(o, "Pset_Superelevation")
            side = ps.get("Side")
            side = side[0] if isinstance(side, (list, tuple)) and len(side) == 1 else side
            check(side == ex["side"], f"{name}: side {side} vs {ex['side']}")
            check(abs(ps.get("Superelevation") - ex["value"]) < 1e-9, f"{name}: superelevation {ps.get('Superelevation')} vs {ex['value']}")
            check(("TransitionSuperelevation" in ps) == ex["transition"], f"{name}: transition property of {o.Name}")
            if "TransitionSuperelevation" in ps:
                t = ps["TransitionSuperelevation"]
                check((t[0] if isinstance(t, (list, tuple)) else t) == "LINEAR", f"{name}: transition {t}")
            own = ifcopenshell.util.element.get_pset(o, "AfroBIM_Superelevation")
            check(abs(own["StationM"] - ex["station_m"]) < 1e-9, f"{name}: AfroBIM_Superelevation station")
            d = o.ObjectPlacement.RelativePlacement.Location.DistanceAlong.wrappedValue
            check(abs(d - (ex["station_m"] - e["start_station_m"]) * 1000.0) < 1e-5, f"{name}: event distance along")
        # every referent's property sets use only template names and types
        for o in stationing + events:
            for set_name, props in ifcopenshell.util.element.get_psets(o).items():
                if not set_name.startswith("Pset_"):
                    continue
                t = templates.get_by_name(set_name)
                check(t is not None, f"{name}: no template for {set_name}")
                if t is None:
                    continue
                allowed = {p.Name: p for p in t.HasPropertyTemplates}
                check(o.is_a() in t.ApplicableEntity.split(",")[0] or t.ApplicableEntity.split("/")[0] in {o.is_a(), "IfcReferent"}, f"{name}: {set_name} applies to {t.ApplicableEntity}, not {o.is_a()}")
                for k in props:
                    if k == "id":
                        continue
                    check(k in allowed, f"{name}: {set_name}.{k} is not in the template")
        # IfcRelPositions
        pos = [r for r in f.by_type("IfcRelPositions") if r.RelatingPositioningElement == a]
        if e["positioned_global_ids"] is not None:
            got = sorted(p.GlobalId for r in pos for p in r.RelatedProducts)
            check(got == sorted(e["positioned_global_ids"]), f"{name}: IfcRelPositions relates {len(got)} products, expected {len(e['positioned_global_ids'])}")
            check(len(pos) == (1 if got else 0), f"{name}: {len(pos)} IfcRelPositions")


def service_rules(f):
    """The alignment rules of the buildingSMART validation service (ifc-gherkin-rules, features
    ALB and ALS), as far as they can be stated without its harness, re-implemented from the
    text of the feature files. This is not the service."""
    hmap = {"LINE": "IfcLine", "CIRCULARARC": "IfcCircle", "CLOTHOID": "IfcClothoid"}
    vmap = {"CONSTANTGRADIENT": "IfcLine", "PARABOLICARC": "IfcPolynomialCurve", "CIRCULARARC": "IfcCircle"}
    settings = ifcopenshell.geom.settings()
    worst = {"position_mm": 0.0, "tangent": 0.0}
    for a in f.by_type("IfcAlignment"):
        n = a.Name
        # ALB003: nested only by horizontal, vertical, cant and referents; ALB030: a local placement.
        for rel in a.IsNestedBy:
            for o in rel.RelatedObjects:
                check(o.is_a() in ("IfcAlignmentHorizontal", "IfcAlignmentVertical", "IfcAlignmentCant", "IfcReferent"), f"ALB003 {n}: nested {o.is_a()}")
        check(a.ObjectPlacement.is_a("IfcLocalPlacement"), f"ALB030 {n}: placement {a.ObjectPlacement.is_a()}")
        layouts = [o for rel in a.IsNestedBy for o in rel.RelatedObjects if o.is_a("IfcAlignmentHorizontal") or o.is_a("IfcAlignmentVertical")]
        # ALB031: one horizontal, or horizontal and vertical.
        check([x.is_a() for x in layouts] in (["IfcAlignmentHorizontal"], ["IfcAlignmentHorizontal", "IfcAlignmentVertical"]), f"ALB031 {n}: layouts")
        # ALB010: a referent is nested.
        check(any(o.is_a("IfcReferent") for rel in a.IsNestedBy for o in rel.RelatedObjects), f"ALB010 {n}: no referent")
        reps = {(r.RepresentationIdentifier, r.RepresentationType): r.Items[0] for r in a.Representation.Representations}
        # ALB021: the curve types correspond to the layouts.
        check((("Axis", "Curve3D") in reps) == any(l.is_a("IfcAlignmentVertical") for l in layouts), f"ALB021 {n}")
        for layout in layouts:
            segs = [x for rel in layout.IsNestedBy for x in rel.RelatedObjects]
            if layout.is_a("IfcAlignmentHorizontal"):
                curve = reps.get(("FootPrint", "Curve2D")) or reps[("Axis", "Curve2D")]
                table = hmap
            else:
                curve = reps[("Axis", "Curve3D")]
                table = vmap
            # ALB022: as many curve segments as layout segments. ALB023: the same geometry type.
            check(len(curve.Segments) == len(segs), f"ALB022 {n}: {len(curve.Segments)} curve segments for {len(segs)} layout segments")
            for x, cs in zip(segs, curve.Segments):
                t = x.DesignParameters.PredefinedType
                check(cs.ParentCurve.is_a(table[t]), f"ALB023 {n}: {t} has parent curve {cs.ParentCurve.is_a()}")
                # ALS004 / ALB023 for the segment's own representation: the same curve segment.
                check(x.Representation.Representations[0].Items[0] == cs, f"ALB023 {n}: segment representation is not the curve's segment")
                # ALS012: the types of SegmentStart and SegmentLength.
                check(cs.SegmentStart.is_a() == "IfcLengthMeasure" and cs.SegmentLength.is_a() == "IfcLengthMeasure", f"ALS012 {n}")
                if layout.is_a("IfcAlignmentVertical"):
                    d = x.DesignParameters
                    if t == "PARABOLICARC":
                        want = d.HorizontalLength / (d.EndGradient - d.StartGradient)
                        check(abs(d.RadiusOfCurvature - want) <= 1e-3, f"ALB012 {n}: radius {d.RadiusOfCurvature} vs {want}")
                    elif t != "CIRCULARARC":
                        check(d.RadiusOfCurvature is None, f"ALB012 {n}: a radius on {t}")
            # ALB015 / ALS015: the zero-length last segments.
            check(segs[-1].DesignParameters.is_a("IfcAlignmentHorizontalSegment") and segs[-1].DesignParameters.SegmentLength == 0.0 if layout.is_a("IfcAlignmentHorizontal") else segs[-1].DesignParameters.HorizontalLength == 0.0, f"ALB015 {n}")
            # ALS016 / ALS017: the end of each segment is the start of the next, by the kernel.
            for s1, s2 in zip(curve.Segments, list(curve.Segments)[1:]):
                fn1, fn2 = w.map_shape(settings, s1), w.map_shape(settings, s2)
                e1 = np.array(w.function_item_evaluator(settings, fn1).evaluate(fn1.end()))
                st2 = np.array(w.function_item_evaluator(settings, fn2).evaluate(fn2.start()))
                worst["position_mm"] = max(worst["position_mm"], float(np.linalg.norm(e1[:3, 3] - st2[:3, 3])) * 1000.0)
                t1, t2 = e1[:3, 0] / np.linalg.norm(e1[:3, 0]), st2[:3, 0] / np.linalg.norm(st2[:3, 0])
                worst["tangent"] = max(worst["tangent"], float(np.linalg.norm(t1 - t2)))
    figures["continuity_by_the_kernel"] = worst
    check(worst["position_mm"] < 1e-3, f"ALS016/ALS017: segments are {worst['position_mm']} mm apart")
    check(worst["tangent"] < 1e-8, f"ALS016/ALS017: tangents differ by {worst['tangent']}")


# ------------------------------------------------------------------------ curve evaluation


def curve_evaluation(f, expect):
    out = {}
    settings = ifcopenshell.geom.settings()
    for a, e in zip(f.by_type("IfcAlignment"), expect["alignments"]):
        reps = {r.RepresentationIdentifier: r.Items[0] for r in a.Representation.Representations}
        gradient = next((r.Items[0] for r in a.Representation.Representations if r.Items[0].is_a("IfcGradientCurve")), None)
        composite = next(r.Items[0] for r in a.Representation.Representations if r.Items[0].is_a("IfcCompositeCurve"))
        evh = w.function_item_evaluator(settings, w.map_shape(settings, composite))
        evg = w.function_item_evaluator(settings, w.map_shape(settings, gradient)) if gradient else None
        dh = dd = dz = 0.0
        per_kind = {}
        for s in e["samples"]:
            m = np.array(evh.evaluate(s["d_m"]))
            err = math.hypot(m[0, 3] * 1000 - s["x_mm"], m[1, 3] * 1000 - s["y_mm"])
            heading = math.atan2(m[1, 0], m[0, 0])
            derr = abs(math.atan2(math.sin(heading - s["heading"]), math.cos(heading - s["heading"])))
            dh = max(dh, err)
            dd = max(dd, derr)
            per_kind[s["kind"]] = max(per_kind.get(s["kind"], 0.0), err)
            if evg is not None and s["z_mm"] is not None:
                mg = np.array(evg.evaluate(s["d_m"]))
                dz = max(dz, abs(mg[2, 3] * 1000 - s["z_mm"]))
                check(abs(mg[0, 3] * 1000 - s["x_mm"]) < 1e-2 and abs(mg[1, 3] * 1000 - s["y_mm"]) < 1e-2, f"{e['name']}: the gradient curve is off the plan curve at {s['d_m']}")
        for s in e["vertical_samples"]:
            mg = np.array(evg.evaluate(s["d_m"]))
            dz = max(dz, abs(mg[2, 3] * 1000 - s["z_mm"]))
            grade = mg[2, 0] / math.hypot(mg[0, 0], mg[1, 0])
            check(abs(grade - s["grade"]) < 1e-6, f"{e['name']}: gradient {grade} vs {s['grade']} at {s['d_m']}")
        out[e["name"]] = {"plan_max_mm": dh, "direction_max_rad": dd, "level_max_mm": dz, "plan_max_mm_by_kind": per_kind, "samples": len(e["samples"]), "vertical_samples": len(e["vertical_samples"])}
        check(dh < expect["tolerances"]["plan_mm"], f"{e['name']}: plan position differs by {dh} mm")
        check(dd < expect["tolerances"]["direction_rad"], f"{e['name']}: direction differs by {dd} rad")
        check(dz < expect["tolerances"]["level_mm"], f"{e['name']}: level differs by {dz} mm")
        # lengths
        check(abs(evh.evaluate(0.0)[0][3] * 1000 - e["samples"][0]["x_mm"]) < 1e-3, "start")
    figures["curve_evaluation"] = out


def recover(referents, station_mm, length_mm):
    """Every distance along at which a displayed station occurs, from the stationing referents
    alone: a referent starts a region whose displayed station is Station at its distance along
    and runs up (or down, after HasIncreasingStation = False) metre for metre to the next one."""
    regions = []
    sigma = 1.0
    for r in referents:
        ps = ifcopenshell.util.element.get_pset(r, "Pset_Stationing")
        if ps.get("HasIncreasingStation") is not None:
            sigma = 1.0 if ps["HasIncreasingStation"] else -1.0
        regions.append((r.ObjectPlacement.RelativePlacement.Location.DistanceAlong.wrappedValue, ps["Station"], sigma))
    out = []
    for i, (d, s, sg) in enumerate(regions):
        end = regions[i + 1][0] if i + 1 < len(regions) else float("inf")
        advance = sg * (station_mm - s)
        if -1e-9 <= advance <= end - d + 1e-9 and d + advance <= length_mm + 1e-9:
            out.append(d + advance)
    # the point reached from both sides of an equation counts once
    dedup = []
    for v in out:
        if dedup and abs(dedup[-1] - v) < 1e-9:
            dedup.pop()
        dedup.append(v)
    return dedup


def derived_curves(path, expect):
    """The business logic and the geometry say the same thing: IfcOpenShell's own mapping of each
    IfcAlignmentHorizontalSegment / IfcAlignmentVerticalSegment to an IfcCurveSegment
    (ifcopenshell.api.alignment, the mapping its alignment API uses to build the geometry from
    the semantic definition) is applied to the segments the file nests, and the curves it gives
    are evaluated at the sample distances and compared with the alignment crate."""
    from ifcopenshell.api.alignment._map_alignment_horizontal_segment import _map_alignment_horizontal_segment
    from ifcopenshell.api.alignment._map_alignment_vertical_segment import _map_alignment_vertical_segment

    f = ifcopenshell.open(path)
    settings = ifcopenshell.geom.settings()
    out = {}
    for a, e in zip(f.by_type("IfcAlignment"), expect["alignments"]):
        layouts = {l.is_a(): l for n in a.IsNestedBy for l in n.RelatedObjects if l.is_a("IfcAlignmentHorizontal") or l.is_a("IfcAlignmentVertical")}
        hsegs = [x for n in layouts["IfcAlignmentHorizontal"].IsNestedBy for x in n.RelatedObjects]
        mapped = []
        for x in hsegs:
            cs = _map_alignment_horizontal_segment(f, x)[0]
            cs.Transition = "DISCONTINUOUS" if x is hsegs[-1] else "CONTSAMEGRADIENT"
            mapped.append(cs)
        composite = f.createIfcCompositeCurve(mapped, False)
        evh = w.function_item_evaluator(settings, w.map_shape(settings, composite))
        worst = 0.0
        for smp in e["samples"]:
            m = np.array(evh.evaluate(smp["d_m"]))
            worst = max(worst, math.hypot(m[0, 3] * 1000 - smp["x_mm"], m[1, 3] * 1000 - smp["y_mm"]))
        worst_z = 0.0
        if "IfcAlignmentVertical" in layouts:
            vsegs = [x for n in layouts["IfcAlignmentVertical"].IsNestedBy for x in n.RelatedObjects]
            vm = []
            for x in vsegs:
                cs = _map_alignment_vertical_segment(f, x)[0]
                cs.Transition = "DISCONTINUOUS" if x is vsegs[-1] else "CONTSAMEGRADIENT"
                vm.append(cs)
            gradient = f.createIfcGradientCurve(vm, False, composite)
            evg = w.function_item_evaluator(settings, w.map_shape(settings, gradient))
            for smp in e["vertical_samples"]:
                m = np.array(evg.evaluate(smp["d_m"]))
                worst_z = max(worst_z, abs(m[2, 3] * 1000 - smp["z_mm"]))
        out[e["name"]] = {"plan_max_mm": worst, "level_max_mm": worst_z}
        check(worst < expect["tolerances"]["plan_mm"], f"{e['name']}: curves derived from the design parameters differ by {worst} mm in plan")
        check(worst_z < expect["tolerances"]["level_mm"], f"{e['name']}: curves derived from the design parameters differ by {worst_z} mm in level")
    figures["curves_derived_from_the_design_parameters"] = out


def stations(f, expect):
    out = {}
    for a, e in zip(f.by_type("IfcAlignment"), expect["alignments"]):
        nest = al.get_stationing_nest(f, a)
        referents = list(nest.RelatedObjects)
        own_wrong = 0
        os_agree = 0
        os_extrapolated = 0
        os_differ = []
        for s in e["displayed"]:
            want = s["candidates_mm"]
            mine = recover(referents, s["displayed_mm"], e["length_mm"])
            if len(mine) != len(want) or any(abs(x - y) > 1e-5 for x, y in zip(mine, want)):
                own_wrong += 1
                print("   displayed station not recovered from the referents:", s, mine)
            got = al.distance_along_from_station(f, a, s["displayed_mm"])
            expected_os = want[-1] if want else None
            if got is not None and got > e["length_mm"] + 1e-6:
                # The function does not know where the alignment ends: a station that is only in
                # an earlier region is placed by the last referent, beyond the end.
                os_extrapolated += 1
            elif (got is None and expected_os is None) or (got is not None and expected_os is not None and abs(got - expected_os) < 1e-5):
                os_agree += 1
            else:
                os_differ.append({"displayed_mm": s["displayed_mm"], "candidates_mm": want, "ifcopenshell_mm": got})
        out[e["name"]] = {"stations": len(e["displayed"]), "recovered_from_referents": len(e["displayed"]) - own_wrong, "ifcopenshell_agrees": os_agree, "ifcopenshell_extrapolates_beyond_the_end": os_extrapolated, "ifcopenshell_differs": os_differ}
        check(own_wrong == 0, f"{e['name']}: {own_wrong} displayed station(s) cannot be recovered from the written referents")
        if not e["decreasing"]:
            # ifcopenshell.api.alignment.distance_along_from_station reads gaps, overlaps and
            # increasing runs correctly; with a decreasing run its search picks the wrong
            # referent for stations before it, which is a limit of that function, not of the data.
            check(not os_differ, f"{e['name']}: IfcOpenShell resolves {len(os_differ)} displayed station(s) differently: {os_differ[:3]}")
        check(abs(al.get_alignment_start_station(f, a) - e["start_station_m"] * 1000.0) < 1e-6, f"{e['name']}: start station")
    figures["displayed_stations"] = out


# --------------------------------------------------------------------------- placements


def world(p):
    return ifcopenshell.util.placement.get_local_placement(p.ObjectPlacement) if p.ObjectPlacement else None


def placements(fl, fa, expect):
    by_guid_a = {p.GlobalId: p for p in fa.by_type("IfcProduct") if p.ObjectPlacement}
    worst_t = worst_r = worst_fallback = 0.0
    compared_r = 0
    linear = 0
    compared = 0
    for p in fl.by_type("IfcProduct"):
        if not p.ObjectPlacement or p.is_a("IfcAlignmentSegment") or p.is_a("IfcReferent"):
            continue
        q = by_guid_a.get(p.GlobalId)
        check(q is not None, f"{p.Name}: not in the absolute file")
        if q is None:
            continue
        ml, ma = world(p), world(q)
        compared += 1
        worst_t = max(worst_t, float(np.abs(ml[:3, 3] - ma[:3, 3]).max()))
        if not p.ObjectPlacement.is_a("IfcLinearPlacement"):
            # The frame of a linearly placed product is the frame of the curve: its geometry
            # (and the placements of its parts) are turned to compensate, which the bounding
            # boxes and the parts' world placements show. Everything else keeps its frame.
            worst_r = max(worst_r, float(np.abs(ml[:3, :3] - ma[:3, :3]).max()))
            compared_r += 1
        if p.ObjectPlacement.is_a("IfcLinearPlacement"):
            linear += 1
            cp = p.ObjectPlacement.CartesianPosition
            check(cp is not None, f"{p.Name}: no CartesianPosition fallback")
            if cp is not None:
                # The fallback stands for the RelativePlacement: the frame the linear placement evaluates to.
                mc = ifcopenshell.util.placement.get_axis2placement(cp)
                worst_fallback = max(worst_fallback, float(np.abs(mc[:3, 3] - ml[:3, 3]).max()), float(np.abs(mc[:3, :3] - ml[:3, :3]).max()))
            check(p.ObjectPlacement.PlacementRelTo is not None and p.ObjectPlacement.PlacementRelTo.PlacesObject[0].is_a("IfcAlignment"), f"{p.Name}: PlacementRelTo is not the alignment's placement")
            a = p.ObjectPlacement.PlacementRelTo.PlacesObject[0]
            pbde = p.ObjectPlacement.RelativePlacement.Location
            check(pbde.is_a("IfcPointByDistanceExpression") and pbde.BasisCurve.is_a() in ("IfcGradientCurve", "IfcCompositeCurve"), f"{p.Name}: basis curve {pbde.BasisCurve.is_a()}")
            check(p.ObjectPlacement.RelativePlacement.Axis is None and p.ObjectPlacement.RelativePlacement.RefDirection is None, f"{p.Name}: Axis / RefDirection are given")
    names_linear = sorted(p.Name for p in fl.by_type("IfcProduct") if p.ObjectPlacement and p.ObjectPlacement.is_a("IfcLinearPlacement") and not p.is_a("IfcReferent"))
    if expect["linear_names"] is not None:
        check(names_linear == sorted(expect["linear_names"]), f"linearly placed products {names_linear}, expected {sorted(expect['linear_names'])}")
    else:
        check(len(names_linear) > 0, "no product is placed linearly")
    figures["placements"] = {"products_compared": compared, "linear": linear, "worst_translation_mm": worst_t, "products_not_linear_compared_for_rotation": compared_r, "worst_rotation_entry_not_linear": worst_r, "worst_fallback": worst_fallback}
    tol = expect["tolerances"]["placement_mm"]
    check(worst_t < tol, f"world positions differ by {worst_t} mm")
    check(worst_r < 1e-9, f"world rotations differ by {worst_r}")
    check(worst_fallback < tol, f"the CartesianPosition fallbacks differ by {worst_fallback}")


def independent_frames(fl, fa):
    """Rebuilds each linear placement from the curve the file names, with IfcOpenShell's curve
    evaluator for position and tangent only and this script's own reading of the offsets, and
    compares the point with the absolute file."""
    settings = ifcopenshell.geom.settings()
    cache = {}
    worst = 0.0
    n = 0
    by_guid_a = {p.GlobalId: p for p in fa.by_type("IfcProduct") if p.ObjectPlacement}
    for p in fl.by_type("IfcProduct"):
        lp = p.ObjectPlacement
        if lp is None or not lp.is_a("IfcLinearPlacement") or p.is_a("IfcReferent"):
            continue
        loc = lp.RelativePlacement.Location
        curve = loc.BasisCurve
        if curve.id() not in cache:
            cache[curve.id()] = w.function_item_evaluator(settings, w.map_shape(settings, curve))
        m = np.array(cache[curve.id()].evaluate(loc.DistanceAlong.wrappedValue / 1000.0))  # metres
        origin = m[:3, 3] * 1000.0
        t = m[:3, 0] / np.linalg.norm(m[:3, 0])
        left = np.cross([0.0, 0.0, 1.0], t)
        left /= np.linalg.norm(left)
        nrm = np.cross(t, left)
        point = origin + (loc.OffsetLateral or 0.0) * left + (loc.OffsetVertical or 0.0) * nrm + (loc.OffsetLongitudinal or 0.0) * t
        ma = world(by_guid_a[p.GlobalId])
        worst = max(worst, float(np.abs(point - ma[:3, 3]).max()))
        n += 1
    figures["independent_frames"] = {"placements": n, "worst_mm": worst}
    return worst


def boxes_of(f):
    s = ifcopenshell.geom.settings()
    s.set("use-world-coords", True)
    out = {}
    it = ifcopenshell.geom.iterator(s, f, 1)
    if it.initialize():
        while True:
            shape = it.get()
            v = np.array(shape.geometry.verts).reshape(-1, 3) * 1000.0
            if len(v):
                out[shape.guid] = (v.min(axis=0), v.max(axis=0), len(shape.geometry.faces) // 3)
            if not it.next():
                break
    return out


def compare_boxes(bl, ba, label):
    check(set(bl) == set(ba), f"{label}: shaped products differ: {len(bl)} vs {len(ba)}")
    worst = 0.0
    tri = 0
    for g in set(bl) & set(ba):
        worst = max(worst, float(np.abs(bl[g][0] - ba[g][0]).max()), float(np.abs(bl[g][1] - ba[g][1]).max()))
        tri += int(bl[g][2] != ba[g][2])
    figures[label] = {"products": len(bl), "worst_mm": worst, "triangle_count_differences": tri}
    check(tri == 0, f"{label}: {tri} product(s) have a different triangle count")
    return worst


def bounding_boxes(fl, fa):
    return compare_boxes(boxes_of(fl), boxes_of(fa), "bounding_boxes")


def fallback_boxes(linear_path, fa):
    """Replaces every linear placement by a local placement at its CartesianPosition, as a tool
    that does not evaluate linear placement would read it, and compares the kernel's world boxes
    with the absolute file."""
    f = ifcopenshell.open(linear_path)
    for lp in list(f.by_type("IfcLinearPlacement")):
        local = f.createIfcLocalPlacement(lp.PlacementRelTo, lp.CartesianPosition)
        for user in list(f.get_inverse(lp)):
            for i in range(len(user)):
                if user[i] == lp:
                    user[i] = local
    check(not f.by_type("IfcLinearPlacement") or all(not f.get_inverse(lp) for lp in f.by_type("IfcLinearPlacement")), "linear placements remain in use")
    # referents are placed linearly too and have no shapes; products only matter here
    return compare_boxes(boxes_of(f), boxes_of(fa), "fallback_boxes")


def main():
    linear_path, absolute_path, expect_path = sys.argv[1:4]
    expect = json.load(open(expect_path))
    fl = ifcopenshell.open(linear_path)
    fa = ifcopenshell.open(absolute_path)
    validate(fl, "linear")
    validate(fa, "geometry")
    structure(fl, expect)
    service_rules(fl)
    curve_evaluation(fl, expect)
    stations(fl, expect)
    derived_curves(linear_path, expect)
    placements(fl, fa, expect)
    worst_independent = independent_frames(fl, fa)
    check(worst_independent < expect["tolerances"]["placement_mm"], f"independent frames differ by {worst_independent} mm")
    worst_box = bounding_boxes(fl, fa)
    check(worst_box < expect["tolerances"]["placement_mm"], f"bounding boxes differ by {worst_box} mm")
    worst_fallback_box = fallback_boxes(linear_path, fa)
    check(worst_fallback_box < expect["tolerances"]["placement_mm"], f"bounding boxes through the CartesianPosition fallback differ by {worst_fallback_box} mm")
    figures["ifcopenshell"] = ifcopenshell.version
    print(json.dumps(figures, indent=1))
    if len(sys.argv) > 4:
        json.dump(figures, open(sys.argv[4], "w"), indent=1)
    if failures:
        print(f"{len(failures)} check(s) failed")
        sys.exit(1)
    print("all checks passed")


if __name__ == "__main__":
    main()
