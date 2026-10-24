# Test-first implementation plan

Plan ID: `PLAN-20261020-UNITY-TCK`. Requirements and proposed lane APIs are in [spec.md](spec.md); budgets, isolation, ownership and current state are in [execution.md](execution.md). This is an executable work plan, with proposed commands clearly separated from checks already performed.

## Execution model

Run P00 through P15 in order. Each phase has an accountable worker and a separate verifier; each verifier reads the current specification, examines behavior and reruns the relevant checks. P03/P05/P07/P09/P11/P13 are independent test-authoring/red-verification tasks. Their acceptance requires the intended behavioral failure, not a compiler/import/tool error. If a missing declaration prevents execution, add the minimum compiling interface only, then demonstrate the missing behavior. P04/P06/P08/P10/P12/P14 implement the preceding tests in small red-green increments. A green phase can return to its red phase for newly discovered behavior; it cannot silently rewrite expectations to fit implementation.

The template methodology supplies spec/preflight/analysis/contracts, sequential worker/verifier gates and integration-first testing. This task deliberately consolidates it into three documents. Requirement/case IDs in test names and one phase record provide traceability; no per-function marker boilerplate, arbitrary property-test quota, extra completion-marker files or automatic rollback is required. Production changes remain focused, existing tests stay intact except separately reviewed contract corrections, and unrelated user/neighbor work is preserved. This planning run spawns no agents and makes no commits. Execution roles below are assignments for the foreground coordinator to schedule.

An hourly checkpoint and every phase-end integration run the accumulated real slice. Before code depends on an unmerged API, P02 publishes its concrete signature/schema to both owners. Within a phase, shared-file edits are serialized. At most two compile jobs run across the effort, and only one GPU/window verifier owns the test display. The first usable release is P04, including the live dashboard and generic harness, before game-specific expansion.

## Phase sequence and acceptance gates

Budgets below include the immediate verifier. They are effort allocation ceilings inside the authorized wall interval, with 1.5 hours reserved for remediation. Foreground setup and planning consume the same interval. Recompute remaining budgets from actual time; the absolute deadline governs.

| Phase | Budget | Accountable worker → verifier | Deliverable / gate |
|---|---:|---|---|
| P00 | 0.50 h | architect → specification reviewer | These three grounded documents; actual source and neighbor boundaries; no implementation. |
| P01 | 0.50 h | H/preflight → T/environment reviewer | Output isolation, dependencies, baseline and content/API census verified; blockers visible in dashboard backlog. |
| P02 | 0.50 h | R/architect → M+T contract reviewer | Cross-lane ABI/layout/schema/camera/host contracts reviewed, source matrix and bounded algorithms agreed. |
| P03 | 0.75 h | T/harness tests → independent H reviewer | Red CLI/dashboard/fixture/worker integration tests with real session/render boundary. |
| P04 | 1.75 h | H/harness → T/desktop verifier | Working dashboard + generic harness + refresh flow first, including live native probe, actual capture, failure and cancellation. |
| P05 | 1.00 h | T/engine contracts → independent M reviewer | Characterization and failing public native/managed/clock/input/view tests. |
| P06 | 3.00 h | R/engine integrator with M subtask → T/engine verifier | Canonical simulation/FFI, external facades/private dispatch, shared view and live host integrated. |
| P07 | 0.75 h | T/import/Snake tests → independent M reviewer | Red actual import/overlay/external-script and complete Snake journey assertions. |
| P08 | 2.50 h | T/import integrator with R/M providers → H+independent T verifier | Complete unchanged-source Snake played/captured through dashboard and generic harness. |
| P09 | 0.75 h | T/physics/Breakout tests → independent R reviewer | Red real solver, package input, tile/sprite/coroutine and all Breakout feature contracts. |
| P10 | 3.00 h | R/physics integrator with M/T subtasks → independent T verifier | Complete unchanged-source Breakout, both powerups, actual play/reset/captures; Snake still passes. |
| P11 | 0.75 h | T/legacy/Pong tests → independent M reviewer | Red authentic legacy graph/events/UI/sleep/coroutine and both-score journey assertions. |
| P12 | 2.25 h | M/Pong integrator with R/T providers → independent T verifier | Complete unchanged-source Pong played/scored/reset/relaunched with readable UI captures; earlier games pass. |
| P13 | 0.50 h | T/migration consumer tests → independent R reviewer | Red additive source-interop/Stranded-compatible view/targets/stages/input/capture contracts. |
| P14 | 1.00 h | R/shared API integrator → T/migration verifier | Local native consumer uses shared contracts; preserve existing render/shadow/generation/portable consumers. |
| P15 | 3.00 h | H/release integrator → independent T acceptance reviewer | All required games, dashboard refresh/rerun, cumulative quality and teardown accepted; remaining limitations explicit. |

