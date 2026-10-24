# G05 — Scenes, instantiation, prefabs, and serialization

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** No exact Unity 6.3 guarantee. Signatures/code are authored illustrative subsets from memory, not complete declarations or compile-tested examples. See U001–U003, U016–U018.

**Boundary:** engine `UnityEngine.SceneManagement`, object cloning, and Unity field serialization. Prefab authoring/overrides and asset persistence are editor concerns; serialized Unity file compatibility is a separate, unspecified target.

## G05.T01 — Scene handles and discovery

`Scene` is a value-type handle, not a GameObject and not a direct asset object. Familiar members include `name`, `path`, `buildIndex`, `handle`, `isLoaded`, `rootCount`, `IsValid()`, `GetRootGameObjects()`, and a list-filling root query overload. A valid scene handle need not mean the scene is fully loaded. Unloaded or invalid handles require validation.

```csharp
// SceneManager subset.
static int sceneCount { get; }
static int sceneCountInBuildSettings { get; }
static Scene GetActiveScene();
static Scene GetSceneAt(int index);
static Scene GetSceneByName(string name);
static Scene GetSceneByPath(string path);
static Scene GetSceneByBuildIndex(int buildIndex);
static bool SetActiveScene(Scene scene);
static Scene CreateScene(string sceneName);
static void MoveGameObjectToScene(GameObject go, Scene scene);
```

`GameObject.scene` identifies its scene. Active scene is a default destination/context for object creation and lighting-related state, not “the only scene that updates” or “the scene of every visible object.” Additively loaded scenes can all have active content. Moving a root moves its hierarchy in familiar behavior; root restrictions and persistence transitions need U016 verification. Duplicate scene names make name-only resolution ambiguous.

`SceneUtility.GetScenePathByBuildIndex(int)` and `GetBuildIndexByScenePath(string)` relate build entries to paths. Build index is not a universal stable identifier across differently configured players.

## G05.T02 — Loading, activation, unloading

```csharp
static void LoadScene(string sceneName, LoadSceneMode mode);
static void LoadScene(int sceneBuildIndex, LoadSceneMode mode);
static AsyncOperation LoadSceneAsync(string sceneName, LoadSceneMode mode);
static AsyncOperation LoadSceneAsync(int sceneBuildIndex, LoadSceneMode mode);
static AsyncOperation UnloadSceneAsync(Scene scene);
static AsyncOperation UnloadSceneAsync(string sceneName);
```

Overloads using `LoadSceneParameters`, `CreateSceneParameters`, and unload flags exist. `LoadSceneMode.Single` replaces ordinary loaded scenes; `Additive` keeps existing ones. `LocalPhysicsMode.None`, `Physics2D`, and `Physics3D` request scene-local physics worlds in remembered parameter APIs. Exact combination rules are U016.

Async scene operations distinguish loading from activation. With `allowSceneActivation = false`, progress commonly plateaus around 0.9 and `isDone` remains false; activation release completes the operation. This can block later queued scene operations. A generic AsyncOperation implementation cannot assume that all progress ranges represent bytes transferred. Synchronous load calls also have engine timing implications; they are not equivalent to parsing a file synchronously in user code.

`SceneManager.sceneLoaded` uses `(Scene, LoadSceneMode)`, `sceneUnloaded` uses `(Scene)`, and `activeSceneChanged` uses `(Scene previous, Scene next)` delegates. Recalled sceneLoaded notification occurs after scene object enable initialization and before Start for typical scene loads. Runtime instantiation and explicit activation gates complicate ordering; U016 remains open. Unloading a scene destroys owned scene objects but is not a promise to free every referenced asset. `DontDestroyOnLoad` objects survive ordinary scene replacement.

## G05.T03 — Instantiation and cloning

```csharp
// Selected Object.Instantiate families.
static T Instantiate<T>(T original) where T : Object;
static T Instantiate<T>(T original, Transform parent) where T : Object;
static T Instantiate<T>(T original, Transform parent, bool worldPositionStays) where T : Object;
static T Instantiate<T>(T original, Vector3 position, Quaternion rotation) where T : Object;
static T Instantiate<T>(T original, Vector3 position, Quaternion rotation, Transform parent) where T : Object;
```

Non-generic Object-returning overloads exist. Cloning a GameObject clones its hierarchy and components; cloning a Component yields the corresponding component on a cloned owning hierarchy. Position/rotation overloads use world placement in familiar behavior. In recalled behavior, the parent-only Instantiate overload uses local placement relative to the supplied parent (equivalent to the boolean overload with false), unlike Transform.SetParent(parent), whose default preserves world placement. Explicit position/rotation overloads still specify world placement even when supplying a parent. Inactive clones preserve relevant activity state; active clones can execute Awake/OnEnable during instantiation before the caller assigns more fields. Exact overload declarations/defaults remain U017.

