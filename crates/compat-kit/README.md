# Compatibility process harness

`compat-kit` supervises trusted compatibility workers and evaluates their actual evidence. It contains no game world, managed facade, physics, importer, or renderer implementation. The foreground host integrates those providers behind the worker boundary.

## Commands

Run from the workspace root. Source the worktree environment for every build.

```sh
source .session/env.sh
cargo build -p compat-kit --offline -j 2
.work/target/debug/compat-kit doctor
.work/target/debug/compat-kit fixture check
.work/target/debug/compat-kit fixture refresh snake
.work/target/debug/compat-kit fixture refresh snake --accept
.work/target/debug/compat-kit run snake --profile source-6000.3
.work/target/debug/compat-kit run pong --desktop
.work/target/debug/compat-kit dashboard .work/compat/dashboard
.work/target/debug/compat-kit serve --port 8787
```

`COMPAT_ROOT` selects the workspace root. Fixture names resolve to `fixtures/<name>.json`. Explicit JSON paths are also accepted. `COMPAT_RUNS` can select a runs directory inside the root's `.work`; the runner, dashboard and server share this setting. Output directories containing symlinks or traversal are rejected. Editable source roots can be selected in fixture JSON, including local roots outside `.work`; the harness reads those inputs without modifying them.

Catalog discovery reserves `fixtures/compatibility-catalog.json` for the `compatibility-traceability/1` manifest. Its top-level schema and document shape are checked before exclusion from executable fixtures. Other JSON files remain strict executable fixtures, including misplaced manifest copies. Full traceability validation and acceptance dispositions belong to `tools/compat-traceability/check.py`; catalog discovery supplies no traceability acceptance.

`run` uses the specification's completion dispositions: 0 for the selected required assertion gate passing, 1 for assertion or required-status failure, 2 for invalid invocation/specification/profile, 3 for infrastructure errors, and 4 for deadline/cancellation. Combined causes use precedence 4 > 3 > 2 > 1. This replaces the previous run-only 0/1/2 convention. It writes one JSON record to stdout and an authoritative `run.json` under its unique run directory. CLI usage and malformed configuration errors go to stderr and exit 2. A worker-reported unsupported profile outcome fails the required gate with 1; requesting a profile absent from the fixture is invalid selection and exits 2. A configured executable that is missing records blocked with an infrastructure cause and exits 3. An unconfigured worker records blocked and exits 1. A successful process without assertions is inconclusive and exits 1. Passing this selected fixture gate grants no complete-game, playable or fidelity badge. `fixture check` and `fixture refresh` accept an omitted name to process the catalog. A failed refresh/check exits 2. `doctor` reports tools, current source revisions and dirty state without claiming executed game coverage.

## Worker boundary

A fixture has `worker: {"program":"/trusted/worker", "args":["argument"]}`. `null` records blocked. Program and arguments are passed directly to `Command`; shell expansion never occurs. Literal `{root}`, `{source}`, and `{output}` substitutions support portable command configuration. Only trusted programs should be configured. This is a process-group supervisor on Linux, with no operating-system sandbox. Workers must remain in their owned process group and must not daemonize.

The child receives absolute paths in `COMPAT_REQUEST`, `COMPAT_RESPONSE`, `COMPAT_ARTIFACTS`, and `COMPAT_ROOT`. The request uses schema version 1. Public Rust types in [`src/model.rs`](src/model.rs) are the serialization contract:

```json
{
  "schema_version": 1,
  "run_id": "unique-id",
  "fixture": "snake",
  "profile": "source-6000.3",
  "desktop": false,
  "seed": 17,
  "timeline": [
    {"frame":0,"phase":"load","action":"load"},
    {"frame":1,"phase":"input","action":"input","keys":["W"],"axes":{}},
    {"frame":1,"phase":"update","action":"advance","dt":0.02},
    {"frame":1,"phase":"observe","action":"observe","selector":"world.snapshot"},
    {"frame":1,"phase":"render","action":"capture","path":"captures/frame.png"},
    {"frame":2,"phase":"reload","action":"reload"},
    {"frame":3,"phase":"unload","action":"unload"}
  ],
  "assertions": [],
  "source_root": "/workspace/.work/games/snake",
  "source_hashes": {"Assets/Scripts/GameLoop.cs":"sha256"},
  "artifact_root": "/workspace/.work/compat/runs/unique-id",
  "metadata": {"scene":"Assets/Scenes/GameScene.unity"}
}
```

