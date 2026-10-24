# Stage 5 — pinned local games versus the written Unity reference

**Completed offline source comparison; not a build, runtime, rights, or Unity 6.3 compatibility certification.** The companion [JSON inventory](game-comparison-inventory.json) is authoritative for normalized requirement records, source locations, guide evidence, dimensional assessments and counts. The guide was not edited to remove differences.

## 1. Outcome and resolved access issue

The earlier pass was blocked because `read_file`/`list_directory` rejected paths under `/tmp`. On this resume, explicitly authorized foreground shell reads successfully inspected the supplied checkouts. That access problem is **resolved**, not a continuing reason for null counts or placeholder comparisons. No copying, vendoring, symlink workaround, network, fetch/clone/pull, installation, compilation or game execution was needed.

Only the generated reference directory and the three supplied game roots were read. `git rev-parse HEAD` matched every supplied pin; `git status --short` returned empty for each game. All authored gameplay/support C# and Pong's separate test source were read, together with root licenses, saved editor versions, manifest/lock metadata where present, relevant scene/prefab/configuration/physics/input/rendering fields and supporting asset identity metadata. Serialized text inspection was directed at relevant components and wiring, not a claim to have analyzed every boilerplate field. No Unity/package/vendor implementation code or DLL contents were read.

**Recommendation:** use **B, modern Snake, as the first source fixture**, then **A, modern Breakout**, with **C, historical Pong, as a separate regression/test-design source**. Snake has more scripts than Breakout but a smaller engine-facing capability set. This recommendation is conditional on explicitly resolving Snake's missing sprite reference and application defects; it is not a promise that the unmodified checkout runs.

## 2. Verified local identities, versions, licenses and populations

| ID | Local project | Verified HEAD | Saved editor | Root notice | Gameplay/support C# | Separate test C# | All authored C# |
|---|---|---|---|---|---:|---:|---:|
| A | admtrv / Breakout | `2b960a3589f30b6e0025f2509ef00bf9155d6f45` | **6000.3.5f2** | MIT; 2026 Anton Dmitriev | **7** | **0** | **7** |
| B | eguldali / Optimized Snake Game | `0ab43c53e834db714439be71c4151d953d241847` | **6000.3.7f1** | MIT; 2026 eguldali | **8** | **0** | **8** |
| C | historical Pong-TDD | `e2d3368728e236b0ab5a04afb142a3ae49260dd1` | **2017.1.0f3** | MIT; 2017 Marcello Vaz | **20** | **11** | **31** |
| **Total populations** | | | | | **35** | **11** | **46** |

Absolute roots, also stored in JSON:

- A: `/tmp/real-disunity-modern-games/admtrv--Breakout`
- B: `/tmp/real-disunity-modern-games/eguldali--Optimized-Snake-Game`
- C: `/tmp/real-disunity-tck-research/pong-tdd-licensed-v1`

Version evidence is each root's `ProjectSettings/ProjectVersion.txt:1–2` for A/B, `:1` for C. License evidence is A `LICENSE.md:1–21`, B `LICENSE:1–21`, C `LICENSE.MD:1–21`. The script inventory lists every path and line count; totals are **435**, **284**, and **349** gameplay/support source lines, respectively, plus **670** historical test-source lines. Line counts include comments/blanks and are bookkeeping, not complexity metrics. Recursive `Assets/**/*.cs` enumeration found no additional generated/vendor C# candidates. Pong's managed support definitions and interfaces are included in its twenty, rather than falsely counting only its MonoBehaviours. No authored test scripts were found in A/B; installing a test package is not evidence of tests.

**Rights boundary:** these are observed root notices, not blanket clearance. Breakout's `README.md:3–5` explicitly credits Petraheim's third-party Breakout sprite pack. Its external terms were not consulted. Snake's sprite provenance is unresolved, as is the missing referenced asset. Pong includes `Assets/DLLs/NSubstitute.dll`; only its filename was observed, not its contents, version, origin or license. Art, fonts, Unity built-ins, shaders, packages and plugins require separate rights handling. No legal research, clearance or clean-room/provenance certification is claimed.

