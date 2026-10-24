# Unity 6.3 desktop target and sample-game research

**Decision/research date:** 2026-10-20. **Status:** design only; no Unity installation, project upgrade, game execution, compilation, runtime implementation or vendoring performed.

This is the controlling update to the [compatibility TCK proposal](unity-compatibility-tck-proposal.md). The user chose **Unity 6.3 LTS (`6000.3`) desktop**, replacing the historical Unity 2017.1 Pong target. Historical source research remains useful; its implementation order and 8–12 engineer-week estimate are not carried forward.

## 1. Recommendation and exact version policy

**Select Unity's official Brick Breakout sample as the first conditional feasibility target**, not the entire 2d-techdemos collection:

- Repository: [Unity-Technologies/2d-techdemos][brick-repo]. Source pin: **`6593d544df2ea598e51f5cf1d7165d5ed42ceba7`**.
- Entry scene: **`Assets/Tilemap/Brick/Brick.unity`**. Three original gameplay scripts: `Ball.cs`, `Brick.cs`, `GameEnd.cs`. The [README][brick-readme] calls it a Breakout-style game and says to open Brick and run it to start playing. It is an actual scene/game with source, not a video or empty starter template.
- **Version-specific publisher evidence:** the official [`6.3` branch][brick-branch] resolved to that exact commit in a fresh `git ls-remote` check. [ProjectVersion.txt][brick-version] records **`6000.3.3f1` (`ef04196de0d6`)**. README's prose version guidance is only **2021.1.0f1 onwards**. Do not turn that broad statement into a claim that the publisher tested every 6.3 patch.
- Root [MIT license][brick-license], copyright 2016 Unity Technologies, is explicit. It supplies the available permission evidence for repository-owned scripts/scene/sprites; no separate Brick sprite notices were found. It does **not** relicense external packages, built-in assets or third-party content. Independent sprite authorship was not separately established.
- **Initial proposed reference editor: `6000.3.26f1` (`de8167e7440c`)**, Linux x86-64 first, Windows x86-64/macOS arm64 subsequently qualified independently. Compatibility line remains `6000.3`, not all Unity 6 releases. Source saved with `.3f1` and a proposed oracle at `.26f1` are different facts.

Fresh release evidence: Unity's [release API][unity-releases] returned `.26f1` first (2026-10-08T13:38:24.193Z), then `.25f1` (September 24), `.24f1` (September 10), `.23f1` and `.22f1`. Its [release page][unity-26] independently says October 8, 2026 and links revision `de8167e7440c`. Thus **latest 6.3 patch observed on this research date** is `.26f1`; this is not a rolling promise or a claim based on a repository date. The page also lists known issues, including Bloom with the 2D Renderer; do not assume current equals issue-free. No editor or installer was downloaded.

**Qualification is still required.** Open an isolated copy at the pinned reference patch only after the rights gate below, record every editor/package/settings change, and retain original hashes. Do not overwrite the original source pin or its saved-version evidence. A `.3f1` comparison, if legitimately available and needed, is a separate baseline, not an automatic prerequisite or a substitute for qualification on `.26f1`.

**Honest conclusion:** this search found a strong official **6.3-branch + 6000.3-saved + MIT game-source** candidate, and official tutorials explicitly listing 6.3. It did **not** establish a fully rights-cleared, executed fixture combining all desired properties, nor a tiny third-party 2D tutorial with an explicit 6.3 prose claim and fully cleared assets. Recommend Brick conditionally, rather than falsely calling it tested by us or calling Unity's packages open source under MIT.

## 2. Evidence vocabulary

| Claim | What it establishes | What it does not establish |
| --- | --- | --- |
| **Claims support for 6.3** | Publisher tutorial version metadata, README requirements, or explicitly named publisher branch; identify which kind. | Compilation, successful game play, every 6.3 patch, or redistribution rights. A `?version=6.3` URL alone is insufficient. |
| **Source saved with 6000.3.x** | Actual pinned `ProjectSettings/ProjectVersion.txt`. | That README/tutorial targets that patch, that packages resolve, or that the project runs. |
| **Tested by us** | Actual dated build/run/test evidence with editor, platform, packages, scenario and result. | Nothing here has this status. Static inspection is not execution. |

All source pins below were read from local Git clones under `/tmp`; no external game was copied into this workspace. Searches covered official Learn, official sample branches and third-party tutorial/game repositories. Search results were leads only; settings, licenses, source and package manifests determined the findings. This is a bounded search, not proof that no better project exists.

## 3. Ranked source-backed options

Ranked for this project's initial feasibility study. Only rank 1 has an explicit publisher `6.3` branch; ranks 2–3 are **source-saved 6.3 alternatives**, not invented support claims. None was executed.

