# Playing the original games

`worldrunner-play` runs Snake, Pong or Breakout from its original Unity scene in a window. It fetches the game with `worldrunner-original-games`, plans the scene with `unity-import`, compiles the game's scripts unchanged with `csharp/OriginalGames`, and loads the scene through the `csharp/Disunity.Play` bridge. Each frame advances the world and draws its presentation in the desktop host from `worldrunner-scene-viewer`.

## Command line

```sh
cargo build -p worldrunner-disunity-ffi
cargo run -p worldrunner-play -- pong
cargo run -p worldrunner-play -- breakout --frames 300 --fixed-dt 20 --key 30:a:down --key 90:a:up --capture 120:breakout.png
```

| Option | Meaning |
| --- | --- |
| `--size WxH` | Window size in pixels; 960x540 by default. |
| `--frames N` | Close the window after N frames. Without it the game runs until the window is closed. |
| `--fixed-dt MS` | Advance every frame by MS milliseconds instead of the measured frame time. |
| `--key FRAME:KEY:down\|up` | Press or release a key before frame FRAME. Keys are lowercase names: letters, digits, `f1` to `f12`, `space`, `enter`, `escape`, `backspace`, `tab`, `delete`, `insert`, `home`, `end`, `pageup`, `pagedown`, `arrowup`, `arrowdown`, `arrowleft`, `arrowright`, `shift`, `control` and `alt` (the left keys), `capslock`, `numlock`, `scrolllock`, `pause` and the punctuation keys. They reach the game as the standard `KeyCode` values. |
| `--capture FRAME:FILE` | Write frame FRAME as it was presented to an RGB PNG. |
| `--seed N` | Seed the game's `Random` before the scene loads, so runs with `--fixed-dt` and the same keys repeat exactly. Without it, as in Unity, each run differs. |
| `--font FILE` | The TrueType file to use for Unity's built-in Arial. On Linux the default is the font `fc-match Arial` selects, usually Liberation Sans, whose metrics match Arial. |

Keyboard input from the window works the same way as scheduled keys. Pong's left paddle uses W and S, and Breakout's paddle uses A and D.

The engine library is found through `DISUNITY_FFI_LIBRARY` or beside the `play` executable, where `cargo build -p worldrunner-disunity-ffi` puts it. Builds go to `target/play/<game>`. The command needs the .NET 8 SDK and system Git, and network access the first time a game is fetched.

Settings the engine does not reproduce and input axes it cannot deliver are printed when the scene loads. Snake is steered with W, A, S and D.

## Built Unity players

`play --player <data directory>` starts a scene of a built Unity player, the `<Game>_Data` directory beside its executable, without a window:

```sh
cargo run -p worldrunner-play -- --player /path/to/Game_Data --scene level0 --frames 60
```

It writes the script layouts of the player's game assemblies with `csharp/Disunity.Layouts`, plans the scene file with `unity-import`'s `plan_player_scene`, and boots the same `Disunity.Play` bridge. The bridge resolves the player's assemblies in its load context, runs the player's startup hooks in Unity's phase order around the scene, loads the scene's plan and the host advances `--frames` frames (one by default) of `--fixed-dt` milliseconds (1/60 s by default). `--scene` names the scene file, `level0` by default. Each `--arg VALUE` (repeatable; each value is one argument, in order) is part of the player's command line: `Environment.GetCommandLineArgs()` returns the player's executable path first, then the `--arg` values, for example `--arg game.zip --arg -headless` for a game that loads the save named on its command line. The executable path is the engine's choice: the data directory's parent joined with the data directory's name without its `_Data` suffix, plus `.exe` for a Windows player. The run logs it as `command line: ...`. Each step prints a `[player]` line, along with what the plan or the loader leaves out, so the last line before an error is the step that failed, and the error names the hook, component or frame. Builds go to `target/play/player`. A player that imports a native library it does not ship for this platform, such as Old World's Wwise `AkSoundEngine`, loads its own `Plugins` file when there is one and otherwise the engine's substitute from the engine library's directory (the silent Wwise library, which `--player` builds there). See "Player native plugins" in `docs/unity-projects.md`.

## Offscreen work from a player

`worldrunner_play::offscreen` maps the offscreen work a player's scripts record in the world (camera renders into render textures, `Graphics.Blit` and `Graphics.CopyTexture`) onto `worldrunner-render-program`'s `OffscreenFrame`, so the player's own shaders draw it. It is in `play` because it needs the world's types (`world-simulation`), the importer's readers for the player's serialized files (`unity-import`, a Tool crate) and render-program; `scene-viewer`, the one Host crate, may not depend on the importer.