### Local package and content setup

| ID | Enabled scene | Input actually referenced by gameplay | Local package metadata | Rendering/content closure |
|---|---|---|---|---|
| A | `Assets/Scenes/SampleScene.unity` | `UnityEngine.InputSystem.Keyboard.current`, A/D held polling in `FixedUpdate` | Input System **1.17.0**, URP **17.3.0**, test-framework **1.6.0** | All six quality tiers select URP asset; 2D renderer/light/camera data, spritesheet subassets, native tilemap wall collision, 2 gameplay prefabs |
| B | `Assets/Scenes/GameScene.unity` | Legacy `Input.GetKeyDown(KeyCode.W/S/A/D)` in Update-driven reader | Input System **1.18.0**, feature.2d **2.0.2**, test-framework **1.6.0**; no URP in manifest/lock | Zero custom pipeline references, consistent with built-in; config asset and 2 sprite/Transform prefabs, no gameplay bodies/colliders in selected scene/prefabs |
| C | `Assets/Scenes/Main.unity` | Legacy named `Vertical` / `AlternateVertical` axes | **No `Packages` directory or manifest/lock**; historical embedded/plugin boundary unresolved | Old prefab schema, 7 Resources prefabs, 2D bodies/triggers, Canvas/RectTransform/legacy Text and persistent score events |

Evidence: all `ProjectSettings/EditorBuildSettings.asset:7–10`; A `Packages/manifest.json:14–21`, `packages-lock.json:156–164,201–210`; B `manifest.json:3–12`, `packages-lock.json:147–161,181–189`; C `InputManager.asset:25–38,297–310`. A's `GraphicsSettings.asset:40` has a zero default pipeline, **but** `QualitySettings.asset:50,102,154,206,258,310` all reference the URP asset. That asset's `Assets/Settings/UniversalRP.asset:19–21` selects the renderer data; the `.meta` identities resolve locally. Do not infer built-in rendering from the zero default alone. B has zero defaults and quality overrides (`GraphicsSettings.asset:42`, `QualitySettings.asset:51,104,157,210,263,316`).

A/B `ProjectSettings.asset:683` / `:907` contain `activeInputHandler: 1` / `2`. Interpreting these as package-only / Both uses training knowledge; it is not externally verified numeric-schema binding. Both include action assets (Player 9 actions/35 bindings, UI 10/38), but authored gameplay does **not** call action maps, `PlayerInput`, rebinding or action callbacks. Those capabilities are not awarded or charged just because template assets or packages are present.

All three `TimeManager.asset:6–9` specify fixed step .02, maximum .33333334 and scale 1. A/C physics depends on project and prefab setup, not source alone. B's grid is an authored `GridSystem` using `Vector2Int`, **not** `UnityEngine.Grid`, Tilemap or native Physics2D.

## 3. Measurement contract

### Denominator and stable IDs

A requirement is a **distinct, source-evidenced normalized Unity-facing capability obligation** in the inspected gameplay/support C# and minimal relevant serialized/package closure. It may be a type/declaration family, an observable semantic requirement, or a content/package requirement. Repeated call sites merge; materially different clone shapes, conversion/operator identities, timing/ownership conditions and content contracts remain distinct.

This is a curated, bounded capability denominator, **not exhaustive symbol or exact-overload extraction**, a percentage of Unity's API, or the 108 author-defined guide topics. Ordinary C# application rules, .NET collections/LINQ, unused imports, template-only/editor-only assets and installed-but-unused packages are not extra runtime obligations. Retained support facade surfaces are labelled: Pong exposes float Random/`deltaTime` facade members even though its movement path consumes integer Random/`fixedDeltaTime`.

