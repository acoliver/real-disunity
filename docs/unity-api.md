# Supported UnityEngine API

`Disunity.Engine` implements a subset of the `UnityEngine` API. This page lists what exists today, grouped the way a game uses it. If a type or member is not listed here, it does not exist yet, and code that uses it will not compile. Where a member exists but a particular setting is unsupported, it throws an exception naming the setting.

The source of truth is `csharp/Disunity`. Each type lives in a file of the same name, so `csharp/Disunity/Physics2D.cs` holds `Rigidbody2D` and the 2D colliders. Writing scripts and hosting them is covered in [Writing C# scripts](csharp-games.md).

## Objects, components and lifecycle

These cover creating, finding and destroying things, which nearly every script uses.

| Type | Supported |
| --- | --- |
| `Object` | `Instantiate` (original; with parent; with position and rotation; with position, rotation and parent), `Destroy(obj)` for game objects and `ScriptableObject`s, `Destroy(obj, delay)` for game objects, `FindObjectOfType`, `FindFirstObjectByType`, `FindObjectsByType`, Unity-style null and equality checks on destroyed objects |
| `GameObject` | `name`, `tag`, `CompareTag`, `activeSelf`, `activeInHierarchy`, `SetActive`, `GetComponent<T>`, `AddComponent<T>`, `transform`, `FindGameObjectWithTag` |
| `Component` | `gameObject`, `transform`, `tag`, `CompareTag`, `GetComponent<T>` |
| `Behaviour` | `enabled`, `isActiveAndEnabled` |
| `MonoBehaviour` | Messages `Awake`, `OnEnable`, `Start` (also as a coroutine), `FixedUpdate`, `Update`, `LateUpdate`, `OnDisable`, `OnDestroy`, `OnCollisionEnter2D`/`Stay2D`/`Exit2D`, `OnTriggerEnter2D`/`Stay2D`/`Exit2D`; `StartCoroutine`, `StopCoroutine`, `StopAllCoroutines` |
| `Transform` | `position`, `rotation`, `localPosition`, `localRotation`, `localScale`, `parent`, `SetParent(parent, worldPositionStays)` |
| `ScriptableObject` | `CreateInstance<T>`, `CreateInstance(Type)` and `CreateInstance(string)`, with `Awake`, `OnEnable`, `OnDisable` and `OnDestroy` |
| Attributes | `SerializeField`, `Header`, `Range`, `CreateAssetMenu` |

Instantiated objects have their internal references remapped to the clone, as in Unity, and their `Awake` and `OnEnable` run before `Instantiate` returns. `Destroy` follows Unity's timing: the object stops receiving update messages at once, but searches still find it and it compares non-null until the end of the frame, when `OnDisable` and `OnDestroy` run. Destroying a single component throws `NotSupportedException`.

## Time, input and coroutines

| Type | Supported |
| --- | --- |
| `Time` | `deltaTime`, `fixedDeltaTime`, `time`, `frameCount` |
| `Input` | `GetKey`, `GetKeyDown`, `GetKeyUp`; `GetAxis` (smoothed) and `GetAxisRaw` for any axis the host registered |
| `KeyCode` | `Space`, `A` to `Z`, the four arrow keys, `Return`, `Escape` |
| `UnityEngine.InputSystem.Keyboard` | `Keyboard.current.aKey` and `dKey` with `isPressed` |
| Yield instructions | `WaitForSeconds`, `WaitForSecondsRealtime`, `WaitForFixedUpdate`, `null` (next frame), nested `IEnumerator` |

No axes exist until the host registers them with `DisunityHost.RegisterInputAxis`, giving each a name, negative and positive keys, alternate keys, gravity, sensitivity, dead zone, snap and invert, as in Unity's Input Manager. Asking for an unregistered axis throws `ArgumentException`. Mouse input, gamepads and Input System actions are not implemented. Coroutines cannot be joined by yielding a `Coroutine`, and `WaitForEndOfFrame` and custom yield instructions are not supported.

## 2D physics

| Type | Supported |
| --- | --- |
| `Rigidbody2D` | `bodyType` (Dynamic, Kinematic, Static), `velocity`, `linearVelocity`, `angularVelocity`, `mass`, `gravityScale`, `freezeRotation`, `constraints` (`RigidbodyConstraints2D` freeze position X, Y and rotation), `simulated`, `position`, `MovePosition`, `Sleep`, `WakeUp`, `IsSleeping`, `sharedMaterial` |
| `Collider2D` | `enabled`, `isTrigger`, `offset`, `attachedRigidbody`, `sharedMaterial` |
| `BoxCollider2D`, `CircleCollider2D` | `size`; `radius` |
| `UnityEngine.Tilemaps.TilemapCollider2D` | Collides with the `Tilemap` on its object, following its cells as they change. Contacts and queries return it like any other collider. |
| `UnityEngine.Tilemaps.Tilemap` | `enabled`, `color`, `tileAnchor`. The host sets the grid layout, tile palette and cells with `DisunityHost.ConfigureTilemap`, `SetTile` and `SetCells`; the script `Tile` and `Grid` APIs are not implemented. |
| `PhysicsMaterial2D` | `name`, `friction`, `bounciness` |
| `Collision2D` | `collider`, `otherCollider`, `gameObject`, `transform`, `rigidbody`, `contacts`, `contactCount`, `GetContact` |
| `Physics2D` | `Raycast` (nearest hit), `OverlapCircle` (first hit) |
| `RaycastHit2D` | `collider`, `point`, `normal`, `distance`, conversion to `bool` |

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
| `Canvas` | `renderMode` (screen space overlay, screen space camera, world space), `enabled`, `sortingOrder`, `scaleFactor` (the factor for the current screen), `planeDistance` |
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
| `Mathf` | `Min`, `Clamp` |
| `Random` | `value`, `Range(int, int)`, `Range(float, float)`. Not seedable. |
| `Debug` | `Log`, `LogError`, written to the console |
| `UnityEngine.Events.UnityEvent<T>` | `AddListener`, `RemoveListener`, `RemoveAllListeners`, `Invoke`. Persistent listeners set in the editor are not loaded yet. |

The math types are deliberately small and grow as games need them. Adding a missing helper such as `Vector3` addition is a welcome contribution; see the [developer guide](../dev-docs/README.md).
