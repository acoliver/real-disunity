# Importing Unity projects

The goal for Unity projects is that you point the engine at an unmodified project and it plays: scenes are imported, scripts are compiled against `Disunity.Engine`, and the scene is built in the native world before any `Awake` runs. The importer half of that is in place. The loader that builds an imported scene in a running world is still being written ([#77](https://github.com/acoliver/real-disunity/issues/77)), so today you can inspect a project but not play it.

## What the importer reads

`crates/unity-import` reads a project's text-serialized files directly. It never writes to the project, and Unity does not need to be installed.

- **Project catalog**: every asset and its `.meta` GUID, the importer type of each asset, and `.meta` files whose asset is missing.
- **Scenes and prefabs**: Unity's text YAML, for both current editors and Unity 2017. Prefab instances are expanded from their prefab files with the scene's overrides applied, including stripped references. Prefabs and `ScriptableObject` assets that the scene refers to are loaded once as templates.
- **Textures and sprites**: PNG and JPEG images with their import settings (filter, wrap, color space, pixels per unit) and the sprite rectangles and pivots in single and multiple sprite mode.
- **Components with typed views**: orthographic `Camera` and the `AudioListener`, `GUILayer` and `FlareLayer` beside it; `SpriteRenderer` and `TilemapRenderer`; `Rigidbody2D`; `BoxCollider2D`, `CircleCollider2D` and `TilemapCollider2D` with their physics materials; `Grid` and `Tilemap`; `Canvas` and `RectTransform`; and the package components `Text`, `CanvasScaler`, URP `Light2D` and `UniversalAdditionalCameraData`. Package script identities are resolved from their DLL GUID and file ID. Settings the engine does not implement, such as perspective cameras, a scene skybox material, sprite masks, body damping, effectors or composite colliders, fail with the component named.
- **Input axes** from `ProjectSettings/InputManager.asset`, in file order with Unity's button names.
- **Sprite physics shapes** for tiles that collide with their sprite. A fully opaque sprite without an authored shape collides with its rectangle, as Unity generates it. Authored shapes and outlines traced from transparent pixels are not supported yet.

Anything the importer cannot resolve is reported as a missing reference or a typed error that names the file, object and field. The importer never invents a replacement asset.

## The `unity-import` tool

The tool prints what the importer sees, which is the quickest way to find out how far a project gets. Paths after the project directory are relative to the project root.

```sh
cargo run -p worldrunner-unity-import -- <command> <project> [path]
```

| Command | Prints |
| --- | --- |
| `catalog <project>` | Editor version, asset count, orphan `.meta` files and assets per importer type. |
| `documents <project> <asset>` | The YAML documents in one scene, prefab or asset file, counted by class. |
| `scene <project> <scene>` | The expanded object hierarchy with each object's components and position, where each object came from, and every missing reference. |
| `texture <project> <texture>` | Image size, import settings and every sprite in one texture. |
| `sprites <project> <scene>` | Every texture the scene's sprites use, and any sprite reference that does not resolve. |
| `components <project> <scene>` | The typed view of each camera and camera participant, sprite and tilemap renderer, Rigidbody2D, 2D collider with its physics material, grid, tilemap, Canvas, RectTransform and supported package component. |
| `axes <project>` | The project's input axes from `ProjectSettings/InputManager.asset`, in file order. |

For a project with one scene holding one object, `scene` prints:

```text
$ cargo run -p worldrunner-unity-import -- scene MyProject Assets/Scenes/Main.unity
Player [Transform] at (2, 0, 0)
scene: 2 objects
missing references: 0
```

## Status of the test games

Three small open-source Unity games are the first targets. They will run from their original repositories at pinned commits, with no changes to their source or assets.

| Game | Unity version | Import status |
| --- | --- | --- |
| [Snake](https://github.com/eguldali/Optimized-Snake-Game) | 6000.3 | Imports except for one sprite that comes from Unity's 2D sprite package, whose exact metrics are not confirmed. |
| [Pong](https://github.com/mcrvaz/pong-tdd) | 2017.1 | Imports with no missing references. |
| [Breakout](https://github.com/admtrv/Breakout) | 6000.3 | Imports with no missing references. |

None of them runs yet, because of the missing scene loader. The engine work they need is tracked in [#70](https://github.com/acoliver/real-disunity/issues/70). Named input axes, position constraints on 2D bodies and Unity's deferred `Destroy` timing are done; camera, `CanvasScaler` and `Text` style settings over the ABI and the loader itself remain.

## When a project does not import

If `scene` reports missing references or an error, the cause is one of three things: an asset the project really does not contain, a Unity package asset the importer does not know yet, or a serialized feature the importer does not support. The fix belongs in the importer or the engine, never in the game project. Please open an issue with the command you ran and its output.
