#!/usr/bin/env bash
# Install-and-smoke-test the CheckMyDrive .deb across Linux distros in Docker.
#
# Usage: scripts/test-docker.sh [-o OUTDIR] [-d img1,img2,...] /path/to/CheckMyDrive_x.deb
#   -o  output directory (default: ./docker-test-out/<timestamp>)
#   -d  comma-separated subset of images to run (default: all in DISTROS below)
#
# Each image runs in a fresh --privileged container (amd64, emulated on Apple Silicon).
# Per distro, OUTDIR/<slug>/ gets: results.tsv (step, status, note), numbered step logs,
# container.log, and gui.png (the GUI screenshot). A summary table is printed and written
# to OUTDIR/summary.txt. Exit status is 1 if any step FAILs (SKIP and NOTE do not fail).
#
# Docker: the default socket may belong to another user, so DOCKER_HOST defaults to the
# current user's socket unless already set.
set -u

usage() { echo "usage: $0 [-o OUTDIR] [-d img1,img2,...] /path/to/CheckMyDrive_x.deb" >&2; exit 2; }

OUT=""
ONLY=""
while getopts "o:d:h" opt; do
  case "$opt" in
    o) OUT="$OPTARG" ;;
    d) ONLY="$OPTARG" ;;
    *) usage ;;
  esac