The allocation totals 22.5 hours plus 1.5 hours reserve. No phase is omitted to recover time. If setup, review or a gate exceeds its allowance, record the revised forecast and highest-risk blocker immediately. Continue safe independent red/preflight work while fixing the blocker; do not mark a dependent production phase accepted.

## P00: Specification and source baseline

P00 establishes REQ-01 through REQ-11. Review the source-backed integration table and the explicit difference between inspected cd06502 and the already-local canonical-rendering branch. Confirm that all three complete games, source preservation, original-content versus overlay fidelity, dashboard-first and Rust authority are explicit. The planning verifier checks that proposed APIs are labelled proposed and that no implementation/test result is claimed.

Deliverables are exactly `project-plans/unity-tck/{spec.md,plan.md,execution.md}`. P00 can be accepted after document/source/link checks. It does not imply P01 has passed.

## P01: Preflight before behavior implementation

H coordinates with the foreground owner, who is already setting up tools and launching the harness. Reuse their local environment rather than installing or launching competing infrastructure.

1. Source `.session/env.sh` after reading it. Check every write destination, including rustup/Cargo, NuGet/MSBuild, font/GPU/browser caches, home/temp, X11 files and subprocess outputs. Read-only external tools and fixtures are permitted; output paths outside the worktree are rejected. Record disk usage and active process/output reservations.
2. Verify installed Rust 1.97, .NET SDK/runtime/hostfxr/nethost, Vulkan adapter/features, X11 driver, PNG/browser interaction tooling and Python dependencies. Use already installed tools or worktree-local provisioning. Tool absence is an actionable prerequisite, not an intended red test. Also inspect xtask::commands::process: committed code overrides CARGO_TARGET_DIR with root/target/validation or validation-alt. Update that construction with behavioral output-path tests so all new validation artifacts go under .work; environment export alone is insufficient.
3. Run existing CPU/managed/GPU/viewer checks within isolated output settings, recording baseline failures separately. Confirm portable triangle loading and the native image resolver before new session work. Existing managed tests build fixtures in csharp/bin and obj; redirect those into .work and update their explicit artifact discovery with tests before enforcing the new layout.
4. Rewalk all 35 gameplay/support files, selected scenes/prefabs/metas/settings and the comparison inventory. Select the exact player compiler Include graph; inspect all source-required forms, aliases and serialized fields. Keep Pong's 11 tests separate.
5. Resolve missing Snake sprite via an independently authored overlay; inspect Breakout asset notices and stale values; inspect both Pong goal overrides and font references. Produce content-ready, overlay-required or blocked per edge. Do not wait for a frozen-SHA approval round.
6. Evaluate a native 2D solver, selected YAML parser and font decoder against local availability, license, determinism and required geometry/CCD/MovePosition/shape updates. A solver candidate must pass a bounded executable probe before selection; record the decision and new dependency/lock changes. Do not copy official implementation material.
7. Resolve rendering API skew: inspect local `0c144717`/`origin/refactor/canonical-rendering-api` diff against main, identify what foreground/neighbor agent already owns, and choose an additive integration order. Do not independently recreate GPU/post/lighting services already present there. No checkout/merge in a neighboring tree.

Verifier checks real commands and exact caller/provider types. All required assumptions must be verified or narrowed with an explicit failing capability result before the affected production task. Unresolved rights do not license asset distribution; blocked original-content lanes coexist with independently owned overlay lanes.

## P02: Cross-lane contracts and analysis

R and M inspect both sides of the spec's current native response/managed entry families, sizing rules, schemas, callback eligibility, session attachment and error/retry semantics. H confirms RunControl/dashboard requests and typed public selectors; T confirms importer-to-materializer schemas. Record any corrections in execution.md before implementation. Keep v1 exports and portable fixture behavior unchanged.

Use these numbered algorithms as the analysis blueprint. Split helpers as needed; trace phase-level work to these ranges without adding marker headers to every function.

### Algorithm A: Fixture and run admission

```text
01 Resolve permitted fixture/ref and copy an immutable run snapshot inside .work.
02 Walk source/content closure; compute current hashes and overlay preconditions.
03 Compare local/upstream identity; stage refresh and affected test selection if changed.
04 Validate schemas, capabilities, paths, output reservations and required tools.
05 Build original-source assemblies from explicit Includes and facade references.
06 Admit canonical content/resources; bind script fields before activation.
07 Publish admission status; create one native session and one host only on success.
08 Preserve the previous active fixture until refresh tests accept the new revision.
```

