---
name: disunity-code-standards
description: Use when writing or reviewing Rust or C# code in real-disunity. Defines the architecture ownership (world-simulation, disunity-ffi, DisunityHost, Disunity.Engine, render-plan/render-scene, unity-import), code-quality rules (separation of concerns, inversion of control, DRY, complexity and size limits) and forbids parallel stacks, compatibility shims and preserved legacy paths.
---

# real-disunity code standards

## Ownership (issue #38)

| Area | Owner |
| --- | --- |
| Running object state, hierarchy, clone, lifetime, lifecycle, clock, input, 2D physics, UI layout | `crates/world-simulation` (typed Rust, no JSON, CLR, wgpu or filesystem) |
| ABI boundary only: validation, handles, result ownership | `crates/disunity-ffi` |
| Managed execution, one host | `DisunityHost` in `crates/disunity-runtime` |
| `UnityEngine` names for C# games | `csharp/Disunity` (`Disunity.Engine`), one assembly |
| Render extraction and drawing | `render-plan` -> `render-scene` and the existing post/viewer services |
| Unity scene, prefab and asset import | `crates/unity-import`, an offline content tool |

Never add a second world, a second host, a second Unity API, a second renderer or a selectable "mode" or "profile" for the same behavior. Extend the owner.

## Architecture

- Separation of concerns: each crate and module owns one responsibility.
- Inversion of control: pass services and configuration in explicitly. No global state or ad hoc environment variables in library code.
- DRY: one implementation per concept. Shared test setup lives in one support module.
- Respect the layer rules enforced by `cargo xtask policy` and `crates/xtask/src/layers.rs`.

## Limits (enforced, never loosened)

- Functions: at most 60 lines, 6 arguments, cognitive complexity 15.
- Type complexity 250; at most 3 bool fields per struct.
- Source files: aim under 750 lines, hard limit 1000. Split by responsibility.
- Warnings are errors. Never add an `allow`, raise a limit or add an exception.

## Change discipline

- When behavior changes, change it everywhere. No legacy twins, compatibility shims, `v1`/`v2` pairs or flags that select old behavior.
- No per-game overlays, field aliases or substitutions. If a game needs something, implement the general engine or importer behavior and test it with synthetic fixtures.
- Unsupported Unity API or content fails explicitly with an actionable error. No no-op stand-ins.
- Tests assert real behavior. Do not weaken an assertion, widen a tolerance, add a retry or ignore a test to make it pass.
- Write the test first and see it fail.