| Rank / pinned source | Version claim versus actual project | Packages and observed game surface | Rights and decision |
| --- | --- | --- | --- |
| **1. [Official Brick][brick-repo]**, `6593d544df2ea598e51f5cf1d7165d5ed42ceba7` | Publisher `6.3` branch at pin; README says runnable Breakout and 2021.1+; [settings][brick-version] say **6000.3.3f1**. | [Manifest][brick-manifest]: Input System **1.17.0**, URP **17.3.0**, pixel-perfect 5.1.1, sprite/tilemap modules, Git 2d-extras `#package_6000.3`; also unrelated navigation/timeline/editor packages. All 3 Brick scripts read. One selected scene, 2 PNGs, 4 tile assets, input actions, box/polygon/tile collisions and scene reload. | [Root MIT][brick-license]; package rights separate. Best publisher provenance and bounded real game. **Recommended subject to legal/reference/package/render/physics gates.** |
| **2. [admtrv/Breakout][adm-repo]**, `2b960a3589f30b6e0025f2509ef00bf9155d6f45` | README shows demo and sprite credit, **no explicit 6.3 support prose**. [Settings][adm-version]: **6000.3.5f2** (`3fa8bc678cb0`). | [Manifest][adm-manifest]: Input System **1.17.0**, URP **17.3.0**, UGUI 2.0.0, Test Framework 1.6.0, 2D animation/tooling/extras. All **7** `Assets/Scripts/*.cs` read. `SampleScene`, Ball/Brick prefabs, sprite sheet/background; Keyboard A/D, `linearVelocity`, MovePosition, runtime spawning/destruction, sprite animation coroutines, widening/multiball powerups and modern object lookup APIs. | [MIT][adm-license], copyright 2026 Anton Dmitriev, for repository software. README credits **Petraheim** sprites. [Asset page][petraheim] permits developing a game and says attribution not required, but no explicit open-content license or standalone redistribution grant was found there; `space.png` provenance not established. **Second choice only after asset permission/provenance and reference checks**, not clearly cheaper than tilemaps. |
| **3. [eguldali/Optimized-Snake-Game][snake-repo]**, `0ab43c53e834db714439be71c4151d953d241847` | README says Unity/C#, no exact 6.3 claim. [Settings][snake-version]: **6000.3.7f1** (`696ec25a53d1`). | [Manifest][snake-manifest]: 2D feature **2.0.2**, Input System **1.18.0**, UGUI **2.0.0**, Visual Scripting **1.9.9**, test/timeline/editor modules. All **8** gameplay scripts read. Enabled `GameScene`, recovery scene excluded, segment/food prefabs and GameConfig ScriptableObject. Actual input is legacy `Input.GetKeyDown(W/A/S/D)`, settings `activeInputHandler: 2` (Both), not Input System actions. Pooling uses Instantiate/SetActive; frame accumulator, Random, Vector2Int, orthographic visuals. | [MIT][snake-license], copyright 2026 eguldali. No external image/font/audio files in the inspected asset inventory; prefab visuals reference built-in resources whose mapping/rights still need treatment. Lowest physical-simulation cost, **but a weaker compatibility demonstration and unfinished behavior**. Source does not enforce non-wrapping bounds, retries can choose occupied food, and `GameOver` only logs. README claims such as fully Unity-independent core are overstated: model/grid use Unity Vector2Int. Do not fix game logic silently. |

Snake's `Update` advances at most one tick per frame, not a catch-up `while` loop; preserve that behavior if selected. A simpler model does not entitle us to port the model to Rust and call it unchanged Unity gameplay. Root MIT alone is not a full built-in-resource or package license audit for any option.

### Explicit 6.3 Pong lead rejected on rights

[davi133/NeuralPong][neural-repo], pin **`1f5d9ad82cc1797884bcf0db2ec44041fc3ac619`**, explicitly says **“Requirements: Unity 6.3”** in its README and points to `PlayScene.unity`. Its project is nested under **`NeuralPong/`**; [actual version][neural-version] is **6000.3.11f1** (`3000ef702840`), not inferred from its old repository creation date.

[Manifest][neural-manifest] includes UGUI 2.0.0, 2D animation 13.0.4, pixel-perfect 5.1.1, navigation 2.0.11, tests and other modules; no Input System or URP declaration was present. Inventory: **18 C# files**, five scenes including four training variants; representative `PlayerInputGatherer`, `TheBall`, `UI_manager` and agent-loading code read. Legacy axes, Rigidbody2D.linearVelocity/AddForce, rotations, TMP UI and neural-network file I/O make it wider than plain Pong. **No project license grant found** (including a case-insensitive license/notice filename search). Bundled LiberationSans has an OFL text; EmojiOne has attribution, not a complete project license. Those notices cannot license the gameplay code. **Do not vendor or treat as an open-source fallback without permission.** It proves that an exact support claim can exist while still failing our requirements.

## 4. Official tutorial alternatives: genuine 6.3 claims, different evidence gaps

### Roll-a-Ball — strongest beginner tutorial, not yet a pinned open-source fixture