| Piece | What it does |
| --- | --- |
| `GraphicsSnapshot` | `worldrunner_world_simulation::graphics::GraphicsSnapshot` holds what the frame's records reach: the cameras they name with their pose, the renderers those cameras may draw (every layer a camera's mask admits, not only the screen camera's), the materials and shaders of those renderers and of the blits, the render textures with their samplers, the global state, and the compute shaders and buffers the dispatches name. `DisunityHost::graphics_snapshot()` drains it once per call, and `GpuFlush::snapshot()` does the same inside a flush; a frame with no work gives an empty snapshot. |
| `OffscreenAdapter::adapt` | Loads each shader (`ShaderLoader`, or any `ShaderSupply`), uploads the meshes and stored textures the frame names through `Assets` (implemented for `DisunityHost`), and maps the snapshot with `adapt`. A mesh keeps its vertex channels under the numbers the host stored them with (0 position, 1 normal, 2 tangent, 3 colour, 4 to 11 texture coordinate sets 0 to 7; blend weights and indices are left out), each as 1 to 4 floats, and its ranges become the sub-meshes by range number; a range that draws and is not triangles refuses the mesh by the name of its topology (`QuadTopology`, `LineTopology`, `LineStripTopology`, `PointTopology`). A stored texture is uploaded with its wrap along u and v and its filter as render-program's `SamplerState`; a render texture brings the same from its `WorldSampler` wherever a property names it. Mirror once is read as mirror; the wrap along w, anisotropy and mip bias are not carried, and a render texture read as the source of a blit or a copy is read with the fixed sampler. A blit of every pass (-1) becomes one record for each pass of the material's shader, in order. A record that names a destroyed texture, camera or material, or something the host cannot supply, is skipped and counted by reason in the `Report`. A dispatch is not a frame record: `Adapted::dispatches` says how many records come before it, and the render textures its properties name are in the frame. |
| `build` | `OffscreenState::build` for the adapted frame. When the state refuses the frame, each render texture and then each record is built alone, the ones refused are left out and counted by the error's name, and the rest are built together. |
| `ShaderLoader` | Opens the serialized file a `ShaderSource` names under the data directory (a path that leaves it is refused), decodes the shader object, takes the chunks of its Vulkan platform and builds the `ProgramShader`. Sources are cached, failures included. |
| `builtins`, `camera` | The per-draw values (`unity_ObjectToWorld`, `unity_WorldToObject`) and per-camera values (`unity_MatrixV`, `unity_MatrixVP`, `_WorldSpaceCameraPos`). The module documentation gives the evidence for each name, how it is computed, and the names left missing with the number of the player's variants that hold them. |

`play shader-census <player data directory>` loads every shader of a player through the loader and prints counts. All 171 shader objects of the Old World player (Unity 6000.3.15f1) load as programs, with 663 passes.

`unity-import player-shader-builtins <player data directory>` is the aggregate measurement the built-in names rest on; see [the importer guide](../../docs/unity-projects.md).

### Compute and the GPU flush

`OffscreenHost` is the player's side of a device: the `ProgramRuntime` and its arena (made once), the `OffscreenState`, the `OffscreenAdapter` and a `ComputeAdapter`. `ComputeAdapter` asks a `ComputeSupply` for the `StageProgram` of a kernel, the way `ShaderSupply` gives a shader's passes, and keeps it until the shader's generation rises. A dispatch is prepared by name: the properties pack into the kernel's constant buffers, the textures the properties name bind to the texture parameters the kernel declares (`StageProgram::texture_targets`), and each compute buffer the dispatch binds is uploaded from the contents the snapshot holds and bound to the storage buffer of the same name (`StageProgram::buffer_targets`). A dispatch that cannot run is left out and counted as `dispatch:` and the cause (`StaleShader`, `NoCompiledCode`, `ProgramLayout`, `MissingTexture`, `TextureUnavailable`, `UnboundBuffer`, `StaleBuffer`, `BufferUpload`, `Constants`).

`OffscreenHost::flush` runs a snapshot's work to completion in call order, offscreen records and dispatches alike. Each run of records is built, admitted and encoded into one frame of the runtime's authority, followed by the dispatch after it, and that frame is submitted. A last submission copies every compute buffer a dispatch bound, and the call waits for that submission alone. The buffers whose bytes changed come out as `BufferWrite`s, and `FlushOutcome::apply` hands them to a `BufferSink` (`GpuFlush::write_compute_buffer` in a flush, `World::set_compute_buffer_data` in a test). Each run is its own submission because the buffer the runs' constants share is rewritten for the next run. On an error nothing is written and the managed engine returns the contents the CPU holds.

