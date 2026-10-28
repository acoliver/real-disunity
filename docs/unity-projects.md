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
| `player-decode <data directory>` | Decodes every object of the classes with layouts (GameObject, Transform, RectTransform, MonoScript, the MonoBehaviour header, MeshFilter, MeshRenderer, SkinnedMeshRenderer, Mesh, Material, Texture2D, Camera, Light, Shader, Canvas, CanvasRenderer, CanvasGroup, TextAsset, Sprite, SpriteAtlas, Font, BuildSettings, TagManager, ResourceManager, PreloadData and RenderSettings) across the player's files, and counts them by class. Reports the first five errors per class and exits unsuccessfully if any decode fails. MonoBehaviour script fields remain an opaque byte range. Texture pixels and mesh index and vertex buffers are located (a range inside the object, or a `.resS` stream) but not read or interpreted. A shader's compiled programs are located as a compressed byte range with per-platform offsets and lengths. A text asset's contents are located as a byte range and need not be UTF-8. A sprite's index and vertex bytes are located like a mesh's, and a font's font file is located as a byte range. |
| `player-object <serialized file> <path ID>` | Every decoded field of one object. Fails on objects whose class has no layout. |
| `player-shader <serialized file> <path ID> <platform index> <output directory>` | Decompresses one platform of a shader's compiled programs and writes each LZ4 chunk to `chunk<i>.bin` in a new output directory. The platform index is the position in the shader's platform list; the command prints that platform's `ShaderCompilerPlatform` value and the chunk sizes. In the Unity 6000.3.15f1 player checked, the first chunk starts with the table of sub-programs, and every entry fits inside the chunk its segment index names. Each program is listed by `player-shader-programs`. |
| `player-shader-programs <serialized file> <path ID> <platform index>` | Lists the shader's compiled variants for one platform (Direct3D 11 and Vulkan are supported), one line each: blob entry, sub-shader and pass index, stage, chunk, Unity's `ShaderGpuProgramType`, keywords, code length and the bytes after the code, which are not interpreted yet. For a Vulkan program it also lists the stage modules by slot, encoding (SMOL-V or SPIR-V) and length; in the Unity 6000.3.15f1 player checked, slot 0 holds the vertex module and slot 1 the fragment module. `player-shader-modules` decodes them. |
| `player-shader-modules <serialized file> <path ID> <platform index> <output directory>` | Decodes every stage module of the shader's Vulkan programs, from SMOL-V where they are stored that way, and writes each as `<blob entry>.<slot>.spv` in a new directory. Each module's SPIR-V header and instruction lengths are checked, and the command prints its word count and the execution model of each entry point (0 vertex, 4 fragment). A program several variants share is written once. The SMOL-V decoder follows [smol-v](https://github.com/aras-p/smol-v), used under its public-domain option; on the 21,546 distinct modules of a Unity 6000.3.15f1 player its output is byte-identical to smol-v's own decoder. |
| `player-shader-report <player data directory>` | Prepares each distinct Vulkan stage module of the player's shaders for naga, checks it with naga's SPIR-V parser and validator, and prints how many pass and the failures by cause, each with its first shader and module. Preparation splits combined image samplers into separate images and samplers with [spirv-webgpu-transform](https://crates.io/crates/spirv-webgpu-transform), which moves later bindings in the same descriptor set up to make room, and records where each resource's Unity binding ends up; replaces specialization constants with their defaults; and rewrites `OpSNegate` on unsigned integers as a subtraction from zero. Passing means naga parses the module and validates it with every naga capability enabled; it does not show that a particular GPU or wgpu backend supports the module, and the module is not checked with spirv-val. A naga panic on a module is reported as a failure of that module. Modules are counted once by content: a Unity 6000.3.15f1 player's 21,546 stage modules hold 2,928 distinct ones, of which 2,809 pass (1,308 of them needing the sampler split); the rest fail validation with `InvalidStoreTypes` (112), make naga panic (5) or use input attachments (2). `OpSpecConstantOp` is not rewritten, so a module using it fails naga's parse. |
| `player-scene <data directory> <scene file>` | The serialized scene's transform hierarchy with positions, active state and component class names. Script names resolve through same-file or external MonoScript references. Invalid parent/child links, cycles and hierarchies deeper than 256 levels fail explicitly. Cross-file hierarchy links are unsupported. |

The object layouts have been checked against Unity 6000.3.15f1 player data.
Exact byte consumption detects size differences, but does not establish compatibility
with other Unity versions or layouts of the same size. These commands inspect content;
they do not instantiate or run the player.

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
