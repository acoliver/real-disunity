# Importing Unity projects

The goal for Unity projects is that you point the engine at an unmodified project and it plays: scenes are imported, scripts are compiled against `Disunity.Engine`, and the scene is built in the native world before any `Awake` runs. `unity-import plan` turns a scene into a plan, `DisunityHost.LoadScene` builds the plan in a running world (see [Loading scenes](unity-api.md#loading-scenes)), and the `play` command runs a game in a window:

```sh
cargo build -p worldrunner-disunity-ffi
cargo run -p worldrunner-play -- pong
```

`play` takes the three test games below by name; its options for scripted keys, fixed frame times and captures are in the [play crate's README](../crates/play/README.md).

## What the importer reads

`crates/unity-import` reads a project's text-serialized files directly. It never writes to the project, and Unity does not need to be installed.

- **Project catalog**: every asset and its `.meta` GUID, the importer type of each asset, and `.meta` files whose asset is missing.
- **Scenes and prefabs**: Unity's text YAML, for both current editors and Unity 2017. Prefab instances are expanded from their prefab files with the scene's overrides applied, including stripped references. Prefabs and `ScriptableObject` assets that the scene refers to are loaded once as templates.
- **Textures and sprites**: PNG and JPEG images with their import settings (filter, wrap, color space, pixels per unit) and the sprite rectangles and pivots in single and multiple sprite mode. The `Square` sprite of Unity's built-in 2D Sprite package, which Snake draws, comes from the package rather than the project; the importer describes it from the package's own import settings and its uniformly white pixels, so no package file is copied.
- **Components with typed views**: orthographic `Camera` and the `AudioListener`, `GUILayer` and `FlareLayer` beside it; `SpriteRenderer` and `TilemapRenderer`; `Rigidbody2D`; `BoxCollider2D`, `CircleCollider2D` and `TilemapCollider2D` with their physics materials; `Grid` and `Tilemap`; `Canvas` and `RectTransform`; and the package components `Text`, `CanvasScaler`, URP `Light2D` and `UniversalAdditionalCameraData`. Package script identities are resolved from their DLL GUID and file ID. Settings the engine does not implement, such as perspective cameras, a scene skybox material, sprite masks, body damping, effectors or composite colliders, fail with the component named.
- **Input axes** from `ProjectSettings/InputManager.asset`, in file order with Unity's button names.
- **Sprite physics shapes** for tiles that collide with their sprite. A fully opaque sprite without an authored shape collides with its rectangle, as Unity generates it. Authored shapes and outlines traced from transparent pixels are not supported yet.

Anything the importer cannot resolve is reported as a missing reference or a typed error that names the file, object and field. The importer never invents a replacement asset.

### Scene plans

`plan_scene` turns a scene into the plan the managed loader builds from. It lists every object of the scene and of the prefabs the scene references, parents first, with its engine components and scripts in authored order. Prefab objects are marked as templates: the loader keeps them for `Instantiate` and never activates them. Assets, sprites, textures, package fonts and input axes are listed once and referenced by index.

Script fields keep Unity's text, because only the compiled script says how to read a value. A reference in a field names a plan entry: `{"$object": i}` for a GameObject, `{"$transform": i}` for its transform, `{"$component": [i, j]}` for component `j` of object `i`, `{"$asset": i}` for a ScriptableObject or physics material, `{"$sprite": i}` and `{"$texture": i}` for texture content, and `{"$font": "Arial"}` for a package font. Planning fails on a missing reference, on references to other asset types, on unsupported components and settings, and on game objects outside every hierarchy.

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
| `plan <project> <scene>` | The scene plan as JSON: what the managed loader builds. Planning fails, naming the object, on anything the engine cannot build as authored. |
| `player <data directory>` | For a built player's `<Game>_Data` directory: each serialized file (`globalgamemanagers`, `level0`, `*.assets`) with its Unity version, format version, externals and objects counted by class, then the totals. Reading built players is in progress ([#147](https://github.com/acoliver/real-disunity/issues/147)); this command reads file metadata only. |

For a project with one scene holding one object, `scene` prints:

```text
$ cargo run -p worldrunner-unity-import -- scene MyProject Assets/Scenes/Main.unity
Player [Transform] at (2, 0, 0)
scene: 2 objects
missing references: 0
```

## Status of the test games

Three small open-source Unity games are the first targets. They will run from their original repositories at pinned commits, with no changes to their source or assets.

| Game | Unity version | Status |
| --- | --- | --- |
| [Snake](https://github.com/eguldali/Optimized-Snake-Game) | 6000.3 | Plays in a window: W, A, S and D steer the snake on its wrapping grid, it grows when it eats, and the food moves. |
| [Pong](https://github.com/mcrvaz/pong-tdd) | 2017.1 | Plays in a window: the ball launches, W and S move the left paddle and goals score. |
| [Breakout](https://github.com/admtrv/Breakout) | 6000.3 | Plays in a window: bricks and the ball come from their prefabs, A and D move the paddle, bricks break and the tile walls keep the ball in play. |

Two tests play them. `crates/disunity-runtime/tests/original_scenes.rs` checks scripts, physics, input and UI state without a window, and `crates/play/tests/play_window.rs` checks the pixels of frames presented in a window. Pong's text uses a substitute for Unity's Arial, and the settings each scene uses that the engine does not reproduce, such as Breakout's lit tilemap, are printed when it loads.

## When a project does not import

If `scene` reports missing references or an error, the cause is one of three things: an asset the project really does not contain, a Unity package asset the importer does not know yet, or a serialized feature the importer does not support. The fix belongs in the importer or the engine, never in the game project. Please open an issue with the command you ran and its output.