[Unity Learn: 3D Beginner Game: Roll-a-Ball][roll-course] has machine-readable `unity_version: 6.3` and `supported_unity_versions` explicitly including 6.3, 6.0 and 2022.3. This is more than choosing a query parameter. Its [setup][roll-setup], [movement][roll-movement] and [desktop build][roll-build] lessons were fetched. Build instructions select Windows/Mac/Linux, although the same lesson retains a stale “Linux support is currently in preview” note. Treat that inconsistency as tutorial wording, not a current OS support guarantee.

Observed tutorial requirements: Universal 3D/URP, primitive sphere/plane/cubes and materials; Input System `PlayerInput`/`InputValue.Get<Vector2>()`; Rigidbody AddForce in FixedUpdate; camera follow; rotating collectible prefabs; trigger/tag/SetActive; count/win text. The current course also includes **AI Navigation with an enemy**: full-course acceptance would add NavMesh, agent movement, enemy collision and win/lose behavior. A pre-AI collectible slice must be named as a partial tutorial profile, not the full current game.

The inspected course/build materials expose lessons/transcripts/educator resources, not an immutable completed-project Git commit with `ProjectVersion.txt`, package manifest and explicit permissive project/asset license. **Those source/version/package/license checks are therefore not established**, rather than fabricated from lesson code. Primitive geometry reduces external-art risk but does not turn course text, example scripts, templates, fonts, URP or Input System into MIT content. Authorized internal learning/reference use and public vendoring/redistribution are different permissions; even internal compatibility analysis needs terms review.

Two licensed third-party Roll-a-Ball leads were cloned and rejected as exact-6.3 evidence:

- [ceefour/rollaball][old-roll-apache], pin `8aeb5950b3add94d742a652652b22858e869342a`: root Apache-2.0 text, three scripts and `MiniGame` scene, but **no ProjectVersion.txt or Packages/manifest.json** in the inspected tree; no 6.3 claim. Its recent repository activity is not engine-version evidence. A third party's root license does not settle ownership of copied tutorial content.
- [pradeep-giri/roll_a_ball_unity][old-roll-mit], pin `31c4c18010e644e456eaf9379e9ea6da54bb7281`: root MIT text, actual **2019.4.19f1**, TMP 2.1.1/UGUI 1.0.0 in manifest, three scripts/two scenes; no 6.3 claim. Not an exact-version alternative without a separately reviewed migration.

A small, legally usable 3D Roll-a-Ball could be preferable to an unlicensed tiny 2D game, but we have not established that complete source/rights combination. Its 3D rigidbody/friction/rolling contacts plus optional NavMesh would also be a different solver workload, even though existing mesh rendering may help.

### Sprite Flight — explicit modern 2D tutorial, but larger presentation scope

[Unity Learn: 2D Beginner Game: Sprite Flight][flight-course] likewise explicitly lists **6.3** and 6.0 in supported-version metadata. It says to start with a blank project, **“no assets provided”**, then build a complete triangle-ship obstacle-survival game. The course includes Input/2D physics, **UI Toolkit** scoring/restart, particle effects and optional audio; final publishing lesson is web-oriented. Desktop qualification would be a separate requirement.

No pinned finished-project source/license/ProjectVersion/manifest was established from the inspected course. It meets the tutorial claim, not our ready-to-pin open-source-game requirement. UI Toolkit/UXML/USS, particles and restart interactions would be more expensive than Brick's lack of UI/audio. Do not present this as a small Pong substitute or a redistributable tutorial asset pack.

## 5. Selected Brick source and scene census

This is a static, source-derived estimate, not a completed importer, compiler reachability proof or Unity-observed scene export.

### Gameplay API closure

All three original scripts under [`Assets/Tilemap/Brick/Scripts`][brick-scripts] were read completely:

| Script | Observable behavior / required API |
| --- | --- |
| `Ball.cs` | `Start`: same-object Rigidbody2D lookup, initial `linearVelocity` from `Random.Range(-1f, 1f)` and Vector3 right/down; scene overrides initialVelocity to **(2,15)**. Resolves assigned Tilemap GameObject. `OnCollisionEnter2D`: compares collision GameObject identity; for each real contact computes `point - 0.01 * normal`, calls `WorldToCell`, then **`SetTile(cell, null)`**. Contact order/normal and transform-to-cell rounding matter. |
| `Brick.cs` | `Start`: resolves Rigidbody2D and global **`InputSystem.actions.FindAction("Move")`**. `Update`: reads Vector2 action value, sets horizontal `linearVelocity = move.x * right * moveSpeed`, scene speed **5**. This is frame Update writing physics state, not the old Pong FixedUpdate/MovePosition path. |
| `GameEnd.cs` | Real trigger callback looks up `Ball` on other collider, then calls **`SceneManager.LoadScene(0)`**. Reload must replace the full scene and restart scripts/input state; it is not a delayed coroutine serve or an in-place ball teleport. |

