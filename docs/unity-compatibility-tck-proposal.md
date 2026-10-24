# Unity compatibility kit and first desktop game

**Research/design proposal — 2026-10-20**

**Repository baseline:** `acoliver/real-disunity`, `b4ee12fcba69c2ced21bef7bfa891caa6e1b5361`

**Status:** research/design only; **the Unity 2017 target below is superseded** by the user's Unity 6.3 decision. No compatibility runtime, importer, fixture, or game was implemented or executed for either report.

## Current decision — Unity 6.3 LTS desktop

The user selected **Unity 6.3 LTS (`6000.3`)** on 2026-10-20. The controlling follow-up is [Unity 6.3 target and sample-game research](unity-6.3-target-and-sample-research.md). It supersedes the target, candidate ranking, reference-editor plan, first-game scope, estimates and prioritized delivery order in sections 1–16 below; those sections remain historical research, not the active implementation backlog.

- **Recommended initial sample:** the **Brick** Breakout scene in Unity's official `Unity-Technologies/2d-techdemos`, immutable commit `6593d544df2ea598e51f5cf1d7165d5ed42ceba7`. The publisher's `6.3` branch resolves to that pin and its project settings say `6000.3.3f1`; README describes a playable game but only says 2021.1+ in its version guidance. This is explicit version-branch/source evidence, not a prose claim of testing on every 6.3 patch.
- **Proposed initial reference:** **`6000.3.26f1` (`de8167e7440c`)**, freshly verified against Unity's release API and release page (2026-10-08). This differs from the sample's saved editor. Reopening/upgrading and configuring a standalone build are future, recorded qualification gates, not work already performed.
- **First scope:** unchanged three Brick gameplay scripts, one imported scene, sprites, keyboard Input System action subset, 2D polygon/box/tile contacts, mutable tilemap and loss-triggered scene reload. No score UI, TMP, audio, full URP or arbitrary Unity compatibility claim. Packages and serialized assets expand the work well beyond three scripts.
- **Rights gate:** repository MIT is not a license for the referenced Input System/URP/2d-extras packages. Their Unity Companion License and its competitive-analysis/competing-product restriction require explicit review/clearance even for proposed internal reference work. No package implementation or proprietary assets may be copied into the independent runtime.
- **Tutorial evidence:** Unity Learn explicitly lists 6.3 for Roll-a-Ball and Sprite Flight, but an immutable, permissively licensed finished-project source was not established for either. Alternatives, source pins, license evidence and fallback are recorded in the follow-up.

The versioned, game-driven TCK architecture, unchanged-source integrity rules, explicit unsupported diagnostics, trusted managed-host boundary, separate CPU/managed/GPU/reference CI and file-level rights accounting remain useful design principles. Their 2017-specific APIs, replay seams, serialization assumptions, fixtures and tolerances do **not** automatically become 6.3 contracts. The previous 8–12 engineer-week Pong estimate does not apply to the new target.

## 1. Historical decision — superseded Unity 2017.1 target

Build a **versioned, game-driven Unity test-compatibility kit (TCK)**, not a general Unity replacement. First demonstrate an existing two-player desktop Pong running its original C# gameplay and imported scene/sprites through independently implemented APIs.