References within a cloned object graph are remapped to corresponding cloned objects; references to external assets/objects ordinarily remain shared. Instantiate is not a general deep copy of arbitrary managed data, nor a promise to duplicate a material/texture dependency. Cloning ScriptableObjects differs from cloning scene hierarchies.

Unity 6-era `InstantiateAsync`/`AsyncInstantiateOperation` are remembered as relevant additions, but exact generic forms, batching, activation, cancellation, and callback timing are deliberately omitted (U017). Do not invent a Task-returning wrapper as a source-compatible replacement.

## G05.T04 — Prefab runtime/editor boundary

A prefab is an authored asset representing a reusable object graph, with editor instance links, nested prefabs, variants, and property overrides. In a player, ordinary usage is loading a GameObject reference and calling Instantiate. The player does not need an AssetDatabase to instantiate an already built prefab reference.

`UnityEditor.PrefabUtility` manages editing, saving, applying/reverting overrides, prefab instance relationships, and prefab-content editing. These are not `UnityEngine` runtime APIs. A compatible subset may support runtime graph cloning without supporting prefab editor workflows, but must state that limitation. A class named Prefab or a custom JSON object graph does not establish Unity prefab file compatibility.

Serialized reference identity, hierarchy/component IDs, editor metadata, variant inheritance, and imported dependencies require a separate asset model. Exact YAML or binary encodings are not described and should not be reconstructed from guesses in this reference.

## G05.T05 — Serialized fields and references

Unity field serialization is not ordinary .NET serialization. Recalled eligible fields are public or explicitly opted in with `[SerializeField]` or `[SerializeReference]`, instance, non-const/non-readonly, and of supported types; `[NonSerialized]` excludes a field. SerializeReference has its own managed-reference type restrictions described below; it is not an unrestricted substitute for SerializeField. Properties are not generally serialized as properties. Attributes targeted at generated backing fields, such as `[field: SerializeField]`, work through fields rather than property semantics.

Common supported families: primitives, strings, enums, familiar engine value types, references to `UnityEngine.Object`, supported arrays and `List<T>`, and `[System.Serializable]` custom structs/classes. Dictionaries, multidimensional/jagged/nested containers, arbitrary generics, and recursive graphs are not a general promise of field serialization; wrappers/custom callbacks are commonly needed. Exact modern support boundaries are U018.

Inline serialized custom classes normally have value-like embedding semantics rather than preserving arbitrary CLR graph identity. `[SerializeReference]` opts eligible managed fields into managed-reference serialization, enabling polymorphic references and shared/cyclic graphs within the owning host's scope. It is not for replacing UnityEngine.Object references. Type identity, nulls, missing types, and managed reference IDs require careful treatment; do not imply cross-host object sharing.

Related attributes: `[HideInInspector]`, `[Header]`, `[Tooltip]`, `[Range]`, `[Min]`, `[TextArea]`, `[Multiline]`, and `UnityEngine.Serialization.FormerlySerializedAsAttribute`. Inspector presentation is not the same as runtime validation. FormerlySerializedAs is a serialized-field migration aid, not a C# source rename alias.

## G05.T06 — Serialization callbacks and JSON

```csharp
public interface ISerializationCallbackReceiver
{
    void OnBeforeSerialize();
    void OnAfterDeserialize();
}
// JsonUtility subset.
static string ToJson(object obj);
static string ToJson(object obj, bool prettyPrint);
static T FromJson<T>(string json);
static void FromJsonOverwrite(string json, object objectToOverwrite);
```

Callbacks permit transforming unsupported structures into serializable fields. They can run in contexts not safe for most engine calls; keep transformations local and deterministic. Ordering across managed references is not a general dependency scheduler. Constructors/field initializers and domain reload have subtle interactions with deserialization.

JsonUtility follows Unity serialization conventions, not a general JSON object/array/dictionary mapper. Wrap top-level primitive or array data in a serializable object rather than assuming arbitrary JSON roots work. Existing MonoBehaviour/ScriptableObject instances generally require overwrite rather than constructing them with FromJson. JSON references to engine objects are not a portable asset packaging format. Missing/extra fields and default initialization behavior require U018 verification before depending on exact outcomes.

Proposed tests (not run): additive scenes have independent roots; active-scene changes do not disable other scenes; a clone's internal sibling references remap while asset references remain shared; inactive clones defer activation callbacks; serialization excludes a property but includes its eligible field; managed-reference graph identity survives a supported round trip. Native asset, scene-file, and prefab-file interoperability remain unknown.