Minimum compile surface: UnityEngine Object/GameObject/Component/MonoBehaviour/Transform, Vector2/Vector3 operations/conversions/constants, Random float Range, Rigidbody2D.linearVelocity, Collider2D/Collision2D/ContactPoint2D, GetComponent, Unity object equality/null semantics; `UnityEngine.Tilemaps.Tilemap`/TileBase, WorldToCell/SetTile; `UnityEngine.InputSystem.InputSystem`, InputActionAsset/FindAction, InputAction/ReadValue<Vector2>; `UnityEngine.SceneManagement.SceneManager.LoadScene(int)`. Serialized rendering/physics/grid types are additional to script calls. Generic .NET collections/usings do not imply every namespace API is runtime-required.

### Serialized state and assets

The selected scene is **2,432 lines**, with **6 GameObjects / 6 Transforms**, **3 Rigidbody2D**, **5 BoxCollider2D**, **1 PolygonCollider2D**, **1 TilemapCollider2D**, **1 CompositeCollider2D**, Grid/Tilemap/TilemapRenderer, 2 SpriteRenderers, 3 gameplay MonoBehaviours and an orthographic camera (size **8**). It also contains AudioListener, HaloLayer, lighting/render/navigation records and scene-root metadata. No prefab-instance record occurs in this scene. Do not implement all 17 scenes or all 18 C# files in the collection merely because they share a repository.

- GameObjects: `Brick` (paddle), `Tilemap`, `Grid` (bounds), `Main Camera`, `Ball`, `Game End`. Paddle has a **five-vertex polygon**, not a box; Rigidbody is dynamic, gravity scale 0, constraints 6. Ball has **BoxCollider2D**, not CircleCollider2D; dynamic mass **0.25**, gravity scale 0, constraints 4. Both use discrete collision detection. Three boundary boxes plus ball and end-trigger make up the five box colliders.
- Tilemap stores **128 cells**, x=0..15 / y=8..15, four colored tile assets using `Brick.png`. Tilemap transform is translated y=8 and scaled y=0.5 under Grid. Grid/cell/anchor/parent-transform conversion cannot be replaced by integer truncation of world position. Tile asset references built-in script `(guid 0000000000000000e000000000000000, fileID 13312)`; identify its exact versioned semantics before claiming import support.
- Tilemap Rigidbody is static; TilemapCollider and CompositeCollider are both serialized. The tile collider's `m_CompositeOperation` is **0**, and the composite's stored paths are empty. Do not assume they are actively merged, drop the composite blindly, or use merged rectangles without checking contact outcomes. Collider regeneration timing after SetTile is a feasibility gate.
- Physics material has **friction 0, bounciness 1.001**, not exactly 1. Fixed timestep **0.02**, maximum allowed **0.33333334**, timeScale 1; Physics2D velocity/position iterations 8/3. Do not silently clamp the material to a convenient solver default or promise constant ball speed over long runs.
- Two original PNGs plus `.meta`: Brick has **64 pixels/unit**, Ball **4 pixels/unit**, distinct import modes/filter/mipmap settings and source geometry. Four `.asset` tile definitions supply colors, transforms and collider type. `TilePalettes/Blocks.prefab` is an editor palette, not a runtime scene prefab dependency in the static GUID walk.
- Initial static scene GUID walk found local references to scripts, tiles, the physics material, sprites and `BrickSettings.lighting`; external/default sprite material **`a97c105638bdf8b4a8650670310a4cd3`** and built-in IDs are unresolved locally. Project GraphicsSettings select URP assets; this is **not** proof of built-in-render-pipeline rendering just because scripts do not call URP. Renderer/global settings and actual material mapping need qualification. Independent unlit sprite rendering is an explicit proposed substitution, not full URP execution/parity.
- No gameplay audio clips, fonts, score UI, TMP or UnityEvents in this selected closure. Serialized AudioListener/Halo/navigation/lighting records require exact inert classification with evidence, not a blanket skip-unknown policy.

Inspected byte identities (not authorship attestations):

| File under `Assets/Tilemap/Brick/Sprites/` | SHA-256 |
| --- | --- |
| `Brick.png` | `53ae81a216e95e628f149430c8168dcd2f62b9b4eb87d759eabc659e657dfb40` |
| `Ball.png` | `82e8394a3122deb48992b7c6df384cba4b04bc389b776f3d26100c390815e091` |

### Input, package and standalone-build traps

[`EditorBuildSettings.asset`][brick-buildsettings] has **an empty scene list**. README's Play instructions are not proof that loss/reload or a desktop build succeeds as checked in. Create a disclosed **configuration-only fixture patch** listing Brick as scene 0 for reference/desktop qualification. Keep original gameplay and scene hashes unchanged unless an actual editor migration requires a separately recorded derived asset variant. If any gameplay correction is necessary, stop claiming original-source parity for that variant.

Input settings: active handler **1** (new Input System), project-wide [`InputSystem_Actions.inputactions`][brick-actions] and inputsettings assigned through build config objects. Player/Move is a Vector2 Value action with Dpad composite: WASD and arrow keys, plus gamepad/XR/joystick bindings. Selected profile is **keyboard-only**. Composite normalization, opposing keys, phase visibility, focus release and auto-enabling/global asset lifetime must be characterized; simply hardcoding A/D into a float is insufficient. Input update mode is serialized as 1 and must be interpreted against 1.17.0, not guessed from the enum number.

