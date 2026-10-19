#!/usr/bin/env python3
"""Validate checkmydrive-engine NDJSON test output: every line parses, no error event,
exactly one report event with score 0-100 and a verdict. Prints a summary."""
import json
import sys

path = sys.argv[1]
reports, errors, progress, bad = [], [], 0, 0
with open(path, encoding="utf-8") as f:
    for n, line in enumerate(f, 1):
        line = line.strip()
        if not line:
            continue
        try:
            ev = json.loads(line)
        except ValueError:
            bad += 1
            print(f"line {n} is not JSON: {line[:200]}")
            continue
        t = ev.get("type")
        if t == "report":
            reports.append(ev["report"])
        elif t == "error":
            errors.append(ev)
        elif t == "progress":
            progress += 1

print(f"progress events: {progress}, reports: {len(reports)}, errors: {len(errors)}, bad lines: {bad}")
fail = False
if bad:
    fail = True
if errors:
    print("ERROR events:", json.dumps(errors))
    fail = True
if len(reports) != 1:
    print("expected exactly one report event")
    fail = True
else:
    r = reports[0]
    score, verdict = r.get("score"), r.get("verdict")
    print(f"score={score} verdict={verdict} mode={r.get('mode')} drive={r['drive'].get('id')}")
    for t in r.get("tests", []):
        print(f"  {t['id']:<15} {t['status']:<8} score={t.get('score')}")
    print(json.dumps(r.get("issues", []))[:1000])
    if not isinstance(score, (int, float)) or not 0 <= score <= 100:
        print("score out of range")
        fail = True
    if verdict not in ("buy", "caution", "avoid"):
        print("bad verdict")
        fail = True
sys.exit(1 if fail else 0)
