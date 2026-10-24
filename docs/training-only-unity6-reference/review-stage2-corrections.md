# Stage 2 — Training-only corrections review

**Status: applied training-derived review, unverified, unofficial.** A fresh independent reviewer read all 18 generated Gxx Markdown files, the index, and the uncertainty register in this directory. Only these generated artifacts and the reviewer's training knowledge were used. No internet, search, shell, repository/game sources, outside documentation, package manifests, compilation, tests, or Unity execution were used. No exact Unity 6.3 compatibility is verified. Independent model agreement is not external verification, and training provenance/completeness cannot be certified.

## Scope, counting, and evidence

The review covers the existing **18 groups / 108 inventory topics** and **57 uncertainty entries U001–U057**. Group IDs, topic IDs/order, depth labels, and uncertainty IDs/review targets are preserved. The initial inventory was 20 authored Markdown files; this report is the 21st file, not another API group.

A finding is one correction unit, not necessarily one false statement or one changed line. `error` denotes an incorrect API proposition or misleading declaration; `omission` supplies an important missing contract; `clarification` disambiguates an existing proposition without counting it as an established factual error. High severity means substantial misuse risk; medium means API/behavior/ownership consequences; low means limited-scope wording or bookkeeping. No severity is an externally measured defect rating.

Two evidence bases are kept separate:

- **Memory-based judgment (M):** the API proposition is recalled from training. Its correctness is not proved by this review, even when the original wording/absence is directly visible locally. There are 18 such findings.
- **Local consistency/provenance (L):** original file inventory and historical review-status statements can be reconciled from these artifacts and the operations performed here. This proves only local bookkeeping, not Unity behavior. There is 1 such finding, C2-019. Initial-authoring provenance remains the author's assertion, not something independently certified here.

Before excerpts below preserve the relevant authored-file evidence read before editing; they are not external citations. Ellipses indicate excerpts, not full-file snapshots. The file/topic inventory below preserves evidence for unchanged files too. No immutable copy or byte-level hash of the original directory was made; the report preserves proposition-level before/after evidence, not a complete original revision.

## Exact counts

| Category | Applied | Unresolved C2 findings | Total | Stable IDs |
|---|---:|---:|---:|---|
| error | 3 | 0 | 3 | C2-001, C2-012, C2-013 |
| omission | 8 | 0 | 8 | C2-003, C2-004, C2-005, C2-009, C2-010, C2-015, C2-017, C2-018 |
| clarification | 8 | 0 | 8 | C2-002, C2-006, C2-007, C2-008, C2-011, C2-014, C2-016, C2-019 |
| **Total** | **19** | **0** | **19** | **C2-001–C2-019** |

| Severity | Applied | Unresolved C2 findings | IDs |
|---|---:|---:|---|
| high | 1 | 0 | C2-004 |
| medium | 13 | 0 | C2-002, C2-003, C2-005, C2-006, C2-007, C2-009, C2-010, C2-012, C2-013, C2-014, C2-015, C2-017, C2-018 |
| low | 5 | 0 | C2-001, C2-008, C2-011, C2-016, C2-019 |

**Applied does not mean verified.** Zero unresolved C2 findings means that each proposed edit listed here was applied, not that the reference has no remaining issues. **All 57 U entries remain unverified/unresolved; 0 were closed.** No measured API-coverage or error-rate claim follows from these counts.

## Findings

### C2-001 — Do not imply an Mathf.Abs array overload

- **Category / severity / status / basis:** error / low / applied / M.
- **Affected:** [G03-math.md](G03-math.md), G03.T01; U002, U010.
- **Before:** “Min/Max and Abs also have integer/array overload families.” This groups Abs with the array-taking Min/Max families.
- **After:** Min/Max have integer and params-array families; recalled Abs forms take scalar float or int, not an array.
- **Rationale:** The grouped wording suggests a nonexistent collection operation. This is one narrow overload-family error, not an indictment of the rest of Mathf's inventory. Exact target declarations remain unverified.

### C2-002 — State Ray's own normalization contract

- **Category / severity / status / basis:** clarification / medium / applied / M.
- **Affected:** [G03-math.md](G03-math.md), G03.T05; U002, U012.
- **Before:** Ray's description says “Caller direction normalization and API-specific distance interpretation matter” without saying what Ray stores.
- **After:** The recalled Ray constructor and direction setter normalize the input; GetPoint uses that stored direction. Zero direction is degenerate; independent physics direction-vector overloads need their own contracts.
- **Rationale:** Avoid implementing Ray as if it necessarily retained the supplied vector's magnitude. The original caution is not counted as an explicit false assertion.