Timeline entries are forwarded unchanged and in authored order. The host owns action execution, fixed-step policy, input edges, frame phases, observation selector resolution, reload and unload. Unknown actions/selectors must produce explicit diagnostics. The host must use the same driver as desktop play and the shared `worldrunner::managed` and `compat_content` providers. No fixture-specific simulation belongs in this crate.

The successful child writes a response before exiting:

```json
{
  "schema_version": 1,
  "evidence_label": "original",
  "observations": [
    {"selector":"player","value":{"x":1.25},"frame":1,"phase":"update"}
  ],
  "captures": [
    {"path":"captures/frame.png","frame":1,"phase":"render","origin":"original"}
  ],
  "diagnostics": [],
  "teardown": {"completed":true,"native_worlds":0,"managed_roots":0,"pending_readbacks":0}
}
```

Diagnostic entries have `status` and `message`. The worker's evidence label must match its fixture. Harness-only protocol test peers are labelled `harness-only`; their generated 2x2 PNGs establish infrastructure behavior only. Gameplay workers must capture actual renderer readback. Capture origin labels describe the source or declared synthetic visual sidecar.

The harness validates JSON shape/version, observation frame ordering, zero teardown counters, capture count, path confinement and symlinks. It decodes PNG pixels with bounded memory, validates the PNG end, records dimensions and SHA256, and checks those hashes again before dashboard image display. Missing/corrupt images are errors. Worker-reported pass diagnostics cannot override failed assertions or absent assertions.

## Assertions and outcomes

Assertions select an observation by exact `selector`, optional `frame`/`phase`, and a JSON `pointer`. Equality and tolerance use the last matching observation in response order. Observations must be nondecreasing by frame. Missing JSON pointers remain missing evidence, including in the latest observation and event sequence. Numeric tolerance is absolute and nonnegative. Changed-value assertions require both frames and `before < after`. Event-order assertions compare the entire selected sequence, including duplicate events. A capture assertion requires validated evidence at the specified frame, phase and origin.

```json
[
  {"op":"equal","selector":"score","pointer":"/value","value":1,"frame":10},
  {"op":"tolerance","selector":"ball","pointer":"/speed","value":6.0,"tolerance":0.001},
  {"op":"changed","selector":"player","pointer":"/position","before":0,"after":10},
  {"op":"event_order","selector":"contact","pointer":"/phase","events":["enter","stay","exit"]},
  {"op":"capture","frame":10,"phase":"render","origin":"original"}
]
```

Statuses are `pass`, `fail`, `unsupported`, `not-implemented`, `blocked`, `skipped`, `error`, `flaky`, and `inconclusive`. Unattempted dashboard cells show `no record`; the status filter includes that state. Worker diagnostics can report any status. `flaky` may be retained from explicit worker evidence. The report reducer synthesizes it only from separate completed pass and fail attempts under matching controlled identities. Conflicting assertions within one attempt remain fail. Serde emits `not-implemented` and accepts the bounded legacy spelling `not_implemented`. Missing tools/source/worker are blocked, unsupported profile selection is unsupported, assertion mismatches are fail, and process/protocol/teardown errors are error. Stop cancellation is skipped with a structured cancelled cause and disposition 4. Deadline is error with a structured deadline cause and disposition 4. Successful exit with no assertions is inconclusive. All regular source input bytes, including scenes/assets/settings, and Git revision are observed before and after worker execution; drift fails the run. Hashes describe current inputs and never freeze fixture admission.

