#!/bin/bash
# usage: capture.sh <outfile.png> [wait_secs] — runs app on :0.0, captures root, kills app
OUT=$1; WAIT=${2:-16}
pkill -x planet-gen 2>/dev/null; sleep 1
WGPU_BACKEND=vulkan DISPLAY=:0.0 setsid env WGPU_BACKEND=vulkan ./target/debug/planet-gen > design-captures/run.log 2>&1 < /dev/null &
APID=$!
sleep "$WAIT"
if ! kill -0 $APID 2>/dev/null; then echo "APP_DIED"; tail -8 design-captures/run.log; exit 1; fi
for i in 1 2 3; do
  DISPLAY=:0.0 import -window root "$OUT" 2>/dev/null && break
  sleep 2
done
if [ -s "$OUT" ]; then echo "CAPTURED $OUT ($(stat -c%s "$OUT") bytes)"; else echo "CAPTURE_FAILED"; fi
kill $APID 2>/dev/null; pkill -x planet-gen 2>/dev/null
grep -E "\[terrain|\[wind|panic|GPU" design-captures/run.log | tail -4 || true