[Manifest][brick-manifest] and [package lock][brick-lock] **disagree**: manifest requests Git `2d-extras#package_6000.3`; lock records registry **6.0.1** instead. Fresh remote lookup resolved that branch to **`56c3a1345f3e7948a639e8f1df3725e00d1b4b18`**; its [license][extras-license] is Unity Companion License, not the sample's MIT. Record both observations, not a fictitious reproducible upstream lock. Before oracle generation, resolve/pin an approved package graph and publish the diff/hashes in the derived reference profile. Brick's inspected scripts/tile definitions do not use custom extras APIs; a minimal-closure project may omit that package **only after a reference build establishes it is unnecessary**, with the omission disclosed. Other demos cannot be used to infer Brick's dependencies, or vice versa.

## 6. Rights and distribution gates

The sample's MIT grant is valuable but not a blanket “all Unity content is open source” statement.

| Content | Evidence / permitted planning disposition |
| --- | --- |
| Brick repository-owned source, scene, PNGs and metadata | Root MIT; preserve notices. No separate sprite notice found, independent authorship not separately proved. File-level provenance/rights sign-off required before public inclusion. |
| Input System 1.17.0 / URP 17.3.0 | Official [Input license][input-license] and [URP license][urp-license] pages explicitly say **Unity Companion License for Unity-dependent projects**. They are not unrestricted MIT packages. Do not copy or adapt their implementation into Disunity. |
| 2d-extras at observed branch pin | [Pinned license][extras-license] is UCL. Entire dependency need/lock remains unresolved; do not fetch/embed arbitrary moving package content as a normal CI step. |
| Built-in sprite material, Tile script IDs, engine DLLs/shaders/resources | Serialized references are not redistribution grants. Map to independently authored equivalents where feasible, list substitutions and qualify their behavior. Do not extract/copy Unity binaries or built-in resources. |
| Learn lessons, scripts, templates and any downloaded tutorial assets | No permissive finished-project grant established here. Public/free access is not an open-source license. Audit actual download terms/notices if a reference project is later obtained. |
| Reference players, screenshots, traces, imported IR | Review rights/terms before publication; internal retention and public CI distribution are different. An observation artifact is not automatically distributable merely because it was generated by our harness. |

**Important reference-use blocker:** current [UCL v1.4][ucl] clause 1 restricts competitive analysis and developing a competing product/service, in addition to its Engine License connection. Whether this compatibility project can use those package works even on an internal licensed reference machine must be resolved with qualified review/permission **before such use**, not waived by labeling it “internal.” This report inspected package license notices, not their implementation code. Independent API implementations must be derived from approved public contracts and behavior sources, not copied UCL implementation. The [Editor terms][editor-terms] and [Asset Store EULA][asset-terms] are separate, content-specific considerations; neither is a replacement for an actual course/asset grant. Do not casually apply one commercial-learning agreement to all free Learn content.

If clearance is unavailable, do not run or redistribute restricted package works for this study. Investigate an approved reference configuration without those dependencies or a separately permissioned fixture. Do not remove packages and silently claim equivalence to the publisher's original configuration.

## 7. Scope, first acceptance scenario and go/no-go gates

**In:** source-recompilation compatibility for the three original scripts; one source-derived scene and asset closure; desktop keyboard; independent API/material implementations; real 2D contacts/triggers and tile deletion/rebuild; scene reload; orthographic rendering. Linux x86-64 first; actual Windows/macOS qualification later. Keep existing CoreCLR/.NET host strategy, explicit native state ownership and facade/layer boundaries from the proposal. No new runtime code is authorized by this research change.

**Out:** all 2d-techdemos, general Tilemap/RuleTile/brush tools, arbitrary physics/serialization, full Input System/device catalog, URP/RenderGraph/shader execution, pixel-perfect/TMP/UI Toolkit/audio/NavMesh/3D gameplay, mobile/web/consoles, Unity binary compatibility and IL2CPP. Declaring a package in manifest does not make its whole API part of the compatibility promise; serialized/reachable requirements still cannot be ignored.

### First useful acceptance scenario — proposed, not run

Use the **complete selected Brick scene**, not a hand-created ball/paddle arrangement. After legal/package/build-list/render-mapping gates:

1. Compile all three original gameplay files unchanged and import scene/settings/tile/PNG metadata. Start a fresh world at a fixed documented viewport/frame schedule, fixed timestep 0.02; observe 128 initial tiles and one ball/paddle, initialized through original callbacks.
2. Feed keyboard Right/D then release through the declared Move action binding. Assert visible paddle displacement/stop and Rigidbody2D.linearVelocity driven by original Update; compare input phase and displacement with the reference. Separately test simultaneous keys/diagonal normalization/focus loss. Do not call Brick.Update manually as the E2E input path.
3. Let a genuine ball/paddle contact rebound upward, then hit the tilemap. Assert that the original collision callback removes the **reference-observed cell set** and the corresponding rendered cells/collision geometry disappear. Do not insist on exactly one cell: the source iterates all contacts, so a seam/multiple contacts may delete more. A repeat encounter must not collide with a removed cell. No direct Ball.OnCollisionEnter2D invocation or scripted tile deletion as proof.
4. Let the ball reach the end trigger. Original GameEnd calls LoadScene(0); observe complete old-world teardown, a fresh ball/paddle, 128 restored tiles and functioning input. No duplicate input subscriptions, old rigidbodies or sprites. Repeat reload before awarding the game-level badge.

