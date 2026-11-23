# Playing the original games

`worldrunner-play` runs Snake, Pong or Breakout from its original Unity scene in a window.
It fetches the game with `worldrunner-original-games`, plans the scene with `unity-import`, compiles the game's scripts unchanged with `csharp/OriginalGames`, and loads the scene through the `csharp/Disunity.Play` bridge.
Each frame advances the world and shows its presentation in the retained host's window from `worldrunner-scene-viewer` (`play_game_window`, which applies a `RetainedUpdate::World` per frame).

## Command line

```sh
cargo build -p worldrunner-disunity-ffi
cargo run -p worldrunner-play -- pong
cargo run -p worldrunner-play -- breakout --frames 300 --fixed-dt 20 --key 30:a:down --key 90:a:up --capture 120:breakout.png
```

- `--size WxH`: Window size in pixels; 960x540 by default.
- `--frames N`: Close the window after N frames.
  Without it the game runs until the window is closed.
- `--fixed-dt MS`: Advance every frame by MS milliseconds instead of the measured frame time.
- `--key FRAME:KEY:down|up`: Press or release a key before frame FRAME.
  Keys are lowercase names: letters, digits, `f1` to `f12`, `space`, `enter`, `escape`, `backspace`, `tab`, `delete`, `insert`, `home`, `end`, `pageup`, `pagedown`, `arrowup`, `arrowdown`, `arrowleft`, `arrowright`, `shift`, `control` and `alt` (the left keys), `capslock`, `numlock`, `scrolllock`, `pause` and the punctuation keys.
  They reach the game as the standard `KeyCode` values.
- `--capture FRAME:FILE`: Write the frame presented after game frame FRAME to an RGB PNG.
  The game waits for the capture, so the image is that game frame's, with that frame's UI.
  The file is written when the capture is delivered, with `frame N captured to FILE` on standard output, so a run that stops later keeps the frames it reached. The run fails naming any requested frame that was not written.
  A FRAME that is not among the `--frames` run is refused before the game starts.
- `--seed N`: Seed the game's `Random` before the scene loads, so runs with `--fixed-dt` and the same keys repeat exactly.
  Without it, as in Unity, each run differs.
- `--font FILE`: The TrueType file to use for Unity's built-in Arial.
  On Linux the default is the font `fc-match Arial` selects, usually Liberation Sans, whose metrics match Arial.

Keyboard input from the window works the same way as scheduled keys.
Pong's left paddle uses W and S, and Breakout's paddle uses A and D.

The engine library is found through `DISUNITY_FFI_LIBRARY` or beside the `play` executable, where `cargo build -p worldrunner-disunity-ffi` puts it.
Builds go to `target/play/<game>`.
The command needs the .NET 8 SDK and system Git, and network access the first time a game is fetched.

Settings the engine does not reproduce and input axes it cannot deliver are printed when the scene loads.
Snake is steered with W, A, S and D.

## Built Unity players

`play --player <data directory>` starts a scene of a built Unity player, the `<Game>_Data` directory beside its executable, without a window:

```sh
cargo run -p worldrunner-play -- --player /path/to/Game_Data --scene level0 --frames 60
```

It writes the script layouts of the player's game assemblies with `csharp/Disunity.Layouts`, plans the scene file with `unity-import`'s `plan_player_scene`, and boots the same `Disunity.Play` bridge.
The bridge resolves the player's assemblies in its load context, runs the player's startup hooks in Unity's phase order around the scene, loads the scene's plan and the host advances `--frames` frames (one by default) of `--fixed-dt` milliseconds (1/60 s by default).
`--scene` names the scene file, `level0` by default.
Each `--arg VALUE` (repeatable; each value is one argument, in order) is part of the player's command line: `Environment.GetCommandLineArgs()` returns the player's executable path first, then the `--arg` values, for example `--arg game.zip --arg -headless` for a game that loads the save named on its command line.
The executable path is the engine's choice: the data directory's parent joined with the data directory's name without its `_Data` suffix, plus `.exe` for a Windows player.
The run logs it as `command line: ...`.
Each step prints a `[player]` line, along with what the plan or the loader leaves out, so the last line before an error is the step that failed, and the error names the hook, component or frame.
Once the scene and resource plans exist, each distinct plan difference that concerns a shader (a material that names no shader, a material whose shader reference did not resolve, or a shader that did not plan) is printed as a `[player] shader:` line, up to 40, followed by a count of the rest.
These are the causes of the materials that load with the fallback `Hidden/InternalErrorShader`.
Builds go to `target/play/player`, and game assemblies that give native code delegates for `[MonoPInvokeCallback]` static methods are rewritten into `target/play/player/callback-cache` so those delegates stay callable (see `docs/csharp-games.md`).
A player that imports a native library it does not ship for this platform, such as Old World's Wwise `AkSoundEngine`, loads its own `Plugins` file when there is one and otherwise the engine's substitute from the engine library's directory (the silent Wwise library, which `--player` builds there).
See "Player native plugins" in `docs/unity-projects.md`.

`--report-each-frame` prints the GPU report after every game frame of a run, in a window or not, not only at the end (the counts are the run's so far, except the stand-in census, which is the frame's own).
`--screen FRAME:FILE.png` (repeatable, like `--capture`, and several files may name one frame) plays the run in a windowless host with no display and writes the image after game frame FRAME (counting from 0) to FILE as an RGB PNG: the newest image of the screen with the game's UI over it, at `--size` (the window's default, 1920x1080, when it is not given).
Each file is written when its frame's capture is delivered (`frame N captured to FILE`), and the run fails naming any frame that was not written.
It takes `--frames` (one by default) and `--fixed-dt` (60 Hz by default) like a window run, and no `--key` or `--capture`, which make a window.
The screen is what a command buffer's `CameraTarget` draws into when no camera texture names it, so the option needs a game whose scripts draw into the screen.
A FRAME that is not among the `--frames` run is refused before the scene starts.
A FRAME whose work drew nothing into the screen gets the image of the last frame that did, and an error (`no screen image after frame N`) when none has.
A frame or flush the GPU refuses whole, or whose work goes without an allocation the storage budget refuses (a render texture; a texture or mesh upload the budget refuses is asked for again, see "The storage budget"), ends the run at that frame with an error naming the refusal and a non-zero exit, and writes no screen file for that frame or after it: the image such a frame would show is the last good one, which looks valid.
The report still prints, and `refused whole` and `failures` keep counting what was refused.
The UI is the screen-space overlay the world holds at FRAME, laid out for the screen's size (see "Windowless runs" below); a world with no overlay canvas gives the screen as it is.
The player reads the same size as its `Screen`.
A run without `--screen`, `--size`, `--capture`, `--key` or `--read-target` has no viewer and no screen: it advances the game and does its offscreen work on the player's own device.

### Reading a record's target

`play --player DATA --read-target FRAME:RECORD:FILE.png` (repeatable) writes the target of one record of a frame as it stands right after that record, to compare each pass of a frame with an original capture:

```sh
cargo run -p worldrunner-play -- --player OW_Data --frames 43 --size 1920x1080 --read-target 42:0:first.png
cargo run -p worldrunner-play -- --player OW_Data --frames 43 --size 1920x1080 --read-target 42:17:opaque.png --read-target 42:23:fog.png
```

- `FRAME` counts from 0 like `--screen`; `RECORD` is the index into the frame's adapted records, the numbering of `[gpu] frame F record R ...` lines.
  The two numbers and `FILE` are split at the first two colons, so `FILE` may be `C:\out\a.png`; it must end in `.png`.
- For every frame that has a `--read-target`, the run prints one `[gpu] frame F record R KIND ...` line for each record of the frame on standard error, before the frame is built, with the record's kind, its target texture, size and depth bits, and the id, size and depth bits of the depth attachment when that is another texture than the target. A depth read is the depth the record rendered with, so a record that draws into a colour texture with a depth texture of its own reads that depth texture.
  Run once with any record to find the numbers, then ask for the ones you want.
  The numbering follows the frame path only: a script's flushes build their own, smaller record lists, which are not numbered.
  The numbers stay the same between runs of one player, scene, flags and `--fixed-dt`, and may change with the game or the engine.
- The read takes the record's target at mip 0, layer 0: the target of a camera render, a blit or a copy, the texture mips are generated for, or the first colour of a clear, a draw list or a procedural draw.
  A read is a copy recorded in the frame's own submission right after the record, so it sees what the records up to that one drew, and the final image of the frame is the same with or without it.
- Colour goes to `FILE` as an 8-bit RGB PNG, top row first, without alpha.
  A float target (`Rgba16Float` and the like) is clamped to 0..1 and encoded as sRGB, so linear 0.5 becomes 188; an 8-bit target is written as stored.
  Values above 1 clip.
- Depth goes beside it, to `FILE.depth.png` (for `a.png`, `a.depth.png`), as a 16-bit grey PNG of the stored value: 1 is near and 0 is far, because every player run draws with reversed depth, and the value is not linearised.
  The target must have a depth buffer whose depth aspect can be copied (`Depth32Float`, `Depth32FloatStencil8`, `Depth16Unorm`); a 24-bit depth buffer cannot be.