### Algorithm B: One engine frame and callback bridge

```text
09 Apply ordered normalized OS/replay ingress to selected input projections.
10 Accumulate rational time; choose zero or more fixed steps from native clock policy.
11 Build eligible initialization/fixed callbacks and release world locks/borrows.
12 Dispatch managed messages; reentrant public commands/query use the same session.
13 Commit validated body/shape changes, step one solver, snapshot real pair events.
14 Deliver contact messages outside locks; honor deferred destruction/query policy.
15 Dispatch eligible Update, due coroutine continuations and LateUpdate.
16 Apply destruction, settle native UI, extract immutable state and resolved view.
17 Update canonical resident render contributions, draw and present when requested.
18 Capture the tagged target; publish frame/phase values and resource diagnostics.
```

### Algorithm C: Supervision and teardown

```text
19 Serialize RunControl commands; reject past input, invalid generations and recursion.
20 Stream bounded results and artifacts to a create-new run directory.
21 On assertion failure preserve observations and continue required teardown.
22 On hang/crash/disconnect cancel only the owned worker group and finalize error.
23 Detach managed roots, release readbacks/resources, destroy the session once.
24 Recheck original input bytes, record drains and publish atomic run/dashboard index.
25 Evaluate required statuses and complete assertions; return the documented exit code.
```

The verifier exercises contract examples on paper and checks ABI widths and serialized context values against both language representations. P05 turns them into executed round-trip and behavioral tests. Final agreements include the pending-Destroy visibility, Start eligibility under creation, coroutine cancellation and collider synchronization policies, with explicit training/source expectation provenance and bounded tests. A model's agreement is not external Unity verification.

## P03/P04: Generic harness and working dashboard first

P03 writes integration tests before component tests. Execute the proposed CLI/dashboard routes against a disposable real child using the existing native triangle/session and renderer. Test selectable game entries with missing providers, valid native state/capture, changed input/source hash detection, atomic refresh acceptance and rejection, wrong expected state, malformed fixture, child crash/hang/cancel, partial result retention and output traversal rejection. Browser tests must click Run/Stop/Refresh and inspect actual expected/observed rows and the captured image. Do not populate the dashboard with invented green game results.

The independent red verifier confirms intended failures for missing supervisor/report/UI behavior. Minimal command/declaration scaffolding is allowed solely to reach those failures. A missing browser or native library is a preflight failure and must be corrected first.

P04 extends the in-flight `compat-kit` Tool-layer crate and extends existing `crates/xtask/src/commands.rs`, main dispatch, metadata/layers and quality selections. Implement one Rust supervisor/worker protocol, fixture manager, result/status/gate reducers, bounded artifacts and local dashboard. Add dashboard assets under tools/unity-tck; serve them through the same Rust process. Preserve existing report generator and documents. Integrate current viewer/render services for the real native probe and frame capture. No game-specific engine branches. The observed compat-kit main currently prints implementation pending and exits 2; compat-content import_project currently returns an empty scene. These are red-phase scaffolds, not completed features. Recheck them before work because foreground edits are active.

P04 gate: launch from dashboard, inspect actual native output and image, deliberately fail one assertion, stop one live worker, refresh a changed local fixture through tests, rerun from the dashboard, export/open the report, and observe drain/exit statuses. Run all accumulated checks. Game cards may still show unbuilt providers, but the dashboard/harness itself must work fully before P05 expansion.

## P05/P06: Canonical simulation, ABI, facade and shared view

P05 first characterizes existing v1 queue visibility, invalid handles, partial application, export truncation, native image selection, triangle transforms and 100-cycle unload/token drain. Then add public integration tests:

- Two components share one owner/Transform; clone graph internal references remap; activity and Unity-null survive destroy/reuse/reload.
- External private Awake/Start/Update/FixedUpdate methods run in selected order; serialized references exist at Awake; reentrant Instantiate/read/write completes; recursive Advance and wrong-thread access fail without mutation.
- A native mutation is read through the facade; managed mutation is read natively and rendered. Existing transform caches cannot satisfy this test.
- Six 1/60 frames produce five .02 fixed steps; pause/resume, rational boundaries, repeated GetKeyDown, short taps and focus release produce expected state.
- C#/Rust serialized frame context/callback round-trip values and buffer bounds work. Incompatible handshake, duplicate/expired request IDs, callback exceptions, sizing retry and oversized input produce their exact errors without duplicate mutation or callback dispatch.
- Orthographic half-height, explicit planar basis, resize landmarks and inverse queries match actual pixels through the renderer; existing perspective/FreeFly/RTS/Orbit callers retain behavior.

