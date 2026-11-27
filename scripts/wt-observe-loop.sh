#!/usr/bin/env bash
# Linux orchestrator: launch+watch a play on the connected Windows agent via wt,
# pull status/screenshots/crash text. Exit 0 only on PASS.
set -euo pipefail
wt() { python3 /home/founder/prod/windows-toolkit/client/wt.py "$@"; }
TAG="${1:-$(date -u +%Y%m%d%H%M%S)}"
TIMEOUT="${TIMEOUT_SEC:-300}"
# Optional: NO_DEBUG=1 to pass -NoDebug (faster functional pass)
REMOTE_PS1='C:\Users\ifix4\AppData\Local\Ad3lHub\wt-play-observe.ps1'
REMOTE_OUT="C:\\Users\\ifix4\\AppData\\Local\\Ad3lHub\\observe"
LOCAL_OUT="/tmp/wt-observe-$TAG"
mkdir -p "$LOCAL_OUT"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
wt put "$ROOT/scripts/wt-play-observe.ps1" "$REMOTE_PS1" >/dev/null

NODEBUG_ARG=""
if [[ "${NO_DEBUG:-0}" == "1" ]]; then NODEBUG_ARG=" -NoDebug"; fi

# Start observe in the interactive user session (session 0 cannot create a swap chain).
wt exec --timeout 45 @ "
Remove-Item '$REMOTE_OUT\\done-$TAG.txt','$REMOTE_OUT\\status-$TAG.json' -ErrorAction SilentlyContinue
Unregister-ScheduledTask -TaskName 'VictoryEvidenceInt' -Confirm:\$false -ErrorAction SilentlyContinue
\$arg = '-NoProfile -ExecutionPolicy Bypass -File $REMOTE_PS1 -TimeoutSec $TIMEOUT -Tag $TAG$NODEBUG_ARG'
\$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument \$arg
\$trigger = New-ScheduledTaskTrigger -Once -At ((Get-Date).AddMinutes(-1))
\$principal = New-ScheduledTaskPrincipal -UserId 'ifix4' -LogonType Interactive -RunLevel Highest
Register-ScheduledTask -TaskName 'VictoryEvidenceInt' -Action \$action -Trigger \$trigger -Principal \$principal -Force | Out-Null
Start-ScheduledTask -TaskName 'VictoryEvidenceInt'
Start-Sleep -Seconds 3
'started interactive'
" >/dev/null

echo "[observe] tag=$TAG timeout=${TIMEOUT}s"
deadline=$((SECONDS + TIMEOUT + 60))
while (( SECONDS < deadline )); do
  if wt get "$REMOTE_OUT\\done-$TAG.txt" "$LOCAL_OUT/done.txt" >/dev/null 2>&1; then
    break
  fi
  sleep 10
done

wt get "$REMOTE_OUT\\status-$TAG.json" "$LOCAL_OUT/status.json" >/dev/null 2>&1 || true
wt get "$REMOTE_OUT\\launch-$TAG.log" "$LOCAL_OUT/launch.log" >/dev/null 2>&1 || true
echo "[observe] $(cat "$LOCAL_OUT/done.txt" 2>/dev/null || echo 'NO_DONE')"
if [[ -f "$LOCAL_OUT/status.json" ]]; then
  cat "$LOCAL_OUT/status.json"
  echo
fi
grep -E 'PASS|CRASH|FAIL|TIMEOUT|WORLD|D3D12 debug|did not close|device removed' "$LOCAL_OUT/launch.log" 2>/dev/null | tail -40 || true

if grep -q '^PASS' "$LOCAL_OUT/done.txt" 2>/dev/null; then
  exit 0
fi
exit 1