For deterministic oracle runs, control/record the initial Random.Range draw and keyboard schedule in an approved reference harness using supported public boundaries; no pong-tdd proxy seams exist in these scripts. Exact seeded Unity/Disunity PRNG equality is **not** assumed. A separately labeled physics microfixture may initialize body position/velocity for a seam case, but does not replace the natural complete-game scenario. Reference repeatability, input injection and observation timing remain gates; no trace has been recorded.

Event/cell/reload identities are exact where deterministic; motion/contact tolerances must be measured from repeatable runs and fixed before implementation comparisons. Do not reuse the historical Pong numeric tolerances or auto-widen them. Rendering substitution acceptance must specify geometry/color/visibility metrics and excluded shader effects, not claim pixel-exact URP parity.

### Delivery gates replacing the old Pong backlog

| Gate | Required evidence / no-go condition |
| --- | --- |
| **G0 — rights and source lock** | Three unchanged file hashes, scene/metas/resource inventory, explicit per-file notices/dispositions and package/reference-use clearance. No implementation commitment or restricted reference execution before clearance. |
| **G1 — reference qualification** | Licensed `.26f1` environment, recorded editor upgrade diff, resolved package graph, disclosed Brick-at-index-0 config patch, successful desktop build/play/contact/tile-delete/loss-reload on reference. Repeat three times and record platform/backend. Failure yields concrete blocker, not a support badge. |
| **G2 — bounded feasibility** | Independent public-API behavioral microfixtures for input actions, transformed WorldToCell/SetTile, real polygon/box/tile contacts and collider refresh timing, scene reload/lifetime; material/type-ID mapping and inert-record policy. Solver must handle the actual polygon, discrete contacts and restitution rather than a Pong-only reflection rule. |
| **G3 — first end-to-end slice** | Original 3 scripts compile against independent APIs; complete imported Brick scenario above in a desktop window with declared substitutions. Compilation/stubs alone cannot pass. |
| **G4 — qualification/publication** | Independent Linux/Windows/macOS results, teardown/reload/focus/resize tests, approved golden artifacts/license BOM, CPU/managed/GPU regressions. Unrun platforms explicitly not-run. |

Budget a **timeboxed 1–2 engineer-week G0–G2 investigation**, contingent on legal turnaround and access, then re-estimate. This is a planning allowance, not execution performed or a commitment that a full game can be delivered in that time. Tilemap import/rebuild and package/reference qualification make the previous 8–12 week historical Pong forecast inapplicable; no defensible replacement end-to-end estimate exists before these gates.

## 8. Costs versus historical 2017 Pong and TCK profiles

| Concern | What changes for the 6.3 Brick target |
| --- | --- |
| Source/API binding | 3 scripts instead of 20, but newer `linearVelocity` and package namespaces. Original C# can still compile against independently authored APIs in the existing managed host; that is not Unity's scripting runtime/binary identity. Remove old proxy-specific assumptions/defines; record new compile symbols/language/package signatures. |
| Assets/serialization | No runtime prefab-instance overrides, scores or custom font in Brick, but modern YAML/class versions, tile tables/colored Tile assets, built-in type IDs, polygon path, renderer/package settings and `.meta` import semantics. Keep importer CPU-only, GUID/fileID/resource provenance and fail-closed unsupported diagnostics. |
| Input | Replace old named GetAxisRaw replay/keyboard adapter with a bounded action-asset/global Move/InputAction contract, update-phase/composite and lifetime tests. Do not implement all devices or ship Input System package code. |
| Physics | Removes extreme-mass-ratio paddle MovePosition from historical Pong; adds real five-point paddle shape, tilemap transform/cell mapping, ordered contact payloads and dynamic collider invalidation. Ball is box/discrete and material restitution is 1.001. Solver equivalence remains a risk. |
| Rendering/UI | No camera-space UGUI Text/scaler/font/event binding needed for this sample. Add orthographic sprite/tile draw/update support and explicitly resolve URP/default material mapping. Existing mesh rendering is not proof of either sprites or URP support. Full URP is not the first milestone. |
| Game flow | No scoring coroutine/two-second serve or UnityEvent. SceneManager reload is now **required initially**, not historical P2 work; must clear scene, physics, callbacks/input and rendered state. There is no score/win UI in the inspected game to invent. |
| Reference/rights | Modern desktop editor availability is better than 2017, but actual installation/build/run still unverified. Modern package graphs/UCL create additional rights and reproducibility gates; “current sample” does not remove them. |

