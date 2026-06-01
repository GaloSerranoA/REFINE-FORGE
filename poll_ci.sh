#!/usr/bin/env bash
SHA="${1:?usage: poll_ci.sh <sha>}"
REPO=GaloSerranoA/REFINE-FORGE
API="https://api.github.com/repos/$REPO/actions"
for i in $(seq 1 40); do
  J=$(curl -s "$API/runs?head_sha=$SHA&per_page=10")
  SUMMARY=$(echo "$J" | python3 -c "import sys,json
d=json.load(sys.stdin); rs=d.get('workflow_runs',[])
print(', '.join('%s:%s/%s'%(r['id'],r['status'],r['conclusion']) for r in rs) or 'no-runs')")
  echo "poll $i: $SUMMARY"
  DONE=$(echo "$J" | python3 -c "import sys,json
d=json.load(sys.stdin); rs=d.get('workflow_runs',[])
print('1' if rs and all(r['status']=='completed' for r in rs) else '0')")
  if [ "$DONE" = "1" ]; then
    echo "=== ALL RUNS COMPLETED for $SHA ==="
    for RID in $(echo "$J" | python3 -c "import sys,json
d=json.load(sys.stdin); print(' '.join(str(r['id']) for r in d.get('workflow_runs',[])))"); do
      echo "--- run $RID ---"
      curl -s "$API/runs/$RID/jobs" | python3 -c "import sys,json
d=json.load(sys.stdin)
[print('  [%s] %s'%(j['conclusion'],j['name'])) for j in d.get('jobs',[])]"
    done
    break
  fi
  sleep 78
done
