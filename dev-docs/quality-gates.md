# Quality gates

All checks operate on the current checkout, with no baseline of accepted exceptions. Cargo subprocesses that compile code are bounded to two jobs. Formatting is check-only. Dependency resolution is locked except for a disposable external facade-consumer probe, which resolves offline and writes only to a unique temporary directory.

## Commands and runners

`cargo xtask` accepts `fmt`, `lint`, `complexity`, `check`, `build`, `test`, `policy`, `source-size`, `ci`, the three CI profiles below, and the GPU splitting commands `ci-gpu-static`, `ci-gpu-shard K/N` and `ci-gpu-plan N`. Standalone compilation commands and `ci` select the entire workspace with all features; they require the prerequisites of every selected package, including graphics and the managed host. Tests and doctests are separate invocations.

- `ci-cpu`: strict lint, complexity, check, build, tests, and doctests for explicitly classified CPU packages, the content tool, and xtask. Also performs each check separately for `worldrunner --no-default-features`. It neither selects GPU packages nor the CLR host for compilation. All-package metadata resolution and repository-wide formatting/source-size checks still apply.
- `ci-gpu`: the same checks for all workspace packages except the managed host, with all features and targets. This includes the renderer, desktop host, facade examples, and any workspace games. Required GPU mode runs **all** tests and doctests, including ignored tests, serially, without test-name filters. Current renderer tests fail when device acquisition fails; the task runner does not turn unavailable adapters into success. Linux CI selects the lavapipe Vulkan ICD explicitly and verifies the reported device before testing.
- `ci-gpu-static`: policy, source-size and formatting checks, then lint, complexity, check, build and doctests for the same package selection as `ci-gpu`, with the same GPU-required doctest arguments. It runs no other test.
- `ci-gpu-shard K/N`: runs shard K of N (1-based, `1 <= K <= N`) of the tests that `ci-gpu` runs, with the same `--include-ignored --test-threads=1` arguments. It uses one cargo invocation shape with the `ci-gpu` package selection and `--all-features`, so feature unification matches `ci-gpu`. It lists every test of the lib, bin, test, bench and example targets with `--list`, removes duplicate names (a name that exists in several binaries runs in all of them, in one shard), sorts the names and deals them to shards round robin. Targets that declare `harness = false` in their manifest (`play_window`, `callback_failure` and `live_window` today) own their `main` and cannot be listed, so each runs whole in one shard, dealt round robin by sorted name. The command fails if a `harness = false` target shares its kind and name with a libtest target, because a single cargo invocation selects targets by name. It fails when listing finds no test. Doctests belong to `ci-gpu-static`.
- `ci-gpu-plan N`: lists the tests as `ci-gpu-shard` does, verifies that the N shards cover each test and each whole-binary target exactly once, and prints the per-shard counts. It runs no test.
- `ci-managed`: requires `dotnet --info` and a warning-as-error build of the .NET 8 portable fixture, then checks/builds/tests the native FFI and managed-host packages. The lifecycle integration test must execute; there is no missing-SDK or missing-runtime skip. CI provisions .NET 8 on Linux, Windows, and macOS.

Every profile first runs policy, source-size, and workspace formatting checks. Failure stops the profile and is reported, not suppressed. Native host dependency downloads/builds can fail independently of an installed SDK; those failures remain failures. Cross-platform workflows are verification requirements, not evidence that cross-platform builds have already passed.

