# Compatibility process harness

`compat-kit` supervises trusted workers and evaluates their observations, assertions, captures and measured teardown. The worker owns compilation, import, managed execution and rendering. See the [fixture configuration](../../fixtures/README.md) and [worker wire types](src/model.rs).

## Commands and configuration

Build with a bounded Cargo job count, then use the executable from your selected target directory:

```sh
cargo +1.97.0 build --offline -p compat-kit -j 2
target/debug/compat-kit doctor
target/debug/compat-kit fixture check
target/debug/compat-kit fixture check snake
target/debug/compat-kit run snake --profile source-6000.3
target/debug/compat-kit run pong --desktop
target/debug/compat-kit dashboard .work/compat/dashboard
target/debug/compat-kit serve --port 8787
```

`COMPAT_ROOT` selects the fixture and output workspace. `COMPAT_RUNS` selects a runs directory inside that root's `.work`. Discovery treats every top-level `fixtures/*.json` as an executable fixture. Fixture names and explicit JSON paths are accepted.

Each fixture names an original game with `"game": "snake"`, `"breakout"` or `"pong"`. The kit calls [worldrunner-original-games::checkout](../original-games/src/lib.rs) to fetch and verify that game's pinned revision in `target/original-games`. The checkout belongs to the compiled source workspace, independently of `COMPAT_ROOT` and `CARGO_TARGET_DIR`. Existing checkouts must have the pinned HEAD and no local changes. First use requires network access to the game's upstream repository. Fixture checks report the resolved source path, editor metadata and profiles. Fixture JSON rejects unknown top-level fields.

`RunRecord.schema` is the required `real-disunity.tck-run/1` contract; schema-less records are rejected. Status names use kebab-case, including `not-implemented`. `run` prints one result record and stores it with the request, worker response, bounded logs, process diagnostics, start/finish events and validated captures. Completion dispositions are 0 for pass, 1 for a nonpassing required outcome, 2 for invalid invocation/specification/selection, 3 for infrastructure, and 4 for deadline/cancellation. Combined causes use precedence 4 > 3 > 2 > 1. Missing configured executables are blocked infrastructure; an unconfigured worker is blocked. Successful exit without assertions is inconclusive.

## Worker protocol

A fixture selects `worker: {"program":"/selected/worker", "args":[]}` or leaves it null to report blocked. Arguments are passed directly without shell expansion. Literal `{root}`, `{source}` and `{output}` substitutions are supported. Only trusted programs should be configured. Workers run in their own Unix process group, so they must stay in that group and never daemonize. The harness supports Linux and macOS; Windows lacks the process-group and inode identity primitives it relies on. There is no OS sandbox in this crate.

The child receives absolute `COMPAT_ROOT`, `COMPAT_REQUEST`, `COMPAT_RESPONSE` and `COMPAT_ARTIFACTS` paths. `RunRequest` contains schema version 1, run/fixture/profile identifiers, desktop selection, seed, authored timeline/assertions, the resolved absolute `source_root`, artifact root and metadata. Unknown request fields are rejected. Per-file source hashes and source snapshots are absent; the original-games checkout verifies the pin before execution. Workers compile the supplied checkout without editing it.

`WorkerReport` contains schema version 1, the fixture's evidence label, ordered observations, captures, diagnostics and teardown:

```json
{
  "schema_version": 1,
  "evidence_label": "original",
  "observations": [{"selector":"score","value":1,"frame":10,"phase":"observe"}],
  "captures": [{"path":"captures/frame.png","frame":10,"phase":"render","origin":"original"}],
  "diagnostics": [],
  "teardown": {"completed":true,"native_worlds":0,"managed_roots":0,"pending_readbacks":0}
}
```

The worker executes the timeline in authored order, resolves selectors and handles reload/unload. Unknown operations require explicit diagnostics. Harness-only test workers generate small PNGs to exercise infrastructure; their `harness-only` results establish no game behavior.