- Each file written prints `[player] read-target frame F record R KIND colour|depth WxH FORMAT -> FILE`.
- A read that cannot be made is named and counted in the report's errors, never skipped silently: a `Globals` record (it draws nothing), a record past the end of the frame, a frame with no work, a depth buffer that is missing or cannot be copied, and a frame the build refused records of (it is built again without the reads, since removing records changes the numbering).
  A read that wrote a file but not the other one (no depth buffer, or a depth-only target with no colour) does not fail the run; a read that wrote no file makes the run exit with an error after the other files are written.
- The option needs `--player` and a run with no viewer: it takes no `--screen`, `--key` or `--capture`, which show their frames through the viewer, and a FRAME that is not among the `--frames` run is refused before the scene starts.
  `--size` sets the size of the screen the player's `Screen` reports and the run draws its screen in, 1920x1080 when it is not given.
- Stand-ins, time and `--global` values change a frame: use the same flags and `--fixed-dt` as the run you compare with.
  Do not take frame timings from a run that has reads.

### Depth range

Every player run draws with reversed depth: near is 1 and far is 0, depth clears to 0, a pass's `ZTest LEqual` is `GreaterEqual` (and `Less` is `Greater`), a shadow lookup's comparison is `GreaterEqual`, and a camera's `unity_MatrixVP` maps near to w and far to 0.
There is no setting for it, so the GPU side and the managed side cannot disagree:

- `prepare_player`: Writes `"reversed_z_buffer": true` into the bridge's request, and the managed `PlayerDevice` declares a reversed depth buffer (`SystemInfo.usesReversedZBuffer`).
- `OffscreenState`: Clears depth to `1 - d`, reverses depth tests and the shadow comparison, and negates the constant and slope terms of a depth bias.
- `clip_from_gl`: The adapter builds the clip space of every camera render and renderer list view with it.
  It also negates clip Y, ahead of the negation every compiled player stage applies, so the scene's up lands in the last rows and a render texture's rows are bottom first like a stored texture's (a shader's texture coordinate, a pixel read and a blit then agree on which row is which).

Forward depth gives constant camera textures.
The evidence is an Old World RomeTurn42 run, frame 16 (board 133, posts 3214, 3220 and 3238).
With forward depth (clear 1, `ZTest LEqual` as `LessEqual`), every camera texture held one value across the screen: the normals were exactly (0, 0, -1, 0) and the depth 0.0076.
With reversed depth and the request field declaring it to the managed side, the normals varied across the screen (mean z -0.99) and real geometry won the depth test.

`glstate_matrix_projection` and `_ZBufferParams` are still not set (see `builtins`), so a shader that reads them gets zero.
A depth clear record maps its value d to 1 - d, so a record that clears to 1 stores 0.
A windowed player run (`--size`, `--capture` or `--key`) draws the same way.

### Supplying a named global for one run

`play --player DATA --global NAME=v0,v1,v2,v3` gives the global `NAME` a value for one run of a player's offscreen work, headless or in a window, for example `--global unity_LightData=1,1,1,1`.
The option is repeatable, one name each (a name given twice is refused), and it is off by default: a run without it is unchanged.
`NAME` is letters, digits and `_`, not starting with a digit.
The value is one to four finite floats, comma separated; the components left out are 0.
A malformed name or value is refused at startup, naming the option and the text it refused.

Each value goes into the globals a frame starts from, as a fallback exactly as the host's main light does (`Mapper::new`).
A name the world's own globals already hold keeps the world's value, and a `SetGlobal` the script records replaces the supplied value from the place in the frame where it is recorded.
A global the script writes directly (`Shader.SetGlobalTexture`, `SetGlobalFloat`, `SetGlobalVector`, `SetGlobalMatrix`, `EnableKeyword` and the like) reaches the host the same way: the world puts a `SetGlobal` or `SetGlobalKeyword` for each name written since the previous record in front of the next record, so a draw reads the value the script had set when it recorded the draw and not the one the frame ends with.
A global an executed command buffer sets (`CommandBuffer.SetGlobalTexture`, `SetGlobalFloat`, `EnableShaderKeyword` and the like) is such a direct write, so it also stays in the state every later take starts from.
Only a global set to a temporary render texture is a recorded `SetGlobal` that ends with its take.
`tests/persistent_globals_gpu.rs` (in `offscreen/tests`) checks this on a real adapter with two takes: the first sets a global to a filled render texture and the second draws two quads that sample it, with no stand-in.
The world also puts a record for each name written after the frame's last record at the end of the work, and the state the frame starts from is the world's global state as it was before the frame's first write, so a draw before a write reads the value the script had set when it recorded the draw.
A supplied name that is also `_MainLightColor` or `_MainLightPosition` takes the place of the host's light for that name.
A value a draw reads for a name comes from the first of these that has one: a `SetGlobal` recorded before the draw, the world's own global, the `--global` value, the host's main light (for the two light names), and last the default the shader declares for the property.
The declared defaults of the two light names and of every supplied name sit below the globals for that reason (`Material::below_globals`); the declared default of any other name keeps its place above them.
The value is stored as a four-component vector, so any constant-buffer member of one to four floats that reads the name packs its leading components.
The option does not say what any name means or holds; it only puts the value you give where a constant no layer sets would otherwise pack as zero.
`unity_LightData` is the exception to that example: in a frame with a directional light every draw sets it itself, per draw and above the globals, so `--global unity_LightData=` reaches only the draws of a frame with no directional light.

The run's GPU summary has one line when any value is supplied, for example `[gpu] globals supplied for this run: unity_LightData=(1, 1, 1, 1)`, which lists each name with the components given.
A windowed player (`--size`, `--capture` or `--key`) installs the same offscreen host on the viewer's device and applies the values as a headless run does, so `--global` is accepted there.
A run that is not a `--player` run (an original game) has no offscreen work, so `--global` is refused there too instead of being dropped.
The path is `PlayerLaunch::globals` to `PreparedPlayer::globals` to `PlayerGpu::headless` (`PlayerGpu::shared` for a window, through `window_gpu`) to `Settings::supplied` to `Mapper::new`.
`tests/supplied_gpu.rs` draws a synthetic shader that writes a constant-buffer member named `unity_LightData` as colour on lavapipe: it reads 0 with no option, the supplied value with the option, and the recorded value when the script records the same name.

## Windowless runs

`play --player DATA --screen FRAME:FILE.png` runs the window's driver without a window.
`play_player_windowless` makes the viewer's `SharedGpu` (`SharedGpu::headless`, so there is no event loop and no display), builds `PlayerGpu` on its device with `window_gpu` and `SharedDevice`, and plays the scene in `RetainedHost::windowless` with the same `PlayerRetainedDriver` and `WindowScript` that `play_player_window` runs, so there is one path from the world to a captured frame.
The loop calls `WindowlessRetained::frame` and `poll` until the script closes the session, and fails when no frame is submitted for five minutes or a capture does not arrive in that time.
The viewer renders at a fixed time (`RenderClock::Fixed`), so a frame does not depend on the wall clock.

Frame N is the image after game frame N.
The script counts game frames, not the viewer's updates.
After it asks for the capture of frame N it holds the game, the lights, the UI and the camera as they are until the capture arrives, so the image is N's even when the viewer takes several frames to be ready for it, and `--frames` counts game frames.
The UI is the frame's `UiPresentation` converted by `PlayerUi`, as in a window, with the render textures it reads offered from the player's device (`PlayerGpu::ui_offers`).
The viewer samples the screen in its opaque pass and draws the UI over it in the same submission, and nothing is copied through the CPU between them.
A windowless run that asks for a frame no work had drawn the screen by fails with `no screen image after frame N`: the viewer would show its stock scene in the screen's place, which is not a frame of the game.
A window shows the stock scene until the game draws.

The run prints `[player] ran N frames` and `[player] frame time: first X, mean Y, slowest Z, N frames` as a headless run does.
The time of a game frame runs from its update to its presented frame, or to its capture when it asked for one, and the viewer overlaps the GPU's work with the next update, so it is not the time of a submit-and-wait run.

## Offscreen work from a player

`worldrunner_play::offscreen` maps the offscreen work a player's scripts record in the world (camera renders into render textures, `Graphics.Blit` and `Graphics.CopyTexture`) onto `worldrunner-render-program`'s `OffscreenFrame`, so the player's own shaders draw it.
It is in `play` because it needs the world's types (`world-simulation`), the importer's readers for the player's serialized files (`unity-import`, a Tool crate) and render-program; `scene-viewer`, the one Host crate, may not depend on the importer.

- `GraphicsSnapshot`: `worldrunner_world_simulation::graphics::GraphicsSnapshot` holds what the frame's records reach: the cameras they name with their pose, the renderers those cameras may draw (every layer a camera's mask admits, not only the screen camera's), the set of renderers each camera render drew when the script called it (`renderer_sets`; an object a script switched on only around `Camera.Render` is in its set though it is inactive when the snapshot is taken), the materials and shaders of those renderers and of the blits, the render textures with their samplers, the global state, and the compute shaders and buffers the dispatches name.
  `DisunityHost::graphics_snapshot()` drains it once per call, and `GpuFlush::snapshot()` does the same inside a flush; a frame with no work gives a snapshot with no records that still lists every live render texture, because the host gives back the render textures a snapshot omits.