`tests/flush.rs` (in `offscreen/tests`) runs a synthetic world on a real adapter: a camera clears an R16 float render texture to 0.25 and draws 0.5 over its left half, a kernel multiplies each texel by a constant-buffer vector (2) and writes a storage buffer, and the flush writes back sixteen values of 0.5 and 1.0 that the test predicts from those numbers. A second test puts a dispatch before and one after the camera render and checks that only the second buffer changes.

`PlayerKernels` is the supply `PlayerGpu` uses. A world compute shader's source names a serialized file and the path id of the compute shader object in it. The loader decodes the object with `unity-serialized`'s `ComputeShader`, takes the kernel at the dispatch's index from the SPIR-V platform (renderer 21), runs its variant compiled for no keywords and builds the `StageProgram` with `render_program::compute_kernel::kernel_stage`. A dispatch whose kernel cannot load counts as `dispatch:` and the cause: `Unreadable` for a file or object that does not decode, `NoCompiledCode` for a shader without a SPIR-V platform, `NoSuchKernel`, `NoVariant` for a kernel that has no variant without keywords, and `Program` for a variant the renderer cannot build, such as one that writes a storage texture. `unity-import player-compute-report <player data directory>` counts the same outcomes over a whole player. In the Old World player, 10 of its 76 kernel variants become programs with layouts and targets, and 66 write storage textures, which the renderer does not bind. The flush test above reads its kernel through this path, from a serialized compute shader object it writes into a temporary data directory.

### Wiring a frame

`PlayerGpu` (in `gpu_player.rs`) owns a device, the `OffscreenHost` on it and the counts of a run. `run_player` makes one when the machine has an adapter (`--gpu-work off` skips it), registers its flush handler with `DisunityHost::set_gpu_flush` before the scene loads, and after each advance calls `PlayerGpu::frame`, which takes `graphics_snapshot()`. A frame whose snapshot holds no work returns at once and leaves the device alone, so the run is the one it was before. With work, the records are adapted, built, admitted against a frame of the authority, encoded, submitted and waited for; the counts of records, dispatches, skips and failures print at the end as `[gpu]` lines.

The flush handler reaches the engine through `GpuFlush`, which has no read of meshes and textures, so it reads them from an `AssetCache` that the frame loop fills from the host between advances with the identities a flush asked for. A mesh the first flush names is left out of that flush as `MeshUnavailable` and is there for the next. A `GpuFlush::mesh_channels`, `player_texture` and `texture_sampler` would remove this.

The windowed paths do not run the offscreen work yet. `SceneGpu::admit_offscreen` takes the operations before the scene is encoded, but the viewer makes the device, its submission authority and the renderer's allocation reservation for the program arena, and the retained and live drivers have no hook that gives them to the player. That hook is the remaining step of the live wiring.

`play_player_window` draws built-player mesh objects and screen-space overlay UI
through the retained scene renderer. Text coverage and image quads retain native
hierarchy order and pixel clip bounds. UI replacements track draw changes, image
texture generations and viewport size; unchanged frames reuse the overlay.
Ordinary 2D scene quads and world-space UI remain outside this retained path.

## Tests

The library's synthetic retained-player UI test runs the managed host and captures
windowless frames. It requires .NET 8 and a GPU adapter and checks image bytes,
text, clipping and unchanged-frame UI update counts without opening a window.

`tests/play_window.rs` plays all three games in a 480x270 window and checks the captured pixels. It needs an X display; run it as the lavapipe CI job does:

```sh
env -u WAYLAND_DISPLAY xvfb-run --auto-servernum cargo test -p worldrunner-play --test play_window
```

The test starts each game in a child process without `WAYLAND_DISPLAY` itself. Do the same for the `play` command under `xvfb-run`: with a Wayland session in the environment the window opens there instead, and a window the compositor never shows is never presented.

The offscreen adapter's tests build synthetic worlds and serialized files and need no game data. The GPU tests run a camera render, a blit of every pass and a copy through the adapter and render-program on a real adapter and compare predicted pixels; run them on lavapipe with:

```sh
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json WGPU_BACKEND=vulkan cargo test -p worldrunner-play --lib offscreen -- --test-threads=1
```
