# Training-only Unity 6 conceptual API reference

**Status: training-derived, unverified, unofficial; Stage 2 corrections and Stage 4 J3 queue applied as bounded training-only additions.** Initial authoring consulted no external sources, local files, documentation, repository code, or games. Stage 2 read only the generated Markdown in this directory and used training knowledge/local consistency; it did not consult external documentation, repository/game sources, or execute Unity. This is a recollection-based authoring artifact, not an official reference. Training provenance and completeness cannot be certified. No independent-discovery, legal-clearance, or clean-room guarantee is made. All authored code/signatures are illustrative from memory, may be incomplete, and are not compile-tested. Independent model review/agreement is not external verification.

**Target:** Unity 6 concepts, with Unity 6.3 as the desired compatibility line. This document does **not** assert that any particular API was added, removed, renamed, or changed in 6.3. Exact-version questions remain open (U001). Familiar older API shapes are included where useful and are not evidence of current non-obsolescence.

## Stage 4 resolution status

The [Stage 4 resolution report](review-stage4-resolution.md) maps J3-001–J3-020 individually. All 20 proposals were applied either in existing group text or the new [Stage 4 supplemental contracts](stage4-supplement.md); the latter is companion material, not a new inventory topic. Counts remain **18 groups / 108 inventory topics**; supplemental material is counted separately. All 57 U entries remain unresolved and unverified. Stage 2 and Stage 3 reports are retained as historical reviews; this pass does not certify any API or exact Unity 6.3 behavior.

## Initial scope and navigation

There are **18 major-group documents**, each with **6 explicitly scoped topics**, for **108 inventory topics**. These are author-defined review units, not counts of Unity's namespaces, classes, or actual API surface. A topic can contain many types and omissions. An additional uncertainty register contains **57 entries, U001–U057**, all still unverified/unresolved. The original authored inventory contains 20 Markdown files: this index, 18 group files, and the register. With the added [Stage 2 corrections report](review-stage2-corrections.md), the directory contains 21 Markdown files. Group/topic IDs, inventory order, and depth labels are preserved.

## Stage 2 review status

All 18 group documents (108 topic sections), this index, and the uncertainty register were read by a fresh independent reviewer, using only generated Markdown and training knowledge. The [corrections report](review-stage2-corrections.md) preserves before/after propositions and file/topic evidence under stable IDs C2-001–C2-019.

| Category | Applied | Unresolved C2 findings | Total |
|---|---:|---:|---:|
| error | 3 | 0 | 3 |
| omission | 8 | 0 | 8 |
| clarification | 8 | 0 | 8 |
| **Total** | **19** | **0** | **19** |

Severity: 1 high, 13 medium, 5 low. Evidence basis: 18 memory-based judgments and 1 local bookkeeping/provenance reconciliation (C2-019), not 19 externally verified defects. All 57 U entries remain open; applied edits do not certify the recalled contracts or exact Unity 6.3 compatibility. No tests, compilation, Unity execution, internet, shell, or source inspection occurred.

| ID | Major-group file | Self-assessed depth |
|---|---|---|
| G01 | [Object model and components](G01-object-model.md) | substantial |
| G02 | [Lifecycle, execution, coroutines, and time](G02-lifecycle-time.md) | substantial |
| G03 | [Mathematics and value types](G03-math.md) | substantial |
| G04 | [Transforms and spatial hierarchy](G04-transforms.md) | substantial |
| G05 | [Scenes, instantiation, prefabs, serialization](G05-scenes-serialization.md) | substantial |
| G06 | [2D physics](G06-physics2d.md) | substantial |
| G07 | [3D physics](G07-physics3d.md) | substantial |
| G08 | [Legacy Input and package Input System](G08-input.md) | partial |
| G09 | [Cameras, graphics, meshes, materials, textures, pipelines](G09-rendering.md) | substantial |
| G10 | [Sprites and tilemaps](G10-sprites-tilemaps.md) | partial |
| G11 | [uGUI, EventSystems, TMP, UI Toolkit](G11-ui.md) | partial |
| G12 | [Animation](G12-animation.md) | substantial |
| G13 | [Audio](G13-audio.md) | substantial |
| G14 | [Resources, AssetBundles, Addressables](G14-assets.md) | substantial |
| G15 | [Application, platforms, screen, preferences, files](G15-platform.md) | substantial |
| G16 | [Networking and UnityWebRequest](G16-networking.md) | partial |
| G17 | [Jobs, native collections, Burst, ECS](G17-jobs-ecs.md) | partial |
| G18 | [Editor-only APIs and testing](G18-editor-testing.md) | partial |