P06 generalizes the in-flight `disunity-ffi/src/{world,components,transform,input,physics,protocol,response}.rs` modules into one coherent CPU simulation owner. A later extraction to `crates/simulation` is justified only if it improves reuse without duplicating or delaying the working session. Add a CPU importer/materializer boundary only as needed and keep FFI as the sole image registry. Extend disunity-runtime's dynamically loaded native adapter and bridge. Extend the in-flight `csharp/UnityCompat` facade and managed bridge, separating public module identities only where compilation requires it; compile game scripts in independent assemblies through .work build plans. Replace Disunity transform authority and scheduling with compatibility adapters. Keep any static native simulation library free of a separately instantiated global registry.

Extend camera and SceneView to one resolved view; adapt scene-viewer camera, app and renderer to a live host/session API exposed through worldrunner. Existing static viewer `run` remains callable. Do not replace render-scene with a second sprite renderer. Sprite pipeline work later extends its draw/extraction model.

Verifier runs CPU/ABI/managed/view/GPU tests, private-callback fixture from another assembly, deliberate stale-cache and disconnected-render negative controls, and the portable triangle soak. Every failure is shown in the already working dashboard. This phase ends with a real live managed microfixture controlled and captured by RunControl.

## P07/P08: Authentic import and full Snake

P07 writes actual selected YAML/meta/config/prefab import tests with missing GUID/fileID, unknown required component, duplicate GUID, invalid overlay precondition and cleanup-on-binding-failure cases. Test source bytes before/after build and after run. Compile all eight original files. Test exact Vector2Int/hash/division/conversion forms, parent-only Instantiate, delayed growth, one-tick-per-Update and speed cap through public source interfaces.

Define the complete desktop Snake assertions from REQ-06 before implementing providers. Run them through generic RunControl and verify intended red outcomes. The food driver reads public FoodSpawner.CurrentFoodPos and admitted visible segment transforms; it submits input, with bounded routing/failure, and never mutates private state. Use multiple source-driven seeds/config edge cases rather than a single lucky route.

P08 completes the in-flight `crates/compat-content` / `worldrunner-compat-content` (World-layer, CPU-only) as a selected-schema adapter to existing asset/world admission and R materialization. Add fixture and overlay data under `fixtures/unity-tck/snake`, leaving original checkout bytes untouched. Import camera, config, prefab references, sprite metadata and native clone/activity state. Complete the in-flight `render-scene::DynamicQuad` quad/UV/alpha/sorting path and connect it to canonical extraction/contributions and resident update APIs. Independently author the missing-sprite replacement and label the content variant.

P08 gate: actual GameScene play with turns/reversal/wrap, food/growth/speed, self-contact/Game Over, reset and renewed control; captures from the real harness and a bounded exploratory run. Original-content missing-sprite failure stays visible. Re-run dashboard, native/managed/triangle/view tests and fixture refresh. An empty board or drawn imitation Snake cannot pass.

## P09/P10: Native Physics2D and full Breakout

P09 adds integration tests through public facade and native state, then focused solver tests. Cover zero-gravity integration, dynamic/static/kinematic contacts, circle/box and imported wall geometry, elastic material, continuous thin-wall contact, MovePosition, collider resize/disable, tag/layer/trigger filters, sleep/teleport/wake and both velocity aliases. Assert real trajectories, contacts and visible transforms with quantity-specific tolerances chosen before implementation. No per-game reflection/bounce engine.

Compile all seven original Breakout files. Add selected tilemap collision/render import and spritesheet subasset sizing/PPU/pivot tests. Verify source D-over-A precedence, width-dependent clamp, immediate coroutine first segment, exact frame sequence, stopped prior width routine, scaled recovery and destruction/query visibility used by respawn. Full-game tests require actual brick hits and both multiball and widening via seeded genuine contacts. T's red verifier checks that bypassing physics or collider disable makes assertions fail.

P10 integrates the selected solver behind simulation components and adapts package input, collision payloads and coroutine dispatch. T adds source-specific overlays/settings/field mapping and assets. M adds package/core/physics facade declarations and adapters without engine-owned caches. R extends canonical tile geometry, resident sprite updates and selected light/material behavior. Record fidelity separately for original pipeline and named translation; no unsupported original-pipeline result becomes pass through relabelling.