done
shift $((OPTIND - 1))
[ $# -eq 1 ] || usage
DEB_IN="$1"
[ -f "$DEB_IN" ] || { echo "no such file: $DEB_IN" >&2; exit 2; }
DEB="$(cd "$(dirname "$DEB_IN")" && pwd)/$(basename "$DEB_IN")"

export DOCKER_HOST="${DOCKER_HOST:-unix://$HOME/.docker/run/docker.sock}"
docker info >/dev/null 2>&1 || { echo "docker daemon not reachable at $DOCKER_HOST" >&2; exit 2; }

[ -n "$OUT" ] || OUT="$(pwd)/docker-test-out/$(date +%Y%m%d-%H%M%S)"
mkdir -p "$OUT/_stage"
cp "$DEB" "$OUT/_stage/"
OUT="$(cd "$OUT" && pwd)"

DISTROS="ubuntu:22.04 ubuntu:24.04 ubuntu:25.04 ubuntu:25.10 debian:12 debian:13 linuxmintd/mint22-amd64 kalilinux/kali-rolling fedora:41"
# No official Docker images exist for these derivatives; reported as SKIP.
NO_IMAGE="elementary-os(no official image) pop-os(no official image)"

# ---------------------------------------------------------------------------
# Container script for apt-based distros. Runs inside the container as root.
# /deb holds the staged .deb (read-only); /out is the per-distro output dir.
# ---------------------------------------------------------------------------
read -r -d '' APT_SCRIPT <<'CONTAINER_EOF'
export DEBIAN_FRONTEND=noninteractive
OUT=/out
RES=$OUT/results.tsv
: > "$RES"
rec() { # step status note
  local n
  n=$(printf '%s' "$3" | tr '\t\n' '  ')
  printf '%s\t%s\t%s\n' "$1" "$2" "$n" >> "$RES"
  echo "[$1] $2 - $n"
}
cat > /tmp/vlist.py <<'PYEOF'
import json, sys
raw = open(sys.argv[1]).read().strip()
try:
    d = json.loads(raw)
    if not isinstance(d, list):
        raise SystemExit("top-level JSON is not an array")
    mode = "json-array"
except json.JSONDecodeError:
    d = [json.loads(l) for l in raw.splitlines() if l.strip()]
    mode = "ndjson-lines"
print(f"mode={mode} drives={len(d)}")
for x in d:
    print(f"id={x.get('id')} kind={x.get('kind')} capacity={x.get('capacity_bytes')} system={x.get('is_system_disk')} model={x.get('model')}")
PYEOF
cat > /tmp/vreport.py <<'PYEOF'
import json, sys
lines = [l for l in open(sys.argv[1]).read().splitlines() if l.strip()]
events = []
for i, l in enumerate(lines):
    try:
        events.append(json.loads(l))
    except Exception as e:
        print(f"BAD_JSON line {i+1}: {e}")
        sys.exit(2)
counts = {}
for e in events:
    counts[e.get("type")] = counts.get(e.get("type"), 0) + 1
print("events=" + json.dumps(counts))
if not events:
    print("NO_EVENTS")
    sys.exit(3)
last = events[-1]
if last.get("type") == "error":
    print("ENGINE_ERROR code=" + str(last.get("code")) + " params=" + json.dumps(last.get("params")))
    sys.exit(4)
if last.get("type") != "report":
    print("LAST_EVENT_NOT_REPORT " + json.dumps(last))
    sys.exit(5)
r = last["report"]
s = r.get("score")
v = r.get("verdict")
print(f"score={s} verdict={v} tests=" + ",".join(f"{t['id']}:{t['status']}" for t in r.get("tests", [])))
ok = isinstance(s, int) and 0 <= s <= 100 and v in ("buy", "caution", "avoid")
sys.exit(0 if ok else 6)
PYEOF

DEBF=$(ls /deb/*.deb 2>/dev/null | head -n 1)
if [ -z "$DEBF" ]; then
  rec install FAIL "no .deb staged at /deb"
  exit 0
fi
PKG=$(dpkg-deb -f "$DEBF" Package 2>/dev/null)
ENG=/usr/bin/checkmydrive-engine

# 1. install with apt (dependency resolution only; no extra packages)
apt-get update -o Acquire::Retries=3 > $OUT/01-apt-update.log 2>&1 || echo "apt-get update exit $?" >> $OUT/01-apt-update.log
apt-get install -y "$DEBF" > $OUT/01-install.log 2>&1
RC=$?
if [ $RC -ne 0 ]; then
  rec install FAIL "apt-get install exit $RC (see 01-install.log)"
  for s in version list loop-test disk-test gui remove; do rec $s SKIP "install failed"; done
  exit 0
fi
SMV=$(dpkg-query -W -f='${Version}' smartmontools 2>/dev/null)
ENGV=$(dpkg-query -W -f='${Version}' "$PKG" 2>/dev/null)
rec install PASS "$PKG $ENGV installed; smartmontools=${SMV:-none}"

# python3 is only used by this harness to validate JSON output.
apt-get install -y python3 > $OUT/02-python3.log 2>&1 || echo "python3 install failed" >> $OUT/02-python3.log

# 2. version
if [ ! -x "$ENG" ]; then
  rec version FAIL "$ENG missing after install"
else
  $ENG version > $OUT/02-version.out 2> $OUT/02-version.err; RC=$?
  VOUT=$(cat $OUT/02-version.out)
  if [ $RC -eq 0 ] && python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); assert "version" in d and "smartctl" in d' $OUT/02-version.out; then
    rec version PASS "rc=0 $VOUT"
  else
    rec version FAIL "rc=$RC stdout=$VOUT stderr=$(head -c 300 $OUT/02-version.err)"
  fi
fi

# 3. list (as root)
$ENG list > $OUT/03-list.out 2> $OUT/03-list.err; RC=$?
if [ $RC -ne 0 ]; then
  rec list FAIL "rc=$RC stderr=$(head -c 300 $OUT/03-list.err)"
else
  LSUM=$(python3 /tmp/vlist.py $OUT/03-list.out 2>&1); VRC=$?
  if [ $VRC -eq 0 ]; then rec list PASS "$LSUM"
  else rec list FAIL "validator rc=$VRC: $LSUM"; fi
fi

# 4. safe test against a 1 GiB loop device
if [ ! -e /dev/loop-control ]; then mknod -m 660 /dev/loop-control c 10 237 2>/dev/null; fi
for i in 0 1 2 3 4 5 6 7; do [ -e /dev/loop$i ] || mknod -m 660 /dev/loop$i b 7 $i 2>/dev/null; done
truncate -s 1G /tmp/d.img
LOOP=$(losetup -fP --show /tmp/d.img 2> $OUT/04-losetup.err); LRC=$?
if [ $LRC -ne 0 ]; then
  rec loop-test FAIL "losetup exit $LRC: $(cat $OUT/04-losetup.err)"
else
  $ENG test --drive "$LOOP" --mode safe > $OUT/04-test.ndjson 2> $OUT/04-test.err; TRC=$?
  VR=$(python3 /tmp/vreport.py $OUT/04-test.ndjson 2>&1); VRC=$?
  if [ $VRC -eq 0 ]; then
    rec loop-test PASS "$LOOP engine rc=$TRC $VR"
  else
    rec loop-test FAIL "$LOOP engine rc=$TRC validator rc=$VRC: $VR; stderr=$(head -c 300 $OUT/04-test.err)"
  fi
  losetup -d "$LOOP" 2>/dev/null
fi
rm -f /tmp/d.img

# 4b. extra probe: safe test on a real non-system, non-loop disk the engine lists (if any),
# so the engine still produces a real report when loop devices are invisible to it.
CAND=$(python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); c=[x["id"] for x in d if not x.get("is_system_disk") and not str(x.get("path","")).startswith("/dev/loop")]; print(c[0] if c else "")' $OUT/03-list.out 2>/dev/null)
if [ -z "$CAND" ]; then
  rec disk-test SKIP "no non-system, non-loop disk listed by engine"
else
  $ENG test --drive "$CAND" --mode safe > $OUT/04b-test.ndjson 2> $OUT/04b-test.err; TRC=$?
  VR=$(python3 /tmp/vreport.py $OUT/04b-test.ndjson 2>&1); VRC=$?
  if [ $VRC -eq 0 ]; then
    rec disk-test PASS "$CAND engine rc=$TRC $VR"
  else
    rec disk-test FAIL "$CAND engine rc=$TRC validator rc=$VRC: $VR; stderr=$(head -c 300 $OUT/04b-test.err)"
  fi
fi

# 5. GUI smoke test under Xvfb
apt-get install -y xvfb imagemagick dbus-x11 > $OUT/05-gui-deps.log 2>&1
if [ $? -ne 0 ]; then
  rec gui FAIL "installing xvfb/imagemagick/dbus-x11 failed (see 05-gui-deps.log)"
else
  Xvfb :99 -screen 0 1024x768x24 -nolisten tcp > $OUT/05-xvfb.log 2>&1 &
  XPID=$!
  sleep 3
  export DISPLAY=:99
  eval "$(dbus-launch --sh-syntax)" 2>/dev/null
  GUI_RESULT=FAIL
  GUI_NOTE="no attempt ran"
  APID=""
  for attempt in A B; do
    if [ $attempt = A ]; then ENVS=""; WAIT=15
    else ENVS="WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1"; WAIT=20; fi
    env $ENVS /usr/bin/checkmydrive > $OUT/05-gui-$attempt.log 2>&1 &
    APID=$!
    sleep $WAIT
    ALIVE=no
    if [ -d /proc/$APID ] && ! grep -q '^State:.*Z' /proc/$APID/status 2>/dev/null; then ALIVE=yes; fi
    import -display :99 -window root $OUT/gui-$attempt.png 2>> $OUT/05-gui-$attempt.log
    SD=$(convert $OUT/gui-$attempt.png -format '%[fx:standard_deviation]' info: 2>/dev/null)
    kill $APID 2>/dev/null; sleep 2; kill -9 $APID 2>/dev/null
    GUI_NOTE="attempt $attempt env=[${ENVS:-none}] alive_after_${WAIT}s=$ALIVE screenshot_stddev=${SD:-n/a}"
    if [ $ALIVE = yes ] && awk "BEGIN{exit !(${SD:-0} > 0.01)}"; then
      GUI_RESULT=PASS
      cp $OUT/gui-$attempt.png $OUT/gui.png
      break
    fi
    [ $ALIVE = no ] && GUI_NOTE="$GUI_NOTE; stderr=$(tail -c 300 $OUT/05-gui-$attempt.log | tr '\n' ' ')"
  done
  rec gui $GUI_RESULT "$GUI_NOTE"
  kill $XPID 2>/dev/null
fi

# 6. remove; the engine must be gone
[ -n "$APID" ] && kill -9 $APID 2>/dev/null
apt-get remove -y "$PKG" > $OUT/06-remove.log 2>&1; RC=$?
if [ $RC -eq 0 ] && [ ! -e "$ENG" ]; then
  rec remove PASS "$PKG removed; $ENG gone"
else
  rec remove FAIL "apt-get remove rc=$RC; engine still present=$([ -e "$ENG" ] && echo yes || echo no) (see 06-remove.log)"
fi
exit 0
CONTAINER_EOF

# ---------------------------------------------------------------------------
# Container script for Fedora: the .deb cannot be installed there, so only the bare
# engine binary is extracted (ar + tar) and `version` is run. Reported as NOTE.
# ---------------------------------------------------------------------------
read -r -d '' FEDORA_SCRIPT <<'CONTAINER_EOF'
OUT=/out
RES=$OUT/results.tsv
: > "$RES"
rec() {
  local n
  n=$(printf '%s' "$3" | tr '\t\n' '  ')
  printf '%s\t%s\t%s\n' "$1" "$2" "$n" >> "$RES"
  echo "[$1] $2 - $n"
}
DEBF=$(ls /deb/*.deb 2>/dev/null | head -n 1)
dnf install -y binutils tar xz zstd > $OUT/f-deps.log 2>&1 || echo "dnf deps failed" >> $OUT/f-deps.log
mkdir -p /tmp/x/root && cd /tmp/x && ar x "$DEBF" && tar -xf data.tar.* -C /tmp/x/root
ENG=/tmp/x/root/usr/bin/checkmydrive-engine
MISSING=$(ldd $ENG 2>&1 | grep 'not found' | tr '\n' ' ')
$ENG version > $OUT/f-version.out 2> $OUT/f-version.err; RC=$?
rec version NOTE "fedora:41 bare engine (no smartmontools, not a .deb install) rc=$RC stdout=$(cat $OUT/f-version.out) stderr=$(head -c 200 $OUT/f-version.err) ldd_missing=[$MISSING]"
exit 0
CONTAINER_EOF

STEPS="install version list loop-test disk-test gui remove"

run_distro() { # $1 = image
  local img="$1" slug d SCRIPT RC
  slug=$(echo "$img" | tr '/:' '__')
  d="$OUT/$slug"
  mkdir -p "$d"
  SCRIPT="$APT_SCRIPT"
  case "$img" in fedora*) SCRIPT="$FEDORA_SCRIPT" ;; esac

  if ! docker image inspect "$img" >/dev/null 2>&1; then
    echo "pulling $img ..."
    if ! docker pull --platform linux/amd64 "$img" > "$d/pull.log" 2>&1; then
      printf 'install\tSKIP\timage not pullable: see pull.log\n' > "$d/results.tsv"
      return
    fi
  fi

  echo "=== $img ==="
  docker run --rm --privileged --platform linux/amd64 \
    -v "$d:/out" -v "$OUT/_stage:/deb:ro" \
    "$img" /bin/bash -c "$SCRIPT" > "$d/container.log" 2>&1
  RC=$?
  if [ ! -s "$d/results.tsv" ]; then
    printf 'install\tFAIL\tcontainer exited rc=%s before writing results; see container.log\n' "$RC" > "$d/results.tsv"
  fi
}

IFS=' ' read -r -a IMAGES <<< "$DISTROS"
for img in "${IMAGES[@]}"; do
  if [ -n "$ONLY" ]; then
    case ",$ONLY," in *",$img,"*) ;; *) continue ;; esac
  fi
  run_distro "$img"
done

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
SUM="$OUT/summary.txt"
FAILS=0
# No pipe here: the counter must update in this shell for the exit status below.
{
  printf '%-28s' "distro"
  for s in $STEPS; do printf '%-11s' "$s"; done
  echo
  for img in "${IMAGES[@]}"; do
    if [ -n "$ONLY" ]; then case ",$ONLY," in *",$img,"*) ;; *) continue ;; esac; fi
    slug=$(echo "$img" | tr '/:' '__')
    d="$OUT/$slug"
    printf '%-28s' "$img"
    for s in $STEPS; do
      st=$(awk -F'\t' -v s="$s" '$1==s{st=$2} END{print st}' "$d/results.tsv" 2>/dev/null)
      [ -n "$st" ] || st="-"
      [ "$st" = "FAIL" ] && FAILS=$((FAILS + 1))
      printf '%-11s' "$st"
    done
    echo
  done
  printf '%-28s%-11s\n' "elementary (no image)" "SKIP"
  printf '%-28s%-11s\n' "pop!_os (no image)" "SKIP"
  echo
  echo "Notes:"
  for img in "${IMAGES[@]}"; do
    if [ -n "$ONLY" ]; then case ",$ONLY," in *",$img,"*) ;; *) continue ;; esac; fi
    slug=$(echo "$img" | tr '/:' '__')
    echo "--- $img  ($OUT/$slug)"
    [ -f "$OUT/$slug/results.tsv" ] && while IFS=$'\t' read -r s st note; do
      echo "  $s $st: $note"
    done < "$OUT/$slug/results.tsv"
  done
  echo
  echo "Not tested: elementary-os and Pop!_OS (no official Docker images)."
  echo "Total FAIL cells: $FAILS"
} > "$SUM"
cat "$SUM"

echo "Output: $OUT"
[ "$FAILS" -eq 0 ] || exit 1
exit 0