Read [the uncertainty register](uncertainty-register.md) alongside the groups. “Substantial” describes the amount of useful treatment **within this selected scope**, not verified correctness, exhaustiveness, implementation readiness, or a fraction of the Unity API. Allowed labels for later review are `substantial`, `partial`, `outline`, and `unknown`.

## Machine-readable coverage inventory

The JSON below is the authoritative list of authored review topics. Topic identity is the parent group ID followed by `.T01` through `.T06`, in the listed order. For example, G01.T03 is “Component discovery and creation.” Labels are self-assessment only. They are not measured coverage.

```json
{
  "schema": "training-only-unity-reference/1",
  "target": "Unity 6 conceptual; Unity 6.3 desired, unverified",
  "assessment": "self-assessed-only",
  "stage2_review": {
    "status": "training-only-reviewed-unverified",
    "report": "review-stage2-corrections.md",
    "finding_ids": "C2-001–C2-019",
    "applied": {"error": 3, "omission": 8, "clarification": 8},
    "unresolved_findings": 0
  },
  "group_count": 18,
  "topics_per_group": 6,
  "topic_count": 108,
  "uncertainty_count": 57,
  "uncertainty_file": "uncertainty-register.md",
  "topic_id_rule": "group_id + .T + one-based two-digit topic index",
  "groups": [
    {"id":"G01","file":"G01-object-model.md","label":"substantial","topics":["Object identity and Unity-null","GameObjects, activity, tags, layers","Component discovery and creation","Lifetime, destruction, persistence","Messages, attributes, events","Managed/native ownership contract"],"uncertainties":["U004","U005","U006"]},
    {"id":"G02","file":"G02-lifecycle-time.md","label":"substantial","topics":["Lifecycle callbacks","Player loop and execution ordering","Coroutines and yield instructions","Time and fixed stepping","Invoke, cancellation, asynchronous work","Minimal scheduler behavioral contracts"],"uncertainties":["U007","U008","U009"]},
    {"id":"G03","file":"G03-math.md","label":"substantial","topics":["Scalar math and Mathf","Vectors and integer vectors","Quaternions","Matrices and projection math","Geometric value types","Randomness, interpolation, numerical contracts"],"uncertainties":["U010","U011","U012"]},
    {"id":"G04","file":"G04-transforms.md","label":"substantial","topics":["Local and world TRS","Hierarchy and parenting","Point, vector, direction conversion","Movement and rotation helpers","RectTransform","Transform/physics/render synchronization"],"uncertainties":["U013","U014","U015"]},
    {"id":"G05","file":"G05-scenes-serialization.md","label":"substantial","topics":["Scene handles and discovery","Loading, activation, unloading","Instantiation and cloning","Prefab runtime/editor boundary","Serialized fields and references","Serialization callbacks and JSON"],"uncertainties":["U016","U017","U018"]},
    {"id":"G06","file":"G06-physics2d.md","label":"substantial","topics":["Bodies and simulation","Colliders and materials","Queries and filtering","Collision and trigger events","Joints and effectors","Stepping, scenes, numerical compatibility"],"uncertainties":["U019","U020","U021"]},
    {"id":"G07","file":"G07-physics3d.md","label":"substantial","topics":["Rigidbody state and forces","Collider geometry and materials","Queries and filtering","Contacts, callbacks, triggers","Joints, CharacterController, vehicles","Simulation scenes and synchronization"],"uncertainties":["U022","U023","U024"]},
    {"id":"G08","file":"G08-input.md","label":"partial","topics":["Legacy frame polling","Legacy axes, touches, sensors","Package devices and controls","Actions and interactions","PlayerInput and UI integration","Update timing, rebinding, compatibility"],"uncertainties":["U025","U026","U027"]},
    {"id":"G09","file":"G09-rendering.md","label":"substantial","topics":["Cameras and coordinate conversion","Mesh data and drawing","Materials, shaders, property blocks","Textures and render targets","Graphics, render state, command buffers","Built-in, SRP, URP, HDRP boundaries"],"uncertainties":["U028","U029","U030"]},
    {"id":"G10","file":"G10-sprites-tilemaps.md","label":"partial","topics":["Sprite assets and creation","SpriteRenderer and sorting","Tilemap cell/world mapping","Tiles, colors, transforms, bulk edits","Tilemap rendering and collision","Atlases, packages, runtime subset"],"uncertainties":["U031","U032","U033"]},
    {"id":"G11","file":"G11-ui.md","label":"partial","topics":["Canvas and uGUI layout","uGUI controls and events","EventSystem and raycasting","TextMesh Pro","UI Toolkit retained trees","UI Toolkit styling, events, binding boundaries"],"uncertainties":["U034","U035","U036"]},
    {"id":"G12","file":"G12-animation.md","label":"substantial","topics":["Animator controllers and parameters","Animator state, layers, transitions","Root motion and animation callbacks","Clips, curves, legacy Animation","Playables and Timeline boundaries","Humanoid rigs, skinning, compatibility"],"uncertainties":["U037","U038","U039"]},
    {"id":"G13","file":"G13-audio.md","label":"substantial","topics":["AudioSource playback","AudioClip data and loading","Listeners, spatialization, attenuation","Mixer groups, parameters, snapshots","DSP scheduling and callbacks","Import, platform, lifecycle contracts"],"uncertainties":["U040","U041","U042"]},
    {"id":"G14","file":"G14-assets.md","label":"substantial","topics":["Resources loading and lifetime","AssetBundle loading and assets","AssetBundle dependencies and unloading","Addressables keys and handles","Addressables scenes, instances, catalogs","Ownership, errors, serialization boundaries"],"uncertainties":["U043","U044","U045"]},
    {"id":"G15","file":"G15-platform.md","label":"substantial","topics":["Application lifecycle and platform","Screen, display, quality","PlayerPrefs","Persistent paths and filesystem","SystemInfo and capability checks","Logging, environment, platform boundaries"],"uncertainties":["U046","U047","U048"]},
    {"id":"G16","file":"G16-networking.md","label":"partial","topics":["UnityWebRequest lifecycle","Download handlers","Upload, forms, request bodies","Errors, headers, security, cancellation","Transport and multiplayer package boundaries","Compatibility and protocol test contracts"],"uncertainties":["U049","U050","U051"]},
    {"id":"G17","file":"G17-jobs-ecs.md","label":"partial","topics":["Job interfaces and dependencies","Native containers and allocation","Parallel safety and data ownership","Burst compilation boundary","Entities worlds, components, queries","Systems, command buffers, baking boundaries"],"uncertainties":["U052","U053","U054"]},
    {"id":"G18","file":"G18-editor-testing.md","label":"partial","topics":["Editor/player assembly boundary","AssetDatabase and importers","SerializedObject, inspectors, Undo","Prefab, scene, build tooling","Unity Test Framework and execution","Compatibility harness and independent review"],"uncertainties":["U055","U056","U057"]}
  ]
}
```