Each run retains request/response JSON, capped stdout/stderr, process exit metadata, start/finish events, assertions, source snapshots and validated captures. Logs continue to drain after their retention cap. Deadline, cancellation or an artifact-cap violation kills the owned process group, then reaps the direct child. Descendants in that group are killed even after a successful leader exit. Artifact checks include hidden directories. Rejected oversized/unsafe outputs are discarded from the new run directory while retaining bounded harness logs and request/process diagnostics. Limits are polled, so trusted children can briefly exceed a storage threshold before termination. The supervisor measures the entire worktree's 500 GB budget before each child and once per second during execution. At 450 GB it prunes oldest idle harness-owned runs and candidates until below the soft limit. An external ownership registry and shared lifetime leases protect active workers, their results and candidate inputs. Accepted/configured sources, unmarked pre-existing directories, foreign files and trees with symlinks are preserved. Invalid source locks fail pruning closed. Ownership markers are retained for inspection. The checkpoint script also records that budget periodically.

## Additive reporting selection and records

New run records identify their wire contract with `schema: "real-disunity.tck-run/1"`; `schema_version` remains 1. Explicit unknown or malformed run schema identities reject. Legacy records without the identity retain its absence on read/export, without acquiring new measured qualifications. Existing records read without reporting fields; their missing qualifications stay unknown and their attempts remain separate. The bounded legacy status alias `not_implemented` is still accepted. Worker request/response JSON and existing Rust worker report fields remain compatible.

A fixture can add `metadata.reporting`. This standalone harness-only example declares no external provider/overlay inputs; real providers must list their actual native/managed images and external overlay files instead of copying the empty lists:

```json
{
  "case_id": "UCK-HAR-001",
  "game": "harness",
  "assertion_dimensions": {"0": "semantic"},
  "diagnostic_dimensions": {"0": "content"},
  "units": {"0": "count"},
  "required_dimensions": ["semantic"],
  "provider_inputs": [],
  "overlay_inputs": []
}
```

Indices refer to the authored assertion list and the actual worker diagnostic list. Unknown dimension names and invalid assertion/unit indices are invalid specification. An absent diagnostic index contributes no outcome. Supported dimensions are exactly `compile`, `content`, `semantic`, `desktop`, `exploratory`, and `fidelity`. A dimension passes only through mapped executed assertions. Non-pass mapped diagnostics preserve their dimension's disposition; pass diagnostics cannot promote untested fidelity or another unmeasured dimension. Unmapped/unexecuted dimensions serialize as null. Missing required dimension evidence makes an otherwise passing run inconclusive. Units are explicit fixture data; the harness never guesses them.

`RunRecord.reporting` adds `case_id`, `game`, `mode`, `platform`, `identities`, `provider_files`, `prerequisites`, `outcomes`, `required_dimensions`, `failure_causes`, and `exit_code`. Assertion results add typed `expected`, bounded frame/phase-filtered `actual`, and nullable `unit`, while retaining `assertion`, `passed` and `message`. Source snapshots add `inputs`, a path-to-SHA256 map excluding Git internals, alongside the existing script hashes.

Identities cover before/after original source snapshots, selected metadata plus declared overlay bytes, observed executable/file argv plus declared provider bytes, and assertion/timeline/seed/worker/limits/selection inputs. External provider/overlay bytes are rechecked after execution. Missing `provider_inputs` or `overlay_inputs` leaves the corresponding qualification identity null. An explicit empty list declares that no external inputs of that kind are required. The observed worker executable hash remains available in `provider_files` even when full provider qualification is unknown. Hidden dependencies cannot be inferred; the fixture author must declare them before trend grouping is justified.

Groups require matching fixture, case/game, profile, evidence label, mode, platform and source/overlay/provider/test qualifications. Incomplete qualifications are isolated by run ID. Every group retains its attempt IDs and every full record remains in `runs`. Error/blocked/unsupported/not-implemented/skipped/inconclusive attempts prevent a completed flaky summary and retain a failing required gate. Separate completed pass/fail attempts reduce to flaky; one attempt never creates that inference. Independent dimension outcomes reduce across actual measured attempts. No comparison across different provider images or original inputs is implicit.

