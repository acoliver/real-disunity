# Quality gates

All checks operate on the current checkout. No source history or historical exception baseline is required. Cargo subprocesses that compile code are bounded to two jobs. Formatting is check-only. Dependency resolution is locked except for a disposable external facade-consumer probe, which resolves offline and writes only to a unique temporary directory.

## Commands and runners

Existing `fmt`, `lint`, `complexity`, `check`, `build`, `test`, `policy`, `source-size`, and `ci` commands remain available. Standalone compilation commands and `ci` select the entire workspace with all features; they require the prerequisites of every selected package, including graphics and the managed host. Tests and doctests are separate invocations.

- `ci-cpu`: strict lint, complexity, check, build, tests, and doctests for explicitly classified CPU packages, the content tool, and xtask. Also performs each check separately for `worldrunner --no-default-features`. It neither selects GPU packages nor the CLR host for compilation. All-package metadata resolution and repository-wide formatting/source-size checks still apply.
- `ci-gpu`: the same checks for all workspace packages except the managed host, with all features and targets. This includes the renderer, desktop host, facade examples, and any workspace games. Required GPU mode runs **all** tests and doctests, including ignored tests, serially, without test-name filters. Current renderer tests fail when device acquisition fails; the task runner does not turn unavailable adapters into success. Linux CI selects the lavapipe Vulkan ICD explicitly and verifies the reported device before testing.
- `ci-managed`: requires `dotnet --info` and a warning-as-error build of the .NET 8 portable fixture, then checks/builds/tests the native FFI and managed-host packages. The lifecycle integration test must execute; there is no missing-SDK or missing-runtime skip. CI provisions .NET 8 on Linux, Windows, and macOS.

Every profile first runs policy, source-size, and workspace formatting checks. Failure stops the profile and is reported, not suppressed. Native host dependency downloads/builds can fail independently of an installed SDK; those failures remain failures. Cross-platform workflows are verification requirements, not evidence that cross-platform builds have already passed.

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

### Initial enforced scope and remaining gaps

The DTO registry initially covers `RtsTuning` (13 fields) and `ViewConvention` (handedness), backed by the existing camera consumer tests. The no-Default registry additionally covers `CameraTuning`, `ResolutionPolicy`, `RuntimeProfileLimits`, and `RuntimeProfileDiscoveryLimits`. Numeric ownership applies to camera `profile.rs` and runtime `profile_discovery.rs`; there are initially no owned numeric exceptions.

This is intentionally not a claim that every engine policy has been registered. Other camera DTOs, generation/render documents, authored runtime handler contracts (some legitimately have optional fields), package build/reader defaults, and numeric/WGSL policy outside the two listed files remain outside these type/literal obligations. Expanding scope requires meaningful consumer tests and, where needed, source/API changes by the owning component; it must not be achieved with blanket exemptions. The no-Default check does not outlaw every named preset constructor.

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
cargo xtask ci-managed   # requires .NET 8 plus native hosting dependencies
```

The tooling tests exercise real Cargo metadata over disposable workspaces and reject reversed layers, renamed/target/build/dev/optional dependencies, transitive GPU/CLR reachability, escaping paths, excluded game dependencies, unclassified packages, and a leaking no-default facade. Other fixtures reject permissive/defaulted DTOs, default/alias evasions, unowned numbers, missing/stale/fake consumer entries, malformed configuration, and incomplete source scans.