For documentation-only edits, run `cargo xtask fmt`, `cargo xtask policy` and `cargo xtask source-size`, check links and review the documentation-only diff. Full local build/test profiles are unnecessary for that scope. Protected `main` requires the `PR gate` check described under [continuous integration](#continuous-integration); for a documentation-only change it runs the documentation checks and skips the engine builds. [CONTRIBUTING.md](../CONTRIBUTING.md#verification-and-private-consumers) defines the public verification path; private consumer validation and pin updates belong to the consumer owner.

Only Andrew manually merges all PRs after the required checks. Agents never merge, enable auto-merge or use a merge queue. Daily review attention provides no same-day merge guarantee.

## Dependency and layer policy

`crates/xtask/src/layers.rs` is the authoritative package classification. New workspace packages fail until classified; workspace packages beneath `games/` are classified as games by their canonical manifest location. Package names are not classified by substrings such as `cpu` or `loader`.

Allowed internal dependency directions:

| Layer | May depend on |
| --- | --- |
| Foundation (including GPU-free camera, script, and FFI) | Foundation |
| World | Foundation, World |
| Bridge (`render-plan`) | Foundation, World, Bridge |
| Graphics | Foundation, Bridge, Graphics |
| Facade / desktop host | Foundation, World, Bridge, Graphics, Host, Managed |
| Managed host | Foundation, Managed |
| Tool | Engine layers, Tool |
| Game | Facade only |

Cargo metadata resolves workspace inheritance, dependency renames, target-specific declarations, optional declarations, build dependencies, and dev dependencies before checking edges. Every local dependency path in the metadata must canonicalize inside the workspace. Engine dependencies on game/tool layers are forbidden, including excluded `games/` or `tools/` path dependencies.

For every explicitly classified CPU package, the resolved **transitive** graph is walked, including build/dev edges and all targets represented by metadata. GPU/windowing/shader backends and CLR hosting packages are forbidden, and errors show a dependency chain. Workspace declarations are checked separately so inactive optional/target declarations cannot hide a forbidden direct dependency. The forbidden backend-family list is explicit, not a claim to recognize every possible future GPU/CLR library; adding another backend requires updating that list.

The no-default facade graph is obtained from a separate temporary consumer with `worldrunner = { path = ..., default-features = false }`. Running workspace metadata with `--no-default-features` alone is insufficient: other workspace members can unify graphics features into the facade. Optional graphics may exist, but cannot be reached by this external consumer's resolved graph.

## Explicit policy registry

`crates/xtask/policy/explicit.toml` contains current obligations, not a history-based suppression list. Unknown keys, duplicate types/coverage, missing definitions/files, traversal paths, stale entries, parse errors, and source symlinks fail the checks.

- Registered DTOs require unconditional `serde(deny_unknown_fields)` for named content. Container/variant/field serde defaults and nested `Option` fields are forbidden; fields cannot flatten or skip deserialization.
- Registered DTOs and runtime types cannot derive `Default`, implement the trait (including renamed imports), or define an inherent `default` constructor. Registered-name renames/type aliases and Option aliases are rejected conservatively to prevent simple scanner evasions. Implementations are searched across scanned source files, not just the definition file.
- Every registered DTO leaf field needs exactly one consumer-test mapping. Fields containing another registered DTO delegate coverage to that DTO. The mapped function must exist uniquely as `#[test]`, must not be ignored, and must access the leaf field as a Rust member. Comments and strings do not count. This is a static index/anti-staleness check, **not proof of dataflow, behavioral assertions, test reachability, or end-to-end consumer effectiveness**. Package tests execute the mapped tests and reviewers must assess their behavioral strength.
- Numeric literals in explicitly listed production policy files must be algebraic primitives (`0`, `1`, `-1`, `0.5`, `2`), structural array lengths/tuple indices, or occur inside a specifically registered constant with a nonempty ownership rationale. Numeric macro arguments are scanned too. Unused ownership entries fail. Owned constants do not authorize unrelated numbers elsewhere. Attributes are not numeric-policy expressions.
- CPU source is also scanned for process environment lookup calls and environment-capable imports, including common nested imports and macro arguments. This auxiliary rule is **not** the explicit-policy system and does not prove absence of all ambient inputs.

### Current scope and gaps

The DTO registry covers `RtsTuning` (13 fields) and `ViewConvention` (handedness), backed by the camera consumer tests. The no-Default registry also covers `CameraTuning`, `ResolutionPolicy`, `RuntimeProfileLimits`, and `RuntimeProfileDiscoveryLimits`. Numeric ownership applies to camera `profile.rs` and runtime `profile_discovery.rs`, with no owned numeric exceptions.

Most engine policy is not registered. Other camera DTOs, generation/render documents, authored runtime handler contracts (some legitimately have optional fields), package build/reader defaults, and numeric/WGSL policy outside the two listed files remain outside these type/literal obligations. Expanding scope requires meaningful consumer tests and, where needed, source/API changes by the owning component; it must not be achieved with blanket exemptions. The no-Default check does not outlaw every named preset constructor.

These are source-level syntactic checks, not a Rust type resolver or procedural-macro expander. Arbitrary macro-generated types/impls, indirect function-pointer environment calls, and unregistered wrapper types require compiler/tests/review coverage. Inline `#[cfg(test)]` items are excluded from production type/literal checks; external source files are scanned conservatively regardless of their filenames. Ordinary test directories are read when referenced by the coverage index. A unique test function's existence does not prove its module is wired into a test target.

Source-size checks count unterminated final lines, warn above 750 lines, and fail above 1000. Clippy warnings remain errors. Complexity thresholds remain 15 cognitive complexity, 60 lines/function, six arguments, three struct booleans, and type complexity 250. Local source lint attributes can override Clippy deny levels; some test modules retain complexity allowances, so a green gate is not a claim that every function is exception-free. Production schema validation and canonical encoding are split into bounded helpers rather than exempted. FFI and managed-host packages isolate necessary unsafe code and do not inherit the workspace's unsafe prohibition; the script package also has its own lint configuration.

## Focused verification

```sh
CARGO_BUILD_JOBS=2 cargo test --locked -p xtask --jobs 2
CARGO_BUILD_JOBS=2 cargo clippy --locked -p xtask --all-targets --jobs 2 -- -D warnings
cargo xtask policy
cargo xtask source-size
cargo xtask ci-cpu
cargo xtask ci-gpu       # requires a real or software adapter
cargo xtask ci-gpu-plan 8 # lists and verifies the GPU test shards; runs no test
python3 -m unittest discover -s .github/scripts -p 'test_*.py'
cargo xtask ci-managed   # requires .NET 8 plus native hosting dependencies
```

The tooling tests exercise real Cargo metadata over disposable workspaces and reject reversed layers, renamed/target/build/dev/optional dependencies, transitive GPU/CLR reachability, escaping paths, excluded game dependencies, unclassified packages, and a leaking no-default facade. Other fixtures reject permissive/defaulted DTOs, default/alias evasions, unowned numbers, missing/stale/fake consumer entries, malformed configuration, and incomplete source scans.

## Continuous integration

Two workflows run the profiles. `.github/workflows/pr.yml` (name `PR`) validates pull requests and every push to `main` on Linux. `.github/workflows/platforms.yml` (name `Platforms`) runs the Windows and macOS jobs and the exhaustive serial GPU suite after merge and every night. The target for required pull request validation is 15 minutes of wall time.

### Pull request gate

A pull request is merged on one required check, `PR gate`. It reads the results of its needed jobs and passes only when they match the classification of the change:

- `classify` and `docs` must have succeeded.
- `classify` must report `docs_only` as exactly `true` or `false`.
- For `docs_only=true`, the four build jobs (`cpu`, `managed`, `gpu-static`, `gpu-tests`) must all be `skipped`.
- For `docs_only=false`, all four must be `success`. A skipped, cancelled or failed build job fails the gate.

The Linux jobs run in parallel:

| Job | Runs |
| --- | --- |
| `classify changes` | The documentation-only classification and the unit tests of `.github/scripts`. |
| `documentation checks` | `git diff --check`, and a check that relative links and anchors in added or modified Markdown files resolve. It runs for every change. |
| `cpu (ubuntu-latest)` | `cargo xtask ci-cpu`. |
| `managed (ubuntu-latest)` | `cargo xtask ci-managed`. |
| `gpu static checks` | `cargo xtask ci-gpu-static` on lavapipe under Xvfb. |
| `gpu tests (K/8)` | `cargo xtask ci-gpu-shard K/8` for K from 1 to 8, each on its own lavapipe runner. |

The eight test shards replace the serial test run of `ci-gpu`, which took about 50 minutes, almost all of it in the `worldrunner-render-scene` tests. Run `cargo xtask ci-gpu-plan 8` to see the current split. Raise the shard count in `pr.yml` (both the matrix and the argument) when a shard approaches its 20 minute timeout.

New pushes to a pull request cancel its earlier run. Pushes to `main` are never cancelled, and only they save the Rust caches.

### Documentation-only changes

`classify` runs `git diff --raw -z --no-renames --no-abbrev` between the first parent and the merge or head commit. The change is documentation-only only when the diff is not empty and every record is an added, modified or deleted regular file (mode `100644`) whose path ends in `.md` (lowercase). Renames appear as a delete plus an add, so code renamed to `x.md` counts as code. Symbolic links, submodules, executable files, type changes, other extensions, an empty diff, unparseable output and any git error select the full build. Manual dispatches always build.

The classifier runs from the base commit, never from the pull request, so a pull request cannot change the rule that decides whether its own code is built. A base commit without the script selects the full build.

Skipping the build is sound only while no Markdown file is an input to a build or test. `ci_changes.py markdown-inputs` enforces this on every code run. After the Linux build jobs finish, it fails when a cargo dep-info file under `target/validation` lists a Markdown path inside the repository (including `include_str!` and `#[doc = include_str!(...)]`), when a build script's `output` file watches a Markdown path, or when a `.csproj`, `.props` or `.targets` file under `csharp/` mentions `.md`. A change that makes Markdown a build input therefore fails its own pull request, and the same pull request has to change the classification rule.

### After merge and nightly

`Platforms` runs on every push to `main` and at 06:17 UTC daily:

| Trigger | Jobs |
| --- | --- |
| push to `main` | `cpu` and `managed` on Windows and macOS. |
| nightly schedule, manual dispatch | `cpu` and `managed` on Ubuntu, Windows and macOS, and `lavapipe`, the full serial `cargo xtask ci-gpu` with a 90 minute timeout. |

Pull requests do not run Windows or macOS, so a regression that affects only those platforms is found after merge, through the failure issue described below. Running those jobs on every pull request would add runner queue time to the 15 minute budget, and the project accepts later detection in exchange. Code must still build on all three platforms.

To run the complete suite on a branch before merging, dispatch the workflow on that branch:

```sh
gh workflow run platforms.yml --ref <branch>
```

### Failure issues

A failed run on `main` (`PR` after a push, `Platforms (push)` after a push, `Platforms (nightly)` on the schedule) opens an issue titled `CI failure on main: <workflow>` with the `ci-failure` label, the commit, the run link and a table of failed jobs. Each job is labelled `infrastructure` (a setup, cache, toolchain or install step failed, or no step ran), `cancelled`, or `build or test`. A further failure of the same workflow comments on the open issue. A fully green run comments and closes it. A documentation-only run proves nothing about earlier failures, so it never closes an issue. `ci_report.py` only touches open issues that carry the `ci-failure` label, were created by `github-actions[bot]` and contain the workflow's marker comment.

## Running the CI profiles locally

Run the same commands before pushing. `ci-gpu` runs the complete GPU suite serially and is the command to use locally and in the nightly job; the pull request workflow splits it into `ci-gpu-static` and eight `ci-gpu-shard` runs. On Linux without a GPU, select lavapipe explicitly:

```sh
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json xvfb-run -a cargo xtask ci-gpu
```

A single shard can reproduce a CI failure locally:

```sh
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json xvfb-run -a cargo xtask ci-gpu-shard 3/8
```

A passing Linux run does not show that Windows or macOS pass. Code must not assume `/` path separators, `/dev/urandom`, symlink-free temporary directories or non-verbatim canonical paths.