Proposed profile family: **`unity-6000.3-desktop-source-v1`**, with a game profile **`brick-keyboard-6000.3.26f1-v1`** recording source pin, saved editor `.3f1`, reference `.26f1`/revision, compiler, exact package lock, OS/backend, build-list patch and material substitutions. These are proposed manifest identities, **not implemented schema values**. Keep historical `pong-desktop-unity-2017.1-v1` separate/unscheduled; its observations never establish 6.3 behavior.

Retain L0 inventory/provenance, L1 unchanged-source compilation, L2 independent microfixtures, L3 differential traces, full-game/desktop acceptance and negative diagnostics from the proposal, with these changes:

- L0 includes package resolution **and manifest/lock disagreement**, `.inputactions`, Grid/Tile asset tables, collider/renderer closure, source editor versus oracle editor and upgrade/config deltas.
- L2/L3 add action composites/global enabling, `linearVelocity`, collision contacts/cell rounding/SetTile collider timing and reload. Keep compiler-declared, behavior-tested, reference-compared and game-exercised statuses distinct. Old UGUI/coroutine/Pong random seams are not first-game acceptance requirements.
- Reference clocks/input/physics control must use APIs actually available and cleared for 6.3. Input frame writes versus fixed simulation need explicit observations. Unity remains an optional, trusted, separately entitled reference tier; missing/blocked reference is **not-run**, not pass.
- Ordinary CI uses approved independent fixtures and approved immutable observations, with no Unity credentials/binaries/package implementations. CPU/managed/GPU layers remain separate per [quality gates](quality.md); do not make a live remote clone or Unity project opening a normal PR requirement.
- Public claims enumerate this game's reachable subset and substitutions. A Brick pass means neither “all Unity 6.3 supported” nor that other 6.3 projects/pipelines are compatible.

### Fallback if Brick does not clear gates

Keep the **6.3 decision**. Do not quietly return to 2017 or an unlicensed Pong. First consider the pinned MIT Snake only for a clearly narrower **grid/sprite/source** profile after reference/resource qualification, retaining its upstream behavior; it cannot validate Unity physics. admtrv Breakout is an option only with sprite/background permission and package clearance, and adds powerup/coroutine/spawning scope.

If an explicit **prose/tutorial 6.3 claim + redistributable finished source** is mandatory, none fully met it in this study. Obtain a permissioned completed Roll-a-Ball/other tutorial project or an explicit author license/update, then re-pin/inspect/version-check it before replacing Brick. In the meantime use small independently authored permissive **6.3 microfixtures**, not a claimed tutorial-game success. No game or runtime is implemented by this proposal.

## 9. Verification and remaining limitations

**Performed for this update:** read the existing proposal, repository README/quality conventions and searched for local/ancestor project instructions; checked initial worktree status (the proposal was already untracked). Fetched official Learn course/version metadata and selected Roll-a-Ball lessons, Unity release API/release page, sample and package license notices, UCL and Petraheim asset page. Resolved official 6.3/extras branch refs; inspected pinned ProjectVersion/manifest/license files for all three ranked candidates and NeuralPong; read all Brick/admtrv/Snake gameplay scripts and representative rejected Pong code. Inspected Brick serialized component/scene inventory and local GUID references, settings, tile assets, input actions, build settings, package-lock mismatch, sprite metadata/hashes; inspected alternative scene/prefab/asset inventories. External clones remain under `/tmp`; no vendored source/package/binary was added.

**Not established:** legal clearance (especially UCL reference use), completed file-level rights BOM, Sprite PNG independent authorship, complete package/built-in asset provenance/type mapping, package restoration, editor upgrade/compile/playability, transformed tile/collider behavior, deterministic injection/traces, Disunity compatibility, or any desktop platform's execution. The static reference walk is not a validated expanded Unity scene. Official course support metadata is a publisher claim, not our test result; absent finished-project evidence is explicitly absent. A guessed Learn-specific legal URL returned 404 and was not used as a licensing basis.

Documentation validation passed: **29 local-link occurrences**, reference-label resolution, balanced code fences, consistent table columns, final newlines and whitespace checks across both documents. **82 unique external citation endpoints** responded successfully: 78 to HEAD, and four (Learn Markdown/release API) to GET after HEAD was unsupported or returned a misleading error. No Markdown linter executable was installed; these were focused structural/link checks, not a full Markdown lint suite. Confirmed that the historical proposal body after its decision heading remains byte-for-byte unchanged. Rechecked all six inspected candidate checkouts were clean at their recorded commits, selected source/scene counts, tile count, empty build list, package-lock discrepancy and sprite hashes. No Cargo/.NET/game tests were run because no executable code changed. No commit was made.

