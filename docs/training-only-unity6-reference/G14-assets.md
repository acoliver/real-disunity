# G14 — Resources, AssetBundles, Addressables

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** Exact Unity 6.3 and package behavior are unverified. All code/signatures are illustrative authored subsets from memory, not full declarations or compile-tested examples. See U001–U003, U043–U045.

**Three distinct systems:** `UnityEngine.Resources` is an engine resource lookup facility; `UnityEngine.AssetBundle` is an engine content-container/loading system; Addressables (`com.unity.addressables`, `UnityEngine.AddressableAssets`, `UnityEngine.ResourceManagement`) is a separately versioned content/operation layer that often builds on bundles. None is the editor AssetDatabase.

## G14.T01 — Resources loading and lifetime

```csharp
static T Load<T>(string path) where T : Object;
static Object Load(string path, System.Type systemTypeInstance);
static T[] LoadAll<T>(string path) where T : Object;
static ResourceRequest LoadAsync<T>(string path) where T : Object;
static void UnloadAsset(Object assetToUnload);
static AsyncOperation UnloadUnusedAssets();
static T[] FindObjectsOfTypeAll<T>() where T : Object;
```

A non-generic Load(path) also exists. Resources paths are logical paths relative to Resources folders, conventionally without file extension and using forward slashes. They are not arbitrary absolute filesystem paths or AssetDatabase paths. Multiple Resources roots contribute to the logical namespace; ambiguous names are a content problem. Build inclusion, type filtering, path case and missing-resource behavior need verification for the selected platform (U043).

`ResourceRequest : AsyncOperation` exposes asset when completed. Request completion and object lifetime are separate. Repeated Load may return an already loaded asset rather than create independent ownership. `FindObjectsOfTypeAll` includes assets/hidden/internal objects beyond ordinary scene discovery; it is not a safe “all gameplay actors” query.

`UnloadAsset` is for eligible non-GameObject/non-component assets in remembered restrictions, not scene-instance destruction. UnloadUnusedAssets traverses engine-relevant references, with subtleties around temporary stack/local references. It is not equivalent to `GC.Collect()`, and CLR reachability alone does not fully specify native asset reachability. A runtime should define supported root tracking rather than let arbitrary managed locals invisibly determine every native lifetime.

## G14.T02 — AssetBundle loading and assets

```csharp
// Selected static AssetBundle loading forms.
static AssetBundle LoadFromFile(string path);
static AssetBundleCreateRequest LoadFromFileAsync(string path);
static AssetBundle LoadFromMemory(byte[] binary);
static AssetBundleCreateRequest LoadFromMemoryAsync(byte[] binary);
// Selected instance forms.
T LoadAsset<T>(string name) where T : Object;
AssetBundleRequest LoadAssetAsync<T>(string name) where T : Object;
T[] LoadAllAssets<T>() where T : Object;
string[] GetAllAssetNames();
string[] GetAllScenePaths();
void Unload(bool unloadAllLoadedObjects);
```

CRC/offset/stream overloads exist; exact integer widths, buffering and stream ownership are U044. `AssetBundleCreateRequest.assetBundle` yields the container; `AssetBundleRequest.asset`/`allAssets` yield loaded objects. `LoadAssetWithSubAssets` families return a main asset plus subassets where supported. A scene bundle's scene path is loaded through SceneManager, not by treating a Scene as an ordinary Object-returning LoadAsset.

`UnityWebRequestAssetBundle.GetAssetBundle(...)` and `DownloadHandlerAssetBundle.GetContent(request)` support network/cache-aware bundle loading. Version/hash/CRC forms are relevant but not enumerated as if interchangeable. A successfully downloaded file can still be an incompatible/corrupt bundle. Compression mode affects memory, random access, recompression and loading cost.

AssetBundles are platform/build-pipeline artifacts. A bundle for one target is not assumed loadable on another, and editor-version compatibility has constraints. This reference does not specify their file format, type trees, shader assets, compression framing, managed script requirements or native resource serialization.

## G14.T03 — AssetBundle dependencies and unloading

`AssetBundleManifest` exposes bundle/dependency/hash queries such as `GetAllAssetBundles`, `GetAllDependencies(string)`, `GetDirectDependencies(string)`, and `GetAssetBundleHash(string)`. The application or higher-level loader must arrange dependency availability; loading one bundle is not a blanket promise that remote dependent bundles are fetched automatically.

`Unload(false)` releases bundle container data while leaving already loaded objects alive in remembered semantics. Loading the bundle again later can produce duplicated asset instances rather than reconnecting to every surviving object. `Unload(true)` also destroys loaded objects from the bundle, potentially invalidating references from scene objects/materials. Releasing a bundle is not the same as destroying all instantiated GameObjects, and clone/shared-resource relationships complicate teardown.