The initial extraction is frozen under `A-R0001…A-R0043`, `B-R0001…B-R0025`, and `C-R0001…C-R0027`. Append future discoveries without renumbering. Pong's `C-R0028…C-R0032` are **test-harness-only** records in a separate `testRequirements` array. Their five units and eleven scripts are excluded from the gameplay denominator. No requirement IDs existed in the prior blocked pass.

Each record contains exact root-relative source file/line references, the normalized contract and source observation, guide evidence IDs resolving to exact guide file/topic/lines, three ordered dimensional assessments, and one aggregate status. Imports, explicitly declared receivers, local class declarations and inheritance provide **textual identity evidence**, not compiler/assembly semantic binding. Uncertain overload/inference/alias/conversion chains remain explicitly uncertain.

### Three dimensions; four aggregate statuses

1. **Declaration:** does the written text name the relevant type/member family or exact shape at the stated level?
2. **Semantics:** does it state the required observable behavior, timing, lifetime or numerical distinction?
3. **Serialized/pipeline/package:** does it supply the required content/configuration/package contract rather than just similarly named C# objects?

`not-applicable` is used only for a genuinely different dimension, not unread source. Ignore it when aggregating:

- **Covered:** every applicable obligation for this bounded row is explicitly written. This does not make it correct, complete, compiled or exact-version verified.
- **Partial:** some relevant treatment is written, but required detail is missing or bounded away.
- **Absent:** no required treatment was found after reading all groups and supplement; nearby citations document the search boundary, not positive evidence.
- **Ambiguous:** identity, exact binding or source-to-written-contract mapping is indeterminate. Matching call spelling or a generic family alone does not settle an exact overload.

An ambiguous applicable dimension takes precedence. Otherwise all-covered → covered, all-absent → absent, mixtures → partial. **Missing member documentation is not a proven false Unity API claim.** Examples: `Collision2D.gameObject` is missing from G06's selected list, not disproven as a Unity member; `new Vector3(x,y)` is unresolved at exact-constructor level, not declared invalid.

## 4. Exact local coverage counts

### Gameplay/support plus minimal content closure

| Game | N | Covered | Partial | Absent | Ambiguous | Covered / N |
|---|---:|---:|---:|---:|---:|---:|
| **A Breakout** | **43** | **26** | **12** | **2** | **3** | **60.47%** |
| **B Snake** | **25** | **15** | **5** | **2** | **3** | **60.00%** |
| **C historical Pong** | **27** | **16** | **7** | **2** | **2** | **59.26%** |
| **Game-requirement instances, not union** | **95** | **57** | **24** | **6** | **8** | **60.00%** |

For every row **N = covered + partial + absent + ambiguous = length of `requirements`**. There is no fractional credit assigned to partial rows. Similar percentages do not imply similar implementation effort: A has 43 capability units versus B's 25 and much more native/package closure.

**Separate historical test population:** 5 requirements: **1 covered, 2 partial, 1 absent, 1 ambiguous**. Combining C's gameplay and tests only for an explicitly labelled all-source view gives **32 = 17 covered + 9 partial + 3 absent + 3 ambiguous**; it is not the table's gameplay denominator.

No deduplicated cross-game-union coverage figure is claimed. Correct deduplication would retain declaration/behavior and editor/package/pipeline contexts; uncertain bindings and historical/modern aliases cannot be merged speculatively. The 95 total counts game-requirement instances only.

### Dimensional diagnostic counts

Each cell is **covered / partial / absent / ambiguous**, over applicable dimensions. A tag row has both semantic and content obligations, so dimensional totals need not sum to aggregate N.

| Game | Declaration | Semantics | Serialized/pipeline/package |
|---|---|---|---|
| A | **18 / 1 / 1 / 2** (22 applicable) | **8 / 6 / 0 / 1** (15) | **0 / 6 / 1 / 0** (7) |
| B | **8 / 1 / 1 / 3** (13) | **7 / 1 / 0 / 0** (8) | **0 / 3 / 1 / 0** (4) |
| C gameplay | **12 / 2 / 1 / 2** (17) | **4 / 3 / 0 / 0** (7) | **0 / 2 / 1 / 0** (3) |