[brick-repo]: https://github.com/Unity-Technologies/2d-techdemos/tree/6593d544df2ea598e51f5cf1d7165d5ed42ceba7
[brick-branch]: https://github.com/Unity-Technologies/2d-techdemos/tree/6.3
[brick-readme]: https://github.com/Unity-Technologies/2d-techdemos/blob/6593d544df2ea598e51f5cf1d7165d5ed42ceba7/README.md
[brick-version]: https://github.com/Unity-Technologies/2d-techdemos/blob/6593d544df2ea598e51f5cf1d7165d5ed42ceba7/ProjectSettings/ProjectVersion.txt
[brick-manifest]: https://github.com/Unity-Technologies/2d-techdemos/blob/6593d544df2ea598e51f5cf1d7165d5ed42ceba7/Packages/manifest.json
[brick-lock]: https://github.com/Unity-Technologies/2d-techdemos/blob/6593d544df2ea598e51f5cf1d7165d5ed42ceba7/Packages/packages-lock.json
[brick-license]: https://github.com/Unity-Technologies/2d-techdemos/blob/6593d544df2ea598e51f5cf1d7165d5ed42ceba7/LICENSE.md
[brick-scripts]: https://github.com/Unity-Technologies/2d-techdemos/tree/6593d544df2ea598e51f5cf1d7165d5ed42ceba7/Assets/Tilemap/Brick/Scripts
[brick-buildsettings]: https://github.com/Unity-Technologies/2d-techdemos/blob/6593d544df2ea598e51f5cf1d7165d5ed42ceba7/ProjectSettings/EditorBuildSettings.asset
[brick-actions]: https://github.com/Unity-Technologies/2d-techdemos/blob/6593d544df2ea598e51f5cf1d7165d5ed42ceba7/Assets/InputSystem_Actions.inputactions
[unity-releases]: https://services.api.unity.com/unity/editor/release/v1/releases?version=6000.3&limit=5
[unity-26]: https://unity.com/releases/editor/whats-new/6000.3.26f1
[adm-repo]: https://github.com/admtrv/Breakout/tree/2b960a3589f30b6e0025f2509ef00bf9155d6f45
[adm-version]: https://github.com/admtrv/Breakout/blob/2b960a3589f30b6e0025f2509ef00bf9155d6f45/ProjectSettings/ProjectVersion.txt
[adm-manifest]: https://github.com/admtrv/Breakout/blob/2b960a3589f30b6e0025f2509ef00bf9155d6f45/Packages/manifest.json
[adm-license]: https://github.com/admtrv/Breakout/blob/2b960a3589f30b6e0025f2509ef00bf9155d6f45/LICENSE.md
[petraheim]: https://petraheim.itch.io/breakout-pixelart
[snake-repo]: https://github.com/eguldali/Optimized-Snake-Game/tree/0ab43c53e834db714439be71c4151d953d241847
[snake-version]: https://github.com/eguldali/Optimized-Snake-Game/blob/0ab43c53e834db714439be71c4151d953d241847/ProjectSettings/ProjectVersion.txt
[snake-manifest]: https://github.com/eguldali/Optimized-Snake-Game/blob/0ab43c53e834db714439be71c4151d953d241847/Packages/manifest.json
[snake-license]: https://github.com/eguldali/Optimized-Snake-Game/blob/0ab43c53e834db714439be71c4151d953d241847/LICENSE
[neural-repo]: https://github.com/davi133/NeuralPong/tree/1f5d9ad82cc1797884bcf0db2ec44041fc3ac619
[neural-version]: https://github.com/davi133/NeuralPong/blob/1f5d9ad82cc1797884bcf0db2ec44041fc3ac619/NeuralPong/ProjectSettings/ProjectVersion.txt
[neural-manifest]: https://github.com/davi133/NeuralPong/blob/1f5d9ad82cc1797884bcf0db2ec44041fc3ac619/NeuralPong/Packages/manifest.json
[roll-course]: https://learn.unity.com/course/roll-a-ball.md?language=en&version=6.3
[roll-setup]: https://learn.unity.com/course/roll-a-ball/tutorial/setting-up-the-game?language=en&version=6.3
[roll-movement]: https://learn.unity.com/course/roll-a-ball/tutorial/moving-the-player?language=en&version=6.3
[roll-build]: https://learn.unity.com/course/roll-a-ball/tutorial/building-the-game.md?language=en&version=6.3
[flight-course]: https://learn.unity.com/course/2d-beginner-game-sprite-flight.md?language=en&version=6.3
[old-roll-apache]: https://github.com/ceefour/rollaball/tree/8aeb5950b3add94d742a652652b22858e869342a
[old-roll-mit]: https://github.com/pradeep-giri/roll_a_ball_unity/tree/31c4c18010e644e456eaf9379e9ea6da54bb7281
[input-license]: https://docs.unity3d.com/Packages/com.unity.inputsystem@1.17/license/LICENSE.html
[urp-license]: https://docs.unity3d.com/Packages/com.unity.render-pipelines.universal@17.3/license/LICENSE.html
[extras-license]: https://github.com/Unity-Technologies/2d-extras/blob/56c3a1345f3e7948a639e8f1df3725e00d1b4b18/LICENSE.md
[ucl]: https://unity.com/legal/licenses/unity-companion-license
[editor-terms]: https://unity.com/legal/editor-terms-of-service/software
[asset-terms]: https://unity.com/legal/as-terms