## How to read signatures

Code fences are **selected callable shapes**, not complete class definitions, metadata dumps, implementations, or promises that all overloads exist in the desired editor. A signature with parameter names but without default values does not claim those defaults. Types generally live in the namespace named in the surrounding section. Fully qualified names are supplied where collisions matter. Generic constraints, attributes, optional parameters, nullability annotations, native bindings, and obsolete aliases are often omitted. Semantics are recalled expectations, not quotations of documentation.

Implementors should distinguish the following compatibility targets:

1. **Source/API compatibility:** client code can refer to a name, type, member, enum value, and overload with sufficiently similar behavior. Merely compiling stubs is not behavioral compatibility.
2. **Runtime/engine compatibility:** lifecycle, object identity, hierarchy, physics, rendering, scheduling, resource ownership, and observable errors agree with the chosen contract. Many C# types front native engine state.
3. **Serialized-content compatibility:** loading scenes, prefabs, imported assets, managed references, bundles, or player data requires formats, type identities, reference IDs, dependency resolution, import metadata, and native resource reconstruction. Implementing similarly named classes does not establish this.
4. **Editor/tooling compatibility:** asset importing, inspectors, Undo, scene editing, prefab overrides, build pipelines, and test runners are a separate environment. Player support does not imply editor support.
5. **Package compatibility:** Input System, Addressables, URP/HDRP, Collections, Burst, Entities, UI-related packages, and other packages have their own versions and dependencies. A Unity editor version alone is insufficient to select these contracts.
6. **Binary/native compatibility:** managed assembly identities, native entry points, shaders, serialized files, native plugin ABIs, graphics backends, and platform build outputs are not specified here.