The strongest gap is therefore not simply “more names needed.” The reference explicitly excludes exact native scene/prefab/importer formats and package asset fidelity. Good selected API/lifecycle coverage does not erase that missing layer.

## 5. Source findings versus the actual written guide

All **G01–G18** were read, including the current Stage 4 edits, `stage4-supplement.md`, index and uncertainty/provenance material. Old proposal queues are not substitute current contracts. Citations below are file-relative to this reference folder; game citations use the A/B/C roots above. Complete row evidence is in JSON.

### A — modern Breakout

Written strengths include generic component discovery/Unity-null, modern find families, callback eligibility, fixed-clock MovePosition, Random interval distinctions, coroutine handles/scaled waits, SpriteRenderer swapping and pixel/PPU sizing. For example:

- `A-R0025`: `Platform.cs:76–91` stops the prior width routine, widens immediately and restores after `WaitForSeconds`. **G02.T03, `G02-lifecycle-time.md:55–80`** explicitly supplies handle identity, immediate first advance and scaled/frame-quantized waiting. No Animator support is inferred from the word “animation.”
- `A-R0029`: `BrickSpawner.cs:61–74` derives spacing from `sprite.rect / pixelsPerUnit * localScale`. **G10.T01, `G10-sprites-tilemaps.md:7–33`**, plus **G04.T01:7–31**, supplies pixel/local/world distinctions.

Important differences:

| IDs / status | Exact source evidence | Written evidence and missing obligation |
|---|---|---|
| `A-R0021` **absent declaration** | `Assets/Scripts/Brick.cs:28–35`, `Platform.cs:64–70`: `collision.gameObject` on `Collision2D` | **G06.T04 `G06-physics2d.md:57–65`** names other Collision2D properties, not gameObject. **G01.T03:74** describes Component, and **G07.T04:65** 3D Collision, neither this separate type. |
| `A-R0015/16` **ambiguous exact overload** | `BallSpawner.cs:38`, `BrickSpawner.cs:77`: world clone, with/without parent | **G05.T03 `G05-scenes-serialization.md:48–59`** gives matching generic families and semantics but acknowledges non-generic forms/unverified declarations. Actual overload selection was not bound. |
| `A-R0014` **ambiguous behavior** | `DeathZone.cs:12–17`: Destroy then find/count then last-ball respawn | **G01.T04:89–93** supplies deferred destruction, not exact pending-object query visibility during this trigger phase. No inferred runtime respawn verdict. |
| `A-R0039/40/42` **partial** | `Platform.cs:45–61`, manifest `:14`, PlayerSettings `:683` | **G08.T03/T06 `G08-input.md:43–97`** supplies null current-device/spaceKey/isPressed/update-mode boundaries, not aKey/dKey declarations and pinned fixed-phase/package configuration. |
| `A-R0023/36/43` **partial** | Ball prefab `:141–159`, scene `:168–186,842–879`, material `:10–14`, Physics2DSettings `:41–56` | **G06.T01/T02/T04/T06** and **supplement J3-005:5–13** supply baseline non-trigger pairing/participation, not exact CCD/material-combine/resize-disable/contact-order/native-settings contracts. Supplement does not completely specify 2D trigger edge cases. |
| `A-R0034` **absent content schema**; `35/37/38` **partial** | scene `:325–350,1005–1016,1123–1140,1641–1701`; Ball prefab `:161–172`; spritesheet metadata `:400–405,454–470`; URP/Renderer2D assets | **G05.T04/T05:63–79** explicitly omits native formats. **G10.T03–T06:45–105**, **G09.T06:112–120**, **supplement J3-007:23–25** explain concepts/boundaries, not native tile/outline/subasset/URP2D interoperability. |

Fixture caveats, kept distinct from guide defects:

- Ball prefab `Assets/Prefabs/Ball.prefab:18` uses `Ball`; `TagManager.asset:6` has `tags: []`. Script `CompareTag("Ball")` occurs in `Brick.cs:33` and `DeathZone.cs:9`. The observed mismatch needs explicit handling; no execution failure/error text was measured (`A-R0033`, partial).
- Ball prefab `:172` stores **startSpeed**, while source `Ball.cs:9` declares **speed**. Brick prefab also retains extra historical tuning fields. Exact missing/stale-field load behavior must not be guessed from a source initializer. This is evidence for a content/migration boundary, not permission to silently patch the fixture.
- This is a live tilemap collision dependency, not just unused palette content: scene `:1665–1701` contains `TilemapCollider2D`; the tile asset `Assets/Images/Palette/brick.asset:14–36` explicitly identifies a Tile and collider mode. The editor palette prefab and URP scene template are not additional enabled gameplay scenes.

### B — modern Snake

The smaller first slice is justified by actual source: Update/legacy key edges, Vector2Int, Random, ScriptableObject tuning, direct world-space Transform placement, basic sprites, and a managed List pool. There is no authored physics/coroutine/Animator/Tilemap/URP usage in the selected gameplay closure.

- `B-R0018/20`: `SnakeView.cs:19–38` reuses active/inactive objects and calls `Instantiate(segmentPrefab, transform)` only when the list must grow. **G05.T03:50–59** explicitly covers Component-owning clone graph and parent-only local placement, and **supplement J3-011:39–41** explains reuse versus Destroy/second Start. This earns bounded semantic credit even though exact selected overload `B-R0019` remains ambiguous. It does **not** claim the game calls `UnityEngine.Pool.ObjectPool<T>`.
- `B-R0015`: `GameLoop.cs:28–37` reads `PlayerInputReader.cs:7–10` before consuming `InputBuffer.cs:8–14`. **G08.T01:11–29** supplies frame-stable non-destructive edges; buffering/reverse-direction policy itself is managed game logic.

| IDs / status | Exact source evidence | Written evidence and missing obligation |
|---|---|---|
| `B-R0003` **absent authoring declaration** | `GameConfig.cs:4`: `[CreateAssetMenu(menuName = "Snake/GameConfig")]` | **G01.T01/T04/T05** (`:9,95,101–103`) names ScriptableObject/CreateInstance/other attributes, not this attribute/member. **G18.T01** does not fill it. This is editor authoring, not a prerequisite to player config loading. |
| `B-R0004/6` **partial**; `5/9` **ambiguous** | `GridSystem.cs:14–31`; `SnakeModel.cs:14–15,28,46`; `GameLoop.cs:20`; `FoodSpawner.cs:22`, `SnakeView.cs:25` | **G03.T02 `G03-math.md:34–59`** names integer components/discrete semantics and warns type-specific equality/hash; not complete integer directions/operators/division/equality nor the exact two-argument Vector3 constructor/optional-Z declaration. |
| `B-R0021` **absent native graph schema** | scene `:334–346,385–399`, config asset `:12–20` | **G05.T04/T05:63–79** supplies field concepts, excludes exact native scene/prefab/config format. Actual config is **40×20, wrap=true, 8 ticks/s**, not simply source defaults. |
| `B-R0022` **partial content** | both visual prefabs `:84` reference sprite GUID `311925a002f4447b3a28927169b83ea6` | No local `Assets/*.meta` defines this GUID. **G10.T01/T02:7–43** covers sprites, not asset recovery; **G05:69** requires separate identity model. Do not invent a placeholder and call the original content supported. |
| `B-R0024/25` **partial setup** | PlayerSettings `:907`, manifest `:3–12`, lock `:147–161`, Graphics/Quality pipeline references | **G08:5,93–97**, **G09.T06:112–120** and **supplement J3-017:63–67** supply boundaries, not exact backend/pipeline/import/build package closure. |

Application-source caveats, **not additional Unity-API coverage failures**:

- README `:12–29` calls the model “zero Unity dependencies,” but `SnakeModel.cs:4,9–15` and `GridSystem.cs:1–8` depend on Unity value types. Separation from MonoBehaviour/infrastructure is still useful; it is not independence from Unity types.
- README `:37–38` claims identical low/high-FPS behavior and zero gameplay Instantiate calls. `GameLoop.cs:32–34` performs **at most one tick per Update** (an `if`, not a catch-up loop), and `SnakeView.cs:35–38` instantiates when the pool grows. Neither assertion is accepted as measured behavior.
- `GridSystem.InBounds` exists at `:37–38`, but `SnakeModel.Tick:40–65` never calls it. The non-wrapping boundary-death rule is not implemented in that path. The serialized fixture uses wrap=true; do not silently assert non-wrap game completeness.
- `FoodSpawner.cs:15–21` returns the last candidate after its attempt limit even if still occupied. Tail-cell occupancy/growth handling at `SnakeModel.cs:46–62` also deserves characterization. These are future application tests, not reasons to alter Unity Random or vector contracts.
- `FoodSpawner.cs:22` uses unscaled cell coordinates; `SnakeView.cs:25` multiplies by its own serialized cellSize. `GameConfig.cellSize` at `:10` is not consumed there. Defaults/fixture use 1, but a generalized scale setting needs application-level agreement.
- Recovery scene `_Recovery/0.unity` is **not identical** to the enabled scene: values at `:273,277` are -12.11/-15.27 versus 0/0 in GameScene's food-position overrides. It was not counted as another runtime scene.

### C — historical Pong and its tests

Pong contributes useful explicit input/time/random seams and score/reset/relaunch scenarios. It is **Unity 2017.1.0f3 source, not evidence that these historical declarations or files work under Unity 6.3**.

| IDs / status | Exact source evidence | Written evidence and missing obligation |
|---|---|---|
| `C-R0004/5/26` **partial** | `Ball/Ball.cs:28–36`, body/material prefab data | **G06.T01:7–25** mentions old velocity/new linearVelocity, angular velocity and Sleep, not exact alias availability; **G04.T06:137–143**, **G06.T06:75–81** do not fully specify reset/sync/sleep/native contact behavior. |
| `C-R0015` **absent exact spelling** | `Managers/MatchManager.cs:18`: `GameObject.FindGameObjectWithTag` | **G01.T02:31–45** lists `FindWithTag` and plural `FindGameObjectsWithTag`, not the actual singular long alias. Do not silently substitute another spelling or call the source API false. |
| `C-R0016`, `C-R0009` **ambiguous** | MatchManager `:19` inherited historical find qualifier; Ball `:12,35` plus BallMovement `:7–9` vector conversion | **G01.T04:93**, **G03.T02:59** acknowledge families, not selected historical declaration/inherited qualifier or concrete conversion operators. |
| `C-R0024` **partial declaration** | `Score/ScoreView.cs:6–17`: legacy `Text.text` get/set | **G11.T02:19** describes string/font/fontSize, no explicit Text.text property declaration. Nearby `InputField.text` (`:30`) and TMP_Text.text (`:56`) are not the same receiver. |
| `C-R0013` **partial semantics**; `25` **absent native schema** | `Goal/Goal.cs:4–23`, Goal prefab `:80–96`, Main `:559–592`, Scores prefab `:123–135` | **G01.T05:103** generic/persistent event distinction is useful but not enum listener/type-binding/native override format. **G05:63–79** excludes those schemas. |
| `C-R0020/27` **partial content** | InputManager `:25–38,297–310`, Main `:942–1017`, Scores prefab `:68–99,195–247` | **G08.T02:35** axis concepts; **G11.T01/T02:7–19**, **G04.T05:94–135** UI/layout concepts, not historical font/native UI/configuration reconstruction. |

Historical serialized caveats: Goal prefab `:80` stores **team**, while source now uses **scoresTo** (`Goal.cs:9`). Its event type string still says **Goal+ScoreEvent** (`Goal.prefab:95–96`), whereas source declares top-level `ScoreEvent` (`Goal.cs:5`). Main's left goal overrides `scoresTo=1` (`:591–592`) and retains old team; the right goal block (`:1027–1103`) retains team=0 without an observed scoresTo override. Both persistent-call blocks name `ScorePoint`. These are content/migration facts, not proof of successful or failed native event restoration.