Current mode labels are `worker-default` and `desktop-replay`, derived from the actual desktop request flag. Platform records observe OS, architecture and display environment. Adapter, driver and physical-platform certification remain null unless a future provider supplies supported evidence. This increment makes no Windows/macOS, physical GPU or original-pipeline certification claim.

The dashboard filters case, game/fixture, profile, observed OS platform, all nine statuses and no-record cells. View selection exposes reduced groups or all attempt details. Filters compose, including unattempted profiles. Static HTML escapes record text; dynamic task messages use `textContent`. Source text is never embedded as executable JSON.

## Browser integration

`serve` binds IPv4 loopback only. Its first stdout line contains `address` and `url`. Routes are:

- `GET /`: matrix, history, report diagnostics and actual capture images.
- `GET /api/state`: fixtures, authoritative run records, damaged-report diagnostics and current tasks.
- `POST /api/run`: JSON `{ "fixture":"snake", "profile":"source-6000.3", "desktop":false }`.
- `POST /api/stop`: JSON `{ "id":"task-id" }`.
- `POST /api/refresh`: JSON `{ "fixture":"snake", "accept":false }`.
- `GET /files/<run-id>/<relative-png>`: only captures present in validated reports with matching content hashes.

Actions return HTTP 202 and task IDs after admission. State exposes `done` and `result`; at most four jobs are active. Every POST requires `Content-Type: application/json` and `X-Compat-Token: <launch-token>`. Each server launch reads 32 random bytes from Linux `/dev/urandom` in read-only mode and encodes them as 64 lowercase hexadecimal characters. Failure to read randomness aborts startup before announcing the listener. The server compares fixed-length token candidates across all bytes without a prefix-dependent exit. Restarting the server invalidates earlier tokens, including tokens held by an already-open dashboard tab. Reload that tab to obtain the new launch capability.

`Host` must exactly equal `127.0.0.1:<actual-port>` or `localhost:<actual-port>`, including when `--port 0` chooses a port. When present, `Origin` must exactly equal `http://<that-request's-Host>`. The two loopback hostnames are separate origins. Foreign, `null`, wrong-port and mismatched-alias origins receive HTTP 403, even with a valid token. Missing or wrong tokens also receive 403 before Run, Stop or Refresh admission. Duplicate headers are malformed requests and receive 400 before any action. A trusted CLI may omit Origin with a valid token; browser POSTs automatically include their page's Origin. Paths with traversal, percent encoding or query parameters are rejected. Header/body sizes and socket timeouts remain bounded.

Only the live HTML page bootstraps the token, in `<meta name="compat-token" content="…">`. Its JavaScript reads that meta tag and supplies `X-Compat-Token` for Run, Stop and Refresh. The token is absent from startup stdout, URLs, API state and static reports. Do not log headers or save the live HTML as evidence. No cross-origin read permission or JSONP endpoint is provided. Responses use `Cache-Control: no-store`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`, and a CSP with `frame-ancestors 'none'`, `base-uri 'none'` and `form-action 'none'`. Inline scripts/styles remain explicitly allowed for the existing page and its button handlers; network connections and images are limited to the page's own origin.

This capability protects browser mutation admission. Local processes that can read the live page can obtain the token. Configured workers and the local filesystem remain trusted; the server provides no OS sandbox or user-account authentication.

### Direct HTTP clients

Start the server from the workspace root:

```sh
source .session/env.sh
cargo +1.97.0 build -p compat-kit --locked --offline -j 2
"$CARGO_TARGET_DIR/debug/compat-kit" serve --port 8787
```

A separate terminal can obtain the token through the same listener and use it in memory. This example deliberately omits Origin under the trusted-CLI policy and prints only the admitted job response:

```sh
python3 - <<'PY'
import json
import re
import urllib.request

