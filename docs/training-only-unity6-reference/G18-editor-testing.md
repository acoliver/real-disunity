# G18 — Editor-only APIs and testing

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** Unity 6.3/editor/package details are unverified. All code/signatures are authored illustrative subsets from memory, not complete definitions or compile-tested examples. See U001–U003, U055–U057.

**Boundary:** `UnityEditor` assemblies are editor-only, not player runtime libraries. Unity Test Framework is a separately versioned package, commonly `com.unity.test-framework`, with NUnit integration and `UnityEngine.TestTools` facilities. The initial authoring stage performed no inspections, tests, compilation, external validation, or independent reviews. The later [Stage 2 review](review-stage2-corrections.md) reads only this generated Markdown and uses training knowledge/local consistency; it is not external validation or Unity execution.

## G18.T01 — Editor/player assembly boundary

Use an Editor folder under the predefined-assembly rules, or assembly definitions explicitly constrained to the Editor, for tooling. An enclosing asmdef changes those rules: an Editor-named subfolder alone does not make scripts assigned to that assembly editor-only. Give editor tooling its own Editor-constrained asmdef where needed. `#if UNITY_EDITOR` guards can protect isolated references, but a runtime assembly with editor-only types in public contracts can still cause dependency problems. Runtime scripts should not require AssetDatabase to function in a built player. Assembly definition references, package version defines, platform filters and test assemblies are build-time contracts, not namespaces alone.

Common editor extensions: `EditorWindow`, `Editor`, `PropertyDrawer`, `DecoratorDrawer`, `MenuItem`, `CustomEditor`, `CustomPropertyDrawer`, and `InitializeOnLoad`/`InitializeOnLoadMethod`. `EditorApplication` exposes update/delayCall, play-mode state changes and compilation/lifecycle state in remembered APIs. `Selection`, `SceneView`, `Handles`, `Gizmos` and `EditorGUI`/`EditorGUILayout` support editing/visualization, with Gizmos itself engine-visible but mainly editor-rendered.

Editor IMGUI `OnGUI` can execute multiple times per interaction for Layout/Repaint/input events. Do not put irreversible runtime side effects unconditionally inside drawing code. `EditorWindow.CreateGUI` supports UI Toolkit trees in modern editors; exact window lifecycle and serialization are U055. Enter Play Mode options can skip domain/scene reload, so static state and event registrations need explicit cleanup/reinitialization strategies.

## G18.T02 — AssetDatabase and importers

```csharp
// Selected UnityEditor.AssetDatabase shapes.
static T LoadAssetAtPath<T>(string assetPath) where T : UnityEngine.Object;
static UnityEngine.Object[] LoadAllAssetsAtPath(string assetPath);
static string GetAssetPath(UnityEngine.Object assetObject);
static string AssetPathToGUID(string path);
static string GUIDToAssetPath(string guid);
static string[] FindAssets(string filter);
static void CreateAsset(UnityEngine.Object asset, string path);
static void AddObjectToAsset(UnityEngine.Object objectToAdd, UnityEngine.Object assetObject);
static bool DeleteAsset(string path);
static void SaveAssets();
static void Refresh();
static void ImportAsset(string path);
```

Folder-limited FindAssets and ImportAssetOptions overloads exist. Paths are project asset paths, conventionally starting `Assets/` or package asset roots where supported, not arbitrary player filesystem paths. GUIDs derive from asset metadata identity, not file contents. Subassets additionally require local identity; asset path alone is not a unique reference to every object stored in a file. AssetDatabase return objects may be cached imported assets, not fresh deserializations.

`AssetImporter.GetAtPath`, `TextureImporter`, `ModelImporter`, `AudioImporter` and scripted importer types configure importing. `UnityEditor.AssetImporters.ScriptedImporter`/`AssetImportContext` provide custom pipelines. `AssetPostprocessor` hooks can observe/configure import. Importing transforms source content into engine-specific artifacts with platform settings; file extension recognition alone does not reproduce importing.

`StartAssetEditing`/`StopAssetEditing` batch work and require finally-style cleanup. Refresh/import/save are distinct operations and can trigger callbacks/reimports; avoid recursive import loops. `AssetDatabase.GenerateUniqueAssetPath` helps avoid collisions but is not a cross-process transaction guarantee. Package assets can be immutable; do not assume every loaded asset can be edited in place.

## G18.T03 — SerializedObject, inspectors, Undo

`SerializedObject`/`SerializedProperty` expose editor serialization views supporting multi-object editing, prefab overrides and Undo integration. Familiar flow:

```csharp
// Illustrative inspector fragment, not a full Editor class.
serializedObject.Update();
var amount = serializedObject.FindProperty("amount");
UnityEditor.EditorGUILayout.PropertyField(amount);
serializedObject.ApplyModifiedProperties();
```

Selected SerializedProperty concepts: propertyPath, displayName, propertyType, intValue/floatValue/boolValue/stringValue/objectReferenceValue, arraySize, GetArrayElementAtIndex, managedReferenceValue, hasMultipleDifferentValues and relative-property traversal. Exact type-specific accessors and deletion behavior for reference arrays require U055. A SerializedProperty is a view tied to the serialized object state; retaining it across arbitrary structural edits can be unsafe.

Direct object mutation and serialized-property editing have different persistence/Undo/override behavior. `Undo.RecordObject`/`RecordObjects` capture changes; `Undo.RegisterCreatedObjectUndo`, `Undo.DestroyObjectImmediate`, `Undo.SetTransformParent`, grouping/collapse helpers provide specialized workflows. Record before the relevant mutation in ordinary usage. Scene dirty state, asset dirty state, Undo recording and saving are separate concerns.