**Tests were read, not run.** Seven Editor tests and four files under `Tests/PlayMode/Editor` make eleven. NUnit/UnityTest attributes, Resources and PrefabUtility fixture calls, and NSubstitute usage are separately inventoried (`C-R0028…32`). An Editor-named descendant/no asmdefs and a folder named PlayMode are not proof of actual execution mode or a successful historical runner. See **G18.T01:9**, **G18.T05:67–88**.

- `GoalTest.cs:24–34` directly calls `goal.OnTriggerEnter2D(ball)` and asserts a mocked event. It does **not** prove collision/trigger dispatch or persistent listener loading.
- `PlayerTest.cs:24–37` substitutes input and time, then expects public body movement after a fixed-related yield. That is an intended integration scenario, not a measurement, especially with a synthetic delta of 1 and physics movement limits.
- `MatchManagerTest.cs:97–110` waits the same nominal duration as the production coroutine before checking relaunch/visibility. Exact same-deadline ordering is not established by those source assertions.
- `BallTest.cs:42` itself notes that a nonzero launch test should be strengthened to exact starting velocity. Existing test presence is not complete or executed behavioral coverage.
- `C-R0031` is an absent **third-party test-tooling** contract (NSubstitute), not a missing Unity gameplay capability. DLL/version/rights remain unresolved.

### What the other chapters do not establish here

Reading all chapters matters, but titles, package manifests and boilerplate settings do not create source requirements. No authored gameplay here calls 3D Physics (**G07**), Animator/Playables (**G12**), AudioSource/clip playback (**G13**), networking (**G16**) or Jobs/Burst/Entities (**G17**). AudioListener/template settings alone do not establish a gameplay audio contract. **G14 Resources** and **G18 PrefabUtility/TestTools** are specifically observed in Pong's tests, not falsely counted as runtime loading calls in its gameplay scripts. Supplement input override persistence, NavMesh, particles, trails, profiling, video, runtime IMGUI, menu focus, Addressables and property-block additions were inspected; none is awarded gameplay coverage merely for existing in the guide.

## 6. Prioritized follow-up contracts — proposals only

These findings are also machine-readable in JSON. They reference the requirement IDs above, whose source/guide evidence carries through; they do **not** change the guide or claim executed tests.

| Finding | Desktop priority / affected IDs | Missing dimension and smallest useful contract/test |
|---|---|---|
| **S5-GAP-0001** | **P0 first fixture** — B `4/5/6/9/19`, A `15/16`, C `9/16` | Pin declaration/operator/conversion/overload contracts; retain optional-parameter versus overload identity. Future probes: integer coordinates/equality/hash, signed division, two-coordinate Vector3/default Z, clone return graph/local versus world placement. No semantic binding claimed now. |
| **S5-GAP-0002** | **P0 content decision** — A `34/35`, B `21/22`, C `25` | Decide native-content support versus an explicitly authored conversion fixture. Validate GUID/fileID/subasset closure, stale fields and persistent calls; report missing references rather than fabricate successful placeholders. A custom graph is not Unity YAML compatibility. |
| **S5-GAP-0003** | **P1 modern integration** — A `39/40/42`, B `24` | Use legacy polling first for B. Separately specify Input System 1.17 letter controls/current-null and held-state visibility across dynamic/fixed boundaries with future controlled input probes. Backend/schema selection is distinct from input calls. |
| **S5-GAP-0004** | **P1 second fixture** — A `14/21/23/36/43`, C `5/26` | Validate Collision2D.gameObject declaration, then defined-step collision/trigger/body/material/resize/reset/sleep and pending-destruction query contracts. Observe contact, callbacks and motion separately; do not infer last-ball correctness from deferred Destroy alone. |
| **S5-GAP-0005** | **P1 rendering scope** — A `37/38`, B `25` | Begin with B's simple sprite path. Explicitly unsupported URP2D until selected renderer/material/light/tile-geometry tests exist; preserve quality overrides and native import boundaries. |
| **S5-GAP-0006** | **P2 historical regression** — C `13/15/24/27–32` | Explicit legacy aliases/Text.text, enum score dispatch/visible text, native UI separately; port tests intentionally with runner/plugin ownership and rights established. Direct mocked trigger calls do not replace physics tests. |
| **S5-GAP-0007** | **P2 editor authoring** — B `3` | Add selected CreateAssetMenu/menuName authoring contract only if editor workflow is in scope. Runtime config loading must not require recreating the authoring menu. Future editor test observes asset/default persistence. |

