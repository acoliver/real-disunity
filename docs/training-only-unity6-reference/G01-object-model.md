# G01 — Object model and components

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** Unity 6 concepts; exact Unity 6.3 compatibility is unknown. All code/signatures here are authored illustrative subsets from memory, not complete declarations or compile-tested examples. See U001–U006 in the [register](uncertainty-register.md).

**Boundary:** engine object/component model, primarily `UnityEngine` and engine module assemblies. Ordinary managed classes are not automatically engine objects. The observable lifetime of a native-backed object differs from that of its C# wrapper.

## G01.T01 — Object identity and Unity-null

Important types: `UnityEngine.Object`, `GameObject`, `Component`, `Behaviour`, `MonoBehaviour`, `ScriptableObject`. `Component` is an `Object` attached to a GameObject; `Behaviour` adds enabled-state concepts; `MonoBehaviour` participates in scripted callbacks. `ScriptableObject` is an engine object but is neither a component nor a scene hierarchy node.

Selected shapes:

```csharp
// Members of UnityEngine.Object; illustrative subset.
string name { get; set; }
HideFlags hideFlags { get; set; }
int GetInstanceID();
static bool operator ==(Object a, Object b);
static bool operator !=(Object a, Object b);
static implicit operator bool(Object value);
```

Unity's equality/boolean behavior can treat a managed wrapper whose native object has been destroyed as null. That wrapper may still be a non-null CLR reference. `ReferenceEquals(value, null)`, `value is null`, and Unity's overloaded `value == null` need not agree. `?.` and `??` test managed null rather than invoking Unity's equality operators; they do not by themselves protect against destroyed wrappers. Generic code with unconstrained `T` or parameters typed as `object` can also bypass Unity-specific behavior.

Instance IDs are useful within a running engine session, not as durable database keys or cross-run asset identifiers. `name` is not identity and is not unique. Preserve wrapper identity and native liveness separately in a compatible runtime. Do not make destruction equivalent to setting every reference to CLR null.

`HideFlags` includes remembered names `None`, `HideInHierarchy`, `HideInInspector`, `DontSaveInEditor`, `NotEditable`, `DontSaveInBuild`, `DontUnloadUnusedAsset`, `DontSave`, and `HideAndDontSave`. Composite values and nuanced interactions with scene/asset ownership require verification. Hide flags are not access control.

## G01.T02 — GameObjects, activity, tags, layers

```csharp
// Selected GameObject members and construction patterns.
GameObject(string name);
GameObject(string name, params System.Type[] components);
Transform transform { get; }
bool activeSelf { get; }
bool activeInHierarchy { get; }
void SetActive(bool value);
int layer { get; set; }
string tag { get; set; }
bool CompareTag(string tag);
static GameObject Find(string name);
static GameObject FindWithTag(string tag);
static GameObject[] FindGameObjectsWithTag(string tag);
```

Every ordinary GameObject has a Transform or a specialized Transform such as RectTransform. `activeSelf` is the object's own active flag. `activeInHierarchy` also depends on ancestors. Enabling a child under an inactive parent does not make it active in the hierarchy. Activity affects scripts, renderers, colliders, and coroutine lifetime in ways specific to each subsystem; it is not object destruction.

A GameObject layer is an index, conventionally 0–31, while a `LayerMask` encodes bits. `LayerMask.GetMask(params string[])`, `LayerMask.NameToLayer(string)`, and `LayerMask.LayerToName(int)` bridge names and indices. Passing `8` as a mask is not the same as selecting layer 8 (`1 << 8`). Invalid/missing names require explicit handling. Tags are configured names; arbitrary unknown tags are not guaranteed to behave like free-form labels. `CompareTag` is the intended tag-comparison operation.

`Find` and tag queries are not stable ownership registries: duplicate names, inactive objects, scene loading, and traversal order complicate results. Historically global GameObject find calls principally locate active objects; exact scope and overload variations remain U004. Do not promise a result order absent a chosen contract.

## G01.T03 — Component discovery and creation

```csharp
// Callable on GameObject; many discovery forms also exist on Component.
T AddComponent<T>() where T : Component;
Component AddComponent(System.Type componentType);
T GetComponent<T>();
Component GetComponent(System.Type type);
bool TryGetComponent<T>(out T component);
T[] GetComponents<T>();
void GetComponents<T>(System.Collections.Generic.List<T> results);
T GetComponentInChildren<T>(bool includeInactive);
T[] GetComponentsInChildren<T>(bool includeInactive);
T GetComponentInParent<T>();
T[] GetComponentsInParent<T>(bool includeInactive);
```