`EditorUtility.SetDirty` is not a universal substitute for Undo or scene dirty marking. `UnityEditor.SceneManagement.EditorSceneManager.MarkSceneDirty` marks scene changes; prefab instances may need `PrefabUtility.RecordPrefabInstancePropertyModifications` for direct scripted changes. SerializedObject workflows can handle important pieces automatically, but exact behavior should be tested rather than combining every dirty/Undo API indiscriminately.

## G18.T04 — Prefab, scene, build tooling

`PrefabUtility.InstantiatePrefab` creates an editor-linked prefab instance, unlike ordinary runtime Object.Instantiate semantics. Remembered methods include `SaveAsPrefabAsset`, `SaveAsPrefabAssetAndConnect`, `LoadPrefabContents`, `SaveAsPrefabAsset` on edited roots, `UnloadPrefabContents`, `ApplyPrefabInstance`, `RevertPrefabInstance`, and instance/source lookup families. Use `InteractionMode.UserAction`/`AutomatedAction` where applicable. Exact overloads, nested prefab/variant rules and automatic save behavior are U056.

`EditorSceneManager.OpenScene`, `NewScene`, `SaveScene`, `SaveOpenScenes`, and scene setup capture/restore APIs work on editable scene assets. `OpenSceneMode.Single`/`Additive` differs from runtime LoadSceneMode. Tooling should restore the user's open-scene state where its operation temporarily changes it, and should not discard dirty scenes. This is distinct from a player's SceneManager load queue.

`BuildPipeline.BuildPlayer(BuildPlayerOptions)` produces a `UnityEditor.Build.Reporting.BuildReport`. Build options contain scenes, locationPathName, target and BuildOptions in remembered forms. `BuildTarget`, `BuildTargetGroup`, `PlayerSettings`, `EditorUserBuildSettings`, and `BuildOptions.Development`/`AllowDebugging`/`AutoRunPlayer` are important names; new build-profile workflows and exact Unity 6 interfaces are U056. `BuildPipeline.BuildAssetBundles` is not BuildPlayer and not Addressables' complete build pipeline.

Build preprocess/postprocess interfaces include `IPreprocessBuildWithReport` and `IPostprocessBuildWithReport`; scripted callbacks must not leak editor dependencies into player assemblies. A successful managed compile is not proof a player build succeeds under IL2CPP/AOT, stripping, platform SDKs or content packaging.

## G18.T05 — Unity Test Framework and execution

Typical test attributes/assertion namespaces: `NUnit.Framework.Test`, `TestCase`, `SetUp`, `TearDown`, `Assert`; `UnityEngine.TestTools.UnityTest` for IEnumerator-based tests, `LogAssert`, and Unity-specific setup/teardown support. Exact NUnit version and async feature support are package-dependent (U057).

```csharp
// Original illustrative behavioral test, not executed or compile-tested.
[UnityEngine.TestTools.UnityTest]
public System.Collections.IEnumerator DestroyInvalidatesUnityObject()
{
    var go = new UnityEngine.GameObject("probe");
    UnityEngine.Object.Destroy(go);
    yield return null;
    NUnit.Framework.Assert.That(go == null, NUnit.Framework.Is.True);
    NUnit.Framework.Assert.That(object.ReferenceEquals(go, null), NUnit.Framework.Is.False);
}
```

This tests two different public observations. It does not prove exact destruction phase, editor/player equivalence, or all equality cases. Production tests need cleanup even if an assertion throws and must choose an appropriate Play Mode environment.

Edit Mode tests run in editor context and are good for tooling/pure transforms. Play Mode tests exercise engine loop behavior; player tests can expose platform/AOT differences. Yielding null advances through runner/engine scheduling, not an arbitrary guaranteed number of milliseconds. Use explicit completion conditions/time budgets, controlled clocks where possible, and small real scenes/resources instead of fragile sleeps.

Unexpected error logs can fail tests under runner policy; `LogAssert.Expect` verifies an intentionally observed diagnostic. Do not hide unrelated errors to obtain a green run. `TestTools.Utils` comparers and float/vector assertions may help with numerical tolerance, but exact APIs are not specified. Tests that merely assert mock return values do not establish engine behavior.

## G18.T06 — Compatibility harness and independent review

Keep four independent results: compile/source compatibility, behavioral contract compatibility, serialized-content compatibility, and performance/platform compatibility. A stubbed namespace can satisfy one while failing the others. Preserve known differences as explicit unsupported outcomes rather than returning plausible defaults that mask data loss.

Suggested later harness layers:

1. Pure deterministic tests for vectors, quaternions, masks, rectangles and graph-independent rules.
2. Runtime object/lifecycle tests using public callbacks and state, including Unity-null and enable/disable transitions.
3. Integration tests for physics scenes, resource ownership, HTTP with isolated servers, rendered targets and filesystem persistence.
4. Editor tests for serialized edits, prefab overrides, Undo, import artifacts and scene saves.
5. Package-specific contract suites pinned to exact versions of Input System, Addressables, Collections/Burst/Entities and render pipelines.
6. A small number of player/platform probes for thread affinity, AOT, focus/pause, storage, codecs and graphics conventions.

A comparison harness can run equivalent authored probes under a selected real Unity installation and a subset runtime, collecting observable outputs. That is proposed **future work**, not an activity performed in this training-only stage. Results need version/package/platform/backend provenance, floating tolerances, fixture ownership and an honest distinction between specified and incidental behavior.

Reviewers can assign G01–G18 independently and record corrections at Gnn.Tnn/Uxxx granularity. No reviewed/verified label should be inferred from depth labels. This document does not claim that later research has occurred, that training provenance is certifiable, that all signatures compile, or that a compatible engine/editor/package stack can be derived completely from this reference.