### C2-003 — Supply the basic DontDestroyOnLoad root/hierarchy rule

- **Category / severity / status / basis:** omission / medium / applied / M.
- **Affected:** [G01-object-model.md](G01-object-model.md), G01.T04; U005.
- **Before:** “DontDestroyOnLoad preserves suitable objects across scene changes, with root-object restrictions and hierarchy behavior to verify.”
- **After:** Recalled applicability is root GameObjects or components on root GameObjects; their Transform descendants are preserved too. An arbitrary non-root is not made independently persistent. Persistence-scene/HideFlags details remain open.
- **Rationale:** This common restriction is important enough to state while retaining uncertainty about more detailed transitions. No exact error/log behavior is invented.

### C2-004 — Make Awaitable single-await and synchronous-continuation hazards explicit

- **Category / severity / status / basis:** omission / high / applied / M.
- **Affected:** [G02-lifecycle-time.md](G02-lifecycle-time.md), G02.T05; U009.
- **Before:** “Exact cancellation overloads, pooling restrictions, and continuation scheduling are U009” with no operative ownership rule.
- **After:** Recalled Awaitable instances are pooled and must not be awaited more than once; they are not reusable/multi-consumer Tasks. Completion runs continuations synchronously, can re-enter awaiting code, and its thread matters. Exact operation phases and cancellation edge cases remain open.
- **Rationale:** Task-like caching/sharing can produce hangs or invalid reuse; assuming deferred main-thread continuation can also cause reentrancy/thread-affinity mistakes. These are basic safety rules, not an assertion that every async overload was verified.

### C2-005 — Include SerializeReference in explicit field opt-in

- **Category / severity / status / basis:** omission / medium / applied / M.
- **Affected:** [G05-scenes-serialization.md](G05-scenes-serialization.md), G05.T05; U018.
- **Before:** Eligible fields are described as “public or [SerializeField]”; SerializeReference is discussed later only as a managed-reference feature.
- **After:** Explicit opt-in includes SerializeReference, including for otherwise non-public eligible fields, subject to its own managed-reference type restrictions.
- **Rationale:** The eligibility summary could otherwise exclude a supported private managed-reference field. This does not promise unrestricted generic/container serialization or native file interoperability.

### C2-006 — Separate parent-only Instantiate from SetParent defaults

- **Category / severity / status / basis:** clarification / medium / applied / M.
- **Affected:** [G05-scenes-serialization.md](G05-scenes-serialization.md), G05.T03, in relation to G04.T02; U013, U017.
- **Before:** “Parent-only overload semantics must not be conflated with worldPositionStays overloads,” without stating the difference.
- **After:** Recalled parent-only Instantiate uses local placement, equivalent to the boolean overload with false; Transform.SetParent(parent) defaults to world preservation. Explicit position/rotation Instantiate arguments specify world placement even with a parent.
- **Rationale:** Two superficially similar parenting entry points do not have the same default. This fills in the existing warning rather than counting it as a false statement. Exact overload declarations remain U017.

### C2-007 — SliderJoint2D shares JointMotor2D despite torque-named field

- **Category / severity / status / basis:** clarification / medium / applied / M.
- **Affected:** [G06-physics2d.md](G06-physics2d.md), G06.T05; U021.
- **Before:** “A slider motor uses its own force-related configuration; do not blindly substitute hinge torque semantics.”
- **After:** Recalled HingeJoint2D.motor and SliderJoint2D.motor both use JointMotor2D. The slider interprets maxMotorTorque as a force limit and motorSpeed as linear speed; do not invent another struct or maxMotorForce field on JointMotor2D.
- **Rationale:** The original force-versus-torque distinction is useful but could imply a different declaration. This clarifies shared API shape versus distinct units rather than falsely saying the original explicitly named a nonexistent type.

### C2-008 — Bound ContactFilter2D normal-angle filtering by query kind

- **Category / severity / status / basis:** clarification / low / applied / M.
- **Affected:** [G06-physics2d.md](G06-physics2d.md), G06.T03; U020.
- **Before:** “ContactFilter2D allows explicit layer, depth, trigger, and normal-angle filtering” in a section also covering overlaps.
- **After:** Filters apply where the query supports them; overlap queries do not use normal-angle filtering in recalled behavior.
- **Rationale:** A filter's fields are not a promise that every consuming query honors every field. No query overload or result ordering is invented.

### C2-009 — Distinguish CharacterController.Move gravity from SimpleMove