## Validation and outcomes

Assertions support equality, finite absolute tolerance, changed values, exact event sequences and captures at an exact frame/phase/origin. Equality/tolerance inspect the last matching observation. Missing pointers remain unmeasured. Observation frames must be nondecreasing. PNG validation decodes bounded pixel buffers, checks completion and confined regular paths, then retains dimensions and an integrity checksum. Dashboard reads recheck that checksum. Teardown requires completion and zero native worlds, managed roots and pending readbacks. Worker pass diagnostics cannot override failed or absent assertions.

Statuses are pass, fail, unsupported, not-implemented, blocked, skipped, error, flaky and inconclusive. Unattempted cells show no record. Cancellation is skipped with a structured cancelled cause; deadline, protocol and teardown failures retain structured infrastructure diagnostics.

Optional `metadata.reporting` maps measured assertions and nonpass diagnostics to dimensions:

```json
{
  "case_id":"movement",
  "game":"snake",
  "assertion_dimensions":{"0":"semantic"},
  "diagnostic_dimensions":{"0":"content"},
  "units":{"0":"count"},
  "required_dimensions":["semantic"]
}
```

Supported dimensions are compile, content, semantic, desktop, exploratory and fidelity. Invalid assertion/unit indices or dimension names reject the fixture. Unmeasured dimensions remain null; missing required measurements make an otherwise passing run inconclusive. Pass diagnostics alone cannot promote dimensions.

Reporting groups summarize fixture/profile, evidence label, case/game, mode and observed platform. They do not certify identical builds or inputs. Missing mode/platform leaves an attempt separate. Completed pass/fail attempts in a group reduce to flaky; conflicting assertions within one attempt remain fail. All attempts and independently measured dimensions remain available. JSON exports and JUnit retain every loaded attempt within explicit report safety bounds.

## Storage and HTTP security

Retained run directories are measured in-process. Above 16 GiB the oldest idle owned runs are pruned, and above 20 GiB after pruning new runs are refused. Persistent kernel lease files protect active runs. Only owned immediate run directories are eligible. Original-game checkouts live outside `.work`; unmarked directories and trees containing symlinks or special files remain protected. Live traversal retries only complete scans after transient descendant disappearance, with a bounded attempt count. Final artifact scans remain strict.

`serve` binds IPv4 loopback and announces its URL. Routes are `GET /`, `GET /api/state`, `POST /api/run`, `POST /api/stop`, and validated capture URLs under `/files/`. Run accepts fixture/profile/desktop; Stop accepts the task ID. Removed routes return 404 after normal security checks.

Every POST requires JSON and the launch-specific `X-Compat-Token` from the live page's `compat-token` meta tag. Host must exactly match the announced loopback host/port. An Origin, when supplied, must exactly match that request's Host. Duplicate headers, foreign origins, invalid tokens and traversal reject before mutation. Restart rotates the token. Static reports, API state and announcements omit it. Local programs and configured workers remain trusted.

## Verification

```sh
cargo +1.97.0 test --offline -p compat-kit --all-features -- --include-ignored --test-threads=1
cargo +1.97.0 clippy --offline -p compat-kit --all-targets --all-features -- -D warnings
cargo +1.97.0 fmt -p compat-kit -- --check
node --test tools/compat-dashboard/*.test.mjs
node tools/compat-dashboard/poll-contract.mjs
node crates/compat-kit/tests/browser.mjs
```

Rust tests create their own roots and real protocol workers. The browser test uses installed Chrome, clicks Run/Stop, checks decoded images and filters, and removes its temporary root and processes. `COMPAT_BROWSER` selects Chrome; `CARGO_TARGET_DIR` selects built binaries. The production browser runner additionally exercises current game fixtures through the contained display harness. Original-game execution requires current worker/native/managed builds. The kit resolves pinned sources through the original-games crate. Shipped fixtures select `{root}/target/debug/compat-player`.