### Modules versus packages

“Engine/module” means a Unity-provided native/managed capability, frequently surfaced through `UnityEngine` and module assemblies; it does not imply every build includes it. Physics2D, Physics, Audio, animation, AssetBundle, UIElements engine support, and UnityWebRequest are examples of modular engine capabilities. Namespaces do not reliably identify installation provenance. Packages can ship managed code, editor integrations, native support, or mixtures. The rendering group distinguishes the engine's SRP interfaces from URP/HDRP package implementations. The UI group distinguishes engine UI support from uGUI, TMP, and UI Toolkit's version-dependent distribution. Package identifiers are recalled where confident, not a dependency lockfile.

## Suggested subset-runtime strategy

Choose a small supported contract before implementing. A useful first vertical slice is object identity and Unity-null, GameObject/Transform hierarchy, deterministic lifecycle dispatch, scene ownership, basic value math, and one rendering/input path. Advertise unsupported capabilities explicitly. Do not silently treat assets, shaders, physics, or controller graphs as interchangeable with native Unity content.

For every supported call, establish input validation, return values, side effects, lifetime, thread affinity, callback timing, and failure behavior. Preserve a distinction between immediate state and state committed at a frame/physics/render boundary. Build behavior tests at public boundaries; proposed tests in these documents have **not been implemented or executed**. Exact error text and incidental callback order should not become contracts without verification.

## Review handoff

Later reviewers can independently own groups by stable ID. Record findings against `Gnn.Tnn` and uncertainty IDs, identifying the precise editor version, package versions, platform, backend, and test conditions. Separate a signature correction from a behavioral correction. Do not upgrade the depth labels into evidence of correctness. The initial authoring stage performed no verification, compilation, execution, external consultation, source inspection, or independent-agent review. The later Stage 2 review is limited to training knowledge and these generated artifacts; see the [report](review-stage2-corrections.md) for applied changes and the [register](uncertainty-register.md) for all outstanding verification work.

## Completed comparison and final artifact validation

- [Final summary and offline validation](final-summary.md): current counts, methodology, historical review boundaries, report-locator repairs and remaining unknowns.
- [Pinned local game comparison](game-comparison.md): 95 gameplay/support requirement instances plus 5 separately counted historical Pong test requirements.
- [Machine-readable game comparison inventory](game-comparison-inventory.json): source/guide evidence, dimensional assessments and exact counts.

Final population: **27 artifacts (26 Markdown + 1 standalone JSON)**; the 18-group/108-topic inventory is unchanged and all 57 uncertainties remain unresolved. Earlier file counts and Stage 3's 63 substantial / 40 partial / 5 outline grades are historical, not post-Stage-4 reassessments. Links are appended here to preserve existing guide citation lines.
