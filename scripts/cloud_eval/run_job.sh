#!/bin/bash
# Submit a job to a running `cloud_capture OUT SEED WIDTH --watch OUT/job.json`
# server and wait for it. usage: run_job.sh OUT TAG [SHADER] [extra json]
# e.g. run_job.sh out v26 src/shaders/cloud_synthesis.wgsl ',"width":8192,"rows":[1200,512]'
OUT=$1; TAG=$2; SH=${3:-src/shaders/cloud_synthesis.wgsl}; EXTRA=${4:-}
n0=$(wc -l < "$OUT/log.txt")
echo "{\"tag\":\"$TAG\",\"shader\":\"$(realpath "$SH")\"$EXTRA}" > "$OUT/job.json"
for i in $(seq 1 1200); do
  if tail -n +$((n0+1)) "$OUT/log.txt" | grep -qE "job done $TAG|shader error|panicked"; then break; fi
  sleep 0.5
done
tail -n +$((n0+1)) "$OUT/log.txt" | grep -E "error|panicked|columns_ms"