A source recommendation is not a count contest. B's **25** bounded requirements offer a smaller vertical slice, managed model/view seams and no selected solver/URP dependency. A's **43** is a better second integration gate for real native/package boundaries. C's **27 gameplay + 5 test-only** requirements are useful historically but add old aliases, native prefab/event/UI and external mocking concerns; they should not displace a verified-modern-version source when targeting 6.3.

## 7. Verification actually performed and reproducibility

Offline validation passed for JSON parsing, all source/guide citation ranges, exact authored C# path/line populations, unique requirement IDs, dimensional aggregate logic, per-game arithmetic, finding references and overall totals. Pin equality and clean local game working trees were checked. These are **document/inventory checks**, not executable game/engine behavioral tests.

A compact arithmetic check, using only local files and Python standard library:

```python
from pathlib import Path
import json

base = Path('/home/acoliver/projects/real-disunity/branch-1/docs/training-only-unity6-reference')
data = json.loads((base / 'game-comparison-inventory.json').read_text())
statuses = ('covered', 'partial', 'absent', 'ambiguous')
all_gameplay = []
for game in data['games']:
    rows = game['requirements']
    counts = {'N': len(rows)}
    counts.update({s: sum(r['status'] == s for r in rows) for s in statuses})
    assert counts == game['counts']
    assert counts['N'] == sum(counts[s] for s in statuses)
    print(game['id'], counts)
    all_gameplay.extend(rows)
assert len(all_gameplay) == data['totals']['gameplayRequirements'] == 95
for status in statuses:
    assert sum(r['status'] == status for r in all_gameplay) == data['totals'][status]
assert sum(len(g['testRequirements']) for g in data['games']) == 5
```

The full performed validation also checked actual `Assets/**/*.cs` against `scriptInventory`, every stored script line count, all 100 source-evidenced requirement IDs including tests, guide evidence bounds and finding links. It did not validate Unity serialized loading or compile the shown guide/game C#.

## 8. Remaining limits and documented-versus-remembered distinction

- **Observed:** local pins, source text/receivers, saved editor strings, license notices, asset references/configuration, manifest/lock metadata, actual written guide propositions and reproducible counts.
- **Written but unverified:** guide API families/lifecycle/time/math expectations and Stage 4 additions. The reference is training-derived, unofficial, illustrative and not compile-tested. Its **18 groups / 108 topics / 57 unresolved uncertainties** are unchanged.
- **Interpreted using training knowledge, not promoted to written coverage:** numeric input/body/pipeline/serialized mode meanings, GUID-only package/UI component identities and likely overload/conversion candidates. Such interpretation cannot supply a missing guide declaration or certify a selected version.
- **Not established:** full API coverage, exact overload/alias metadata, physics/CCD/restitution/contact ordering, trigger teardown/query visibility, persistent event migration, importer/font/shader/URP/native scene loading, game completeness, actual test-runner mode, desktop builds, runtime compatibility, performance, platform/backend support or rights clearance.

No external docs, legal research, internet access, package/vendor implementation inspection, build/install/test/game execution or native Unity validation occurred. Missing coverage remains visible; no guide rewrite, game fixes, placeholder art, content conversion or claims of an unmodified runnable fixture were made.
