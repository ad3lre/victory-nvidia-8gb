# Anonymous run telemetry

`adel VIctory` sends one anonymous performance report for every
`victory run --execute`. It is **on by default** so frame-rate regressions can
be compared between builds and machines. Reports go to
`https://rxnd.de/vi/v1`; local files are written before upload, so a network
failure leaves the recorded data available locally.

On Windows the recorder runs as a **sibling process** (the same `victory`
executable re-entered with `VICTORY_TELEMETRY_AGENT` pointing at a session
JSON under the telemetry directory). The game only publishes one-second FPS
windows into a named shared-memory ring and signals a named stop event on a
clean exit. The agent owns registration, ping, mid-run and final ingest,
screenshots, and log uploads, and keeps running after a GPU TDR or hard kill
of the game. If the agent cannot be spawned, an in-process thread is used as
a fallback. Hardware/title metadata rides on the opening ping; FPS samples
are ingested about every sixty seconds and again when the agent finishes.
Transient failures are retried a few times; anything still pending stays on
disk (`upload-status-<run_id>.txt` notes incomplete uploads).

## Opting out

Any one of these turns it off completely (no registration, no ping, no upload,
no agent, no screenshots):

```powershell
$env:VICTORY_TELEMETRY = '0'          # environment, highest precedence
victory run ... --no-telemetry        # per-run CLI switch
```

```toml
# victory.toml
[telemetry]
enabled = false
```

`victory config` prints the resolved state (`telemetry : on|off`). The
environment variable takes precedence over `victory.toml`; `--no-telemetry`
takes precedence over both. `VICTORY_TELEMETRY=1` explicitly turns it back on.

## What is sent

The report identifies the **build**, not the player:

| Field | Source |
| --- | --- |
| `fork.name`, `fork.version`, `fork.git_sha` | `adel`, crate version, commit stamped at build time |
| `manifest.title.hash` | BLAKE3 of the original title executable |
| `manifest.title.console` | `Xbox Series X` or `Xbox Series S` |
| `manifest.os` | Windows product name and display version |
| `manifest.cpu` | Processor name and logical core count |
| `manifest.gpu` | Adapter name, driver version, dedicated VRAM |
| `manifest.ram_total_mb`, `manifest.display` | Total physical memory, primary screen width/height |

One sample is recorded for each one-second fps window the GPU present path
already measures (the same numbers shown in the title caption):

- `t_ms` — milliseconds since the recorder started;
- `fps`, `ms`, `draws` — frames/s, ms/frame, draws/frame;
- `cpu_ms`, `gpu_ms` — ms/frame spent preparing submissions and waiting on the
  GPU, from the existing submission counters;
- `after_present_ms`, `until_submit_ms` — ms after present until the title's
  first list call / first submission (Victory frame budget);
- `watch_ms`, `guard_ms`, `guard_faults` — write-watch / guard cost and fault
  rate from the same counters the diagnostics frame line uses;
- `cpu_pct`, `gpu_pct` — system CPU load and summed 3D-engine load, when the
  Windows counters are available;
- `ram_mb` — this process's working set;
- `ram_avail_mb`, `commit_avail_mb` — host physical / commit headroom;
- `page_faults`, `proc_user_ms`, `proc_kernel_ms` — process fault and CPU-time
  deltas for the sample window;
- `cpu_temp_c` — CPU package temperature from Windows thermal zones, when that
  counter exists, plus `cpu_temp_reason` explaining `null` otherwise;
- `gpu_temp_c` — **not collected in v1**: `null` with reason
  `NVAPI temperature reader not enabled in v1`. GPU load and VRAM are present.

The manifest also carries a `launch.switches` fingerprint of active `VICTORY_*`
environment variables (path-like values redacted). Structured `events` may
include `device_removed`. The Victory runtime log
(`…/data/logs/victory-runtime.log`) is uploaded with the other crash/game logs
when present.

The client and server compute summary statistics from these one-second
averages and the server renders an SVG FPS graph. The fields named frame-time
percentiles, 1% low, longest stall and stutter count describe the sampled
window averages; they are not per-frame measurements and can miss short
stalls. They should not be compared with frame-level benchmark results.

### Screenshots