base = "http://127.0.0.1:8787"
with urllib.request.urlopen(base + "/", timeout=5) as response:
    page = response.read().decode("utf-8")
token = re.search(r'<meta name="compat-token" content="([0-9a-f]{64})">', page)[1]
request = urllib.request.Request(
    base + "/api/run",
    data=json.dumps({"fixture": "snake", "profile": "source-6000.3"}).encode(),
    headers={"Content-Type": "application/json", "X-Compat-Token": token},
    method="POST",
)
with urllib.request.urlopen(request, timeout=5) as response:
    print(response.read().decode("utf-8"))
PY
```

Use the announced URL if launching with `--port 0`. The same headers apply to `/api/stop` and `/api/refresh`. Clients that supply Origin must use the exact base URL origin, without a trailing slash. Obtain a new token after restarting the server. The non-HTTP `run` and `fixture refresh` CLI commands are unchanged.

The static dashboard copies validated images into its output and provides `state.json` and `junit.xml` from the same record snapshot. JSON preserves all loaded attempt records and adds reduced identity groups. JUnit emits every attempt, retains its original status property, maps fail/flaky to failure and error to error, and maps the other non-pass statuses to skipped. Completion gates read JSON rather than the JUnit skipped count. Reports reject incomplete aggregates above the 10,000-entry or 64 MiB report safety bounds. Damaged individual records are diagnosed separately. The previous arbitrary 250-record truncation was removed. Live Run/Stop/Refresh controls require `serve`. The page polls actual state and reloads on new records. Task names include fixture/profile; profile controls list the selected fixture’s profiles and distinguish Unity 6.3 (6000.3) from legacy Unity 2017.1. These identify source/API targets for this runtime facade. Run details show the actual request/input timeline, frame/phase captures and expected/observed assertion values. Persisted records must match their containing run ID/path and schema before display. Report strings are HTML-escaped and task messages use `textContent`.

## Live traversal and pre-adoption integrity

Live process supervision uses `files::live_tree_files`. A typed descendant `NotFound` invalidates the complete traversal and permits at most three complete scan attempts. Each restart resets file and byte totals, includes hidden files and verifies the same root directory identity. Missing/replaced roots, symlinks, special files, exceeded caps and other I/O errors fail immediately. Exhaustion retains the failing path and filesystem operation under the `supervision-artifacts` infrastructure stage. Source, capture and final quiescent scans continue to use strict `tree_files`/`source_files`. The independent storage measurement policy still permits exactly three failed `du` measurements.

Refresh records current source manifests, clean Git state, HEAD, local refs, raw FETCH_HEAD and repository configuration for the candidate and original/active snapshots. A configured local upstream branch is resolved afresh for each refresh and re-resolved before adoption. Managed source/candidate leases remain held throughout qualification and adoption. A failed recheck preserves the existing active-source lock. These leases coordinate harness lifetime and pruning. Uncooperative filesystem writers can still mutate between the final read checks and atomic lock replacement.

Existing harness-only gate configurations remain supported. Their qualification and accepted locks explicitly record `integrity.scope: "harness-gates-only"` and `engine_proof: false`. Select canonical strict content/assembly validation through fixture metadata:

```json
{"refresh_integrity":{"schema_version":1,"mode":"strict-closure"}}
```

With this selection, compile gates must produce `{output}/candidate-assembly.json` through the existing canonical Rust receipt/.NET qualification pipeline. The proof's source root must match this candidate; its assembly must be a regular file below gate output. The proof, assembly, trusted checker modules and selected receipt executable are observed after the first compile proof and rechecked after subsequent gates. Missing proof or changed bytes reject adoption.

Immediately before adoption, the supervisor invokes the fixed workspace `tools/compat-refresh/check.py` with an exact serialized fixture snapshot, the observed proof and a confined output directory. Python runs isolated with bytecode writes disabled. `COMPAT_CONTENT_RECEIPT` selects an absolute trusted Rust receipt executable; the default is `.work/target/debug/examples/receipt`. The checker reimports the current closure through Rust, checks the dynamic runtime-source census and local Git identity, and inspects actual assembly metadata. Final source/Git/proof checks precede atomic lock publication. Qualification and accepted-lock provenance retain the exact proof, closure/importer/profile/sidecar, source count, assembly hash, local identities and checker result.

`mode: "dependency-admission"` fails explicitly as unsupported until trusted host/provider inventory composition and `AdmissionProof` recomputation are integrated. Fixture JSON cannot assert provider readiness or select a checker program. Unknown integrity fields/modes reject. Strict closure validation supplies no engine/provider or complete-specification certification; the current protected dependency-policy fixtures remain blocked with their owners.

## Verification

```sh
source .session/env.sh
cargo test -p compat-kit --offline -j 2
cargo clippy -p compat-kit --all-targets --offline -j 2 -- -D warnings
cargo fmt -p compat-kit -- --check
python3 crates/compat-kit/tests/qualification.py
node tools/compat-dashboard/poll-contract.mjs
node crates/compat-kit/tests/browser.mjs
```

The Node test uses installed Chrome and Node built-ins. It clicks Run, Stop and Refresh through the actual page, observes a real worker cancellation and successful local-Git candidate qualification/adoption without rewriting originals, then loads a validated harness-only PNG. Browser state and harness-only profiles are isolated under `TMPDIR` (default `.work/tmp`) and removed after the check. Chrome runs headless with GPU disabled and does not require an X display. The browser test honors `CARGO_TARGET_DIR` for binaries. An isolated target avoids the foreground Cargo build lock when needed.

The security suite exercises the actual loopback server for all three mutation routes, rejected-request file/task preservation, unauthorized Stop through the real worker deadline, duplicate headers, exact Origin/Host policy, CLI Origin omission, token rotation, bootstrap containment and static-report secrecy. Initial missing/wrong-token regressions ran RED against the unmodified server, which admitted Run and Refresh with HTTP 202. Security implementation details are recorded in [the security handoff](../../.session/resumed-dashboard-security.md).

The [reporting recovery handoff](../../.session/resumed-reporting-contract.md) records current verification, retained private binary hashes, compatible JSON examples and integration limits. Reporting controls cover actual retries, independent dimension evidence, identity boundaries, legacy records, completion dispositions and histories beyond the former 250-record limit. Repository policy and source-size checks run separately from behavioral tests. Production fixture mapping and full specification acceptance remain pending.

The initial CLI/refresh tests ran red against the stub. Follow-up stale-lock, forged-report, missing-pointer, concurrent-original-commit and dashboard-detail regressions ran red before their fixes. Pruning tests were added after the initial storage module implementation; they establish ownership/lease outcomes at a small injected soft limit rather than creating 450 GB of data. Storage bypass coverage ran red before its fix. Tolerance/sequence and browser checks were added after the initial evaluator and page implementation. The test subprocess binary is explicitly named `compat-kit-protocol-test-worker` and lives under `tests/support`; it must never be configured as a game worker.

See [fixture qualification](../../fixtures/README.md) for refresh/adoption and [integration notes](../../.session/harness-integration.md) for foreground work.

## Remaining proposed interfaces

The implemented CLI remains `compat-kit run`, `dashboard`, `serve`, `doctor`, and `fixture check|refresh`. The architecture's `cargo xtask tck`, named baseline/gate registry, `--fixture/--mode/--gate/--output` run syntax and `/api/runs` route family remain proposals. No parallel RPC service or unauthenticated helper was introduced. The existing fixture check/refresh command-level failed-check exit 2 is a separate legacy convention; the new 0–4 completion contract applies to `run` and recorded run results. Generic RunControl engine protocol expansion, automatic production dimension/identity mapping and requirement-catalog adoption remain with their owners. No gameplay or original-fidelity result was relabelled by this increment.