**Recommended target:** [mcrvaz/pong-tdd](https://github.com/mcrvaz/pong-tdd), pinned to **`e2d3368728e236b0ab5a04afb142a3ae49260dd1`** (the 2017 license-adding revision immediately after the initial release/build adjustments), using **Unity 2017.1.0f3** as its reference. Call the project **Pong Desktop / Unity 2017.1 profile**. Do not follow the default branch automatically.

Why this revision:

- Twenty small gameplay/support `.cs` files; one enabled scene; two PNG sprites; no bundled custom font, audio clips, packages manifest, network code, animation system, TMP, or asset bundles in the inspected tree.
- Root [MIT license, copyright 2017 Marcello Vaz](https://github.com/mcrvaz/pong-tdd/blob/e2d3368728e236b0ab5a04afb142a3ae49260dd1/LICENSE.MD). Original gameplay has input/time/random interfaces useful for transparent boundary replay, without translating or replacing the game rules.
- Includes real physics, keyboard movement, score display, serialized event bindings, and a two-second post-goal relaunch: small, but meaningful compatibility rather than a moving-square demo.
- The newer 2018 revision adds powerups, a restricted custom font, raycasts, cloning, more lifecycle complexity, and a scene override setting ball speed to zero. An earlier immutable revision is a much better first delivery.

**Approve a bounded feasibility milestone first:** establish the Unity reference and scene closure, compile unchanged original scripts, and prove dynamic `MovePosition` plus elastic collisions. Then fund the desktop milestone. Estimated total **8–12 engineer-weeks** for one experienced engineer, including integration/testing, not a delivery promise. Old-Unity availability, physics agreement, and UI import can change that estimate substantially.

**Success means:** unchanged original gameplay source + source-derived scene/prefab state + original two sprites, playable on desktop, with passing behavioral tests and a machine-readable limitations report. Independently supplied font glyphs/material implementation are explicit substitutions, not claims of identical Unity assets or typography. A hand-written Rust/Lua Pong, hand-recreated scene, or screenshot lookalike is **not** this milestone.

## 2. Evidence and limits

Labels used below:

- **Observed:** read in repository source/settings/license or official documentation.
- **Proposed:** a contract/design to implement and test, not existing functionality.
- **Unverified:** requires reference execution, legal/asset clarification, or additional engineering research.

Research used local source clones outside this workspace, under `/tmp/real-disunity-tck-research/`. All candidate commits below were resolved from Git; the chosen revision's complete `Assets/Scripts/**/*.cs`, scene/prefab serialized inventories, settings, root license, and selected tests were inspected. No Unity Editor, candidate game, candidate tests, .NET build, Cargo test, or graphics run was performed. The source/API census here is manual, **not** a completed compiler-derived reachability report.

## 3. What Real Disunity already provides

Local links are relative to this document. They describe the baseline, not promised results on all platforms.

| Existing boundary | Observed capability and constraint | Consequence for this project |
| --- | --- | --- |
| [Workspace](../Cargo.toml), [facade](../crates/worldrunner/src/lib.rs), [facade features](../crates/worldrunner/Cargo.toml) | Rust 2024, `wgpu` 30, `winit` 0.30; feature-gated engine facade. Games depend on `worldrunner` only. No managed-host facade feature presently exists. | Add an opt-in facade/composition path rather than direct game-to-engine dependencies. Keep no-default facade GPU/CLR-free. |
| [Lua host](../crates/script/src/host.rs), [contract tests](../crates/script/tests/contract.rs) | Sandboxed Lua 5.4 (`mlua`), seeded streams, instruction budgets, named hooks, read-only world queries and generation-stamped handles. | This is not C# or Unity compatibility. Do not translate Pong to Lua and call it unchanged-source execution. Keep Lua independent. |
| [.NET host](../crates/disunity-runtime/src/lib.rs), [bridge](../crates/disunity-runtime/src/bridge.rs) | Already hosts CoreCLR via hostfxr/.NET 8, explicitly selected native image/assembly/runtimeconfig/bridge type. One active host; CLR cannot unload. Current `Run` is a whole fixture slice, not a frame-by-frame player loop. | No new CLR is necessary. Extend the explicit bridge with load/advance/input/snapshot/unload, preserving existing fixture entry points. Isolate test cases by process where static state matters. |
| [Managed APIs](../csharp/Disunity/WorldRunner.cs), [objects](../csharp/Disunity/GameObject.cs), [lifecycle](../csharp/Disunity/MonoBehaviour.cs), [time](../csharp/Disunity/Time.cs) | `Disunity`, not `UnityEngine`. `GameObject` is sealed; `Transform` has position/Y rotation/material slot and a managed cache. `Vector3` has uppercase fields. `MonoBehaviour` has a `private protected` constructor and virtual callbacks; cannot directly host external Unity scripts. `AddComponent` creates a fresh GameObject per behavior. Step flushes/steps native state **before** callbacks; no `FixedUpdate`, component graph, input, physics or Unity-null semantics. | Reusing names by aliasing namespaces is insufficient. Introduce a separate managed compatibility surface; do not silently break the portable managed fixture to approximate Unity. |
| [Native FFI store](../crates/disunity-ffi/src/lib.rs), [ABI manifest](../crates/disunity-ffi/disunity_interface.json), [maintenance guide](../csharp/README.md) | Generation-checked world/object handles, copied command batches, position/Y rotation/material slots, bulk export; native `step_world` applies queued commands and ignores elapsed time. Twelve ABI functions, append-only interface, generated binding byte comparison. No physics simulation. | Reuse identity, batching, explicit ownership, ABI generation discipline. Add exports rather than change layouts/signatures. Implement physics/state ownership behind this boundary, not a second unrelated native store. |
| [Managed integration test](../crates/disunity-runtime/tests/managed.rs) | Test code boots real .NET, exports three fixture objects and checks 100 load/unload cycles with drained worlds/tokens. | Good host-lifetime foundation, **not** evidence of Unity semantics. Preserve it and add compatibility behavior tests. |
| [Render planning](../crates/render-plan/README.md), [renderer](../crates/render-scene/README.md), [view](../crates/render-scene/src/view.rs) | Pathless validated scenes, meshes/textures, lit opaque/alpha-mask materials, offscreen RGBA. Main view is perspective; orthographic shadow math is not an orthographic game camera. No Unity importer, sprite/UI system, or general alpha-blend guarantee. | Need an explicit orthographic, unlit, sorted sprite/text path and dynamic frame state; not just asset decoding. Keep asset lookup outside GPU code. |
| [Desktop viewer](../crates/scene-viewer/src/app.rs), [API](../crates/scene-viewer/src/lib.rs) | Resident static scene, orbit input, resize/suspend/surface recovery. No game update callback or managed state loop. Uses Z-up scene camera conventions. | Reuse presentation/device techniques via a game-host seam. Do not put Pong rules into the orbit viewer or reupload the entire scene each frame. |
| [World schema](../crates/world-schema/README.md), [source locks](../crates/import-schema/src/source_lock.rs) | Generic world documents deliberately are not external scene/save formats. Spatial adapters own conversions; observation locks record content-addressed evidence. | Define a separate Unity import IR; reuse identity/hash/provenance primitives where semantics match, not reinterpret arbitrary Unity YAML as `WorldDocument`. |
| [Quality gates](quality.md), [layer registry](../crates/xtask/src/layers.rs), [CI](../.github/workflows/quality.yml), [GPU CI](../.github/workflows/software-vulkan.yml) | Explicit package classifications; foundation/CPU paths forbid transitive GPU/CLR. CPU and managed OS matrices; Linux lavapipe/Xvfb GPU suite; missing prerequisites fail. | Classify new crates explicitly; retain existing gates; create separate CPU, managed, GPU, and licensed-reference tiers. Do not make Unity an ordinary build prerequisite. |

No explicit no-C#/no-CLR restriction was found; the concrete implementation already includes both Lua and .NET. No standalone Unity compatibility roadmap was found in the checked-in docs. The remote open-issue query returned zero issues during research; it is not a historical roadmap assessment. This proposal supplies a new, scoped backlog without asserting that unrelated renderer/world features already support Unity games. It does not alter this repository's licensing policy.

## 4. Ranked candidate shortlist

Ranking is for **first bring-up**, not game quality. Root source licensing is separate from rights to every bundled asset or package.

| Rank / candidate and research pin | Version and actual inspected surface | Source license | Asset/dependency findings and decision |
| --- | --- | --- | --- |
| **1. [mcrvaz/pong-tdd](https://github.com/mcrvaz/pong-tdd/tree/e2d3368728e236b0ab5a04afb142a3ae49260dd1)** | `2017.1.0f3` in settings; 20 gameplay/support scripts, one Main scene, 7 scene prefab instances, 2 sprites, UI scores, coroutine relaunch, box physics. | MIT; full license read. | Chosen revision has no font/audio files. PNGs have no separate license/attribution found: root MIT is the available permission evidence, **independent image authorship not established**. Built-in font/material/UI references need explicit mapping. Exclude editor tests/NSubstitute DLL from runtime fixture. **Recommended.** |
| **2. [kwyckmans/unity-pong](https://github.com/kwyckmans/unity-pong/tree/0196c7a66bccdbdf9e1bba14c55495ebdc22c35d)** | `5.5.4p4`; four scripts read completely; Main/Menu scenes; OnGUI, GUIStyle/skin, camera/screen conversion, AudioSource, Cursor, SceneManager, collision-exit adjustment and LateUpdate. | MIT, copyright 2019 Wyckd; license read. | One paddle PNG and three OGGs, no independent asset license established. Readme credits Grogansoft tutorial and [ainc/unity-pong](https://github.com/ainc/unity-pong), requiring upstream review. Fewer scripts but wider legacy API/UI/audio scope. **Fallback after rights/reference review, not a shortcut.** |
| **3. [nicolaracco/Pong](https://github.com/nicolaracco/Pong/tree/c27ad0c6c7633eb9fc6c2de46229b1fbdd15a449)** | `2019.2.9f1`; source/scene/package inventory plus representative `Disc.cs`, `MainMenu.cs`, `UIBehaviours/ScoreOverlay.cs` read. AI/FSM, menu/game scenes, prefab arena, audio, TMP, CircleCollider2D, UI transitions. Windows/macOS downloads advertised by README. | MIT, copyright 2019 Nicola Racco; license read. | README credits Damaged Panda audio; linked [primary asset page](https://opengameart.org/content/100-plus-game-sound-effects-wavoggm4a) declares **CC BY 3.0**. Exact repository clips not hash-matched to that archive. Joystix TTF embeds Typodermic/Ray Larabie copyright; its distribution terms remain unresolved. TMP includes LiberationSans **OFL 1.1** and EmojiOne attribution directing readers to separate terms (not a complete license). **Later desktop UI/audio/AI target, conditional asset audit.** |

Other findings:

- [zigurous/unity-pong-tutorial](https://github.com/zigurous/unity-pong-tutorial/tree/e2d15e042748401153453d6dd836b9ebd87c897d), actual `2019.4.40f1`: all seven gameplay scripts inspected. Small single-scene tutorial, but **no license file found**; public availability is not permission. Includes a font and TMP/UGUI/2D package declarations. Uses `DefaultExecutionOrder`, `RequireComponent`, `TryGetComponent`, `Invoke/CancelInvoke`, UnityEvents, AddForce/contact geometry and private/inherited callbacks. Do not vendor unless rights are obtained.
- [Emerson-Mistico/clone-pong](https://github.com/Emerson-Mistico/clone-pong/tree/07716a402c94badb4c8558a06ab8aa4fe565b947), actual `6000.2.6f2`: inventory/version/package inspection, not a complete script review. Search text advertised MIT, but no root license file found in the clone. Menus/settings/localization/modern package scope; reject as an initial target.
- [Unity-Technologies/2d-techdemos](https://github.com/Unity-Technologies/2d-techdemos/tree/6593d544df2ea598e51f5cf1d7165d5ed42ceba7): root MIT copyright 2016 Unity Technologies read. Actual pin is **6000.3.3f1**, not the README's broad 2021.1+ guidance. All three `Assets/Tilemap/Brick/Scripts/*.cs` read: `Rigidbody2D.linearVelocity`, Input System actions, contacts, Tilemap `WorldToCell/SetTile`, SceneManager reload. Manifest includes URP 17.3.0, Input System 1.17.0, Git `2d-extras#package_6000.3`. Brick sprites have root MIT evidence, no separate notices found; full project/package rights closure not established. A useful **later Breakout progression**, not a tiny 2017-era substitute; prefer a separately researched older compatible pin rather than pretending this one uses legacy input.

### Why not the latest pong-tdd?

The initially inspected head, [`f627f9d446aa2702ddb380dd27eb524c4e6dedb2`](https://github.com/mcrvaz/pong-tdd/tree/f627f9d446aa2702ddb380dd27eb524c4e6dedb2), is `2018.1.6f1` despite README saying 2017.1. Its Main scene sets ball `speed` to `0`, and it adds grow powerups, countdown, raycasts, cloning of a player's active GameObject in `Start`, collision callbacks and layer filtering. `Assets/Fonts/Open 24 Display St.ttf` embeds: **“this font is an original design of southype | free for personal use.”** Score/countdown prefabs use it. Root MIT does not cure that restriction.

The chosen license commit adds only LICENSE/README relative to its immediate parent. Relative to release tag `v1.0` (`0657be38fccc279fe6b6adb20ca71beff14aeb12`) the intervening build adjustment moves PlayMode test files under Editor. Gameplay/scene selection is deliberate, not silently disabling features of the later game. Do not apply the later font/powerup/version findings to the selected pin.

## 5. Chosen target: source and scene requirements

All links in this section are immutable primary source links at the selected pin.

### 5.1 Original runtime source

| Source | Observable responsibility / API demand |
| --- | --- |
| [Player.cs](https://github.com/mcrvaz/pong-tdd/blob/e2d3368728e236b0ab5a04afb142a3ae49260dd1/Assets/Scripts/Player/Player.cs), [PlayerMovement.cs](https://github.com/mcrvaz/pong-tdd/blob/e2d3368728e236b0ab5a04afb142a3ae49260dd1/Assets/Scripts/Player/PlayerMovement.cs) | `Awake` obtains same-object Rigidbody2D; private `FixedUpdate` reads axis/time, computes Y movement and calls `MovePosition`. No paddle clamping code: collision geometry constrains motion. |
| [Ball.cs](https://github.com/mcrvaz/pong-tdd/blob/e2d3368728e236b0ab5a04afb142a3ae49260dd1/Assets/Scripts/Ball/Ball.cs), [BallMovement.cs](https://github.com/mcrvaz/pong-tdd/blob/e2d3368728e236b0ab5a04afb142a3ae49260dd1/Assets/Scripts/Ball/BallMovement.cs), [RandomUtils.cs](https://github.com/mcrvaz/pong-tdd/blob/e2d3368728e236b0ab5a04afb142a3ae49260dd1/Assets/Scripts/Utils/RandomUtils.cs) | Reads initial world position in `Awake`; `Start` launches with two independently signed speed components. Reset zeroes velocity/angular velocity, teleports transform and sleeps body. Hide/Show changes SpriteRenderer only, **not collider activation**. |
| [Goal.cs](https://github.com/mcrvaz/pong-tdd/blob/e2d3368728e236b0ab5a04afb142a3ae49260dd1/Assets/Scripts/Goal/Goal.cs) | Real `OnTriggerEnter2D(Collider2D)`, tag check, dynamic `UnityEvent<Players>` invokes score recipient. |
| [MatchManager.cs](https://github.com/mcrvaz/pong-tdd/blob/e2d3368728e236b0ab5a04afb142a3ae49260dd1/Assets/Scripts/Managers/MatchManager.cs) | Finds Ball by tag and ScoreManager by type in `Awake`. Goal increments score, resets/hides ball, starts IEnumerator/WaitForSeconds, shows/relaunches after configured delay. No startup countdown, win condition, menus, powerups or scene switching in this revision. |
| [Score source directory](https://github.com/mcrvaz/pong-tdd/tree/e2d3368728e236b0ab5a04afb142a3ae49260dd1/Assets/Scripts/Score) | ScoreManager, ScoreViewManager, ScoreView, Players: dictionary/list/enum .NET behavior; serialized list of ScoreView component references; same-object UI.Text lookup; `text` mutation and parsing. |
| [Proxies](https://github.com/mcrvaz/pong-tdd/tree/e2d3368728e236b0ab5a04afb142a3ae49260dd1/Assets/Scripts/Proxies), [interfaces](https://github.com/mcrvaz/pong-tdd/tree/e2d3368728e236b0ab5a04afb142a3ae49260dd1/Assets/Scripts/Interfaces), [Tags.cs](https://github.com/mcrvaz/pong-tdd/blob/e2d3368728e236b0ab5a04afb142a3ae49260dd1/Assets/Scripts/Constants/Tags.cs) | Compile all, including unused proxy methods: Input.GetAxisRaw, Time.deltaTime/fixedDeltaTime, Random.value, integer and float Random.Range. `SerializeField` attribute; normal CLR collections and IEnumerator. |

**Minimal compile/runtime surface:** `UnityEngine.Object`, GameObject, Component, Behaviour/MonoBehaviour, Transform, Vector2/Vector3 fields/conversions and constants used above, Rigidbody2D, Collider2D/BoxCollider2D, SpriteRenderer, SerializeField, Coroutine/YieldInstruction/WaitForSeconds, Input, Time, Random; `UnityEngine.Events.UnityEvent<T>`; `UnityEngine.UI.Text`. Serialized camera, physics material, sprites, Canvas/RectTransform/CanvasRenderer/scaler state also belong to the runtime closure even without script calls. Compile-only unused proxy members must not be falsely reported as behaviorally verified.

**Not required by this pin's gameplay:** `Instantiate`, `RequireComponent`, Physics2D.Raycast, LayerMask.GetMask, Mathf, Bounds, Quaternion APIs, AddForce, audio playback, SceneManager.LoadScene, TMP, tilemaps, 3D physics, modern Input System. Rotation/scale must still be understood in serialized transforms. Object lifetime/null behavior belongs to the foundational TCK even where this game does not exercise it.

### 5.2 Ingestion is a first-class deliverable

Inspect/import the closure of [Main.unity](https://github.com/mcrvaz/pong-tdd/blob/e2d3368728e236b0ab5a04afb142a3ae49260dd1/Assets/Scenes/Main.unity), [prefabs](https://github.com/mcrvaz/pong-tdd/tree/e2d3368728e236b0ab5a04afb142a3ae49260dd1/Assets/Resources/Prefabs), [sprites and `.meta`](https://github.com/mcrvaz/pong-tdd/tree/e2d3368728e236b0ab5a04afb142a3ae49260dd1/Assets/Sprites), and [ProjectSettings](https://github.com/mcrvaz/pong-tdd/tree/e2d3368728e236b0ab5a04afb142a3ae49260dd1/ProjectSettings).

Observed configuration:

- Main is the sole enabled build scene. Seven prefab instances, stripped objects, same-file and cross-file references, prefab property overrides and dynamic persistent score callbacks. Some local scene objects are not prefab instances. Do not merely instantiate base prefabs.
- Ball: speed 12 in prefab **and** scene override; tiny mass `0.0001`; gravity scale/linear drag zero; continuous collision detection; frozen rotation; BoxCollider2D. Two player bodies: **dynamic**, mass `1000000`, gravity scale zero, freeze X/rotation; speed 20; axes Vertical and AlternateVertical. The extreme mass ratio and dynamic MovePosition need a reference spike, not an automatic kinematic conversion.
- Elastic physics material: friction 0, bounciness 1. Static box bounds and trigger goal geometry include transforms with rotation/nonuniform scale. Import complete composed transforms and collider offsets/sizes; do not derive physics shape from visible pixels.
- Fixed timestep `0.02`, maximum allowed timestep `0.33333334`, timeScale 1. Physics velocity/position iterations 8/3, gravity `(0,-9.81)`, auto simulation enabled; collision matrix all ones. Objects use numeric layer 8 even though that layer name is blank; do not reject it or substitute the newer revision's named layers/masks.
- Keyboard bindings: W/S => Vertical, Up/Down => AlternateVertical. A duplicate joystick Vertical axis also exists in settings; declare keyboard-only profile, do not silently claim joystick aggregation support.
- Orthographic camera size 8, z=-10. Two original sprites have pivot `(0.5,0.5)` and 100 pixels/unit. Camera aspect/viewport, sorting, color, sprite metadata and physical-pixel output affect acceptance.
- Score prefab: two centered Text components, size 72, best fit off, anchor-based RectTransforms. Main Canvas is **Screen Space – Camera** (`m_RenderMode: 1`), scaler reference 1920×1080, width matching. Do not implement overlay-only UI and claim exact ingestion. Camera plane/reference resolution and anchors need tests.
- Scores/scene reference external UI script GUID `f70555f144d8491a825f0804e09c671c` with different fileIDs; source is not present in Assets. Fields indicate Text, EventSystem/input module, scaler/raycaster roles. Resolve the **GUID + fileID pair** via a version-specific, reference-validated class map or licensed editor inspection; this research did not verify that mapping from a Unity installation.
- Built-in font reference `(guid 0000000000000000e000000000000000, fileID 10102)` and sprite material reference must map to independent resources. `.meta` `licenseType: Free` records editor metadata, **not** an asset license grant.
- There are obsolete `team` prefab override paths although Goal uses `scoresTo`. Preserve/report these as stale serialized data; reference-check Unity's handling, then explicitly classify the exact paths as accepted inert data. An importer rejecting all unknown properties without such a policy would reject this project; silently dropping all unknown properties is equally wrong.
- Scene includes RenderSettings/lighting/navigation settings, AudioListener, Halo/GUILayer and EventSystem-related components. Some may be inert for this scene. Require a census and exact inert allowlist with reasons/evidence before calling the scene supported; no blanket “skip unknown MonoBehaviour” rule.

Sprite identity for a future fixture lock:

| Original file | SHA-256 |
| --- | --- |
| `Assets/Sprites/Player.png` | `6f865be9fa9c82fc3b84c83c3f965d72c5af85ed6075c897fce197ad96ca6f44` |
| `Assets/Sprites/Ball.png` | `9c8d24444b6b5b1d4eca67ae1842250f8ec4b3bb1af94b1d2bc7f1425c8a5e36` |

These hashes establish inspected bytes, not authorship or a completed legal audit.

## 6. Scope and source-execution contract

**In:** desktop keyboard local two-player game; one scene's startup/unload; orthographic sprites and numeric scores; box-based 2D contacts/triggers; fixed/frame lifecycle; coroutine relaunch; source-recompilation compatibility for the selected API profile. Linux x86-64 first interactive milestone, then Windows x86-64 and macOS arm64 qualification.

**Out:** binary drop-in UnityEngine replacement, loading arbitrary Unity-built assemblies/players, IL2CPP, Unity editor extensions, binary Unity asset bundles, proprietary shader execution, universal YAML/prefab support, full UGUI, Unity PRNG bit equivalence, arbitrary physics equivalence, mobile/web/consoles, joystick support, general sandboxing of C#.

### Proposed source path

1. Fetch exact source commit and verify a content lock. Select `Assets/Scripts/**/*.cs` explicitly: **20 files unchanged**, retaining source notices and hashes. Exclude `Assets/Tests`, the checked-in NSubstitute DLL, generated Unity project files, Library/Temp, editor-only content and unrelated prefabs from shipped closure.
2. Compile an independently authored `UnityEngine`-namespace compatibility assembly (including Events) plus a narrow `UnityEngine.UI` implementation for net8.0. Compile original scripts against those, **not Unity's DLLs**. Assembly file names may be project-owned; namespace/API binding matters for recompiled source, not Unity binary identity.
3. Game compilation disables implicit usings/nullable analysis for upstream source as needed to avoid ambiguous `Random` and retroactive nullable warnings; engine/harness retain their stricter standards. Record explicit language/compiler/defines (including any required `UNITY_2017_1`/standalone symbols) and diagnostics. No regex rewriting, renaming private callbacks to overrides, or injecting new game logic.
4. A project-owned managed adapter exposes the explicit unmanaged host bridge. It loads a normalized scene IR, resolves script types, constructs the graph, binds serialized state/references/events, runs lifecycle, accepts frame input/time and exports observations/draw state.
5. The Rust desktop host boots existing CoreCLR and the single shared native FFI image, advances the adapter per frame, presents sprite/UI state, and drains it on shutdown. Native state remains authoritative for physical position/velocity; managed getters must observe physics changes, not stale caches.

This uses the **existing CLR**, not a hypothetical Rust C# interpreter. A source-to-Lua/Rust translator would need C# object/interface/collection/coroutine semantics and source maps before proving equivalence; it adds risk with no advantage for this milestone. If an actual future no-managed-runtime product requirement appears, reopen that decision explicitly. Native and managed code here is trusted: it can access .NET filesystem/process APIs. Execute untrusted game code only in an external OS isolation boundary.

### Integrity categories

Publish separately:

- **Original gameplay:** source hashes unchanged, true for the intended delivery.
- **Imported scene/assets:** values derived from pinned scene/prefabs/metas; a hand-authored canonical scene is only a bootstrapping fixture, not import acceptance.
- **Portability substitutions:** independent unlit material/shader and independently licensed numeric glyphs replacing built-in font reference; exact mapping listed. Reference variant receives the same replacement where possible. Typography is not original-asset parity.
- **Harness instrumentation:** adapters/probes and input/random seams disclosed; never inject scoring, reflection-bounce formulas or MatchManager calls to make an E2E test pass.
- **Upstream correction:** any necessary game source/scene bug fix must be a separately published patch and profile. Do not quietly fix behavior or keep the unchanged-source badge.

## 7. Layered TCK

The TCK is an executable compatibility contract for **declared profiles**, not a count of stubbed Unity classes. Every claim links to an observable test, version/reference evidence and limitations. Tests use supported APIs or visible player output, not private calls, file layout assertions or mocked chains.

### L0 — Inventory, provenance and unsupported-feature discovery

Inputs: pinned source tree, selected scenes, source compiler options, dependency lock and profile. Output: member-level API inventory, script/callback inventory, scene/resource closure, serialized component/field inventory, license notices, diagnostics and content hashes.

Use a C# syntax/semantic inventory tool (Roslyn is a proposed **tool-only** dependency, not currently installed in this project), resolving overloads, operators, attributes, generics, reflection/string-based access and defines. Script calls alone miss serialized components, coroutine yields, persistent event target methods and external UI script IDs. Mark unresolved dynamic accesses for review rather than inventing complete reachability. Include all compile-referenced members even if not currently reached at runtime.

Acceptance: changed source or scene invalidates its lock; zero unresolved runtime-required features for the selected closure; unrelated unsupported assets are excluded by explicit closure, not because they happened not to load. Diagnostic tests use real malformed input files and intentionally unsupported scripts/components.

### L1 — Source and API binding

Build all original runtime scripts unchanged against the independent compatibility libraries. Verify no dependency on Unity proprietary assemblies/UnityEditor/NSubstitute. A separate optional Unity compile verifies that the selected original source/reference harness still builds under the pinned editor; its compiler warnings are recorded independently.

Compilation success proves **binding only**. Export a matrix distinguishing `declared`, `compiled`, `behavior-tested`, `reference-compared`, `game-exercised`, `unsupported`. Runtime throwing stubs never count as supported. Unexpected unsupported member calls fail with contextual diagnostics rather than returning default values.

### L2 — Behavioral microfixtures

Author small original, permissively licensed fixtures executable against both Unity and Disunity. They assert public results/phase markers/state transitions, not implementation steps. First fixtures cover:

- Attach two behaviors and a Rigidbody to one GameObject; lookup returns the same component identity; `component.gameObject`/`.transform` agree across paths. Missing component returns Unity-null. Validate active/inactive type/tag lookup and no accidental new owner per attachment.
- Populate serialized values/references **before** `Awake`; scene-wide Awake/OnEnable before Start; Start once; private callbacks discovered by name/signature. Test 0/1/multiple fixed steps per frame, FixedUpdate before physics, Update/LateUpdate and coroutine continuation phases. Assert contractual partial orders, not arbitrary order between unrelated objects.
- Keep references after deferred Destroy/unload; test overloaded `== null`, `!=`, bool conversion and distinct `ReferenceEquals`/object-typed C# behavior, stale handles and slot reuse. Version/player exception differences are recorded, not flattened into current Disunity's immediate IDisposable contract.
- Transform world/local position with parent translation/rotation/nonuniform scale; immediate read-after-write and body/transform synchronization. Verify Unity XY plane -> render adapter basis, units, radians/degrees, orientation and winding with asymmetric geometry. Do not confuse world-schema Y-up with render-scene Z-up.
- Keyboard GetAxisRaw -1/0/1; release, opposing keys, focus loss, repeat events, duplicate settings names. Reference-characterize ambiguity; keyboard-only unsupported joystick diagnostics are explicit.
- Time.deltaTime/fixedDeltaTime values in each phase; frame counters/time accumulation; WaitForSeconds starts at end of yielding frame and resumes on first eligible frame, not wall-clock timer. Disable/destroy cleanup and timeScale=0 behavior tested before claiming those policies.
- Random integer upper bound exclusive, float upper bound inclusive, equal bounds; both proxies compile. Deterministic replay consumption errors fail. A seeded engine PRNG is not claimed to match Unity's private stream.
- Rigidbody2D velocity/angularVelocity/position, Sleep/wake, gravity/drag/constraints/material settings, dynamic MovePosition on next step (last target wins), contact-blocked motion, trigger versus contact, filtering and continuous motion against thin geometry. Actual overlap/step produces callbacks; no direct test call to Goal.OnTriggerEnter2D as proof of dispatch.
- Orthographic camera pixel extent; SpriteRenderer visibility does not disable physics; pivots/pixels-per-unit/color/sort order/alpha; UI.Text getter/setter, anchor/layout/scaler placement and visible score update.
- UnityEvent dynamic enum payload, persistent scene listener target, call state and missing/destroyed target policy. A real trigger leads to exactly one score change; tests must not merely assert that a configured mock event was invoked.

### L3 — Differential runner and artifact contract

One fixture description and scripted stimulus, two runners:

- **Reference:** a legitimate separately installed/licensed Unity 2017.1.0f3 Editor or player built with it, plus original fixture/harness source. Prefer a maintained Windows x64 host/VM for the old editor, not an assumption that its Linux/macOS editor works on 2026 systems. Availability, entitlement, automation rights and safe installation are milestone gates.
- **Implementation:** .NET compatibility adapter/native engine, headless unless rendering is under test.

Proposed run spec: schema/profile/fixture version; source and harness hashes; initial scene/config; input samples keyed by fixed tick/frame/phase; frame-duration sequence; random replay; observation phase; end condition; tolerances. Trace JSONL rows contain fixture/object stable key, frame, fixed tick, phase, property/event and value. Stable keys use source GUID/fileID plus generation/clone ordinal, **not cross-run native instance IDs**. Include OS/architecture, Unity exact revision, scripting backend, physics settings/backend, graphics adapter, compiler and runner revisions in the envelope.

Reference control must be feasible for this old version:

- Unity 2017.1 documents `Time.captureFramerate`; use capture rates 25/50/100 as separate deterministic schedules with fixedDeltaTime 0.02, and verify actual emitted tick/phase counts before accepting a trace. Do not assume modern `PlayerLoop`, Input System injection or manual Physics2D simulation APIs exist there. General variable-delta/slow-frame reference control remains an adapter capability to prove, not an invented guaranteed option.
- `Input.GetAxisRaw` cannot simply be assigned by a harness. For game differential tests, set original `Player.inputProxy` to a tiny harness IInput after Awake and before first fixed tick; keep original time proxy reading engine Time. This is **boundary replay**, not proof of physical keyboard handling. Separate focused desktop tests exercise actual winit -> axis mapping, and optional interactive Unity keyboard captures validate the axis contract.
- Original `BallMovement.random.randomProxy` is an IRandom seam. Install a recording delegate to original RandomProxy before Start in Unity and replay those results in Disunity. Record every method/argument/result; mismatch or extra/missing draw fails. Independently test default compatibility Random bounds and real gameplay without injected proxies. Do not claim Unity PRNG equivalence from a common seed.
- Verify installation/injection timing with a bootstrap fixture and explicit script execution order for the harness; no original source edits. Observation scripts do not mutate observed gameplay state. Some edge-case tests intentionally initialize bodies/objects; those setups are described, not mistaken for natural full-game runs.
- Run reference three times to establish reproducibility. Do not bless a golden trace whose own lifecycle/timing/physics is unstable. A different Unity version or upgraded project is a **new profile**, not a replacement oracle silently sharing the same baseline.

Comparator defaults proposed for the first profile (all must be approved after measurement, never auto-widened):

| Observable | Initial acceptance rule |
| --- | --- |
| Scores, active/visible flags, tags/layers, component ownership, event recipients/counts, serialized bindings | Exact. Callback partial ordering exact where documented. No blanket event sorting to hide dispatch bugs. |
| Pure math / no-contact motion | `abs(a-b) <= 1e-5 + 1e-5*max(abs(a),abs(b))` per scalar; reject NaN/Inf. |
| Short isolated contact scenarios | Position absolute error <= 0.01 Unity units, speed relative error <= 1%; same contact/trigger recipient and event count; transition within 1 fixed tick. Calibrate with measured reference repeatability. |
| WaitForSeconds | No early resume before required accumulated scaled wait; resume first eligible frame using version-characterized phase. Any reference timestamp slack bounded by one scheduled frame, not arbitrary sleep. |
| Sprite/camera capture at fixed 1280×720 | Compare object bounds within 1 physical pixel; interior colors within 2/255 per channel in declared color space; alpha edge mask at most 1 pixel wide. Save expected/actual/diff. No single permissive whole-screen similarity threshold. |
| Score text | Exact string and anchor/layout rectangle; digit visible/readable. Font substitution means reference typography can differ; exclude only documented glyph-rasterization region, or compare a matched independently supplied glyph fixture. Never exclude the whole HUD. |
| Long game | Exact invariants/scoring outcomes for controlled scenarios; bounded events and stable operation. No bit-identical minutes-long physics trajectories across unrelated solvers/platforms promised. |

These numeric bounds are proposed engineering criteria, **not measured facts**. If the solver cannot meet contact criteria, report profile failure/known limitation; do not rewrite Pong or increase thresholds until anything passes.

### L4 — Scene and asset ingestion

Proposed CPU importer handles only the pinned **2017 text serialization dialect** initially. Parse YAML documents/tags and fileIDs, index `.meta` GUIDs, expand prefabs, apply property-path/array/reference overrides, reconstruct parent/component relationships and resolve persistent listeners. Use a bounded data parser, not regex substitution or execution of source content.

Produce normalized scene IR containing source IDs, component data, transforms, scripts, sprite/physics material resources, UI, project timing/input settings, approved substitutions and inert records. Explicit unit/axis conversion belongs at the renderer adapter; preserve Unity coordinates for gameplay/physics. Validate IR before creating engine objects. Asset imports may be cached by content hash, never by ambiguous filename or case-folded path.

Fixtures: unresolved GUID/fileID, duplicate IDs, wrong external UI type ID, corrupt PNG, invalid floats, cyclic parent/prefab graph, unsupported component, missing event target/method, stale approved `team` override, conflicting overrides, path traversal/symlink escape and bounded object/texture limits. Each failure reports source file/object/property and no partially live world. Golden IR compared with a small licensed-reference exporter proves import values; a self-generated IR round trip alone does not.

Source PNGs remain the same bytes. Interpret metadata for pixel size, pivot, filter and sprite identity, not Unity's cached Library assets. Version-specific default filter behavior must be characterized or listed as a deliberate profile choice. Built-in references map via an explicit table to independent resources. Additional Unity native/editor objects are either supported or individually classified inert with evidence; unknown runtime components fail before startup.

### L5 — End-to-end desktop acceptance

For the **imported original Main scene**, not a hand-built approximation:

1. Load without missing components/references; original source hash check passes; initial scores visible as 0/0; ball launches using original Start and BallMovement logic.
2. Actual W/S and Up/Down keys move the corresponding paddles; release stops commanded movement. Holding against bounds does not pass through them or move X. Verify focus loss/reacquisition, resize, high DPI, minimize/restore and close; Escape may be a **host** exit affordance, not an original game feature.
3. Ball bounces off paddles/bounds through real physical contacts; no tunneling in the defined high-speed microcases. Both goals score for the serialized recipient exactly once per crossing.
4. Goal score visibly updates the correct UI.Text; original MatchManager resets and hides the ball with zero velocity; after configured **2 seconds**, on the first eligible frame, it shows and relaunches. No invented startup countdown or win screen.
5. Scripted boundary-replay scenarios exercise both players and repeated goals. Also run the game with normal default input/random proxies to ensure replay has not bypassed an essential implementation.
6. Ten-minute interactive/automated soak plus 100 import/load/step/unload cycles; no uncaught managed/native errors, live worlds/tokens/contact queues/coroutines after unload. CoreCLR remaining mapped is expected, not a leak. Track resident memory trend rather than requiring zero process memory.
7. Publish platform-specific test reports, short capture, input replay and compatibility manifest. A Linux success is not Windows/macOS qualification; keyboard/visual desktop checks run on each supported OS.

The existing upstream tests supply useful domain expectations. At this pin, [PlayerMovement tests](https://github.com/mcrvaz/pong-tdd/blob/e2d3368728e236b0ab5a04afb142a3ae49260dd1/Assets/Tests/Editor/Player/PlayerMovementTest.cs) actually contain arithmetic assertions; [MatchManager tests](https://github.com/mcrvaz/pong-tdd/blob/e2d3368728e236b0ab5a04afb142a3ae49260dd1/Assets/Tests/PlayMode/Editor/MatchManagerTest.cs) exercise relaunch/visibility but use Unity Test Tools/NUnit/NSubstitute and programmatic setup. They were **not run**. Do not ship the unreviewed NSubstitute binary or treat direct callback/mock-event tests as physics compatibility. Re-author independent integration fixtures through supported APIs; adapt upstream tests only with license attribution and an explicit porting record.

## 8. Official Unity semantic baseline

The selected pin targets **2017.1**, not a generic current Unity API. These primary pages were fetched/read; use 2018.1 pages only for research of the later optional target, not silent profile expansion.

| Official source | Design implication |
| --- | --- |
| [2017.1 execution order](https://docs.unity3d.com/2017.1/Documentation/Manual/ExecutionOrder.html) | Awake/OnEnable scene phase before Start; inactive initialization differs; physics after FixedUpdate, potentially multiple/zero ticks per frame; Update/LateUpdate; coroutine continuation and destruction phases. Docs include broad simplifications: tests must establish exact boundary cases, not infer arbitrary cross-object ordering. |
| [Object equality](https://docs.unity3d.com/2017.1/Documentation/ScriptReference/Object-operator_eq.html) | Unity object existence differs from ordinary CLR null. Test operator resolution as well as destroyed engine object identity. |
| [Transform](https://docs.unity3d.com/2017.1/Documentation/ScriptReference/Transform.html) | Hierarchical world/local translation, rotation and scale are distinct. A flat position-only cache is inadequate for scene import. |
| [Input.GetAxisRaw](https://docs.unity3d.com/2017.1/Documentation/ScriptReference/Input.GetAxisRaw.html) | Unsmoothed keyboard values -1/0/1; profile specifies actual settings/bindings. |
| [Rigidbody2D.MovePosition](https://docs.unity3d.com/2017.1/Documentation/ScriptReference/Rigidbody2D.MovePosition.html) | Calculates movement for next physics update, last call wins; collisions may prevent reaching target; speed limits apply; docs recommend kinematic use, but this source uses dynamic bodies and must be characterized as authored. |
| [WaitForSeconds](https://docs.unity3d.com/2017.1/Documentation/ScriptReference/WaitForSeconds.html) | Scaled-time wait, measured from end of yielding frame, resumes on first eligible frame. Old page has a confusing timeScale multiplication sentence: do not derive a timing formula from that alone; test scale 0/0.5/1/2 before claiming nondefault-scale support. |
| [Random.Range](https://docs.unity3d.com/2017.1/Documentation/ScriptReference/Random.Range.html) | Float max inclusive; integer max exclusive; equal integer bounds return min. No documented algorithm equivalence contract. |
| [UnityEvents](https://docs.unity3d.com/2017.1/Documentation/Manual/UnityEvents.html) | Persisted listener targets and dynamic typed arguments are runtime behavior, not ignorable editor metadata. |
| [Sprite Renderer](https://docs.unity3d.com/2017.1/Documentation/Manual/class-SpriteRenderer.html), [UI.Text](https://docs.unity3d.com/2017.1/Documentation/ScriptReference/UI.Text.html) | Sprite pivot/scale/sort and unlit alpha-blended default; Text layout/style/reference font and visible string, not just managed `text` storage. Support only the required subset. |
| [Text scene format](https://docs.unity3d.com/2017.1/Documentation/Manual/FormatDescription.html) | Per-object YAML documents; class IDs and local IDs; decimal/IEEE-754 hexadecimal float syntax. This is not a full specification of prefab/UI internals. |
| [captureFramerate](https://docs.unity3d.com/2017.1/Documentation/ScriptReference/Time-captureFramerate.html), [command-line arguments](https://docs.unity3d.com/2017.1/Documentation/Manual/CommandLineArguments.html) | Deterministic frame interval option and version-appropriate `-batchmode`, `-projectPath`, `-executeMethod`, log/error behavior. Do not paste modern Unity Test Framework CLI flags into a 2017 runner. |

Additional 2018.1 official research relevant to later growth: [body types/contacts](https://docs.unity3d.com/2018.1/Documentation/Manual/class-Rigidbody2D.html), [Raycast](https://docs.unity3d.com/2018.1/Documentation/ScriptReference/Physics2D.Raycast.html), [Instantiate](https://docs.unity3d.com/2018.1/Documentation/ScriptReference/Object.Instantiate.html), [Time.deltaTime](https://docs.unity3d.com/2018.1/Documentation/ScriptReference/Time-deltaTime.html). They explain why later powerups/AI require body filtering/query details, clone hierarchy/reference remapping and correctly deferred Start. They do not authorize marking those APIs supported today.

## 9. Proposed implementation boundaries

Names/paths here are proposals, **not existing packages or commands**. Keep the smallest cohesive ownership boundaries; no interface per class or plugin framework.

| Owner / proposed location | Responsibility and dependency direction |
| --- | --- |
| `tools/unity-tck/` | Inventory, fixture preparation/reference capture, trace comparison, license/content report. Host orchestration may depend on engine through facade. Tool-only Roslyn/reference SDK dependencies never leak into normal Rust CPU libraries. |
| `crates/unity-import` (Foundation/CPU; explicitly classify) | Bounded text scene/meta/prefab decoder and source-to-IR validation. No CLR/GPU/Unity installation. Own version-specific serialization knowledge. Use existing asset IDs/hash/package helpers where appropriate. |
| `csharp/UnityCompat` (+ narrow UI project if helpful) | Independently authored namespace surface, script construction, component ownership, callback dispatch, coroutine state, reference lookup and Unity-null. Depend on generated native ABI, not renderer/host globals. Keep present `Disunity` behavior intact. |
| `crates/disunity-ffi` and a CPU physics module/crate if warranted | Authoritative body/transform/component-handle state and batched commands/events/snapshots. Keep FFI small; solver math need not live inside unsafe export functions. All handles validated against owning world/generation. |
| `crates/disunity-runtime` | Explicit managed load/advance/unload bridge and trusted assembly configuration; preserve one native image, no rlib/cdylib duplicate store. Per-phase input/time passed in, no ambient environment lookup as policy. |
| Render-plan/render-scene or a focused Graphics `sprite` module | Immutable sprite/text resource preparation plus per-frame draw data; orthographic/unlit alpha pipeline, sorting and UI layout output. GPU only consumes validated resources/coordinates, not Unity paths/GUIDs. |
| Desktop host, behind optional `worldrunner` features | Main-thread event loop, fixed-step accumulation, OS input, managed lifecycle, rendering and shutdown. Introduce a cohesive game-loop seam alongside static viewer; share presentation plumbing only where useful. No Pong-specific logic. |
| `games/pong-desktop/` and `csharp/PongDesktop/` | Pin/fixture manifest, original-source selection, bridge composition, acceptance scenarios and game notices. Rust game depends only on facade; facade exposes required opt-in managed/import/sprite APIs. Engine does not depend on game/tool. |

### State and phase contract

- Import is transactional: decode/validate the whole supported closure, allocate native/managed component graph, assign all serialized values/references/listeners, then activate and deliver scene lifecycle. On error release all temporary/native/managed resources and report context.
- One GameObject owns one Transform and multiple components. Cache managed wrappers by stable `(world generation, object/component handle)` identity; never duplicate a physics body because a second script requests it.
- Managed scene topology/lifecycle has one owner; native physics state has one owner. Specify synchronization points: transform setters are visible immediately to subsequent managed reads and to the next query/physics step as required by the reference profile; physics writes become visible before callbacks and render snapshot. Do not leave current fixture's one-step-delayed render state as an accidental Unity contract.
- Proposed frame pipeline: latch input -> deliver due startup/activation work -> zero or more `[FixedUpdate -> native physics -> state sync -> collision/trigger dispatch]` -> Update -> due coroutine continuations -> LateUpdate -> deferred destruction/resource updates -> stable draw/observation snapshot. Exact Start/first-fixed and destruction/render boundaries must be reference-characterized before freezing the adapter.
- Do not hold the native store mutex or a managed/native mutable world borrow while invoking C# callbacks. Drain owned events after stepping, allow callbacks to mutate through commands, invalidate destroyed recipients safely, and make event order/reentrancy observable. Define and test when callback-side teleports/filter changes affect subsequent same-step events.
- Use batch exports for moving instances and event streams; static textures/geometry stay resident. This is a new dynamic path: the current static viewer cannot supply it just by reading object snapshots once.

### Physics decision gate

No 2D solver dependency was found in the inspected workspace. Do **not** add one as part of this documentation change. During the first spike, evaluate a maintained, independently licensed Box2D-compatible integration behind a small physics boundary against a minimal native swept-box alternative. Prefer a proven solver if its license, build portability and measured contact behavior fit; choose the exact library/revision only after that evaluation. A modern Box2D API/version is not evidence of Unity 2017 internal numerical equivalence.

The decisive fixtures are original dynamic MovePosition, the `10^10` paddle/ball mass ratio, frictionless restitution 1, frozen axes, continuous collision detection, transform reset + Sleep + relaunch, and trigger timing. A custom ball-reflection rule special-cased to Pong is not acceptable. If only bounded approximate physics is feasible, publish that profile honestly and retain required game invariants; do not market general Unity Physics2D compatibility.

## 10. Compatibility manifest and diagnostics

Proposed logical schema **`real-disunity.unity-compatibility.v1`**; define an actual strict schema and consumer tests before implementation. Use canonical serialization/content hashes, explicit versions and unknown-field rejection consistent with existing schemas. Do not overload the existing generic render/world contract URIs.

Required manifest sections:

- Identity: profile ID, source URL/full commit/tree hashes, selected source list, editor version/revision, scene closure, importer/compiler/compat/ABI versions.
- License/content entries **per file/category**: source, sprites, UI/font replacements, packages/test libraries; license identifier/text hash/notice and provenance confidence; distribute/fetch-only/excluded disposition.
- Compiler/runtime: C# language/defines/target framework, trusted host bridge, native image/ABI, platform/backend requirements.
- Features: exact member signatures and serialized component types/properties; status levels from L1; behavioral test IDs/reference artifact hashes, intentional deviation and reason. Never a blanket “UnityEngine supported” flag.
- Assets: source GUID/fileID -> IR/resource identity mappings, override provenance, approved inert fields/components and explicit font/material substitutions.
- Simulation: fixed/frame schedule, physics settings/backend revision, input profile, random mode, observation points and per-assertion tolerances.
- Platforms: independent Linux/Windows/macOS qualification, architecture/graphics backend, pass/fail/not-run counts and dated evidence. No implied support from package compilation.
- Exclusions/blockers and final run result: `unsupported`, `missing-reference`, `not-run` distinct from `pass`. Missing licensed reference may permit implementation-only checks, but cannot yield a new reference-verified badge.

Example diagnostic contract (illustrative text, not existing tool output):

```text
UCOMP-SCENE-014 unsupported external component
  source: Assets/Scenes/Main.unity, fileID=..., script=(guid=...,fileID=...)
  profile: pong-desktop-unity-2017.1-v1
  feature: UnityEngine.EventSystems.<unresolved>
  required by: Main -> Canvas/EventSystem
  action: provide a validated 2017.1 type mapping or explicitly classify this
          exact inert component with reference evidence; nothing was loaded
```

Other codes distinguish unsupported member/overload, unknown serialization dialect, unbound event, unknown axis, resource/budget/path violation and physics backend capability. Runtime unsupported calls include source member/callsite where symbols permit, object source key, frame/tick/phase, profile and implementing task. Never silently fall back to a default shader, missing font, no-op physics or zero input.

## 11. CI and desktop portability

**Normal PR CI requires no Unity installation or Unity credentials.** It consumes reviewed public fixture sources/assets and approved golden observations, executing the engine side only. No remote game clones at arbitrary HEAD during CI; fetch a locked archive with integrity checks or vendor the approved minimal closure with notices. Keep external acquisition separate from reproducible offline tests.

| Tier | Required checks / environment |
| --- | --- |
| CPU, all three desktop OSes | Existing `cargo xtask ci-cpu`; importer/manifest/trace comparator tests, invalid-input corpus, deterministic CPU fixtures. Pure fixture source checks may run tool-side; no CLR/GPU enters CPU library dependency graph. |
| Managed, Linux/Windows/macOS | Existing `cargo xtask ci-managed`, native-first binding check, original game compilation, microfixture state tests and teardown soak. Provision .NET 8 explicitly; absence fails the selected managed job. |
| GPU/headless Linux | Existing lavapipe/Xvfb conventions; sprite/UI offscreen captures plus actual window/input smoke tests. GPU missing in a required job is failure, not a successful skip. Preserve full existing GPU suite. |
| Desktop qualification | Linux Vulkan, Windows D3D12 (and Vulkan where selected), macOS Metal; actual game launch, keyboard, resize/high-DPI, focus and close, captures and soak. Scheduled/release hardware jobs where hosted CI cannot reliably exercise window input. Qualify each backend explicitly. |
| Optional licensed reference | Manually dispatched or scheduled **trusted self-hosted** runner, pinned editor/harness/project. Never expose credentials/license to untrusted PR code. Generate JSON/PNG/IR evidence, compare repeatability, review artifact/notice rights, then promote a content-addressed golden set. Missing Unity reports unavailable, not pass; baseline updates require review. |

Ordinary contributors can use existing reference data, inspect specs and add failing implementation tests. New reference-dependent claims wait for reference generation; optional does not mean fabricated evidence. Separate provenance/status metadata identifies stale golden sets after source/profile changes.

Portability work includes:

- OS-specific `.dll`/`.so`/`.dylib`, hostfxr/runtimeconfig selection and architecture consistency (macOS arm64 .NET/native libraries); no hardcoded developer paths. Existing explicit native-image resolution remains authoritative.
- Winit event loop on main thread, especially macOS; focus release of held keys; physical/logical pixel distinction; layout versus key-code policy matched to intended W/S/arrow behavior. OS key repeat must not repeatedly increment state.
- Linux X11/Wayland differences, window-system prerequisites and permission-restricted synthetic input; Xvfb proves only that environment. Windows/macOS smoke tests need appropriate native facilities, not Linux-only scripts.
- Raw RGBA sprites/atlas request empty optional GPU features; do not require BC texture support for this two-PNG game. Independent unlit sRGB/alpha conventions and sort order tested per backend. Reuse presentation recovery behavior for zero-size/lost surfaces.
- Canonical case-sensitive logical asset paths regardless of filesystem case behavior; deterministic locale-independent numeric serialization. Native/managed errors propagate without crossing FFI as panics/exceptions.

## 12. Fixture rights and Unity redistribution boundaries

Use published documentation as a behavioral guide and independently write engine code/fixtures. Do not copy Unity implementation/source or proprietary binaries into the compatibility kit. API familiarity and public GitHub visibility are not blanket license grants.

Primary legal entry points reviewed as **current notices, not historical legal conclusions**: [Unity Editor Software Terms](https://unity.com/legal/editor-terms-of-service/software), [Editor source-code terms](https://unity.com/legal/editor-source-code-terms), [Asset Store EULA](https://unity.com/legal/as-terms). Current terms distinguish editor/runtime use and impose source/asset restrictions. A 2017 license/reference-runner entitlement and interoperability research use require their own review; this proposal does not assert that buying a Unity seat automatically permits every test or artifact redistribution. In particular, the June 30, 2026 editor terms define User Modifications broadly to include conversion/porting/wrapping and discuss separate grants for preparing projects for non-supported platforms; whether those provisions apply to a particular independent reference/import workflow is a legal-review gate, not resolved here. Do not access restricted Unity source as an implementation shortcut.

Proposed distribution rules:

1. Keep a fixture bill of materials with full commit, file hashes, original license/attribution, source of every asset and any adaptation. Preserve Marcello Vaz's MIT notice. Root MIT is verified; two PNGs' independently confirmed authorship is not. Resolve that small remaining review before vendoring/public distribution, or obtain confirmation. Do not equate importer success with permission.
2. Exclude Unity Editor/Player binaries, UnityEngine/UnityEditor DLLs, license files/credentials, Library caches, arbitrary extracted built-in fonts/materials and unreviewed test DLLs. Reference software is installed by its authorized operator, not packaged by TCK.
3. Ship only independent compatibility code, approved upstream gameplay/scene/sprites, independent fixture code and approved observation data. Fixture source can have its own explicit permissive license without declaring or changing the whole repository's license.
4. Built-in font is a reference, not a redistributable file in the target. Prefer a small independently authored/appropriately licensed digit atlas mapped to this exact reference, retaining score string/layout behavior. Record source/license and changed typography. Do not download an arbitrary font and call it free. If broader text is later needed, audit a real font and shaping/rendering dependencies separately.
5. Numerical traces/IR/screenshots require their own content/terms review before public golden publication. Do not redistribute reference-built Unity players as the normal CI oracle. Internal reference execution can retain restricted artifacts separately; public fixtures should avoid restricted content entirely.
6. Later pong-tdd's personal-use font, nicolaracco's Joystix/EmojiOne uncertainty, tutorial-derived code and Unity sample package dependencies remain separate review items. Do not relabel assets MIT because surrounding scripts are MIT.

## 13. Minimal vertical slice and milestones

**Smallest genuine slice:** compile the full chosen original source set; import Ball + both Player prefab closures/settings through the same importer (no hand-coded paddle/ball logic); run original Awake/Start/FixedUpdate and actual keyboard movement in a desktop window; original sprites move and a real ball/paddle/bound collision occurs. Exclude score UI/goal loops from this *interim slice only*, clearly labeled partial Main-scene profile. First create tiny independent behavioral fixtures to reach red/green before adding each implementation. Final game acceptance must import complete Main.

| Milestone | Dependencies / estimate | Exit evidence |
| --- | --- | --- |
| **M0: feasibility and lock** | First; 1 week | Reproducible content/API/scene/license inventory; original 20 files compile against minimally declared independent API; licensed 2017 reference available and repeatable; external UI IDs/inert records resolved; physics spike meets isolated acceptance. If editor/legal/physics blockers remain, report no-go for reference-verified claim. |
| **M1: object graph and phases** | M0; 1–2 weeks | Characterize existing fixture first; new failing public-behavior tests then implement component ownership, callbacks, serialization-before-Awake, lookup, Unity-null, clocks/coroutines. Original gameplay begins through unchanged private callbacks, not manual Ball/Player calls. Existing ABI/100-cycle tests remain green. |
| **M2: importer + headless physical game core** | M0/M1; 2–3 weeks; import/solver work can overlap | GUID/fileID/prefab overrides and project settings produce validated IR; original movement/elastic collision/trigger/reset tests compared against reference; transactional failures and no stale transform cache. |
| **M3: desktop vertical slice** | M2; 1–2 weeks | Dynamic orthographic unlit sprite renderer and game loop; original two sprites/keyboard gameplay in partial profile, measured no-contact/contact agreement, resize/focus/shutdown smoke. Not yet a complete game badge. |
| **M4: full Main and visible score loop** | M2/M3; 1–2 weeks | Camera-space Canvas/scaler/RectTransform/Text subset, persistent events, independent glyph mapping. Complete original Main: both goals score/reset/hide/wait/relaunch; all L5 scenarios on Linux. |
| **M5: portability and publishable TCK** | M4; 1–2 weeks | Windows/macOS qualification, Linux graphics/managed/CPU gates, reviewed golden/provenance set, ten-minute/100-cycle soaks and actionable compatibility report. Remaining deviations explicit; no unexecuted platform marked supported. |

Ranges overlap and are planning estimates; the aggregate 8–12 engineer-week budget assumes no large solver/import/reference surprises. Stop after M0 if the reference cannot legitimately run or the physics spike fails substantially; implementation-only exploratory fixtures can still proceed without a Unity-equivalence claim. A modern editor migration is a separate documented decision with a new pin/profile and upgrade diff, not a hidden workaround.

## 14. Prioritized project backlog

Each future implementation ticket should start with an observable failure, demonstrate the intended red result, then implement/refactor and record actual verification. This documentation change itself does not pretend to be a test-first implementation.

| ID / priority | Proposed task and likely integration point | Depends on / acceptance |
| --- | --- | --- |
| **P0-01** | Pin `e2d3368…`; prepare file-level fixture/license/asset lock and source-selection report (`tools/unity-tck`, `games/pong-desktop`). | No dependency. Verify 20 scripts, Main closure/two PNG hashes, notices; exclude tests/DLL; record built-in substitutions and unresolved origin evidence. |
| **P0-02** | Establish licensed Unity 2017.1 reference adapter and trace schema. | P0-01. Three repeat captures of lifecycle/motion/relaunch; validate UI GUID/fileIDs/stale overrides; exact editor/harness metadata, no credential artifacts. |
| **P0-03** | Physics feasibility spike, dependency/license/build comparison. | P0-02. Dynamic MovePosition/mass-ratio/CCD/restitution/sleep fixtures meet declared bounds or give concrete no-go results; no Pong-special reflection. |
| **P0-04** | Define inventory/manifest/diagnostic schema and comparator with negative corpus. | P0-01/02. Source/scene drift invalidates evidence; incorrect enum event/extra draw/missing reference fails; first-divergence diff is useful. |
| **P1-01** | Add independent managed compatibility project and original-source compile target. | P0-01/04. All original files hash-identical; no Unity DLLs; profile reports declared versus verified APIs; strict engine/harness build retained. |
| **P1-02** | Component graph, managed identity/lookup/null and callback scheduler. | P1-01, P0-02. Public microfixtures establish same-object ownership, private lifecycle, multiple fixed ticks, destruction and nondefault timing only where supported. |
| **P1-03** | CPU importer + normalized Unity scene IR; classify package in xtask. | P0-04. Prefab/reference/array/event/settings tests, invalid/bounded input, independent editor-observed IR comparisons; no CLR/GPU dependency. |
| **P1-04** | Integrate selected physics and append-only native commands/events/snapshots. | P0-03, P1-02/03. Characterization + new real contact/trigger tests; command/state sync and callback mutation behavior; regenerate/review bindings using documented native-first gate. |
| **P1-05** | Extend explicit managed host bridge for incremental load/advance/unload. | P1-02/04. One native image/store, no leaked tokens/worlds; existing PortableFixture entry points still work; failures clean partial state. |
| **P1-06** | Sprite draw resources + orthographic/unlit/alpha/UI-glyph support, dynamic snapshots. | P1-03/04. Offscreen bounds/pivot/sort/alpha tests, raw-PNG GPU feature set, no Unity file lookup in renderer. |
| **P1-07** | Desktop game-loop composition behind facade features; classify any new host crates. | P1-05/06. Winit keyboard->axes, input release/focus, accumulator and teardown; no-default facade remains GPU/CLR-free; static viewer tests remain unchanged. |
| **P1-08** | Camera-space UI subset and persistent score-event integration. | P1-03/06/07. Original ScoreManager/View source visibly drives score after actual goal trigger; 2-second reset/relaunch; incomplete UI components diagnosed, not skipped silently. |
| **P1-09** | OS/backend qualification, CI profiles, approved reference artifacts, acceptance report. | P1-08. M5 criteria and source-integrity badges, all actual platforms separately evidenced. |
| **P2-01** | Later pong-tdd powerup profile, after resolving font and zero-speed scene regression. | M5 plus independently reviewed new pin/patch. Instantiate/reference remap, SetActive, RequireComponent, Physics2D.Raycast/masks, Bounds/scale and grow/reset timing; disabled clone must not recursively Start. |
| **P2-02** | Second small desktop game: nicolaracco Pong menu/AI/audio profile. | M5 + asset/license closure. Circle colliders/contact normals, FSM/events, SceneManager, TMP subset, audio start/stop/volume and resource drain; actual menu->match->return journey. Do not claim it from compile stubs. |
| **P3-01** | Brick Breakout sample or a new separately audited older sample pin. | After second-game reuse evidence. Tilemap cells/collider rebuilding, contacts, destruction/reload; current 6000.3 pin also requires Input System/modern API/URP strategy, explicitly separate from 2017 profile. |

Audio and SceneManager are intentionally deferred for the first game: no audio assets/playback or script-requested scene loading in the selected pin. Scene startup/unload still must work. When introduced, audio tests assert decoded PCM/envelope/start-stop/loop and teardown independent of sound hardware, plus one audible desktop check; scene tests assert complete object/coroutine/physics/UI replacement and missing-scene errors. Do not equate a silent AudioSource or SceneManager no-op with compatibility.

## 15. Risks and go/no-go checks

| Risk | Mitigation / decision |
| --- | --- |
| Old editor unavailable, insecure on current hosts, or reference-use entitlement unclear | M0 on isolated trusted host/VM; no install/license bypasses. If blocked, report reference-unverified status or approve a separate modern upgrade study. |
| “Simple Pong” masks solver-specific behavior | Spike dynamic MovePosition/extreme masses/continuous contacts first. Keep original settings/gameplay. Reject a special-case Pong physics implementation. |
| Serialized external UGUI IDs and obsolete overrides | Reference-assisted type map, explicit dialect and inert-field policy, fail-closed census with diagnostics. Exact Unity export may help validate, but normal importer/CI cannot require Unity. |
| Native/managed state divergence or reentrant deadlock | Single native image, authoritative state ownership, specified phase fences, callback dispatch outside locks, invalid-handle tests and existing soaks. |
| Current renderer/viewer mistaken for ready Unity player | New orthographic/unlit/dynamic/UI seams are budgeted; retain pathless rendering and game-independent desktop host. No per-frame full scene upload workaround as final design. |
| Asset permissions overstated | Per-file evidence/notice and explicit built-in resource mapping. Keep personal-use font out by revision choice, not by pretending root MIT covers it. |
| Nondeterministic tests or permissive screenshots hide errors | Recorded ticks/stimulus/random calls, phase observations, repeatable reference sets, exact score/events and bounded geometry metrics; first-divergence artifacts. |
| Scope balloons to arbitrary Unity games | Freeze one source/version/scene/profile. Add APIs only from failing fixtures or next audited game's inventory; unsupported is a useful result. |

## 16. Research verification and remaining uncertainty

**Performed:** read local README/docs, Cargo feature/dependency/layer policy, Lua/managed/FFI implementations and representative host/renderer/tests/CI; queried open remote issues; cloned six candidate repositories to temporary storage (with history/worktrees for pong-tdd); pinned commits; inspected primary source/license/version/settings/scene/package files at the levels described above; parsed TTF name-table notices for newer pong-tdd/nicolaracco fonts; inspected all selected pin gameplay scripts, relevant prefabs/settings/sprite hashes and selected upstream tests; fetched official versioned Unity docs and the primary Damaged Panda sound license page. Later sample version/package inspection corrected search/README impressions. An attempted `2018.1/Manual/UnityYAML.html` URL returned 404; the valid 2017.1 `FormatDescription.html` is used instead.

**Document checks:** all 26 local Markdown links resolve; all 48 unique external citation endpoints returned HTTP 200 in a HEAD check (reachability, not additional content verification); code fences balance; target pin, 20-script count, absence of TTFs/package manifest and two sprite hashes checked against the selected tree. Patch whitespace checked with Git, including the new untracked document. Git status shows only this new documentation file; no implementation/dependency edits. No build/test execution was warranted for this documentation-only change, and none is claimed.

**Not established:** Unity reference entitlement/installation/execution, game playability in Unity or Disunity, tests/builds passing, automated complete API reachability, UGUI binary type-ID mapping, full expanded scene truth compared with Unity, exact solver choice/equivalence, image authorship independent of root MIT, later candidates' full asset/package rights closure, sound file hash correspondence, or Windows/macOS runtime behavior. Official docs establish intended contracts, not proof that this repository implements them.

**Required before implementation commitment:** P0-01 through P0-04, especially legitimate reproducible reference and physics spike. **Required before distribution:** approve minimal fixture notices/assets and substitution table. The proposal itself adds only documentation; no runtime code, dependency, original game file or reference binary is included.