- **Category / severity / status / basis:** omission / medium / applied / M.
- **Affected:** [G07-physics3d.md](G07-physics3d.md), G07.T05; U024.
- **Before:** Move takes a displacement; SimpleMove has “gravity-related built-in behavior.” Move's lack of automatic gravity, SimpleMove's ignored Y input, and SimpleMove's return meaning are unstated.
- **After:** Move does not apply gravity automatically; callers include it in displacement. Recalled SimpleMove ignores supplied Y speed, applies gravity, and returns a grounding bool.
- **Rationale:** These differences change character motion, not just units or wording. No full controller/grounding numerical equivalence is claimed.

### C2-010 — Specify camera ray origin and ignored input depth

- **Category / severity / status / basis:** omission / medium / applied / M.
- **Affected:** [G09-rendering.md](G09-rendering.md), G09.T01; U028.
- **Before:** ScreenPointToRay is recommended for surface selection, but its origin/input-Z behavior is not described.
- **After:** Recalled ScreenPointToRay/ViewportPointToRay ignore input Z and produce world-space rays starting on the near plane; orthographic ray origins vary while their directions are parallel.
- **Rationale:** Originating all rays at the camera Transform breaks orthographic picking and can change reported hit distances. Stereo/custom-projection edge cases are not certified.

### C2-011 — Explain Tilemap cell-center helpers in terms of tileAnchor

- **Category / severity / status / basis:** clarification / low / applied / M.
- **Affected:** [G10-sprites-tilemaps.md](G10-sprites-tilemaps.md), G10.T03; U032.
- **Before:** “GetCellCenterWorld incorporates the tilemap/grid center conventions.”
- **After:** Recalled Tilemap center helpers use tileAnchor; changing it can make the result differ from a geometric half-cell center. Grid.GetCellCenterWorld is a distinct layout-center API.
- **Rationale:** Avoid silently substituting hard-coded half-cell offsets or merging similarly named Grid and Tilemap methods. Exact hex/isometric/swizzle mappings remain open.

### C2-012 — Do not present an optional-parameter TMP call as an exact one-parameter declaration

- **Category / severity / status / basis:** error / medium / applied / M.
- **Affected:** [G11-ui.md](G11-ui.md), G11.T04; U002, U035.
- **Before:** The declaration-shaped fragment is `void SetText(string text);`; prose also says “shorter ForceMeshUpdate variants exist.”
- **After:** The recalled string declaration is `void SetText(string sourceText, bool syncTextInputBox = true);`. Prose distinguishes one-argument calls from one-parameter method declarations and describes optional ForceMeshUpdate arguments rather than asserting separate shorter overloads.
- **Rationale:** A one-string call is not alleged to be invalid; the problem is the misleading declaration/overload inference for delegate binding, reflection, or source-compatible implementation. Package-specific exact metadata remains unverified, and the selected-shapes disclaimer still applies.

### C2-013 — AudioListener.velocityUpdateMode is not static

- **Category / severity / status / basis:** error / medium / applied / M.
- **Affected:** [G13-audio.md](G13-audio.md), G13.T03; U002, U040.
- **Before:** AudioListener is described as having “static volume, pause and velocityUpdateMode concepts.”
- **After:** Recalled volume and pause are static; velocityUpdateMode belongs to an AudioListener instance.
- **Rationale:** Static versus instance ownership changes the access contract and incorrectly conflates global audio controls with listener velocity sampling configuration.

### C2-014 — EntityManager.Exists does not establish world provenance

- **Category / severity / status / basis:** clarification / medium / applied / M.
- **Affected:** [G17-jobs-ecs.md](G17-jobs-ecs.md), G17.T05; U054.
- **Before:** Entity is an “index/version-like identity valid within a world”; “a stale Entity must not become valid merely because an index is reused.”
- **After:** The recalled index/version value does not encode World identity; another world's matching pair can pass that manager's Exists check. Retain the owner world/manager. Within-world stale versions are rejected; finite-version wrap details remain open.
- **Rationale:** The original within-world statement is not inherently false, but does not warn against accidental cross-world validation or suggest limits on absolute never-reuse guarantees.

### C2-015 — Editor folder naming alone is insufficient under an asmdef

- **Category / severity / status / basis:** omission / medium / applied / M.
- **Affected:** [G18-editor-testing.md](G18-editor-testing.md), G18.T01; U055.
- **Before:** “Use an Editor folder and/or assembly definitions constrained to the Editor for tooling.”
- **After:** Editor-folder separation follows predefined-assembly rules; scripts assigned to an enclosing asmdef do not become editor-only just from an Editor-named subfolder. Use an Editor-constrained tooling asmdef where needed.
- **Rationale:** The apparently interchangeable choices can leak UnityEditor references into a player assembly. This adds the missing assembly-boundary qualification rather than claiming all Editor-folder usage is wrong.