These are representative overload families, not a claim that every generic form has identical constraints. Discovery considers assignable component types; interfaces are used in common generic discovery patterns, but exact accepted constraints should be checked. `GetComponent<T>()` returns one matching component or null, not all matches. Multiple components of one type can exist unless prohibited. Avoid relying on ordering among matches. Child searches include the starting object in familiar behavior, with inactive filtering and parent-search details needing confirmation.

A MonoBehaviour must be created/attached through the engine, not with `new MyBehaviour()`. `AddComponent` may cause lifecycle callbacks immediately when the object is active, so initialize dependencies before activation if order matters. Removing a component uses destruction rather than a list-removal API. Some required engine components cannot be removed normally.

`Component.gameObject`, `Component.transform`, `Component.tag`, `Component.name`, and `CompareTag` are convenient access paths to associated state. They do not make a detached component a meaningful public runtime state.

## G01.T04 — Lifetime, destruction, persistence

```csharp
// Selected static Object APIs.
static void Destroy(Object obj);
static void Destroy(Object obj, float t);
static void DestroyImmediate(Object obj);
static void DontDestroyOnLoad(Object target);
static T FindFirstObjectByType<T>() where T : Object;
static T FindAnyObjectByType<T>() where T : Object;
static T[] FindObjectsByType<T>(FindObjectsSortMode sortMode) where T : Object;
```

`Destroy` schedules removal rather than synchronously disposing native state at the call site. Destruction occurs at an engine boundary, commonly after the current Update work and before rendering; exact micro-order is not specified here. Delayed destruction uses scaled time in remembered behavior. Destroying a GameObject also destroys its components and descendants. Destroying a component does not normally destroy its owning GameObject. Repeated destruction should not create a second lifetime.

`DestroyImmediate` is mainly an editor/tooling escape hatch and is unsafe for many player-loop situations; asset-destruction overloads are deliberately not recommended here. In recalled behavior, `DontDestroyOnLoad` applies to root GameObjects or components on root GameObjects; preserving a root also preserves its Transform descendants. It does not make an arbitrary non-root object independently persistent. This is not persistence to disk. Exact persistence-scene transitions and HideFlags interactions remain U005.

Modern global find families use `FindObjectsInactive.Include`/`Exclude` and `FindObjectsSortMode.None`/`InstanceID` in remembered overloads. Older `FindObjectOfType`/`FindObjectsOfType` families exist historically but may be obsolete. Exact Unity 6.3 signatures and restrictions on finding assets, hidden objects, and inactive objects are not certified.

`ScriptableObject.CreateInstance<T>()` and `CreateInstance(System.Type)` create engine-managed instances. Persisting such an instance as an asset is editor work, not a consequence of calling CreateInstance.

## G01.T05 — Messages, attributes, events

`SendMessage`, `SendMessageUpwards`, and `BroadcastMessage` invoke named receiver methods; overloads accept an optional payload and `SendMessageOptions.RequireReceiver`/`DontRequireReceiver`. These are name-based engine dispatch, not C# events or RPC. Required-receiver failures and receiver signature matching must not be guessed for strict compatibility.

Common attributes include `[RequireComponent(typeof(...))]`, `[DisallowMultipleComponent]`, `[AddComponentMenu(...)]`, `[DefaultExecutionOrder(...)]`, `[ExecuteAlways]`, and historical `[ExecuteInEditMode]`. They affect editor composition or engine scheduling, not ordinary C# inheritance alone. Existing objects are not necessarily repaired retroactively after changing a RequireComponent declaration.

`UnityEngine.Events.UnityEvent` and generic variants expose `AddListener`, `RemoveListener`, `RemoveAllListeners`, and `Invoke` shapes. Serialized persistent listeners differ from runtime listeners; removing runtime listeners should not be confused with deleting serialized callbacks. Delegate identity matters for removal. C# events, UnityEvent, engine message methods, and Input System action callbacks are separate mechanisms.

## G01.T06 — Managed/native ownership contract

For a subset implementation, explicitly model:

- Managed reference existence, engine object liveness, pending destruction, and destroyed state.
- GameObject/component ownership and hierarchy ownership without equating them.
- Activity and component enablement. `Behaviour.enabled` and `Behaviour.isActiveAndEnabled` are distinct observations.
- Session-scoped instance identity; serialized identity belongs to another layer.
- Main-thread restrictions for most engine object operations. A reference reachable on a worker thread is not permission to use the native object there.

Suggested public-boundary tests (not run): a destroyed wrapper compares Unity-null while remaining a CLR reference; deactivating a parent changes descendants' `activeInHierarchy` but not their `activeSelf`; component queries distinguish one/all/absent matches; destroying a parent invalidates its hierarchy; pending destruction does not prematurely erase callbacks already executing. Exact callback counts/order are part of G02 and U006 rather than assumed here.

**Deliberate omissions:** exhaustive reflection and message dispatch rules, native bindings, object marshaling, managed stripping, domain reload internals, and complete historical find overloads.