- `OffscreenAdapter::adapt`: Loads each shader (`ShaderLoader`, or any `ShaderSupply`), uploads the meshes and stored textures the frame names through `Assets` (implemented for `DisunityHost`), and maps the snapshot with `adapt`.
  A mesh keeps its vertex channels under the numbers the host stored them with (0 position, 1 normal, 2 tangent, 3 colour, 4 to 11 texture coordinate sets 0 to 7; 12 blend weights and 13 blend indices of a skinned mesh), each as 1 to 4 floats, and its ranges become the sub-meshes by range number; a range that draws and is not triangles refuses the mesh by the name of its topology (`QuadTopology`, `LineTopology`, `LineStripTopology`, `PointTopology`).
  A stored texture is uploaded with its wrap along u, v and w, its filter and its anisotropy level as render-program's `SamplerState`; a stored 2D array (the record's `dimension` 5, of one layer or more) is uploaded as one source array holding every level of each layer, a stored cube (`dimension` 4) as one source cube of six square faces in the order +X, -X, +Y, -Y, +Z, -Z, each holding its whole mip chain, and a stored 3D texture (`dimension` 3) as one source volume of `depth` slices whose bytes are its mip levels one after another, each level as wide, as high and as deep as the base level halved and at least one texel.
  The four-byte `GraphicsFormat` 87 (the terrain atlases' normal, metallic and roughness maps, which has no `TextureFormat`) is stored blue, green, red, alpha: every array, cube and volume that holds it goes up as sRGB RGBA8 with bytes 0 and 2 of each texel exchanged, so a shader reads its red from the stored red byte (a plain 2D texture has no `TextureFormat` for 87 and is refused as unsupported), and `GraphicsFormat` 88 (a four-byte data volume) goes up in the order it is stored.
  A cube that is not six square faces (`CubeFaces`), a volume with more levels than its size has (`VolumeMips`), a cube or volume whose bytes are not exactly its levels (`Layers`, `Levels`) and a block-compressed volume (`Unsupported`) are refused by name, counted by format, and left out with the draws that read them; a draw whose shader declares a dimension the stored texture does not have (a cube slot over a 2D texture, a 2D slot over a cube or a volume, and the other pairs) is left out as `SlotDimensionMismatch`; a render texture brings the same from its `WorldSampler` wherever a property names it.
  Mirror once is read as mirror; an anisotropy level of 0 and 1 both read as none, a level above 16 as 16, and the sampler applies it only to a trilinear filter.
  The mip bias is not carried, since wgpu samplers have none, and a render texture read as the source of a blit or a copy is read with the fixed sampler.
  A blit of every pass (-1) becomes one record for each pass of the material's shader, in order.
  A blit, mesh draw or procedural draw reads its material as it stood when the script recorded it: the snapshot holds a copy of each earlier state of a material the script edited after drawing with it, under an id of its own, and the adapter maps it as a material (`tests/blit_material_gpu.rs` draws two blits of one material with a texture changed between them and reads each target).
  A blit with a material has no renderer and no camera, so the adapter gives it the identity as `unity_ObjectToWorld`, `unity_WorldToObject` and `unity_MatrixVP` (`blit_layer`, the frame's `blit_values`): its quad is in clip space, and a pass that places the quad by those matrices drew nothing before.
  A record that names a destroyed texture, camera or material, or something the host cannot supply, is skipped and counted by reason in the `Report`.
  A renderer the cameras can see that the frame leaves out is counted in `Report::renderers_skipped` (the `[gpu] skipped renderers:` line of `play`) by cause: `MeshUnavailable`, `EmptySlot` (a slot with no usable material before one that has), `NoMaterial` (no slot names a material), and, when the slots name materials but none of them can be drawn, the cause of the first one left out, `ShaderUnavailable` (the material's shader has no source or failed to load) or `StaleMaterial`. A skinned renderer with no root bone in the world has no placement and is counted as `SkinnedNotDrawn`.
  A dispatch is not a frame record: `Adapted::dispatches` says how many records come before it, and the render textures its properties name are in the frame.
- Renderer sets (`GraphicsSnapshot::record_sets`): A camera render becomes a render-program `Camera` whose `renderers` holds its set (a renderer's id is its index in the snapshot's `renderers`), so the camera draws those renderers that its mask also admits.
  A camera render with no set, which the world leaves for a camera it cannot read, is skipped as a stale camera.
  Records that share a set share a camera of the frame, and the probe's scratch renders of a camera draw the same set.
  A culling record keeps the renderers of its set whose layer its mask admits and that are inside its planes (the world applied the mask when it made the set); a culling with no set keeps nothing.
  The set is what makes a bake work whose objects are active only around `Camera.Render`.
- Mesh draws (`OffscreenWork::DrawMesh`): A command buffer's mesh draw is drawn like one renderer of a renderer list with one sub-mesh, into the render target bound at its place in the frame (`SetRenderTarget`, an optional `ClearRenderTarget`) with the view and projection of the last `SetViewProjection` or camera setup.
  The host reads the mesh by the record's identity through `Assets`, as it reads a renderer's mesh, and the draw takes the mesh's channels, the material's constants and textures, the shader pass by index and the render state of the pass as a list draw does.
  The record's matrix is the object's `unity_ObjectToWorld` (and its inverse `unity_WorldToObject`); `unity_MatrixV`, `unity_MatrixVP` and `_WorldSpaceCameraPos` come from the view, and are missing before any view is set.
  The sub-mesh number is the mesh's range number.
  A `pass` of -1 draws every pass of the material's shader in order inside one draw, so a later pass blends over an earlier one; any other pass draws that one.
  A mesh draw is left out and counted in `Report::skipped` (and `skipped_by_kind` as `DrawMesh: <reason>`) as `MeshUnavailable` (the store does not hold the mesh, or it failed to upload), `EmptyMesh` (the mesh has nothing to draw, which also adds the `mesh with nothing to draw` census line when the mesh is read, or the sub-mesh has no indices), `NoSuchSubmesh` (the sub-mesh number is past the mesh's ranges), `NoSuchPass` (the pass is past the shader's passes), `UnboundTarget`, `PartialViewport`, `StaleMaterial` or `ShaderUnavailable`.
- Viewports (`OffscreenWork::SetViewport`): A command buffer's viewport is the rectangle in pixels of the bound render targets that the draws after it cover.
  One rectangle applies to every colour attachment and to the depth of the bound set, which share the first colour's size at the bound mip (or the depth's, when no colour is bound).
  `SetRenderTarget` and a camera setup reset it to the whole target, and a binding that leaves nothing bound leaves the draws unbound before any viewport is looked at.
  A `DrawRendererList`, `DrawMesh` or `DrawProcedural` that reaches the target while the rectangle lies inside it (x and y at least 0, width and height above 0, x + width and y + height within the target's size at the bound mip) carries the rectangle in its `DrawRaster`, and the render program maps the draws' clip space to it with the pass's viewport; a rectangle that covers the target draws as if none were set, and one that has no area or reaches outside the target leaves the draw out, counted in `Report::skipped` (and `skipped_by_kind` as `<kind>: PartialViewport`).
  The host clears whole targets, so a `ClearRenderTarget` while the rectangle leaves part of the target uncovered is left out the same way.
  `Blit` and `CopyTexture` ignore it.
- Scissor and depth bias (`OffscreenWork::EnableScissor`, `DisableScissor`, `SetDepthBias`): A command buffer's scissor is a rectangle in pixels of the bound targets, from the bottom left as a viewport is, that clips the draws after it until a disable, a `SetRenderTarget` or a camera setup.
  Each `DrawRendererList`, `DrawMesh` and `DrawProcedural` record carries the scissor and the depth bias in force at its place in the frame as its `DrawRaster`, and the render program rounds the rectangle to whole texels of the attachments' mip, drops the part outside them and clips the pass's draws to it; a rectangle with no texel left draws nothing.
  A clear ignores the scissor, and `Blit` and `CopyTexture` do too.
  The depth bias is the pair (constant, slope) of the last `SetDepthBias`, held across target changes until the next one; the pipelines of the triangle draws take them as the constant and slope-scale terms of their depth bias as given (negated when depth is reversed), and a line draw has none.
  The units of the constant are the pipeline's and may not match the units a script means.
- `build`: `OffscreenState::build` for the adapted frame.
  When the state refuses the frame for a record (the refusal's `record` is the index of the record it was planning), that record is built alone with the globals in force for it through `OffscreenState::check`, left out and counted by the error's name if it is refused alone too, and the frame is built again, so a frame with k refused records costs k builds alone and k+1 builds of the frame.
  A record that builds alone was refused for what the records do together, and a refusal that names no record (a render texture, the frame's reads after its records) builds each render texture and then each record alone; the ones refused are left out and counted by the error's name, and the rest are built together.
  `check` plans a frame as `build` does and counts nothing as written, so a build that is never run does not make a render texture look written to a later frame, which would bind it with no stand-in and read zeros.
  `tests/rebuild.rs` holds these claims.
- `ShaderLoader`: Opens the serialized file a `ShaderSource` names under the data directory (a path that leaves it is refused), decodes the shader object, takes the chunks of its Vulkan platform and builds the `ProgramShader`.
  Sources are cached, failures included.
- `builtins`, `camera`, `light_data`: The per-draw values (`unity_ObjectToWorld`, `unity_WorldToObject`, and `unity_LightData`, which is `(0, 0, z, 0)` with `z` 1 when the main light affects the renderer and 0 when it does not; `light_data` has the rule: the main light is the first directional light whose culling mask shares a layer with the culling mask of the camera or culling result the draw comes from, and it affects the layers of its own culling mask) and per-camera values (`unity_MatrixV`, `unity_MatrixVP`, `_WorldSpaceCameraPos`).
  The module documentation gives the evidence for each name, how it is computed, and the names left missing with the number of the player's variants that hold them.
  `<texture>_TexelSize` is not one of the names left to the host: render-program fills any constant buffer member of that name with (1 / width, 1 / height, width, height) of the first level of the texture the draw binds under `<texture>`, unless a layer sets the member (Old World's terrain PVT normal pass reads `_GlobalTerrainLastHeightmap_TexelSize`, which the game never sets).

`play shader-census <player data directory>` loads every shader of a player through the loader and prints counts.
All 171 shader objects of the Old World player (Unity 6000.3.15f1) load as programs, with 663 passes.
It also prints how many loaded shaders have no pass with a `LightMode` tag and the passes by `LightMode`: in the Windows build's data 147 of the 171 shaders have none, and 603 of the 664 passes have no `LightMode`; `DepthOnly` (9), `SHADOWCASTER` (10), `UniversalForward` (13) and `DepthNormals` (8) are the largest tagged groups, and no pass is tagged `SRPDefaultUnlit`.

`unity-import player-shader-builtins <player data directory>` is the aggregate measurement the built-in names rest on; see [the importer guide](../../docs/unity-projects.md).

Vector arrays and matrix arrays (`PropertyValue::VectorArray` and `MatrixArray`) are carried by name like the single values.
An array the global state holds when the frame starts, one a command sets with `OffscreenWork::SetGlobal` (a direct write is ordered in front of the next record as one) and one a dispatch holds is copied into the constant buffer member of its name in element order, each matrix column by column.
A command's array applies to the records after it until another command sets that name.
A pass that declares fewer elements than the array holds is not drawn: its record is left out and counted as `record:Blit:Constant` (the record kind, then the error), with the cause `record:Blit:Constant/ArrayTooLong` and, in the example, the shader and pass of the record followed by the property and the given and declared counts.
A dispatch in that case is left out as `dispatch:Constants`.
`DispatchTrace::constants` lists an array by its element count.
The same mapping serves the values of a material and of a property block.
`tests/global_arrays.rs` (in `offscreen/tests`) checks this on a real adapter with a blit pass that shows one element in each half of its target and a kernel that copies its array constants into a buffer; the matrices it uses have rows that differ from the columns shown, so a transposed matrix changes the result.

### Compute and the GPU flush

`OffscreenHost` is the player's side of a device: the `ProgramRuntime` and its arena (made once), the `OffscreenState`, the `OffscreenAdapter` and a `ComputeAdapter`.
`ComputeAdapter` asks a `ComputeSupply` for the `StageProgram` of a kernel, the way `ShaderSupply` gives a shader's passes, and keeps it until the shader's generation rises.
A dispatch is prepared by name: the properties pack into the kernel's constant buffers, the textures the properties name bind to the texture parameters the kernel declares (`StageProgram::texture_targets`), and each compute buffer the dispatch binds is uploaded from the contents the snapshot holds and bound to the storage buffer of the same name (`StageProgram::buffer_targets`).
A dispatch that cannot run is left out and counted as `dispatch:` and the cause (`StaleShader`, `NoCompiledCode`, `ProgramLayout`, `MissingTexture`, `TextureUnavailable`, `UnboundBuffer`, `StaleBuffer`, `BufferUpload`, `Constants`).

`OffscreenHost::flush` runs a snapshot's work to completion in call order, offscreen records and dispatches alike.
Each run of records is built, admitted and encoded into one frame of the runtime's authority, followed by the dispatch after it, and that frame is submitted.
A last submission copies every compute buffer a dispatch bound, and the call waits for that submission alone.
The buffers whose bytes changed come out as `BufferWrite`s, and `FlushOutcome::apply` hands them to a `BufferSink` (`GpuFlush::write_compute_buffer` in a flush, `World::set_compute_buffer_data` in a test).
Each run is its own submission because the buffer the runs' constants share is rewritten for the next run.
On an error nothing is written and the managed engine returns the contents the CPU holds.

`tests/flush.rs` (in `offscreen/tests`) runs a synthetic world on a real adapter: a camera clears an R16 float render texture to 0.25 and draws 0.5 over its left half, a kernel multiplies each texel by a constant-buffer vector (2) and writes a storage buffer, and the flush writes back sixteen values of 0.5 and 1.0 that the test predicts from those numbers.
A second test puts a dispatch before and one after the camera render and checks that only the second buffer changes.

A planned draw holds its constants, bindings and texture views until its build is submitted, so a flush that holds hundreds of camera renders of thousands of draws each would hold every draw at once.
`OffscreenHost::set_draw_budget` sets the most draws one build plans, `DRAW_BUDGET` (16,384, about 200 MB) unless a host sets another.
Before it plans a run of records, the flush splits the run where the sum of `OffscreenFrame::planned_draw_bound` over the records would pass the budget, builds and submits each part but the last on its own, and drops its plans, then continues with the next part.
The last part is built with the dispatch or pixel read that follows the run, as before, so records keep their call order and a read sees every earlier part.
A single record whose bound is more than the budget is a part of its own.
The report prints `[gpu] planner: builds N, most draws held in one build N, texture views made N and reused N`, and every `flush` and `frame` line ends with the same counts for that piece of work.
`tests/draw_budget.rs` flushes three cameras with budgets from one draw to unlimited, reads the height maps back through a kernel, and checks that every split draws the same values as the unlimited flush, that the builds and the most draws held are the ones the budget implies, and that a dispatch before the renders still reads zeros.

`PlayerKernels` is the supply `PlayerGpu` uses.
A world compute shader's source names a serialized file and the path id of the compute shader object in it.
The loader decodes the object with `unity-serialized`'s `ComputeShader`, takes the kernel at the dispatch's index from the SPIR-V platform (renderer 21), runs its variant compiled for no keywords and builds the `StageProgram` with `render_program::compute_kernel::kernel_stage`.
A dispatch whose kernel cannot load counts as `dispatch:` and the cause: `Unreadable` for a file or object that does not decode, `NoCompiledCode` for a shader without a SPIR-V platform, `NoSuchKernel`, `NoVariant` for a kernel that has no variant without keywords, and `Program` for a variant the renderer cannot build.
`unity-import player-compute-report <player data directory>` counts the same outcomes over a whole player.
In the Old World player all 76 of its kernel variants become programs with layouts and targets (before storage images were bound, 10 did and the 66 that write storage textures failed by name).
The 66 declare 80 storage images between them, every one write-only: `R32Float` 2D (15), `R32Float` 2D array (4), `R32Uint` 2D (24), `Rg32Float` 2D (17), `Rg32Float` 2D array (4), `Rgba32Float` 2D (2), `Rgba32Float` 3D (6) and `Rgba8Unorm` 2D (8).
Each is a format that a device made with no optional features binds as a write-only storage image, so the report counts no variant as needing device features.

A kernel's storage images bind as its buffers do, by name.
`StageProgram::image_targets` lists the storage image parameters with their prepared set and binding, and the dispatch's texture properties of those names must be render textures (any other texture is left out as `StorageSource`).
Before a flush builds anything, `ComputeAdapter::storage_textures` finds the render textures the snapshot's dispatches bind this way and `OffscreenState::bind_as_storage` marks them: a marked texture is made with storage binding usage when the device binds its format, and a texture that existed without it is made again, empty, the first time it is marked.
A texture stays marked, so later flushes keep its contents.
`ProgramRuntime::check_storage_images` then checks each binding before the dispatch is recorded, so a texture the kernel cannot be given leaves that dispatch out and not the flush: `StorageFormat` when the device cannot bind the render texture's format as storage, `StorageFormatMismatch` when the texture's format is not the one the kernel declares, `StorageUsage`, `StorageDimension` (the view the kernel declares is not the one the record's shape binds through: a 2D view for a plain texture, a 2D-array view for an array, a cube or a cube array, a 3D view for a volume; the reason names both shapes), `StorageSamples`, `StorageAliased` (one texture bound to a storage image that is written and to another binding of the same dispatch) and `StorageUnsupported` (the kernel declares a format or access the device cannot bind) and `StorageLimit` (the kernel binds more storage images in its stage than the device allows).
Two Old World variants bind five storage images, one more than the default limit of four. The player asks for the storage images in a stage that the scene's device asks for, render-gpu's `storage_images_per_stage` (the adapter's count up to 16), so a window's device and a headless one agree; an adapter offering only four leaves those two variants out as `StorageLimit`.
`play compute-census <player data directory>` builds every variant as a compute program on that device and prints the counts and the causes: all 76 of the Old World player's variants build on llvmpipe and on the AMD Radeon 780M, with none left out; with the default limit of four, 74 build and the two with five storage images are refused.
The binding is the first mip level of the texture.
A dispatch's writes are in the texture for every later camera render, blit, copy and pixel read, because the host records the dispatch into the same submission, after the operations before it and before the copy a pixel read wants, and wgpu orders the passes of a submission.
`tests/storage.rs` (in `offscreen/tests`) writes a pattern from a kernel into an RGBA8 render texture and reads it with a pixel read and, after a blit, from the blit's target; the expected floats come from the kernel's formula.
The flush test above reads its kernel through this path, from a serialized compute shader object it writes into a temporary data directory.

The world's record of a render texture states its dimension (2D, 3D, cube, 2D array or cube array) and its volume depth (the slices of a 3D texture, the layers of an array, six faces for each cube of a cube array, 1 for a 2D texture or a cube), and the host makes the texture with that shape the first time a frame names it, before any kernel touches it.
A record maps to render-program's `RenderDimension`: `D2`, `D2Array(layers)`, `D3(depth)`, `Cube` (six layers, square) and `CubeArray(cubes)`.
A dimension code the world does not define is carried as `Unknown(code)` and the state refuses it as `rendertexture:UnknownDimension`, which leaves out that texture and the records that name it.
The depth limit is the device's (`DepthLimit`).
A kernel's binding must match the record: a storage image is bound through a 2D view of a plain texture, a 2D-array view of an array, a cube or a cube array (a storage image has no cube view, so the faces are written as layers) and a 3D view of a volume, and a sampled texture through the view of the record's own dimension (2D, 2D array, 3D, cube, cube array).
A mismatch leaves the dispatch out as `dispatch:StorageDimension` or `dispatch:TextureDimension`, and the report's example for that cause and the dispatch trace's `reason` say both shapes (`the kernel declares a D2Array view and the render texture is D2`).
A 3D texture has one mip and no depth buffer, only kernels read and write it, a record that names one (a blit, copy or camera render) is refused as `VolumeInGraph`, and a pixel read answers its first slice; a kernel samples any layer or slice through a sampled `texture_2d_array` or `texture_3d`.
A cube is sampled by a draw, such as a blit, through a cube view.
`tests/volumes.rs` writes every layer of an array and every slice of a 3D texture from a kernel and reads each back through a second kernel and a pixel read, with the expected values from the formula the kernel writes.
`tests/shapes.rs` checks that each record shape is made with its layers before any kernel touches it (including a one-layer array and a 3D texture of depth 8, written in every slice), that a kernel writes the six faces of a cube through an array view and a blit samples one face through a cube view, that a kernel whose view differs from the record is left out by name with both shapes, and that an unknown dimension code is refused.
The report records, for each dispatch, the `GraphicsFormat` code of each render texture it names with what the kernel declares at that binding (`render texture format C bound as storage image F D`, `render texture format C sampled as K D`), even when the host cannot make that texture, so a code the format table lacks can be mapped from the formats it is paired with and not from a table; it also counts the formats of the render textures each frame holds and the dispatches that ran.
A mesh with no vertices or with no sub-mesh that has an index to draw is skipped without a failure and counted as `mesh with nothing to draw` with its sub-mesh count, topologies and whether it has indices and vertices; a sub-mesh that draws from indices the mesh lacks is the failure `mesh:IndicesMissing`.

`Report` also carries what a run needs to find out why a dispatch read back zeros.
Each dispatch leaves a `DispatchTrace`: the shader and kernel, the variant that runs, the group counts and the thread group size the world holds, the numeric values it sets, the textures the kernel declares with what was bound to each (render, asset, global, none or unset, whether the texture is on the GPU, and how many camera renders before the dispatch wrote it, with the draws they recorded and left out), the buffers with their sizes and non-zero words before and after the flush, the outcome (`ran` or the name of the cause it was left out) and whether the buffers were read back.
The report also counts camera draws left out for want of a pass by the `LightMode` values the shader's passes have against the ones the camera selects, the partial texture reads by property and pass (`draw:SampledWrittenLevelsOnly`, which the next paragraph explains and which `Report::notes` holds and `Report::failures` does not), and the texture properties that named no texture the frame can read and got no stand-in from the layers, by name and reason.
`play shader-census` counts the player's passes by `LightMode`.
`Settings::cameras()` is what `PlayerGpu` uses: a camera selects `UniversalForward`, `UniversalForwardOnly`, `SRPDefaultUnlit`, `ForwardBase` and `Always`, and a pass with no `LightMode` counts as `Always`, the last choice, so any selected tagged pass wins over it and a pass tagged with another mode (`DepthOnly`, `SHADOWCASTER`, `UniversalGBuffer` and the rest) is never drawn.
The evidence is in the player's data and our runs: 603 of the 664 passes have no `LightMode`, none carries `Always` or `SRPDefaultUnlit`, 147 of 171 shaders have no tagged pass, every shader a camera render met in a 24-frame RomeTurn42 run had one pass without a tag, and in the 7 shaders that mix tagged and untagged passes the tagged ones are depth, shadow, G-buffer and motion-vector passes.
A camera draw whose pass samples a texture no layer names is left out and counted by the texture property (`draws without a texture`), and the rest of the camera's draws are kept; before that, one such draw dropped the whole camera render.
A texture property a material leaves unset reads the default its shader declares for it, in the dimension it declares, when the host supplies that default: `white` (every channel 1), `black` (colour 0, alpha 1, an assumption), `gray`, `grey` and `linearGrey` (one grey, 0.5 with alpha 1) and `bump` ((0.5, 0.5, 1, 0.5): its RGB is the flat tangent-space normal under the `2 * rgb - 1` decode, and how the game's shaders read its alpha is not known here), each as a 2D texture, a cube, a 2D array of one layer or a 3D texture one slice deep (`DefaultTexture::named(name, dimension)`; 4 colours in 4 shapes, 16 textures).
Names match exactly as the shaders declare them.
The Old World player declares `white` for 221 texture properties, `black` for 23, `bump` for 18, `grey` for 12, `linearGrey` for 1 and no default for 43; on the RomeTurn42 save `draw:MissingTexture` fell from 44,489 to 10 with these defaults, and a property with no default name or a cube-array dimension is still left out and counted.
A draw that reads a declared default is counted in the stand-ins census by property and reason (`unset`, `declared-default` or `not-loaded`).
A texture property no material sets that its shader does not declare at all, such as the `_BlitCubeTexture` of the URP blit program or Old World's `_3DNoiseTexture`, reads the grey default of the slot's dimension: a 1x1 2D texture, a 1x1 cube of six faces or a 1x1x1 3D texture (the grey rows of the table: `DefaultTexture::Grey`, `GreyCube` and `Grey3d`), each RGBA (0.5, 0.5, 0.5, 1), the cube and 3D colours being assumed to match the 2D one.
The draws are recorded, not failures, and counted in the stand-ins census by property with the reason `undeclared`.
A stored texture the engine holds no pixels for is counted as `texture:NotStored` the first time a frame names it; one the host already holds unchanged is not.
A camera draw whose mesh lacks a vertex channel the pass reads is left out and counted by channel and shader input (`draws without a mesh channel`), as a draw without a texture is, so one such mesh no longer drops its whole camera render.
A renderer list names its own `LightMode` values (a URP forward list names `UniversalForward`, `UniversalForwardOnly` and `SRPDefaultUnlit`), and for a list a pass with no `LightMode` counts as `SRPDefaultUnlit`, not `Always`: the list admits a renderer whose shader has such a pass only when it names `SRPDefaultUnlit`, draws the pass of the first listed value that any pass has, and leaves out of the draw, counted as `no pass with the list's tags`, a renderer whose shader has none of the listed values.
A pass explicitly tagged `Always` is drawn by a list only when the list names `Always`.
A drawing list also leaves out, counted once for each renderer as `casts shadows only`, a renderer whose shadow casting mode is `ShadowsOnly` (3): it casts a shadow and draws nothing visible.
A shadow list keeps it.

The `[gpu] renderer lists:` line of `play` counts what the culling records and renderer lists did: culling records, renderers kept by culling (inside the view volume, from the set the world made on the culling record's layer mask), renderers culled by planes, lists made, drawing lists drawn, shadow lists drawn and lists with nothing to draw, then each reason a renderer was left out of a list.
A shadow list casts from the renderers its culling record's layer mask keeps whose bounds meet the split, whether or not the camera sees them, because a caster outside the camera's view can shadow what the camera sees.
`shadow casters outside the camera's view` counts the renderers that the camera's planes dropped and that got at least one caster item in a shadow list, once for each list; a renderer that casts no shadow, lies outside the split's planes or sphere, or has no shadow caster pass is not in that count and is counted by its own reason.

A draw writes the base level of a render texture and no other, so a draw that samples a texture with a mip chain binds only the levels the frame wrote and is counted as `draw:SampledWrittenLevelsOnly` (with the texture property, the levels and the pass in `Report::partial_reads`).
The draw is built, so the count is a note: it is in `Report::notes` and absent from `Report::failures`, and `gpu_player.rs` prints it on a `[gpu] notes:` line.
The row names the property and the pass; the texture's id and size are not at hand where the adapter counts it, since the planner reports the reads by property.
The adapter fills the chain with render-program's `Record::GenerateMips` in two cases.
The script's own `GenerateMips` command (`OffscreenWork::GenerateMips`) becomes the record where the script called it.
A render texture whose `auto_generate_mips` flag is set and that has more than one level gets the record by itself: after the records that write its level 0 (a camera render, a blit or a copy into it, and a clear or a draw while it is the render target at mip 0), once for each run of consecutive writes, before the next record that reads it, and at the end of the frame when it was written and nothing read it first.
A copy and a blit with no material read only their source.
A clear and a generation read nothing.
Every other record (a camera render, a blit with a material, a draw) may read any texture through its materials and globals, so it follows the generation of each flagged texture written before it other than the textures it writes itself, which stay in their run of writes.
A dispatch is placed after the generation of the flagged textures it binds, and a script's own record for a flagged texture stands for its automatic one until the next write.
A flagged texture a dispatch's kernel writes as a storage image is not noticed as written.
Without the flag and without the record the levels stay unwritten and the reads stay counted as before.
Render-program refuses the mips of a 3D texture, a depth-only texture and a format the device does not filter (the 32-bit float formats 49 and 52 unless the device has `FLOAT32_FILTERABLE`, reported by `Available::float32_filterable`).
For a flagged texture the adapter emits nothing in those cases, so the refusal cannot fail the frame, and counts the texture once for the frame as `rendertexture:MipsUnsupported` in `Report::failures`, with the reason in `Report::census` as `auto mips left out: 3D texture`, `auto mips left out: depth-only texture` or `auto mips left out: format C is not filtered`.
A refused explicit record is not checked by the adapter: the frame build refuses it and the record is dropped and counted by the error's name (`record:GenerateMips:MipsOfVolume` or `record:GenerateMips:MipsOfFormat`), as any record that does not build on its own is: the name is `record:`, the record kind and the error's name, and a blit or procedural draw with a material also names its shader and pass at the start of the cause's example.
The records the adapter generated are in `Report::built` and counted in the census as `generated mips of flagged render textures`; the script's own are in `built_by_kind` as `GenerateMips`.
`tests/mips.rs` (in `offscreen/tests`) checks the records' places for each case above, and on a GPU that a camera render into a flagged texture followed by a blit leaves no partial read and holds the 2x2 box average in level 1, that the same work without the flag is still counted, that an explicit record after a blit into an unflagged texture makes the later read bind every level, and that flagged textures render-program refuses leave the frame building.

### The storage budget

The program runtime holds all GPU storage the host makes (stored textures and meshes, render textures, constants and the staging of pixel reads) under one budget.
The budget is a total, not a buffer size: it is not tied to the device's `max_buffer_size`, and the runtime reserves its arena in buffers the device can hold (`ProgramRuntime::arena_allocations_within`). wgpu 30 reports no memory size for an adapter, so `storage_budget::choose` picks it with one rule on every adapter, from the two sizes the platform does publish (a size that is not known counts as none):

- the adapter's device-local memory, which `AdapterMemory::probe` reads where the platform publishes it (Linux amdgpu through sysfs): dedicated VRAM, or an integrated GPU's carve-out;
- a quarter of the host's `MemAvailable`, which is what a software adapter such as lavapipe and an integrated GPU, whose textures live in host memory, can use.

The budget is the larger of the two and no less than 512 MiB.
An adapter that publishes no size on a host that publishes none (Metal and Direct3D today, which have no reader yet) gets 512 MiB: the same rule with both sizes unknown.
`play` has no option for the budget: a run that left one out drew a different frame of the same game than a run that gave it, and the frame looked valid either way.
A discrete GPU on a host with much more memory than VRAM gets the quarter of the host's, which is more than its VRAM; the driver then places the excess in system memory.

The `[gpu] adapter` report line names the budget and which rule chose it.
When an allocation would not fit, the host gives back what the snapshot being mapped does not name, least recently named first: stored meshes, stored textures and render textures.
A snapshot names a render texture when a record reads or draws into it, or a property of a material, a renderer or the globals holds it.
A render texture given back is made again, empty, by the next frame that names it.
The host does this before it builds a frame's render textures, when an upload is refused and before the frame that copies the texture for a pixel read begins, so the staging fits.
The report counts what was given back as `given back to make room`, and a pixel read the budget still has no room for as `pixel reads refused: no copy of the texture could be made: over the storage budget`.
A stored mesh or texture whose upload the budget still refuses is counted as a failure of that snapshot and is asked for again, whole, by the next snapshot that names it, so it is uploaded once room exists even though the engine stores no newer generation of it.
One the host cannot upload for its own sake (a mesh with no position channel, a texture whose bytes are shorter than its base level, a format the host does not decode) is counted once and not read again until the engine stores a new generation.
In both cases the storage of the generation it replaced is given back, so a failed replacement leaves neither old geometry nor old texels to draw from; `tests/mesh_cache.rs` and `tests/texture_cache.rs` (in `offscreen/tests`) hold these claims.

### Seeing the stand-ins a run draws with

A draw that binds a texture the host supplied in place of a real one is a draw with a stand-in: a shader's declared default (`white`, `black`, `grey`, `bump`, in 2D, cube, array or 3D) and the grey that a slot nothing sets and no shader declares reads.
Such a draw is not an error and not a missing texture, so a run can report no failures while many of its draws read stand-ins.
Each recorded draw that binds one is counted, and the report prints two lines (also after every frame when `--report-each-frame` is given; that report counts the draws of the frame, including the flushes it ran, and the summary at the end of the run counts all of them):

```
[gpu] draws with stand-ins N
[gpu] stand-ins by property: <property> <reason> <count>, ...
```

`N` is the number of draws that bound at least one stand-in.
The second line names the properties with the most draws, one entry per property and reason.
A draw that binds stand-ins for two properties is counted once in `N` and once under each property.
When a property block and a material both name the property, the reason is the highest layer's: a block that names a texture the host does not hold over a material's declared default is `exists-not-loaded`, not `unset`.
The reasons are the host's account of why the slot has no real texture:

- `unset`: The shader declares a default for the property and the material lists no slot for it.
- `declared-default`: The slot is null in the game's own data and the shader declares a default for it, so the shader reads that default, as the original does.
  The slot is not `unset`, because the material lists it.
- `not-loaded`: The slot names a texture the engine did not send (kind 5 of `DisunityTextureSlot`, `TextureRef::Unsent`): the managed engine could not carry it as a texture (a destroyed or empty texture, an array, cube or volume it cannot store, or a kind of texture the world does not hold), the importer did not build it (a cube, a volume or a 2D texture it rejects, or a reference the plan does not hold), or it is a render texture that was destroyed.
  The shader reads its declared default in its place.
- `exists-not-loaded`: The slot names a texture the engine knows of and the host holds no pixels of: the engine stored none, the upload failed or went over the storage budget, or the render texture no longer exists.
- `unwritten-render-texture`: The slot names a render texture that nothing has written: no draw, copy or blit of a frame the host built wrote it, so it holds the zeros it was made with.
  A render texture bound as a storage image for a kernel is counted as written, because the host cannot see the kernel's writes.
  The host supplies this texture itself and `--stand-ins magenta` leaves it alone, so it reads zeros.
- `unsupported-format`: The slot names a stored texture whose format the host does not decode.
- `undeclared`: The shader reflects a slot it does not declare and no layer sets it, so the grey of the slot's dimension is read.

A draw whose declared property has no texture and no default the host supplies is left out and counted as `draw:MissingTexture`.

`unsupported-dimension` (a cube, 3D or array texture the engine cannot carry yet) is not reported separately.
The importer plans a texture it does not build (a `Cubemap`, a `Texture3D`, a `Texture2D` it rejects) as a slot marked `unsent` with a line in the plan's differences (`unity-import` `player_materials.rs`, `plan_material`), the managed loader sets it as an empty texture, and `WorldGraphics.TextureOf` sends a named texture it cannot carry as kind 5, `TextureRef::Unsent`.
A slot that is null in the game's data is kind 0, `TextureRef::None`.
The world reads the two alike everywhere except in this report, so `declared-default` says the game left the slot null and `not-loaded` says a texture the game names did not arrive.
The managed engine's own count of what it could not send, with the reason for each property, is printed as `UnsentTextureSummary`.

### Seeing the globals no draw read

The report also lists the texture globals that were set and that no draw's slot asked for, next to the stand-in lines:

```
[gpu] globals set and never read by a draw: <name>, <name>, ... and N more
```

A name is set when the world's globals hold a texture under it at a frame (the names of the `globals at a flush` listing) or when a command of the frame sets one with `SetGlobal`.
A name is read when a planned draw's texture slot looked it up in the layers, whether or not a layer held a texture under it, so a slot that falls back to the grey `undeclared` stand-in counts as a read of its own name.
The names are exact and the engine translates none: a game that sets `Foo` while its shader reads `_Foo` shows `Foo` in this line and `_Foo undeclared` in the stand-ins line.
The line names the first 30 names in name order and counts the rest, and covers the whole run, so `--report-each-frame` prints the run's names so far.
`GlobalReads` in `offscreen/stand_ins.rs` holds the two sets; the lookups come from `OffscreenOperations::looked_up_textures`, which render-program fills when it binds a draw's texture slots.

`--stand-ins magenta` makes every stand-in texture the host supplies read as magenta, RGBA (255, 0, 255, 255), in its own dimension (2D, cube, array and 3D), so a capture shows which draws read stand-ins.
`--stand-ins declared`, the default, leaves the declared colours.
The look is `PlayerLaunch::stand_ins`, which the run's `GpuLaunch` carries into `Settings::with_stand_in_look`.

### The screen

Old World ends each camera's frame by drawing into the camera target, which is the screen.
A `Settings::screen` of a size makes `CameraTarget` mean that screen when the camera set up has no texture (or none is set up, as in a `SetRenderTarget` on its own): `resolve_target` gives the screen's one texture, for colour and for depth alike.
It is a held target of the frame under the name `ids::is_screen` recognizes (the top bit, bit 60 and neither the temporary nor the scratch bit, which no world render texture id has) and is described as a camera's render texture is: `GraphicsFormat` 4 (RGBA8, sRGB) with 24 depth bits, one mip of the screen's size.
A depth target that names the camera target, or a binding that names no depth target, therefore attaches the screen's own depth buffer.
It persists from frame to frame like a render texture of the world, so a draw with no clear keeps the colour and the depth an earlier frame left, until `set_screen` gives another size, which makes it again, empty, and every snapshot names it so it is never given back to make room.
A target that names the camera target with no screen set is left out as `UnboundTarget`, as it was before.
A `CameraRender` record always names a render texture, so it never draws into the screen; a camera with no texture draws through its command buffers.

`OffscreenHost::screen_written` says whether work (a frame or a flush) has drawn into the screen's colour since the last `OffscreenHost::take_screen_written`, which also starts the count again.
`PlayerGpu::set_screen` sets the size, and `PlayerGpu::frame_for_viewer` and `PlayerGpu::frame` number the screen with the frame whose work or flush drew into it (`PlayerGpu::drawn_screen`).
Tests read the screen back with `OffscreenHost::read_screen`, the copy and read a `ReadPixels` uses, in a submission of its own that it waits for.
The result is a `ScreenImage { width, height, rgba, frame }`: 8-bit RGBA as the sRGB texture stores it, rows top first (the GPU copy is bottom first, so its rows are turned over).
A refusal (no screen, no room in the budget for the staging, a copy that cannot be read, a format that is not four 8-bit channels) is its reason.
Unlike a `ReadPixels`, a read of the screen gives no meshes or textures back to make room for its staging.

The window sets the screen size before the scene loads and applies a resize before advancing the managed update, so a synchronous flush in that update writes the current screen allocation.
The player offers the screen texture to the viewer as `GAME_SCREEN` in the same submission as the admitted operations that write it.
The viewer samples that bottom-first sRGB texture in its opaque pass, then draws the game's UI over it.
A later frame can display a screen written by an earlier submitted viewer frame or by a synchronous player flush; replacing a pending writer discards its write.
Until submitted work writes the current allocation, the stock scene stays visible.
Ordinary window frames require no screen readback or sprite upload.
A requested window capture reads the composed output, including the UI, and holds the game until it arrives.

### Wiring a frame

`PlayerGpu` (in `gpu_player.rs`) owns a device, the `OffscreenHost` on it and the counts of a run.
`run_player` makes one and fails naming the missing adapter or device when the machine has none, registers its flush handler with `DisunityHost::set_gpu_flush` before the scene loads, and after each advance calls `PlayerGpu::frame`, which takes `graphics_snapshot()`.
A frame whose snapshot holds no work returns at once and leaves the device alone, so the run is the one it was before.
With work, the records are adapted, built, admitted against a frame of the authority, encoded, submitted and waited for; the counts of records, dispatches, skips and failures print at the end as `[gpu]` lines.

The flush handler reads meshes, textures and samplers through `GpuFlush` (`mesh_channels_since`, `player_texture` and `texture_sampler`), as `PlayerGpu::frame` reads them through `DisunityHost`, so a mesh or texture the first flush names is read for that flush.
`tests` in `gpu_flush_tests.rs` runs a managed fixture whose flush reads a stored texture, its sampler and a stored mesh with no frame before it.

`play_player_window` (the windowed built player, used when `play --player` gets `--size`, `--capture` or `--key`) runs the same offscreen work and answers the same synchronous flushes as the headless run.
It makes the viewer's `ViewerStart` first and builds its `PlayerGpu` on the one device the viewer makes through `ViewerStart::gpu` (`PlayerGpu::shared`; a machine whose device cannot be made fails the run naming the reason), installs it before the scene loads, calls `PlayerGpu::frame_for_viewer` once per update after the frame's meshes are refreshed and before the frame's UI and textures go to the viewer, and prints the `[gpu]` report when the window closes.
`frame_for_viewer` builds the frame's operations and returns them without submitting or waiting; the driver hands them to `RetainedHost::admit_offscreen`, and the viewer runs them and samples any exported `GAME_SCREEN` in its opaque pass in the same submission, through the same authority.
Storage the GPU has finished with is freed by the non-blocking `OffscreenHost::collect_completed`, and a frame that must give meshes, textures or render textures back for room collects only what the GPU has finished (`Settling::Poll`): the bytes of the viewer's work still in flight stay counted, and an allocation they leave without room is refused as over budget until a later frame collects them.
The synchronous paths (`PlayerGpu::frame`, which only a run with no viewer uses, and a flush) wait (`Settling::Wait`).
The window samples a frame's screen in that same submission.
A script's synchronous flush still runs and waits in its own submission, between the viewer's frames.
A script's `Texture2D.ReadPixels` after a `Graphics.Blit` therefore fills its texture, and `Apply` stores the new texels under a higher generation, which `PlayerTextures::refresh` and the UI's image generations pick up in the same frame.
What the player asks of an adapter (storage images, buffer size from the storage budget, block compression, filterable 32-bit floats) is one function in `gpu_device.rs`, used by the headless player, the compute census and the window.
Each startup snapshots host `MemAvailable` once, so repeated device requests in that startup use identical inputs while retaining the adaptive budget policy.
The original games' driver (`play_game_window`) draws 2D worlds through `RetainedUpdate::World`, has no player GPU and runs no offscreen work.
Both drivers share the window script (`window_script.rs`): scheduled keys and the step of each game frame, requested captures with the hold that keeps a capture to its frame, and the frame after which the window closes.
A run with no viewer (no `--size`, `--capture`, `--key` or `--screen`, or one that only reads targets) keeps the player's own device and its submit-and-wait path (`gpu_player/frame.rs`) to measure the game and its GPU work; every run that shows a frame goes through the retained host (see "Windowless runs").

`play_player_window` draws built-player mesh objects and screen-space overlay UI
through the retained scene renderer.
Text coverage and image quads retain native
hierarchy order and pixel clip bounds.
UI replacements track draw changes, image
texture generations and viewport size; unchanged frames reuse the overlay.
An image whose identity has no available sprite texture is left out of the UI
frame.
The driver names each unavailable identity once on stderr as
`UI image <id> is left out: <reason>`: a missing scene texture, an identity the
managed host has not stored, or a texture conversion failure.
A texture stored
later becomes visible without changing the UI draw.
A UI draw whose material has a shader of the game's own (a shader with a source
other than `UI/Default`) is drawn by that shader: the player records the runs of such draws
as offscreen work after the game's own work, into a layer texture that is the size of the screen while the runs fit it and grows (up to
the device's texture limit) when they do not, and
the overlay shows that texture where each run sorts. The run report counts the draws by shader
name in one line, `UI draws with a game material by shader (last frame)`. A run is left out only
when its size cannot be had on the device (a side beyond the texture limit, or a texture
already full at that limit) and is named once as
`UI material <id> (shader <name>) is left out: its run of <w>x<h> pixels does not fit the layer atlas, which grows to at most <n> pixels a side`.
Draws on the engine's own look (a material with no shader source, or `UI/Default`) are
unchanged.
Other images, text and 3D
objects continue to draw; missing text coverage remains an error.
The renderer's
explicit-texture overlay API still rejects unresolved identities.
Ordinary 2D scene quads and world-space UI remain outside this retained path.

The window opens at a fixed default view, which mouse navigation can move.
On the first frame
that shows mesh objects and presents a perspective camera it shows the game camera, and from
then on it follows that camera: a frame whose camera pose or lens changed sends it to the
viewer, and the game moves the view, as in its own player.
While frames with objects present
no camera, or one the viewer cannot draw (an orthographic one, for example), the window keeps
its view and tries again on the next such frame, and the driver names each reason once on
stderr.

### Render textures in the UI

A `CanvasRenderer` over a `RenderTexture` (Old World's minimap) reaches the window and a screen capture as a `UiTexture::Target` draw. Each update, `PlayerGpu::ui_offers` asks the offscreen host for the device texture of every render texture the UI's draws read (`OffscreenHost::render_texture_for_ui`: the texture, if a frame made one, and whether a frame the state built wrote it) and for the sampler the world holds, then `PlayerUi::offer_targets` turns each answer into a `UiTarget` for `RetainedHost::show_ui_textures` or leaves the draw out. A texture no frame has written draws transparent and is named once ("shows nothing until a frame draws it"); a destroyed texture or one the overlay cannot sample is left out and named once with the reason (`destroyed`, `not-2d`, `multisampled`, `not-sampled`, `depth-or-stencil`, `not-filterable`). A run with no GPU work leaves these draws out with `no-gpu`. The viewer runs the update's offscreen operations first and samples the texture in the same submission, so the minimap shows what the camera drew that frame. The read counts as naming the texture for storage reclaiming, so a shown texture is the last one given back. Tests: `offscreen/tests/ui_read.rs`, `offscreen/tests/reclaim.rs`, `gpu_player/ui_offer_tests.rs`, `player_ui_tests.rs` and, in a window on a real adapter, `retained_screen_tests.rs` (the fixture `ui-target-draw.fixture`).

### Lighting the retained scene

The retained scene is lit by the World's main light, chosen in `scene_light.rs`.
These are the
engine's choices:

- The main light is the first directional light the prepared frame lists, in component order.
  `RenderSettings.sun` and the ambient settings live only in managed memory and are not read.
  Without a directional light the scene stays unlit, as before.
- The scene sun shines back along the light's forward axis.
  Its irradiance is the light's
  intensity, at most 10.
  The scene sun has no colour, so the light's colour is not used.
- The ambient light is white, 0.3 from above and 0.15 from below.
  The scene asks for no
  atmosphere, so the game's clear colour stays the background.
- Offscreen draws (camera renders, renderer lists, command-buffer draws) start from an ambient
  default the engine supplies: `unity_SHAr`, `unity_SHAg` and `unity_SHAb` are `(0, 0, 0, 0.3)`,
  `unity_SHBr`, `unity_SHBg`, `unity_SHBb` and `unity_SHC` are zero, and `unity_SpecCube0_HDR` is
  `(1, 1, 0, 0)`.
  The 0.3 is the same constant as the retained scene's ambient from above
  (`AMBIENT_SKY_IRRADIANCE`).
  This is an engine default for a frame in which nothing sets
  these names, and nothing more.
  A world whose scene wrote its RenderSettings gets its ambient
  light from the scene's probe instead (`add_environment`); the default is flat and stands for a
  world that wrote none.
  `(1, 1, 0, 0)` for `unity_SpecCube0_HDR` is a neutral decode chosen without a
  measured convention.
  Like the main light, the values sit in the globals the frame starts from,
  so a world global or a `SetGlobalVector` of the same name replaces them; without them the
  scene shaders of Old World's RomeTurn42 camera output black, because every one of these names
  packed as zero.
- Shadows exist only when the first frame's main light casts them, because the viewer chooses
  its shadow resources when it starts.
  They use three cascades split 10 m, 40 m and 150 m from
  the camera, a 2048-texel map, a depth bias of 0.0005 and a normal bias of 2 cm.
  The light's
  own bias values are not used, since the engine defines no unit for them.
  A light that starts
  casting shadows later lights the scene without them.
- Each frame sends new scene effects only when its lighting changed.
  After a frame has been
  lit, a frame without a directional light keeps the last sun's direction with no irradiance.
- The presented camera's `renderShadows` turns those shadows on and off: each frame publishes
  the camera with the matching shadow policy when the setting changed, at the last accepted
  pose when the camera did not move.
  Until the game's camera is shown that pose is the one the
  window opens with, so the setting applies even when the game presents no camera.
  A camera
  without camera data keeps the published policy.
  A viewer
  that started without shadows ignores the setting, since it cannot add shadow resources.

## Tests

The library's synthetic retained-player UI test runs the managed host and captures
windowless frames.
It requires .NET 8 and a GPU adapter and checks image bytes,
text, clipping and unchanged-frame UI update counts without opening a window.
`retained_gpu_tests.rs` plays the portable readback fixture the same way with the offscreen work on a device: a script blits a colour with no material, reads it back into a `Texture2D` and shows it in a UI image, and the captured frames show the first colour and, after a later `Update`, the second.

`retained_screen_tests.rs` runs a script that clears `CameraTarget` to yellow.
It checks every pixel of the first and fourth window captures and that the update with the script's clears numbered the screen.
A separate managed readback fixture checks that the first update after a resize flushes into the new screen allocation before any viewer submission.
`offscreen/tests/reclaim/pending_completion.rs` holds completion bookkeeping after GPU execution and checks that an evicted texture remains charged until completion is recorded, then releases exactly its bytes and admits the next upload.
Viewer and compositor tests cover row order, UI ordering, dropped writers and resize.
`screen_run_tests.rs` runs the fixtures through `play_player_windowless`.
The first fixture checks every pixel of frames 0 and 2.
A second fixture (`screen-ui.fixture`) puts an overlay canvas with one green rectangle over the yellow screen, and the test checks every pixel of frame 1, the rectangle in the upper left and yellow elsewhere.
A third (`screen-count.fixture`) clears the screen to a different colour and moves the rectangle in every update, so the captures of frames 1 to 3 show which game frame drew them.
A frame with no drawn screen and a frame past the run are refused.
A fourth (`screen-refused.fixture`) asks for a clear that no budget has room for in its second frame, and the run ends at that frame with an error naming the refusal and has written only the file of frame 0.
`retained_screen_tests.rs` checks that the file of a capture exists while the run still has frames to play, and `window_script_tests.rs` that a delivered capture reaches every file asked for its frame and that a file whose frame never arrived fails the run.
`window_script_tests.rs` checks without a game or a viewer which updates advance the game, hold it or end the run.
`offscreen/tests/draw_mesh/screen.rs` draws a mesh over the top half of the screen on a real adapter and reads the top rows back first, then draws again with no clear (what the first left stays), and resizes the screen.
`offscreen/tests/screen.rs` pins the mapping without a device: the camera target binds the screen's colour and depth when a screen is set, the textures are described as held targets of its size, and without a screen the same records are still left out as `UnboundTarget`.

`scene_light` unit tests pin the main-light choice, the sun direction of a turned light, the
irradiance limit, the fixed shadow settings, when a frame sends new effects, which shadow
policy a camera's `renderShadows` publishes and, through sequences of frames, the pose it is
published at, from the window's opening pose, and that a camera the viewer refuses is sent
again.
`camera_start` unit tests pin when the window starts at a game camera that appears late or
after a refused one, that it follows the camera as it moves and sends an unchanged one once,
and that each reason it waits is reported once.
`scene_tables::tests::lit` starts the viewer as `play_player_window` does and checks on a GPU
adapter that a shadow-casting sun draws into the shadow cascades, that no sun, or a sun
without shadows, draws none, and that a camera with `renderShadows` off stops the started
shadows from drawing.

`tests/play_window.rs` plays all three games in a 480x270 window and checks the captured pixels.
It needs an X display; run it as the lavapipe CI job does:

```sh
env -u WAYLAND_DISPLAY xvfb-run --auto-servernum cargo test -p worldrunner-play --test play_window
```

The test starts each game in a child process without `WAYLAND_DISPLAY` itself.
Do the same for the `play` command under `xvfb-run`: with a Wayland session in the environment the window opens there instead, and a window the compositor never shows is never presented.

The offscreen adapter's tests build synthetic worlds and serialized files and need no game data.
The GPU tests run a camera render, a blit of every pass and a copy through the adapter and render-program on a real adapter and compare predicted pixels; run them on lavapipe with:

```sh
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json WGPU_BACKEND=vulkan cargo test -p worldrunner-play --lib offscreen -- --test-threads=1
```

`tests/draw_mesh.rs` (in `offscreen/tests`) binds and clears a render texture and draws synthetic meshes with `DrawMesh` through the flush, with a shader that places its vertices by `unity_ObjectToWorld` and `unity_MatrixVP`: a quad over the left half of clip space under an identity view and projection, the same quad moved by the object matrix, an orthographic 0 to 1 projection, sub-mesh 1 of a two-sub-mesh mesh, and a two-pass shader whose second pass blends over the first.
Each test predicts the 8-bit texels from the matrices, colours and blend factors.
MeshUnavailable, EmptyMesh, NoSuchSubmesh, NoSuchPass and UnboundTarget are checked by name.
Reclamation tests also check that a mesh named only by DrawMesh stays resident, that eviction removes its metadata, and that a later draw reloads it.