### C2-016 — consoleLogPath is not merely editor-oriented

- **Category / severity / status / basis:** clarification / low / applied / M.
- **Affected:** [G15-platform.md](G15-platform.md), G15.T04; U046, U048.
- **Before:** The path list calls consoleLogPath “editor-oriented.”
- **After:** Recalled consoleLogPath identifies the Editor or player log file on supported platforms; it is not editor-only, not a general application-data directory, and may be unavailable/platform-dependent.
- **Rationale:** Avoid inferring Editor-only availability from a soft descriptive label. The original did not explicitly say the API was absent from players, so this is not counted as a factual error.

### C2-017 — DSP time can freeze under AudioListener.pause

- **Category / severity / status / basis:** omission / medium / applied / M.
- **Affected:** [G13-audio.md](G13-audio.md), G13.T05; U042.
- **Before:** dspTime is a double-precision scheduling clock; the document distinguishes timeScale from audio pause but does not state the listener-pause clock rule.
- **After:** In recalled behavior, AudioListener.pause freezes dspTime and pauses affected/scheduled playback. Sources ignoring listener pause are playback exceptions, not proof dspTime advances as wall time. Device/background suspension specifics remain open.
- **Rationale:** Treating DSP time as an unconditionally advancing real-time clock miscalculates pause/resume scheduling. This is separate from C2-013's static/instance declaration correction.

### C2-018 — Main-thread Temp allocations cannot be handed to scheduled jobs

- **Category / severity / status / basis:** omission / medium / applied / M.
- **Affected:** [G17-jobs-ecs.md](G17-jobs-ecs.md), G17.T02; U053.
- **Before:** “Temp is short-lived/thread-context-sensitive” without the scheduling prohibition.
- **After:** Do not pass main-thread Temp allocations into scheduled jobs; use an appropriate consumer lifetime, commonly TempJob or Persistent. Temp allocations made within a job's own thread scope are a separate case.
- **Rationale:** The original lifetime summary is too weak to guide safe ownership across scheduling. No custom allocator enforcement or exact safety exception is asserted.

### C2-019 — Reconcile historical authoring status with this bounded review

- **Category / severity / status / basis:** clarification / low / applied / L.
- **Affected:** [index.md](index.md), status, inventory, JSON metadata and Review handoff; [uncertainty-register.md](uncertainty-register.md), introduction and resolution protocol; [G18-editor-testing.md](G18-editor-testing.md), Boundary (group-wide context, not a new topic).
- **Before:** The index says the directory contains “20 authored Markdown files” and the stage performs no “independent-agent review”; the register says no entry has been checked by “another reviewing agent in this stage”; G18's Boundary similarly says no independent reviews occur.
- **After:** Preserve 20 as the original authored-file inventory and state 21 current Markdown files including this report. Distinguish initial authoring from later generated-Markdown-only independent review. Link the report, add review counts/status, retain self-assessed depth metadata, and explicitly keep every U entry unverified/unresolved.
- **Rationale:** Reading/reconciling these generated files and adding one report are locally observable operations, unlike verification of Unity behavior. The review does not retroactively falsify original-stage provenance or upgrade model agreement into evidence. The 57-entry count remains 3 + 18 × 3; no new U IDs are invented for already covered questions.

## Authored-file review inventory and modifications

Every file below was read in full before editing. Ranges denote the original six named topic sections in the index, not six independently verified API contracts. “Unchanged” means no confident correction was selected in this pass, not that the file is correct or complete.

