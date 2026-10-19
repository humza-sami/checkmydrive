#!/usr/bin/env python3
"""Read `list` JSON on stdin; print the id of the non-system drive whose capacity is closest to
the wanted size in bytes (argv[1]), only if within 15 percent."""
import json
import sys

want = int(sys.argv[1])
best = None
for d in json.load(sys.stdin):
    if d.get("is_system_disk"):
        continue
    diff = abs(d["capacity_bytes"] - want)
    if diff <= want * 0.15 and (best is None or diff < best[0]):
        best = (diff, d["id"])
if best is None:
    sys.exit("no matching drive in list output")
print(best[1])