Cache APIs (`Caching`, `Hash128`, cached bundle version concepts) distinguish local cache state from in-memory object state. A cache entry's presence does not mean its assets are loaded. Cleaning disk cache does not safely unload live objects. Exact in-flight unload restrictions and modern async unload forms are U044.

## G14.T04 — Addressables keys and handles

Addressables resolves keys/labels/references to locations and runs provider operations with reference counting. Important types: `Addressables`, `AssetReference`, typed asset-reference helpers, `IResourceLocation`, `AsyncOperationHandle`, `AsyncOperationHandle<T>`, `AsyncOperationStatus`, and `ResourceManager`.

```csharp
// Selected package API shapes; exact overloads depend on package version.
static AsyncOperationHandle<TObject> LoadAssetAsync<TObject>(object key);
static void Release<TObject>(AsyncOperationHandle<TObject> handle);
static void Release<TObject>(TObject asset);
```

A typed handle exposes Result, Status, IsDone, PercentComplete, OperationException, Completed and commonly Task-related access. A handle is a value-type view of an operation; copying it is not automatically acquiring another owned reference. Do not release every copy independently. `IsValid()` is important after release. Successful completion, a non-null result, and ownership are separate concepts.

`LoadAssetsAsync<T>` supports sets of keys/locations and per-asset callbacks. MergeMode concepts include Union, Intersection and UseFirst; exact overloads and failure-release switches are U045. `AssetReference` stores a stable asset identity/addressing relation, not the asset's live memory residency. Some AssetReference convenience methods restrict repeated load usage until release; do not assume all wrappers have identical ownership semantics.

`InitializeAsync`, resource-location queries, dependency download, download-size queries and cache cleanup are additional workflows. Progress by suboperation fraction is not the same as progress by bytes; GetDownloadStatus-style queries exist for byte-aware reporting. Completed callbacks and Task continuations can have package/platform-specific timing and support; Web/platform differences must be tested.

## G14.T05 — Addressables scenes, instances, catalogs

`InstantiateAsync` loads and instantiates through Addressables, returning an operation handle for the instance. Parent/position/rotation/track-handle overload families exist. `ReleaseInstance(GameObject)` and handle-based release forms should match the acquisition path. If an object is instantiated manually from a loaded prefab, retain the prefab/dependency ownership until all dependent clones are finished; arbitrary Destroy(instance) is not a matching Addressables reference release.

Scene loading returns a handle of `UnityEngine.ResourceManagement.ResourceProviders.SceneInstance` through `Addressables.LoadSceneAsync(...)`; unload uses `UnloadSceneAsync(...)`. Scene activation can be deferred. The SceneInstance activation operation, SceneManager ownership, and addressable handle lifetime must be coordinated. An inactive scene can stall work similarly to underlying engine scene activation gates.

Catalogs map addresses/labels/locations to content. `LoadContentCatalogAsync`, `CheckForCatalogUpdates`, `UpdateCatalogs`, remote initialization and content-update builds have separate versioned contracts. Addressables editor groups, profiles, build scripts and schemas are editor tooling. Addresses are not necessarily filenames, GUIDs, URLs, or unique labels; a label can identify multiple assets.

A subset implementation can offer a simple key-to-provider layer, but must not claim support for Unity catalogs or bundles merely because LoadAssetAsync returns a task-like handle. Catalog hashes, internal IDs, dependencies, provider initialization and retry behavior require concrete verification (U045).

## G14.T06 — Ownership, errors, serialization boundaries

Keep an ownership table per acquisition: resource or bundle handle, addressable operation handle, instantiated object, retained dependency, and eventual release operation. Reference counting is meaningful only when acquisition/release semantics are consistent. Over-release can invalidate other consumers; under-release leaks native assets. Operation failure still requires cleanup according to its handle contract.

Downloading content, loading an asset, deserializing references, uploading GPU resources and creating an instance are distinct phases. A successful HTTP status does not establish asset validity. Validate hashes/CRCs where supported, enforce reasonable download limits, and avoid treating remote content identifiers as unrestricted filesystem paths.

Proposed tests (not executed): missing key reports the selected failure contract; two consumers share an asset until both release; copying a handle does not increment ownership; manual prefab clones keep dependencies alive; Unload(false) differs from true; dependency failure cleans up partial loads; deferred scene activation preserves ownership; and canceled UI workflow does not leak an in-flight load. Native bundle/catalog schemas, exact failure callbacks and every version-specific overload remain unspecified.