P10 gate: paddle control/clamp, ball launch/speed, wall/paddle/brick response, full break animation, collider disable/destruction, last-ball respawn and multiball no-premature-respawn, both powerups, recovery/reset and renewed control. Capture input, brick break, widened/multiball state and post-reset state. Replay Snake and all accumulated microcases through the same dashboard/harness.

## P11/P12: Historical Pong, native UI and persistent events

P11 compiles all twenty source files independently. Import authentic Main and Resources prefab closure, both goal mappings, axes, score listeners and font. Add tests for stale property/type preconditions, left/right enum listener targets, score dictionary/view agreement, fixed-step paddle motion, zeroed angular/linear velocity at reset, native teleport/sleep and velocity wake. Frame-controlled waits test no early relaunch and first eligible continuation.

UI tests go through native Canvas/RectTransform/Text layout and existing renderer. Check fixed/stretched anchor examples from the training reference after settling, resize, parent visibility and readable 0→1 numeric pixel regions. Source-backed event tests plus actual goal physics must fail if the persistent listener, score update, native body reset or text renderer is disconnected.

P12 implements the legacy field/type mapper and facade aliases, native persistent-listener registration and events, selected UI state/layout/text geometry, independently admitted font path and shared rendering contributions. C# UnityEvent holds callable metadata/delegates as glue; Rust owns persistent registration, lifetime and emission scheduling. Historical source proxies use default real facade services. Do not inject mocks into the acceptance game.

P12 gate: both paddle control schemes, actual bounce and both goals, public/visible scores, hide/reset/sleep/wait/show/relaunch, repeated score and reload with renewed control, actual screenshots/exploration. Run the complete earlier games and microcases. Report the separate eleven-test historical tooling profile honestly; NSubstitute absence cannot be hidden in the player compilation gate.

## P13/P14: Source-interop and future Stranded compatibility

P13 re-reads neighbor refs and dirty paths read-only. Write a local consumer fixture under .work/checkouts or a test-owned local crate which consumes the current SDK through worldrunner. It exercises the spec's ResolvedView, explicit generation request, externally supplied device/queue/target, post-stage registration, shared texture ownership, input normalization and correlated readback. No build runs in neighboring roots. If original Stranded source is staged locally, use an isolated copy with local dependency overrides and no source modifications to its external original.

Characterize camera_matrices FreeFly/RTS raster versus reconstruction, fixed capture view, native HDR/readback and streaming-readiness behavior. Test that a canonical shadow change reaches both scene and migration consumer without duplicating allocation/pass code. Existing rd-stranded lighting numerical parity/native-capture regression work is reserved for its active agent. Consume its accepted APIs and tests rather than reproducing that work.

P14 adds only the missing additive seams and adapts local consumers. If foreground integrates `0c144717`, migrate existing scene/viewer internals onto those canonical GPU/post/lighting services and retire duplicate mechanics in the changed paths. Preserve advanced caller-authored effects and state/readiness policies. No broad upstream merge or Stranded rewrite is implied. An unavailable source branch/content blocks that extra migration run, with exact missing input recorded; the local compatibility consumer still runs.

P14 verifier reruns prior games, portable consumers, render-shadow, explicit generation invalid-input/determinism tests and camera/source-interop consumers. A snapshot/export/unload/re-grid implementation fails the live-session test. Any actual Stranded exploration uses the same RunControl with its native adapter and is labelled exploration, not full Stranded gameplay.

## P15: Independent integrated acceptance

Use actual production entry points and the live dashboard. Run all required catalog cases/applicable profiles, all three complete desktop journeys, offscreen comparisons, original-source checks, fixture change/refresh/accept/reject, crash/cancel/quota cases, repeated reload/token/world drains and all current quality commands. Independent reviewer opens the actual game screenshots and controls each game through its real window; retain a short structured exploration note/replay and correlated images. No screenshot fabrication or unexecuted success claim is acceptable.

Run `cargo xtask fmt`, `policy`, `source-size`, `ci-cpu`, `ci-managed`, `ci-gpu` and the final full `ci` as supported by the repository, all with worktree-local outputs. Focused commands and proposed TCK commands appear in execution.md. Failures remain attached to their commands and affected requirements. Do not lower gates or introduce ignores to meet the deadline.

Acceptance requires complete gameplay for all three admitted overlay/original variants, dashboard usability, shared state/view/render path, passing selected behavioral tests and teardown. Original-art or full-pipeline fidelity blockers remain separately visible, with no false equivalence claim. If required behavior remains incomplete at the deadline, publish the functioning system and exact failing requirements/tests as incomplete; never replace completion with a static evidence package.