| File / original authored identity | Topic IDs reviewed | Disposition / evidence |
|---|---|---|
| [G01-object-model.md](G01-object-model.md) — Object model and components | G01.T01–T06 | Modified; C2-003 |
| [G02-lifecycle-time.md](G02-lifecycle-time.md) — Lifecycle, execution, coroutines, and time | G02.T01–T06 | Modified; C2-004 |
| [G03-math.md](G03-math.md) — Mathematics and value types | G03.T01–T06 | Modified; C2-001, C2-002 |
| [G04-transforms.md](G04-transforms.md) — Transforms and spatial hierarchy | G04.T01–T06 | Unchanged; U013–U015 retained |
| [G05-scenes-serialization.md](G05-scenes-serialization.md) — Scenes, instantiation, prefabs, and serialization | G05.T01–T06 | Modified; C2-005, C2-006 |
| [G06-physics2d.md](G06-physics2d.md) — 2D physics | G06.T01–T06 | Modified; C2-007, C2-008 |
| [G07-physics3d.md](G07-physics3d.md) — 3D physics | G07.T01–T06 | Modified; C2-009 |
| [G08-input.md](G08-input.md) — Legacy Input and package Input System | G08.T01–T06 | Unchanged; U025–U027 retained |
| [G09-rendering.md](G09-rendering.md) — Cameras, graphics, meshes, materials, textures, pipelines | G09.T01–T06 | Modified; C2-010 |
| [G10-sprites-tilemaps.md](G10-sprites-tilemaps.md) — Sprites and tilemaps | G10.T01–T06 | Modified; C2-011 |
| [G11-ui.md](G11-ui.md) — uGUI, EventSystems, TextMesh Pro, UI Toolkit | G11.T01–T06 | Modified; C2-012 |
| [G12-animation.md](G12-animation.md) — Animation | G12.T01–T06 | Unchanged; U037–U039 retained |
| [G13-audio.md](G13-audio.md) — Audio | G13.T01–T06 | Modified; C2-013, C2-017 |
| [G14-assets.md](G14-assets.md) — Resources, AssetBundles, Addressables | G14.T01–T06 | Unchanged; U043–U045 retained |
| [G15-platform.md](G15-platform.md) — Application, platform, screen, preferences, files | G15.T01–T06 | Modified; C2-016 |
| [G16-networking.md](G16-networking.md) — Networking and UnityWebRequest | G16.T01–T06 | Unchanged; U049–U051 retained |
| [G17-jobs-ecs.md](G17-jobs-ecs.md) — Jobs, native collections, Burst, ECS | G17.T01–T06 | Modified; C2-014, C2-018 |
| [G18-editor-testing.md](G18-editor-testing.md) — Editor-only APIs and testing | G18.T01–T06 | Modified; C2-015, C2-019 |
| [index.md](index.md) — Training-only Unity 6 conceptual API reference | 18 groups / 108 topics; original navigation/JSON | Modified; C2-019; IDs/order/depth labels preserved |
| [uncertainty-register.md](uncertainty-register.md) — U001–U057 | All 57 entries and resolution protocol | Modified provenance only; C2-019; no U entry closed |

**File totals:** 13 existing group files modified; 5 existing group files unchanged; index and register modified; 1 new report. Thus **15 existing files modified + 1 created = 16 touched files**, all in this directory; 21 current Markdown files.

## Remaining uncertainties and limits

All register entries remain open. Important unresolved areas include:

- **U001–U003:** exact editor/player revision, package locks, omitted overloads/defaults/constraints, source versus native/serialized/editor compatibility. No exact 6.3 addition/removal/rename/obsolescence claim is established.
- **U004–U018:** discovery/null/lifecycle edge cases, continuation and cancellation timing, numerical degeneracies, hierarchy shear, scene queues, cloning declarations, and serialization defaults/managed-reference details. The Awaitable and parenting additions above do not close their broad entries.
- **U019–U024:** exact Unity 6 physics aliases (including velocity/damping and PhysicMaterial/PhysicsMaterial/combine enum availability), query overloads/truncation/order, body-pair callback matrices, composite geometry, solver and controller details. Naming is deliberately not guessed from presumed rename symmetry.
- **U025–U039:** Input System package/update/action/rebinding behavior; graphics/backend/pipeline extension contracts and Render Graph; tilemap layouts/atlas identity; exact uGUI/TMP/UI Toolkit distribution and generated/binding APIs; Animator/update enum naming and animation/rig/Timeline behavior. In particular U034 packaging is not resolved by remembered namespace names, and C2-012 is not a package metadata check.
- **U040–U051:** audio playback/sample/thread/device behavior, bundle/Addressables ownership and formats, platform storage/display/permissions, request factory/encoding/handler/TLS/CORS and multiplayer protocols. No HTTP or service behavior was exercised.
- **U052–U057:** exact Collections/Burst/Entities toolchain constraints and safety, code generation/baking, editor Undo/prefab/build tooling, and Test Framework/NUnit/runtime behavior. The Temp and world-provenance cautions are not substitute safety/toolchain tests.

These are uncertainties, not 57 confirmed errors. A later explicitly authorized verification stage must pin editor/packages/platform/backends and attach declaration or behavioral evidence to the existing stable U IDs. This pass neither executes that work nor claims the remaining text has no undiscovered errors. No external citations are supplied because no external sources were consulted.

## Checks actually performed

Read-only review of the original 20 Markdown artifacts, targeted in-directory edits, and a final Markdown readback/local reconciliation of the report, modified propositions, inventory, links, and counts. No automated validator, tests, lint, compilation, source inspection, or Unity execution was run. Manual consistency checking is not API verification.
