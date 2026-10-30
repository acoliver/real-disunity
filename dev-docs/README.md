# Developer documentation

This directory is for people and coding agents who change Real Disunity itself. It describes how the engine is put together, the exact contracts each subsystem keeps, and the checks every change must pass. Documentation for people who only use the engine to make games is in [docs](../docs/README.md).

Start with [CONTRIBUTING.md](../CONTRIBUTING.md) for contributor policy. Coding agents must read [AGENTS.md](../AGENTS.md) and the [agent procedure](contribution-agent.md), then [architecture](architecture.md) and [quality gates](quality-gates.md). The workflow, code standards, coordination and play-verification skills linked from the agent procedure are binding. Andrew merges external contributors' PRs; his maintainer sessions merge only their own PRs once `PR gate` passes, as set out in [AGENTS.md](../AGENTS.md#coordination-and-review-authority). Andrew intends human review daily, and maintainer agents also review daily.

## Where things live

| Path | What belongs there |
| --- | --- |
| `crates/<name>/src/` | Rust source. One crate per responsibility, as listed in [architecture](architecture.md#ownership). Split files by responsibility before they reach 750 lines. |
| `crates/<name>/tests/` | Integration tests for that crate, with small synthetic fixtures. Tests never read from `.work/`. Public original-game tests fetch pinned sources through committed tooling. |
| `crates/<name>/README.md` | Contracts that are local to one crate and that a caller of that crate needs: units, ranges, ordering, error behavior. Only some crates have one. |
| `crates/worldrunner/examples/` | Runnable examples that use only the public facade. |
| `crates/xtask/` | The `cargo xtask` task runner, the layer table (`src/layers.rs`) and the explicit policy registry (`policy/explicit.toml`). |
| `csharp/Disunity/` | `Disunity.Engine`: the only `UnityEngine` implementation. Files ending in `.g.cs` are generated from the Rust ABI and must not be hand-edited. |
| `csharp/Disunity.Tests/` | Console tests for the managed API, run by `cargo xtask ci-managed`. |
| `csharp/Disunity.SceneTests/` | Loads the pinned Snake, Pong and Breakout scenes and checks loader rules and scene state; run by `crates/disunity-runtime/tests/original_scenes.rs`. |
| `csharp/Disunity.Play/` | The managed side of the `play` command: loads a scene or starts a built player, then returns to the Rust host that advances and renders. |
| `csharp/Disunity.Layouts/` | Writes the script field layouts of a built player's assemblies for `unity-import` to decode script data with. |
| `csharp/OriginalGames/` | Build projects that compile each pinned game's unchanged scripts against `Disunity.Engine`. |
| `csharp/PortableFixture/`, `csharp/UnityConsumer/` | A fixture assembly with a reference host bridge, and a project that proves `UnityEngine` code compiles against `Disunity.Engine` alone. |
| `csharp/README.md` | ABI binding regeneration and managed host behavior. |
| `docs/` | User documentation. See [what belongs in docs](../docs/what-belongs-here.md). |
| `dev-docs/` | This directory: architecture, quality gates and subsystem contracts that span more than one crate. |
| `.github/workflows/` | CI. `pr.yml` runs the Linux `PR gate`; `platforms.yml` runs Windows, macOS and the full serial GPU suite after merge and nightly. `.github/scripts/` holds the change classifier and failure reporter. See [quality gates](quality-gates.md#continuous-integration). |
| `.llxprt/` | Agent configuration: `LLXPRT.md` (shared memory) and `skills/`. Nothing else in it is tracked. |
| `.work/` | Untracked scratch for tool installs and caches. Keep it small and delete what you create. |
| `target/` | Cargo output. Untracked. |

## Pages in this directory

| Page | Covers |
| --- | --- |
| [contribution-agent.md](contribution-agent.md) | Agent steps, scoped claims, review-ready delivery and links to authoritative rules. |
| [built-players.md](built-players.md) | How a built player is read, planned, loaded and run, stage by stage, with each stage's owner and contract. |
| [architecture.md](architecture.md) | Ownership, crate layers, how a frame flows, invariants, coordinate systems. |
| [quality-gates.md](quality-gates.md) | `cargo xtask` commands, CI profiles, the layer and policy checks, size and complexity limits. |
| [simulation/advancement.md](simulation/advancement.md) | `advance`/`resume`, callback order, nested lifecycle scopes, time, input and coroutines. |
| [simulation/physics2d.md](simulation/physics2d.md) | Bodies, shapes, continuous collision, materials, contacts and queries. |
| [simulation/ui.md](simulation/ui.md) | Canvas and RectTransform layout, fonts, text shaping limits. |
| [simulation/sprites.md](simulation/sprites.md) | Sprite, tile and camera extraction, and presentation export from the managed host. |
| [rendering/services.md](rendering/services.md) | What each GPU service crate owns and what callers own. |
| [rendering/frame-executor.md](rendering/frame-executor.md) | One executor and one submission, frames in flight, external features, GPU pass timings. |
| [rendering/resource-contracts.md](rendering/resource-contracts.md) | Resource registry, checked views and bindings, authored raster passes. |
| [rendering/typed-operations.md](rendering/typed-operations.md) | Mixed raster, compute, copy, upload and readback lists. |
| [rendering/checked-draws.md](rendering/checked-draws.md) | Caller-owned draw providers inside executor passes; externally owned frame targets. |
| [rendering/capture-continuation.md](rendering/capture-continuation.md) | Two-segment captures with a CPU readback between them. |
| [rendering/configuration.md](rendering/configuration.md) | Budgeted renderer configuration replacement. |
| [rendering/2d.md](rendering/2d.md) | Per-texture samplers, global 2D lights, camera participants. |
| [rendering/scene-materials.md](rendering/scene-materials.md) | Material features, water bodies, shared frame products. |
| [rendering/terrain-pages.md](rendering/terrain-pages.md) | Adaptive terrain material pages and residency. |
| [rendering/atmosphere.md](rendering/atmosphere.md) | Sky, celestial bodies, height fog, environment preparation. |
| [rendering/ambient-occlusion.md](rendering/ambient-occlusion.md) | SSAO kernels, filters and metadata reductions. |
| [rendering/reflections.md](rendering/reflections.md) | Screen-space reflection composition. |
| [rendering/post-filters.md](rendering/post-filters.md) | The math behind bloom and FXAA. |

Crate READMEs with contracts of their own: [camera](../crates/camera/README.md), [render-plan](../crates/render-plan/README.md), [render-scene](../crates/render-scene/README.md), [render-post](../crates/render-post/README.md), [world-schema](../crates/world-schema/README.md), [world-simulation](../crates/world-simulation/README.md), [world-generation determinism](../crates/world-generation/DETERMINISM.md).

## Which documentation to trust

Repository policy in [AGENTS.md](../AGENTS.md) and [CONTRIBUTING.md](../CONTRIBUTING.md) governs contributions. For implemented behavior, the code on `main` is the source of truth. These pages describe what it does now. If a page disagrees with the code, the page is wrong: fix it in the same PR as the change you are making, or open an issue. Do not write code to match a stale page.

Useful in a doc, and worth reading:

- What a type or function guarantees: units, ranges, ordering, what it rejects and with which error.
- Which crate owns a behavior, and which callers must supply.
- Invariants that are easy to break and expensive to find: single world, single host, generated bindings, determinism.
- Which tests pin a behavior, so you know what to run after changing it.

Not useful, and not to be written:

- History: what used to happen, what a PR changed, what was removed or "retained". Git log and issues hold that.
- Migration or upgrade guides between versions. Nothing is released; change the code and the docs together.
- Plan labels such as "E4" or "slice 2" in place of a description of the behavior.
- Verification receipts, hash manifests, provenance notes, before/after audits and session logs. CI and the PR are the record.
- Restatements of the code line by line. Link to the file instead.
- Hedging about what "may" be supported. State what is supported and what fails.

When a behavior is an approximation (a fitted curve, a simplified lighting model, a fixed iteration count, a tolerance), say so on the page that documents it: what it approximates, the assumption it rests on, where the result departs from the exact behavior, and which test bounds the error.

## Rules that apply to every change

These come from `.llxprt/skills/disunity-code-standards`. CI enforces the configured checks; reviewers also enforce the architecture, source-use and behavior requirements.

- Functions at most 60 lines, 6 arguments and cognitive complexity 15; type complexity 250; at most 3 `bool` fields per struct; source files under 1000 lines (aim for 750). Never raise a limit or add an `allow`.
- Warnings are errors. Run `cargo xtask ci-cpu`, and `ci-gpu` or `ci-managed` when you touch those areas, before pushing.
- Write the test first and watch it fail. Never weaken an assertion, widen a tolerance, add a retry or ignore a test to make it pass.
- When behavior changes, change it everywhere. No duplicate compatibility paths, `v1`/`v2` pairs, legacy twins or flags that select old behavior. `Disunity.Engine` owns thin C# compatibility, including managed values and delegate utilities.
- Code must build and pass on Linux, macOS and Windows.
- Original Unity game scripts, assets and behavior stay unchanged apart from API binding and harness configuration. Private `rd-stranded` validation and pin updates belong to its owner; the existing game pin protects its consumer until updated. External contributors need only public checks, with public regressions free of restricted game material and no secrets or private checkout in fork CI.
- New third-party files need an MIT-compatible license and its notices. The engine is MIT.

## Writing these pages

Follow the project writing guide ([acoliver/vibetools, docs/writing/styleguide.md](https://github.com/acoliver/vibetools/blob/main/docs/writing/styleguide.md)): specific statements, no em dashes, no "not X, but Y" pivots, no empty adjectives. Put a contract where its reader will look: in the crate README when it concerns one crate's API, here when it spans crates. Give each page a title that names its subject and an opening paragraph that says what it covers.
