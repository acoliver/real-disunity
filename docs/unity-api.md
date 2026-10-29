# Supported UnityEngine API

`Disunity.Engine` implements a subset of the `UnityEngine` API. This page lists what exists today, grouped the way a game uses it. If a type or member is not listed here, it does not exist yet, and code that uses it will not compile. Where a member exists but a particular setting is unsupported, it throws an exception naming the setting.

The source of truth is `csharp/Disunity`. Each type lives in a file of the same name, so `csharp/Disunity/Physics2D.cs` holds `Rigidbody2D` and the 2D colliders. Writing scripts and hosting them is covered in [Writing C# scripts](csharp-games.md).

## Objects, components and lifecycle

These cover creating, finding and destroying things, which nearly every script uses.

| Type | Supported |
| --- | --- |
| `Object` | `Instantiate` (original; with parent; with position and rotation; with position, rotation and parent), `Destroy(obj)` for game objects and `ScriptableObject`s, `Destroy(obj, delay)` for game objects, `FindObjectOfType`, `FindFirstObjectByType`, `FindObjectsByType`, Unity-style null and equality checks on destroyed objects |
| `GameObject` | `name`, `tag`, `layer`, `CompareTag`, `activeSelf`, `activeInHierarchy`, `SetActive`, `GetComponent<T>`, `AddComponent<T>`, `transform`, `FindGameObjectWithTag` |
| `Component` | `gameObject`, `transform`, `tag`, `CompareTag`, `GetComponent<T>` |
| `Behaviour` | `enabled`, `isActiveAndEnabled` |
| `MonoBehaviour` | Messages `Awake`, `OnEnable`, `Start` (also as a coroutine), `FixedUpdate`, `Update`, `LateUpdate`, `OnDisable`, `OnDestroy`, `OnCollisionEnter2D`/`Stay2D`/`Exit2D`, `OnTriggerEnter2D`/`Stay2D`/`Exit2D`; `StartCoroutine`, `StopCoroutine`, `StopAllCoroutines` |
| `Transform` | `position`, `rotation`, `localPosition`, `localRotation`, `localScale`, `parent`, `SetParent(parent, worldPositionStays)` |
| `ScriptableObject` | `name`, `CreateInstance<T>`, `CreateInstance(Type)` and `CreateInstance(string)`, with `Awake`, `OnEnable`, `OnDisable` and `OnDestroy` |
| Attributes | `SerializeField`, `Header`, `Range`, `CreateAssetMenu` |

Instantiated objects have their internal references remapped to the clone, as in Unity, and their `Awake` and `OnEnable` run before `Instantiate` returns. The clone's root is named after the original with a `(Clone)` suffix. Prefab assets are templates that take part in nothing until instantiated. `Destroy` follows Unity's timing: the object stops receiving update messages at once, but searches still find it and it compares non-null until the end of the frame, when `OnDisable` and `OnDestroy` run. Destroying a single component throws `NotSupportedException`.

## Loading scenes

A host loads a Unity scene from the plan `unity-import plan` writes, with `DisunityHost.LoadScene(planJson, new SceneLoadOptions(gameAssembly, fontBytes))`. The game's scripts are compiled into `gameAssembly` unchanged; `csharp/OriginalGames` shows a project that does this for a game checkout. `fontBytes` returns the TrueType bytes for a package font by name, such as Unity's `Arial`. The loader builds every object, engine component and script, sets serialized fields, registers the project's input axes, and then runs `Awake` and `OnEnable` for the scene together, as a Unity scene load does. Prefabs the scene references are kept as templates for `Instantiate` until the returned `LoadedScene` is disposed. If anything fails, the world is left as it was and `SceneLoadException` names the object, component or field.

Serialized fields bind as Unity deserializes them. Public and `[SerializeField]` fields are set; static, readonly and `[NonSerialized]` fields are not. A field the scene does not mention keeps its initializer, and scene data for a field the script no longer declares is ignored. Supported field types are numbers, `bool`, enums, `string`, `Vector2`, `Vector3`, `Vector2Int`, `Quaternion`, `Color`, `Rect`, `LayerMask`, references to objects, components, sprites, fonts and `ScriptableObject` assets, `[Serializable]` classes, arrays, `List<T>` and `UnityEvent<T>`. Any other type fails the load. A `[Serializable]` class with no parameterless constructor, public or not, is created without running any constructor, so its state is exactly its serialized data, and a field the data omits holds the type's default; a class with a parameterless constructor runs it and keeps its field initializers. `Object.Instantiate` copies serializable fields the same way. When a field cannot be read, the load error names the field and the innermost exception's type and message.

`LoadedScene.UnregisteredAxes` lists input axes that need mouse, joystick or keys the engine does not deliver; reading one fails as an unregistered axis does. `LoadedScene.Differences` lists authored presentation settings the engine does not reproduce, such as extrapolated bodies and lit tilemaps. `LoadedScene.Textures` gives the identity each scene texture's sprites use, for the host to upload.

## Time, input and coroutines

| Type | Supported |
| --- | --- |
| `Time` | `deltaTime`, `fixedDeltaTime`, `time`, `frameCount` |
| `Input` | `GetKey`, `GetKeyDown`, `GetKeyUp`; `GetAxis` (smoothed) and `GetAxisRaw` for any axis the host registered |
| `KeyCode` | `Space`, `A` to `Z`, the four arrow keys, `Return`, `Escape` |
| `UnityEngine.InputSystem.Keyboard` | `Keyboard.current.aKey` and `dKey` with `isPressed` |
| Yield instructions | `WaitForSeconds`, `WaitForSecondsRealtime`, `WaitForFixedUpdate`, `WaitForEndOfFrame`, `null` (next frame), nested `IEnumerator` |

No axes exist until the host registers them with `DisunityHost.RegisterInputAxis`, giving each a name, negative and positive keys, alternate keys, gravity, sensitivity, dead zone, snap and invert, as in Unity's Input Manager. Asking for an unregistered axis throws `ArgumentException`. Mouse input, gamepads and Input System actions are not implemented. Coroutines cannot be joined by yielding a `Coroutine`. A coroutine that yields `WaitForEndOfFrame` resumes once, after every `LateUpdate` of the frame; one that yields it while resuming from that point waits for the end of the next frame.

## Player settings and scenes

| Type | Supported |
| --- | --- |
| `QualitySettings` | `antiAliasing`, `globalTextureMipmapLimit`, `maximumLODLevel`, `maxQueuedFrames`, `vSyncCount`, `lodBias`, `shadowDistance`, `shadows`, `activeColorSpace`, `names`, `GetQualityLevel` and `SetQualityLevel`. The state is the engine's own and resets when a world loads: one level named "Default", anti-aliasing 0, vertical sync 1, LOD bias 1, shadow distance 150, gamma. The engine does not know a project's per-level values yet, so switching levels changes only the current index. `SetQualityLevel` with an index at or above the level count selects the last level (the original Old World player calls it with 3 and defines three levels, and runs on); a negative index throws. Hosts supply levels and color space with `DisunityHost.SetQualityLevels` and `SetColorSpace`. |
| `SceneManager` | `GetActiveScene`, `SetActiveScene`, `sceneCount`, `GetSceneAt`, and the `sceneLoaded` and `sceneUnloaded` events. `sceneLoaded` is raised once the scene's objects have received `Awake` and `OnEnable`, before any `Start`. `LoadScene` and `LoadSceneAsync` throw `NotSupportedException`: the engine holds one scene and cannot load another scene's plan at run time yet. |
| `Cursor` | `visible`, `lockState` and `SetCursor` with a null texture. A custom cursor texture throws `NotSupportedException`. The play window does not apply the state yet; a presentation host reads `DisunityHost.CursorVisible` and `CursorLockState`. |
| `ColorUtility` | `TryParseHtmlString` (`#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA` and 23 color names), `ToHtmlStringRGB`, `ToHtmlStringRGBA` |

## 2D physics

| Type | Supported |
| --- | --- |
| `Rigidbody2D` | `bodyType` (Dynamic, Kinematic, Static), `velocity`, `linearVelocity`, `angularVelocity`, `mass`, `gravityScale`, `freezeRotation`, `constraints` (`RigidbodyConstraints2D` freeze position X, Y and rotation), `simulated`, `position`, `MovePosition`, `Sleep`, `WakeUp`, `IsSleeping`, `sharedMaterial`, `interpolation` (None and Interpolate; Extrapolate throws `NotSupportedException`) |
| `Collider2D` | `enabled`, `isTrigger`, `offset`, `attachedRigidbody`, `sharedMaterial` |
| `BoxCollider2D`, `CircleCollider2D` | `size`; `radius` |
| `UnityEngine.Tilemaps.TilemapCollider2D` | Collides with the `Tilemap` on its object, following its cells as they change. Contacts and queries return it like any other collider. |
| `UnityEngine.Tilemaps.Tilemap` | `enabled`, `color`, `tileAnchor`. The host sets the grid layout, tile palette and cells with `DisunityHost.ConfigureTilemap`, `SetTile` and `SetCells`; the script `Tile` and `Grid` APIs are not implemented. |
| `PhysicsMaterial2D` | `name`, `friction`, `bounciness` |
| `Collision2D` | `collider`, `otherCollider`, `gameObject`, `transform`, `rigidbody`, `contacts`, `contactCount`, `GetContact` |
| `Physics2D` | `Raycast` (nearest hit), `OverlapCircle` (first hit) |
| `RaycastHit2D` | `collider`, `point`, `normal`, `distance`, conversion to `bool` |

An interpolated body is drawn between its poses before and after the last physics step, by the fraction of a step the clock has run past it, so it moves smoothly when frames and physics steps do not line up. As in Unity it is drawn up to one step behind physics. Unlike Unity, its `transform` keeps the physics pose: interpolation changes what is drawn, and scripts, contacts and queries all see the step pose. A body moved by a script since the last step is drawn where it is.

Physics uses continuous collision detection for all bodies. Joints, effectors, polygon and edge colliders in C#, forces and impulses (`AddForce`), and 3D physics are not implemented.

## Sprites, cameras and lights

| Type | Supported |
| --- | --- |
| `SpriteRenderer` | `sprite`, `color`, `enabled`, `flipX`, `flipY`, `sortingLayerID`, `sortingLayerName` (only `Default`), `sortingOrder`, `bounds`, `sharedMaterial` |
| `Sprite` | An `Object`, so it has `name` (the plan's name for a scene or player sprite, otherwise empty), an instance ID, and works with `Resources.Load<Sprite>` and `Resources.LoadAll<Sprite>`. Members: `rect`, `pivot` (pixels, from the fractional pivot a sprite is made with), `pixelsPerUnit`, `bounds`, `texture`, `textureRect`, `textureRectOffset`, `border` (left, bottom, right, top) and `packed`. `Sprite.Create(Texture2D, Rect, Vector2)` and its overloads with `pixelsPerUnit` (default 100), `extrude`, `SpriteMeshType`, `border` and the fallback physics flag make a sprite over a rectangle of a texture: a null texture throws `ArgumentNullException`, and a rect that is empty or outside the texture, a pivot that is not finite, or pixels per unit that are not finite and positive throw `ArgumentException`. Extrusion, the mesh type and the physics shape are accepted and not simulated: a sprite is drawn as its rectangle. A sprite of the scene is created by the host with `DisunityHost.CreateSprite` and has no managed texture, so `texture` is null, `textureRect` is `rect` and `border` is zero. A sprite from a player's resources or `Sprite.Create` keeps its `Texture2D`. The renderer holds no texels for those textures yet, so such a sprite is left out of the frame and its texture is named once on stderr. |
| `UnityEngine.U2D.SpriteAtlas` | An `Object` with `spriteCount`, `GetSprites(Sprite[])`, `GetSprite(name)`, `CanBindTo(sprite)`, `tag` and `isVariant`. `GetSprites` fills the array from its start with copies, so renaming a returned sprite leaves the atlas's own sprites alone; `GetSprite` returns the atlas's own sprite. A player's atlases load from its `Resources` with their sprites in the planned order. |
| `Texture2D` from a player's resources | `Resources.Load<Texture2D>` returns a texture with the planned size, format, mip count, `isReadable`, `filterMode`, wrap modes, `anisoLevel` and `mipMapBias`, holding the planned bytes (`GetRawTextureData`). The plan's byte count must equal what the format, size and mip levels need, or the load fails naming the texture and both sizes. Only 2D textures of one image load. The pixels stay in managed memory; nothing uploads them to a GPU. A texture takes a renderer identity from 2^40 up the first time a sprite is made over it, so it cannot meet the identities of scene textures, which count up from the scene load's first texture identity. |
| `Material` | `Material.SpriteUnlit` and `Material.SpriteLit` |
| `Camera` | `enabled`, `clearFlags` (Skybox and Solid Color; Depth and Nothing are rejected when drawn), `backgroundColor`, `orthographic`, `orthographicSize`, `fieldOfView`, `nearClipPlane`, `farClipPlane`, `depth` (whole numbers), `cullingMask`, `rect`, `targetDisplay`, `allowHDR`, `allowMSAA`, `allowDynamicResolution`, `useOcclusionCulling`. A frame presents only orthographic full-screen cameras; a perspective lens or a partial `rect` is stored and fails when a frame is prepared. A player plan builds each of these settings from the serialized camera. |
| `AudioListener`, `GUILayer`, `FlareLayer` | Exist as enable-able components. There is no audio output. |
| `UnityEngine.Rendering.Universal.Light2D` | Global lights: `color`, `intensity`, `affectedSortingLayers`, blend style 0. Other light types throw `UnsupportedLight2DException`. |
| `Light` | A native component, not drawn yet: the engine renders no 3D lights, so nothing is lit by one. It is a `Behaviour` and not a `MonoBehaviour`, so `GetComponents<MonoBehaviour>()` and `FindObjectsByType<MonoBehaviour>` do not return it. `type`, `color`, `intensity`, `range`, `spotAngle` (clamped to 1 through 179), `innerSpotAngle` (clamped to 0 through `spotAngle`; a smaller `spotAngle` lowers it), `shadows`, `shadowStrength` (clamped to 0 through 1), `shadowBias`, `shadowNormalBias`, `shadowNearPlane`, `cullingMask`, `renderingLayerMask`, `bounceIntensity`, `colorTemperature`, `useColorTemperature`, `cookieSize`, `cookie`, `lightmapBakeType` and `enabled` are stored in the native light. A write the native side rejects throws `DisunityException` and changes nothing: a negative or non-finite `intensity`, `range`, `cookieSize` or `bounceIntensity`, a `colorTemperature` that is not above zero, a non-finite `color` or shadow bias, and a NaN angle or `shadowStrength`. A disabled light keeps its settings. `AddCommandBuffer` and `RemoveCommandBuffer` throw `NotSupportedException`. `flare` and `renderMode` are not defined. `Instantiate` gives the clone its own native light with the same settings. A player plan builds each light from the serialized one. |
| `UnityEngine.Rendering.Universal.UniversalAdditionalCameraData` | Base camera without post-processing, antialiasing or shadows. Enabling an unsupported option throws `UnsupportedCameraSettingException`. |

## Graphics buffers

| Type | Supported |
| --- | --- |
| `GraphicsBuffer` | `GraphicsBuffer(Target.Raw, count, stride)`, `count`, `stride`, `bufferHandle` (unique for each live buffer, never 0), `SetData(array, managedStart, bufferStart, count)`, `Release`, `Dispose`. The engine has no GPU buffers yet, so it keeps the contents in managed memory and does not draw from them. `SetData` takes arrays of unmanaged elements whose size is a multiple of the stride, and the buffer index counts elements of the array's own type. Other targets throw `ArgumentException`. |
| `UnityEngine.Rendering.BatchRendererGroup` | `BatchRendererGroup(callback, userContext)`, `RegisterMesh`, `UnregisterMesh`, `GetRegisteredMesh`, `RegisterMaterial`, `UnregisterMaterial`, `GetRegisteredMaterial`, `AddBatch` (with and without a buffer window), `RemoveBatch`, `GetConstantBufferMaxWindowSize` (65536), `Dispose`. The engine keeps the registry for the game and does not draw batches yet, so it never calls the culling callback. IDs start at 1 for each group and 0 means none. Registering the same object again returns its ID and counts the registration, and each `Unregister` takes one off. `GetRegistered...` returns `null` for an unknown ID. `AddBatch` copies the metadata and needs the handle of a live `GraphicsBuffer`. |
| `BatchMeshID`, `BatchMaterialID`, `BatchID` | Opaque values with equality, ordering, hashing and a `Null` value. |
| `MetadataValue`, `BatchCullingContext`, `BatchCullingOutput`, `BatchCullingOutputDrawCommands`, `BatchDrawCommand`, `BatchDrawRange`, `BatchFilterSettings` | Structs with the fields the game's culling callback reads and writes. `BatchFilterSettings` defaults to no shadow casting and no shadow receiving. |

## UI

| Type | Supported |
| --- | --- |
| `Canvas` | `renderMode` (screen space overlay, screen space camera, world space), `enabled`, `sortingOrder`, `scaleFactor` (the factor for the current screen), `planeDistance`, `worldCamera` (read; the host sets it with `DisunityHost.RenderThroughCamera`), `pixelPerfect` (on a screen-space canvas, each text moves so its first glyph starts on a whole screen pixel, as Unity's `Text` does; world-space canvases ignore it) |
| `RectTransform` | `anchorMin`, `anchorMax`, `pivot`, `sizeDelta`, `anchoredPosition`, `rect` |
| `UnityEngine.UI.Text` | `text`, `fontSize`, `font`, `color`, `alignment`, `lineSpacing`, `horizontalOverflow`, `verticalOverflow`, `supportRichText`; `fontStyle` Normal only; `resizeTextForBestFit` off only. Fonts are TrueType files registered by the host with `DisunityHost.RegisterFont`; there is no built-in font. |

`UnityEngine.TextCore` and `UnityEngine.TextCore.LowLevel` hold the data types a font asset stores: `FaceInfo`, `Glyph`, `GlyphRect`, `GlyphMetrics`, `GlyphValueRecord` (addable and comparable), `GlyphAdjustmentRecord`, `GlyphPairAdjustmentRecord`, `GlyphAnchorPoint`, `MarkPositionAdjustment`, `MarkToBaseAdjustmentRecord`, `MarkToMarkAdjustmentRecord` and `LigatureSubstitutionRecord`. Their serialized fields use the names a player stores them under, so a font asset's data loads into them. There is no `FontEngine`.

Text covers printable ASCII with line breaks and kerning. Font size is pixels per em. Wrap breaks at word boundaries, splitting words wider than the line, and Truncate drops lines that don't fit; glyphs are never clipped. Rich text is on by default, as in Unity, and markup is rejected while it is on because it isn't rendered. A `CanvasScaler`'s scale-with-screen-size settings are applied by the host with `DisunityHost.ScaleWithScreenSize`; the component itself, `Image`, `Button`, layout groups and the EventSystem are not implemented.

## Assets, web requests, JSON and command buffers

These types exist so that game code which references them loads. Where the engine does not do the work yet, the member throws `NotSupportedException` naming itself.

| Type | Supported |
| --- | --- |
| `AssetBundle` | The type exists. `LoadFromFile` and every instance member (`LoadAsset`, `LoadAssetWithSubAssets`, `LoadAllAssets`, `GetAllAssetNames`, `Unload`) throw `NotSupportedException`: asset bundles are not loaded yet. |
| `UnityEngine.Networking.UnityWebRequest` | The request object model: `UnityWebRequest(url, method)`, `Get`, `EscapeURL` (form encoding, a space becomes `+`), `uri`, `method`, `SetRequestHeader`, `downloadHandler`, `uploadHandler`, `Dispose`, and before any send `isDone` (false), `responseCode` (0), `result` (`InProgress`) and `GetResponseHeaders()` (null). A URL must be absolute and have a scheme, or the constructor throws `ArgumentException`. `Abort` on a request never sent does nothing. `SendWebRequest` throws `NotSupportedException`: the engine makes no web requests yet. |
| `DownloadHandler`, `DownloadHandlerBuffer`, `DownloadHandlerTexture`, `UnityWebRequestTexture` | `data`, `text` and `texture` are null because no content arrives. `DownloadHandler` has a `protected virtual CompleteContent()` that does nothing. `UnityWebRequestTexture.GetTexture` returns a GET request with a texture handler. |
| `UploadHandler`, `UploadHandlerRaw` | `UploadHandlerRaw(bytes)` keeps a copy of the bytes in `data`; `contentType` starts null and can be set. |
| `JsonUtility` | `ToJson(object)` and `FromJson<T>(string)`. Both use the fields a scene load binds: public and `[SerializeField]` instance fields, not `readonly` or `[NonSerialized]`. The JSON is compact, base class fields first. Numbers, `bool`, `string` (null as `""`), enums (as integers), arrays and `List<T>` (null as `[]`), nested classes and structs, `Vector2`, `Vector3`, `Vector4`, `Quaternion` and `Color` are supported. A null class field is written as an object of its default values. A field that refers to a `UnityEngine.Object` throws `NotSupportedException`, as does a NaN or infinite float. `FromJson` ignores unknown names, keeps the constructor value of a missing field, returns `default(T)` for empty text, and throws `ArgumentException` for malformed JSON (with line and position), a value of the wrong kind, or an abstract or `Object` type. |
| `UnityEngine.Rendering.CommandBuffer` | `name` (starts empty; null becomes empty), `Clear`, `Release` and `Dispose`. After `Release` every member except `name` and `Release` throws `ObjectDisposedException`. The recording members (`BeginSample`, `EndSample`, `Blit`, `ClearRenderTarget`, `CopyTexture`, `EnableShaderKeyword`, `DisableShaderKeyword`, `DrawMesh`, `DrawMeshInstancedIndirect`, `GetTemporaryRT`, `ReleaseTemporaryRT`, `SetGlobalColor`, `SetGlobalFloat`, `SetGlobalMatrix`, `SetGlobalTexture`, `SetGlobalVector`, `SetRenderTarget`, `SetViewport`, `SetViewProjectionMatrices`) throw `NotSupportedException`: the engine does not record command buffers yet. `DrawRenderer` is missing because the engine has no `Renderer` type. |
| `ComputeBuffer` | The constructors `(count, stride)`, `(count, stride, type)` and `(count, stride, type, mode)` (a count or stride of zero or less throws `ArgumentException`), `count`, `Release` and `Dispose` (the same operation, repeatable). `GetData` and `SetData` throw `NotSupportedException` (`ObjectDisposedException` after release): the engine has no GPU memory. |

## Values and utilities

| Type | Supported |
| --- | --- |
| `Vector2` | Constructor, `x`, `y`, `zero`, `one`, `up`, `right`, `magnitude`, `sqrMagnitude`, `normalized`, `+`, `-`, `*`, `/` |
| `Vector3` | Constructor, `x`, `y`, `z`, `zero`, `one`, multiplication by a scalar, conversion to and from `Vector2` |
| `Vector2Int` | Constructor, `x`, `y`, `zero`, `up`, `down`, `left`, `right`, `+`, division by an integer |
| `Quaternion` | Constructor and `identity` |
| `Color`, `Rect`, `Bounds`, `LayerMask` | Value types with their fields; `Color.white`, `Color.clear` |
| Formatting | `ToString` on `Vector2`, `Vector3`, `Vector2Int`, `Quaternion` and `Color` prints as current Unity does, for example `(1.00, 2.00)` and `RGBA(1.000, 0.000, 0.000, 1.000)` |
| `Mathf` | `Min`, `Clamp` |
| `Random` | `value`, `Range(int, int)`, `Range(float, float)`, `InitState`. A seed always repeats its sequence, but the values differ from Unity's for the same seed. |
| `Debug` | `Log`, `LogError`, written to the console |
| `UnityEngine.Events.UnityEvent<T>` | `AddListener`, `RemoveListener`, `RemoveAllListeners`, `Invoke`, `GetPersistentEventCount`, `GetPersistentMethodName`, `GetPersistentTarget`. Calls set in the editor load with the scene and run before runtime listeners, in authored order, with the event's argument or their fixed one; `RemoveAllListeners` leaves them, as in Unity. Clones call their own copies of the targets. |

The math types are deliberately small and grow as games need them. Adding a missing helper such as `Vector3` addition is a welcome contribution; see the [developer guide](../dev-docs/README.md).