For the first twenty minutes of a run the recorder also captures the
**foreground game window** every ten seconds (up to 120 frames, named
`001.jpg` … `120.jpg`). Each frame is downscaled to at most 480 px on the long
edge and encoded as a low-quality JPEG, then uploaded to
`POST /vi/v1/runs/<id>/screenshots/<nnn>.jpg`. Failed uploads are retried during
the run and again for up to ~90s at exit; anything still pending stays local.
Capture is skipped when the foreground window is not this process, when Windows
cannot provide pixels, or when the bitmap is effectively blank (common for some
exclusive-fullscreen paths). Local copies stay under `screenshots-<run_id>/`.

### Crash and game logs

About every thirty seconds and again when the run ends, the recorder uploads a
bounded set of Victory and title log/crash artifacts (at most 12 files, 4 MiB
each, 16 MiB total) from:

- `crashes.log` next to the title temp directory;
- the title `T:` temp directory (Breakpad `*.crash.log` / `*.dmp`, Victory and
  game `*.log` / `*.txt` whose names mention crash, victory, gta, rockstar,
  rage, script, error, and similar);
- the title package directory (same name filters, top level only).

Uploads go to `POST /vi/v1/runs/<id>/logs/<name>`. Text logs are redacted on
the server (`C:\Users\<name>`, `/home/<name>`, emails, IPv4). Minidumps are
stored as opaque binaries. Local copies stay under `logs-<run_id>/`.

**Never collected:** game packages, save data, usernames, email addresses, IP
or MAC addresses, or peripheral serials. Embedded paths inside uploaded logs
are redacted on the server as a backstop.

## Anonymous identity

On the first run the client derives a stable `machine_id`: the first 32 hex
digits of BLAKE3 over the CPU signature, the system volume serial and the
logical core count. No account name or path goes into it. It posts that ID to
`/v1/register` and stores the returned per-installation bearer token
(`vti_...`) in `<data_dir>/telemetry/token`. Every later ping and report uses
that token, so one PC's runs can be grouped without knowing who owns it. The
ID is saved locally in `machine_id`; existing installations recover their ID
from the server using their existing token before saving it.
The server stores only a hash of the token; the all-powerful admin token lives on
the server and is **never** compiled into or shipped with the client.

Optional: set `VICTORY_TELEMETRY_ALIAS` (for example `GTX1660-lab`) to label
this installation in the admin's user list. Keep it free of personal data.

## Local files

`<data_dir>/telemetry` (normally `%LOCALAPPDATA%\victory\data\telemetry`)
contains:

- `NOTICE.txt` — the first-run disclosure and opt-out instructions;
- `token` — this installation's bearer token;
- `machine_id` — the installation ID associated with that token;
- `agent-<run_id>.json` — session hand-off for the sibling recorder (paths,
  game PID, shared-memory and stop-event names);
- `manifest-<run_id>.json`, `samples-<run_id>.jsonl`, `summary-<run_id>.json`
  — one set per run, kept even when the upload fails;
- `screenshots-<run_id>/NNN.jpg` — local copies of the bounded game-window
  captures for that run;
- `logs-<run_id>/` — local copies of uploaded crash/game logs;
- `upload-status-<run_id>.txt` — present when registration or final uploads
  did not fully succeed.

## Server

Caddy terminates TLS on `rxnd.de/vi/*` and proxies to a loopback-only Python
service. SQLite is the API's source of truth (`installations`, `runs`,
`samples`, `events`, `heartbeats`); the same data is also kept as JSONL plus a
git archive of runs. There is currently no retention limit. Uploads use
WinHTTP through the Windows system trust store, with short connect and send
timeouts; failed attempts are retried with backoff and otherwise leave local
copies plus a warning in the log.

Admin endpoints (require the server's admin token) include
`GET /vi/v1/users`, `GET /vi/v1/runs?git_sha=<sha>`, `GET /vi/v1/stats` and
`GET /vi/v1/runs/<id>/graph.svg`. The authenticated HTML dashboard is at
`GET /vi/v1/view` and shows installations, per-run FPS/CPU/GPU/RAM charts,
trends across runs, and the screenshot gallery. Installation tokens can only
read and write their own runs. See the server README for deployment details.
