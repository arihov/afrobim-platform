"""Reads an XER file with the third-party `xerparser` package (jjCode01/xerparser, PyPI `xerparser`)
and prints what it found as JSON, for the Rust test `xer_file_parses_with_xerparser`.

Usage: python3 -I xerparser_check.py FILE.xer
Exits with status 3 when the package is not installed, so the test can skip.
"""
import json
import sys

try:
    from xerparser import Xer
except ImportError:
    sys.exit(3)

try:
    xer = Xer.reader(sys.argv[1])
except AttributeError:
    # Another package that is also called xerparser (PyP6XER): not the one this check is for.
    sys.exit(3)

proj = list(xer.projects.values())[0]


def day(d):
    return d.strftime("%Y-%m-%d") if d else None


tasks = []
for t in proj.tasks:
    tasks.append(
        {
            "code": t.task_code,
            "name": t.name,
            "type": t.type.name,
            "status": t.status.name,
            "target_hours": t.target_drtn_hr_cnt,
            "remain_hours": t.remain_drtn_hr_cnt,
            "start": day(t.start),
            "finish": day(t.finish),
            "cstr_type": t.cstr_type,
            "cstr_date": day(t.cstr_date),
            "cstr_type2": t.cstr_type2,
            "cstr_date2": day(t.cstr_date2),
            "wbs": t.wbs.full_code,
            "calendar": t.calendar.name,
            "percent": t.phys_complete_pct,
        }
    )
rels = [
    {"pred": r.predecessor.task_code, "succ": r.successor.task_code, "link": r.link, "lag_hours": r.lag_hr_cnt}
    for r in proj.relationships
]
print(
    json.dumps(
        {
            "name": proj.name,
            "short_name": proj.short_name,
            "data_date": day(proj.data_date),
            "tasks": tasks,
            "relationships": rels,
            "wbs": sorted(n.full_code for n in proj.wbs_nodes if not n.is_proj_node),
            "calendars": sorted(c.name for c in proj.calendars),
            "holidays": {c.name: [day(h) for h in c.holidays] for c in proj.calendars},
        }
    )
)
