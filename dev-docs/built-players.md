# Built players

A built Unity player is the `<Game>_Data` directory beside a game's executable: serialized files (`globalgamemanagers`, `level0`, `*.assets`, `.resS` streams), the compiled assemblies in `Managed`, and the manifests `ScriptingAssemblies.json` and `RuntimeInitializeOnLoads.json`. This page maps how the engine reads and runs one, which component owns each stage, and where each stage's contract is documented. The work is tracked in [#176](https://github.com/acoliver/real-disunity/issues/176).

The pipeline adds no second world, host, API or renderer. A player scene ends up in the same native world through `DisunityHost.LoadScene`, as an imported project scene does ([architecture](architecture.md#ownership)).

## Stages

| Stage | Owner | Input | Output | Contract |
| --- | --- | --- | --- | --- |
| Read serialized files | `unity-serialized` | Each serialized file | Header, types, objects with byte ranges, externals | Crate rustdoc; `unity-import player` |
| Script layouts | `Disunity.Layouts`, using `Disunity.Engine`'s `Players/ScriptLayouts.cs` | The player's game and package assemblies | Script layouts JSON: the serialized fields of each script class | [`player-scripts`](../docs/unity-projects.md#the-unity-import-tool) |
| Decode objects | `unity-import` (`player_objects`, `player_values`, `player_scripts`) | Serialized files and script layouts | Typed objects and script fields; players carry no type trees | [`player-decode`, `player-object`, `player-scripts`](../docs/unity-projects.md#the-unity-import-tool) |
| Plan scenes | `unity-import` `plan_player_scene` | A scene file and script layouts | A scene plan the managed loader builds, with its differences listed | [`player-plan`](../docs/unity-projects.md#the-unity-import-tool) |
| Plan resources | `unity-import` `plan_player_resources` | The player's `ResourceManager` and script layouts | `resources.json`, script asset field files and texture pixel files | [`player-resources`](../docs/unity-projects.md#the-unity-import-tool), [`Resources`](../docs/csharp-games.md#the-players-environment) |
| Prepare shaders | `unity-import` `player_shaders` and `shader_prep` | Compiled shader programs | Vulkan stage modules decoded from SMOL-V, prepared and validated with naga | [`player-shader*`](../docs/unity-projects.md#the-unity-import-tool) |
| Load assemblies | `Disunity.Engine` `PlayerManifest` and `PlayerAssemblies` | `Managed` and the two manifests | An `AssemblyLoadContext` with the game and package assemblies, and the startup hooks | [Built players](../docs/csharp-games.md#built-players) |
| Run | `Disunity.Play` `PlayerRun`, driven by `play --player` | The data directory, its plans and the bridge request | The scene loaded around the startup phases and advanced on the main thread | [play README](../crates/play/README.md#built-unity-players) |

`play --player` runs every stage in order and writes its intermediate files under `target/play/player`. Each stage prints a `[player]` line, so the last line before an error names the stage that failed.

## Failures

Each stage reports what it cannot handle by name: a class without a layout, a script field whose type the engine does not define, a scene that fails to plan, a resource the plan does not hold, a shader module naga rejects. Planning lists engine components it leaves out as differences; it does not substitute them. A scene that fails to plan is listed in the bridge request without a plan, so only loading that scene fails.

## Testing

Tests use synthetic serialized files, assemblies and shader modules. No game content is committed. Comparison against a real game runs outside CI in a private harness that takes the install path, under the [source-use rules](../AGENTS.md#source-use-required-boundaries). Runtime loading of a player's package assemblies does not authorize an agent to inspect their implementation.
