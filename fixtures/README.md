# Original-source fixture catalog

The catalog selects third-party local source roots, source-version and compatibility profiles, original scene paths, input timelines, and observable smoke assertions. It records original scripts and art as `original`. No source corrections or synthetic visual sidecars are enabled. Any later visual replacement must be declared in `metadata.visual_sidecars`, carry a reason and an explicit synthetic label, and use that label in capture assertions and worker captures.

| Fixture | Default source profile | Original scene | Configurable upstream branch |
| --- | --- | --- | --- |
| snake | source-6000.3 | Assets/Scenes/GameScene.unity | origin/main |
| breakout | source-6000.3 | Assets/Scenes/SampleScene.unity | origin/main |
| pong | source-2017.1 | Assets/Scenes/Main.unity | origin/master |

All three also declare `compat-6000.3` and `compat-2017.1`. Declared profiles are intended test cells. They provide no evidence that those profiles have executed or passed. Workers remain null until the foreground supplies the actual importer/managed/native/render host. `run` records blocked for that missing prerequisite, and unattempted profiles remain `no record` in the dashboard.

The current smoke timelines load, observe, supply input, advance, observe movement, capture, reload and unload. They require changed native object snapshots and a decoded original-origin PNG. Precise gameplay, collision, scoring, restart and reload-generation assertions remain foreground integration work. The harness's other assertion operators are exercised by the harness-only protocol tests.

## Current-source diagnostics

```sh
source .session/env.sh
.work/target/debug/compat-kit fixture check
.work/target/debug/compat-kit fixture check snake
```

Checks read the current Git revision, dirty state, original C# SHA256 values, current editor metadata and fixture profile declarations. Local editable source roots are permitted. A plain source directory outside a Git repository has null revision and dirty state. A parent workspace repository is never mistaken for the source repository.

Recorded hashes identify current inputs and cache freshness. Updating source does not require external approval or a frozen-hash admission list. Runs hash scripts before and after worker execution and fail if original script bytes or revision change during the run.

## Refresh and adoption

```sh
.work/target/debug/compat-kit fixture refresh snake
.work/target/debug/compat-kit fixture refresh snake --accept
```

Refresh obtains an exclusive per-fixture lock and rejects dirty originals or dirty active source. It clones a candidate under `.work/compat/candidates`, with hardlinks disabled, configures the selected upstream, fetches the configured branch, and checks out that fetched revision in the candidate. It never force-resets, rewrites, commits or stages the original source.

The report lists changed scripts, package metadata, editor settings, assets and added API-looking lines. This is a diagnostic diff; actual qualification commands decide whether the update can be adopted. Candidate and active-source manifests are checked after the gates to reject source mutation and concurrent edits.

Every fixture has two explicit gate commands:

1. `python3 {root}/fixtures/gates/qualify.py <name> compile` builds the candidate's actual `Assets/Scripts/**/*.cs` against the existing UnityCompat project. `gates/sources.json` declares the required original paths (currently 8 Snake, 7 Breakout and 20 Pong); missing paths, empty populations and symlinks fail. Added scripts are included automatically. Every discovered source appears as an explicit Compile Include with a current hash/count, so an empty or wrong glob cannot yield a compilation claim. It generates a project under `COMPAT_GATE_OUTPUT`, disables implicit .NET usings to preserve original source name resolution, and places artifacts for all project references under `.work` using the SDK's `--artifacts-path` option. It changes no C# source bytes.
2. `python3 {root}/fixtures/gates/qualify.py <name> test` creates a candidate fixture referencing that exact candidate source and compiled assembly. It invokes the normal harness for each declared profile using the configured worker. The harness evaluates actual observations/captures and requires zero native-world, managed-root and readback counters. An absent worker or any non-pass result fails this gate.

The compile gate records `candidate-assembly.json` with exact selected source paths/counts/current hashes, excluded script paths, DLL SHA256 and independently read PE assembly identity/type/reference metadata. The inspector uses framework-provided System.Reflection.Metadata without loading the game assembly. Source bytes are checked after compilation and before/after each runtime qualification. DLL hash/metadata must match the compilation proof. The test gate passes `metadata.assembly` to the host. The worker must load that assembly and must not silently compile or run the old active source. Gate commands run directly, with literal `{root}`, `{source}`, and `{output}` path substitutions. They receive `COMPAT_SOURCE_ROOT` and `COMPAT_GATE_OUTPUT` and must keep generated output there or elsewhere under the worktree's `.work`.

Successful `--accept` writes qualification proof, then atomically replaces `.work/compat/locks/<name>.json` with the qualified candidate's path and revision. That lock is the active-source pointer used by fixture checks and runs. Original `.work/games/<name>` remains untouched. Any failed gate or mutation preserves the previous active pointer and source. Without `--accept`, qualification reports success and leaves the active pointer unchanged. There is no approval handshake.

Failed and unaccepted candidates are retained for diagnostics. At the 450 GB worktree soft limit, automatic pruning removes oldest idle ownership-marked runs/candidates. Shared leases protect active runs/results and candidate sources; accepted lock-selected and configured source roots remain protected. Unmarked pre-existing artifacts and any tree containing symlinks are preserved. The 500 GB hard guard still prevents further execution if pruning cannot reclaim enough space. Refresh locks use persistent kernel-locked inodes. Process death releases the lease automatically; PID/generation fields are diagnostic, so PID reuse cannot make an abandoned lease live or justify removing a live one. Lock files are never unlinked by refresh. Successful accepted candidate directories must be retained while their lock is active.

See the [harness API](../crates/compat-kit/README.md) for protocol/schema and [foreground integration](../.session/harness-integration.md) for current gaps.
