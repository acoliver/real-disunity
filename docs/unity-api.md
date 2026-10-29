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

Serialized fields bind as Unity deserializes them. Public and `[SerializeField]` fields are set; static, readonly and `[NonSerialized]` fields are not. A field the scene does not mention keeps its initializer, and scene data for a field the script no longer declares is ignored. Supported field types are numbers, `bool`, enums, `string`, `Vector2`, `Vector3`, `Vector2Int`, `Quaternion`, `Color`, `Rect`, `LayerMask`, references to objects, components, sprites, fonts and `ScriptableObject` assets, `[Serializable]` classes, arrays, `List<T>` and `UnityEvent<T>`. Any other type fails the load.

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
| `QualitySettings` | `antiAliasing`, `globalTextureMipmapLimit`, `maximumLODLevel`, `maxQueuedFrames`, `vSyncCount`, `lodBias`, `shadowDistance`, `shadows`, `activeColorSpace`, `names`, `GetQualityLevel` and `SetQualityLevel`. The state is the engine's own and resets when a world loads: one level named "Default", anti-aliasing 0, vertical sync 1, LOD bias 1, shadow distance 150, gamma. The engine does not know a project's per-level values yet, so switching levels changes only the current index. Hosts supply levels and color space with `DisunityHost.SetQualityLevels` and `SetColorSpace`. |
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
| `Sprite` | `rect`, `pivot`, `pixelsPerUnit`, `bounds`. Sprites are created by the host with `DisunityHost.CreateSprite`. |
| `Material` | `Material.SpriteUnlit` and `Material.SpriteLit` |
| `Camera` | `enabled`, `clearFlags` (Skybox and Solid Color; Depth and Nothing are rejected when drawn), `backgroundColor`, `orthographic` (perspective is rejected), `orthographicSize`, `nearClipPlane`, `farClipPlane`, `depth` (whole numbers) |
| `AudioListener`, `GUILayer`, `FlareLayer` | Exist as enable-able components. There is no audio output. |
| `UnityEngine.Rendering.Universal.Light2D` | Global lights: `color`, `intensity`, `affectedSortingLayers`, blend style 0. Other light types throw `UnsupportedLight2DException`. |
| `UnityEngine.Rendering.Universal.UniversalAdditionalCameraData` | Base camera without post-processing, antialiasing or shadows. Enabling an unsupported option throws `UnsupportedCameraSettingException`. |

## UI

| Type | Supported |
| --- | --- |
| `Canvas` | `renderMode` (screen space overlay, screen space camera, world space), `enabled`, `sortingOrder`, `scaleFactor` (the factor for the current screen), `planeDistance`, `worldCamera` (read; the host sets it with `DisunityHost.RenderThroughCamera`), `pixelPerfect` (on a screen-space canvas, each text moves so its first glyph starts on a whole screen pixel, as Unity's `Text` does; world-space canvases ignore it) |
| `RectTransform` | `anchorMin`, `anchorMax`, `pivot`, `sizeDelta`, `anchoredPosition`, `rect` |
| `UnityEngine.UI.Text` | `text`, `fontSize`, `font`, `color`, `alignment`, `lineSpacing`, `horizontalOverflow`, `verticalOverflow`, `supportRichText`; `fontStyle` Normal only; `resizeTextForBestFit` off only. Fonts are TrueType files registered by the host with `DisunityHost.RegisterFont`; there is no built-in font. |

Text covers printable ASCII with line breaks and kerning. Font size is pixels per em. Wrap breaks at word boundaries, splitting words wider than the line, and Truncate drops lines that don't fit; glyphs are never clipped. Rich text is on by default, as in Unity, and markup is rejected while it is on because it isn't rendered. A `CanvasScaler`'s scale-with-screen-size settings are applied by the host with `DisunityHost.ScaleWithScreenSize`; the component itself, `Image`, `Button`, layout groups and the EventSystem are not implemented.

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
