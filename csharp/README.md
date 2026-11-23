# Managed ABI and portable fixture

Contributors must follow [CONTRIBUTING.md](../CONTRIBUTING.md) and agents must
follow [AGENTS.md](../AGENTS.md). Player assembly loading can execute listed Unity
packages, and tools read assembly metadata. These capabilities do not authorize
inspection of Unity implementation. Inspect only permitted game-specific material;
stop uncertain manual inspection for Andrew's guidance.

The generated imports `Disunity/NativeBindings.g.cs`, `Disunity/NativeBindings.2.g.cs`
and so on, and the records in `Disunity/NativeRecords.g.cs`, are checked in.
The native FFI build generates them in Cargo's `OUT_DIR`, splitting csbindgen's
partial import class from its namespace-level records. It removes blank import
lines and packs the import declarations, in generated order, into as many parts
of the partial class as keep each file under 750 lines. It compares **all bytes**
in every file, including complete signatures, calling conventions, and struct
fields. A mismatch fails the build;
normal builds never update the checked file. All ABI source files and the
checked bindings are Cargo rerun inputs. Keep LF line endings as required by the
repository attributes.

## Owned asynchronous file reads

The native `DisunityHost` installs one bounded file reader before managed execution
and removes it on host disposal. `Unity.IO.LowLevel.Unsafe.AsyncReadManager.Read(path,
offset, size)` submits a regular-file region without borrowing a target pointer.
The worker owns its buffer through completion; `ReadHandle.GetData()` copies the
terminal bytes into a new managed array. No world, device or frame pump is required.
Submission outside an installed native runtime host throws.

The reader admits at most 64 retained requests and 64 MiB of reserved native result
storage, with at most 16 MiB per read and 4096 UTF-8 bytes per path. Completed results
keep their reservation until disposed. Disposing an in-progress read invalidates
all copies of its handle and requests cancellation; its reservation remains held
until the worker returns. Admission failures throw without creating a handle.
Offsets and sizes are bytes. Negative arguments and overflowing ranges fail.

`Identity` names a request within its host session. `Status` reports `InProgress`,
`Complete`, `Failed` or `Canceled`; `Complete()` waits for a terminal state.
`GetBytesRead()` is zero while pending and the actual transfer count afterward.
A short region is `Failed` with its transferred prefix and `Error` diagnostic;
a missing file fails with zero bytes. A zero-byte read still checks that the file
can be opened. `GetData()` rejects pending reads. Invalid or disposed handles have
failed status, zero readable bytes and an explanatory error.

Cancellation is best effort at worker checkpoints between reads of at most 64 KiB.
It does not interrupt a blocking OS open/read. Status remains pending until the
worker acknowledges cancellation. A fully transferred range or an I/O error wins
over late cancellation. Native host disposal detaches callbacks, drains borrowed
calls, cancels remaining work and joins the worker. Old session handles cannot
access a later host's requests. Serialize disposal against other operations on the
same handle; `Cancel` can run while another thread waits in `Complete`.

Raw `ReadCommand` submission throws `NotSupportedException`: those descriptors
specify neither capacity nor asynchronous buffer ownership. `ReadHandle.JobHandle`
also throws, because the current synchronous job scheduler cannot represent pending
native I/O. Use `ReadHandle.Complete()` and owned results. Persistent `FileHandle`
opening is unsupported; a default file remains absent.

The focused `file_reads` tests in `worldrunner-disunity-runtime` exercise a real
fixture region, channel-controlled pending and cancellation states, bounded
admission, partial EOF results and native-to-managed Release execution. They do
not use sleeps or timing thresholds.

## Explicit binding maintenance

After an intentional ABI change:

1. From the repository root, run
   `cargo build --locked --jobs 2 --package worldrunner-disunity-ffi`.
2. If bindings drift, the failure prints the exact generated file path. Review
   its diff against `csharp/Disunity/NativeBindings.g.cs`, including signatures
   and layouts. Copy that generated file to the checked file **explicitly**:
   `cp <printed-OUT_DIR>/NativeBindings.g.cs csharp/Disunity/NativeBindings.g.cs`
   (PowerShell: `Copy-Item <printed-OUT_DIR>/NativeBindings.g.cs csharp/Disunity/NativeBindings.g.cs`).
   Review and copy every other drifted file the build names from that same output
   directory, including `NativeBindings.2.g.cs` when imports move between parts and
   `NativeRecords.g.cs`. Every generated file has a byte-for-byte drift check.
   Do not guess an old build directory: use the path from this invocation.
3. Update `crates/disunity-ffi/disunity_interface.json` for new exports and
   record the current ABI revision. Removing or changing an export requires a new
   revision in `DISUNITY_ABI_VERSION`. The build generates that number into
   `native.g.rs` (and its numbered parts) and `NativeLayouts.g.cs` (`NativeAbi.Version`), so the runtime and
   managed checks never repeat it. If those files drift too, copy them the same way.
   Revision 11 carries distance mode (0 coverage, 1 fill, 2 outline, 3 glow),
   atlas-pixel spread, physical-pixel effect radius and linear effect RGBA in the
   152-byte `DisunityUiDraw`. Distance draws require textured affine text quads.
   The record also carries optional BL/BR/TR/TL linear RGBA corner multipliers in
   `DisunityUiDraw`, independently of its common tint. Its presence flag is 0 or 1,
   and all channels must be finite and in 0..=1. Revision 12 adds the camera target
   buffers calls (`disunity_camera_target_buffers_write` and `_read`) and the buffer
   arguments of `disunity_offscreen_setup_camera`. Revision 16 adds `auto_generate_mips`
   and a `padding` word (which keeps `generation` 8-byte aligned) to
   `DisunityRenderTexture`, and `disunity_offscreen_generate_mips`, which records a
   generate-mips request for a render texture with more than one level (offscreen
   kind 19). Revision 17 adds `depth`,
   `graphics_format`, `dimension` and a zero `reserved` word to the stored-texture
   record `DisunityTextureInfo` (48 to 64 bytes): a 2D array, a cube or a 3D texture
   states its layer count, `GraphicsFormat` and shader dimension code (2 2D, 3 3D,
   4 cube, 5 2D array), and a writer that sets none stores a plain 2D texture. Revision 18 adds
   `disunity_offscreen_enable_scissor`, `disunity_offscreen_disable_scissor` and
   `disunity_offscreen_set_depth_bias` (offscreen kinds 20 to 22) and the shadow
   caster query `disunity_shadow_query`.
   Revision 19 adds `disunity_environment_read` and `disunity_environment_write`, which move the
   world's environment lighting (the ambient light and the default reflection, with a set flag
   for each section) in the 224-byte `DisunityEnvironment`, and carries the environment in the
   graphics snapshot. Revision 20 adds the property block of one material slot of a mesh
   renderer: shader property scope 4 and `disunity_mesh_renderer_slot_block_target`,
   which gives the target of a renderer's slot. Revision 21 adds texture slot kind 5 (a texture
   the data names that the engine did not send), which every texture slot, environment cube and
   offscreen source accepts with texture identity 0; the world reads it as no texture, and the
   graphics snapshot wire version is 17 to carry it. Revision 22 adds
   `disunity_world_advance_teardown`, an advance that runs only the `OnDisable` and
   `OnDestroy` messages and the asset cleanup that destruction queued. Revision 23 removes
   `disunity_offscreen_set_global`, `disunity_offscreen_set_global_keyword`,
   `disunity_offscreen_set_global_vector_array` and `disunity_offscreen_set_global_matrix_array`
   and adds `disunity_offscreen_set_global_texture`: a command buffer's global value, array or
   keyword is a direct write of the world's global state (`disunity_shader_*`), and only a global
   texture set to a temporary render texture is recorded. Revision 24 removes
   `disunity_mesh_renderer_slots_write` and `disunity_mesh_renderer_slots_read`: a mesh
   object slot draws from the slot's world material alone. Revision 25 gives
   `disunity_canvas_renderer_texture_write` a texture kind (0 a stored image identity, 1 a
   render texture the world holds) and adds `texture_kind` and a padding word to
   `DisunityUiGraphicInfo` (16 to 24 bytes) and `DisunityUiDraw` (152 to 160 bytes); the quad
   texture of a draw whose kind is 1 is the render texture id. Revision 26 adds the
   `world_material` argument of `disunity_canvas_renderer_material_write` (the world material
   of main slot 0, 0 for none) and the `material` and `material_generation` fields of
   `DisunityUiDraw` (160 to 176 bytes). Revision 27 adds `disunity_script_execution_order`, the execution order of a script (a 32-bit signed
   integer, 0 by default), which a lifecycle phase uses to deliver messages to the lowest order first and,
   for equal orders, in registration order. Revision 28 adds `disunity_skinned_renderer_write`, which names the mesh
   identity and the root bone of a skinned mesh renderer with the root bone's bind pose (16
   column-major floats); the renderer is drawn where the root bone's matrix times that pose
   puts it, and one with no root bone is left out. The graphics snapshot, at wire version 19,
   carries the count of skinned renderers it leaves out for want of a root bone. Revision 29 adds
   `disunity_offscreen_draw_mesh_batch` and the 64-byte `DisunityDrawBatch` (offscreen kind 23): a
   draw of the instances a shader reads from a window of a compute buffer, with the game's metadata
   words by property name and the visible instance indexes, carried in the graphics snapshot from
   wire version 20. Revision 30 makes the stored-texture `dimension` 2 the only spelling of a 2D
   texture: `disunity_texture_store` refuses a record whose `dimension` is 0, and the host reads
   only 2, 3, 4 and 5. Revision 31 removes `disunity_offscreen_take`, `disunity_offscreen_detail`
   and `disunity_compute_bindings_read`, the test-only reading calls that ran beside the graphics
   snapshot, and the fields of `DisunityOffscreenWork` that only they wrote (`shader`, `kernel`,
   `groups_x`, `groups_y`, `groups_z`, `bindings`; 104 to 72 bytes). A host reads the frame's work,
   the material copies its records name, the global state they replay from and the values each
   dispatch ran with in `disunity_graphics_snapshot`, whose wire version 21 puts the work list
   first in the body so a reader that needs only the work stops after it. Native and managed hosts reject
   mismatched revisions before creating a world or accessing versioned records.
   Managed startup reads `disunity_abi_version` and uses the generated
   `NativeAbi.VerifyRevision` check. It throws `DisunityAbiRevisionException`,
   whose message and `Expected`/`Actual` properties identify both revisions;
   no world-associated diagnostic is needed. Rebuild all peers.

4. Rerun the native build and
   `cargo test --locked --jobs 2 --package worldrunner-disunity-ffi`.

No environment switch bypasses the comparison or regenerates tracked files.
All blittable C# ABI structs are generated from Rust, not hand-maintained copies.

## Portable managed verification

With Rust and the .NET 8 SDK available to the invoking command, run
`cargo xtask ci-managed`. This pipeline checks the generated native ABI **before**
compiling the portable C# fixture. The generic lifecycle test independently uses
the same native-first ordering:

```text
cargo test --locked --jobs 2 --package worldrunner-disunity-runtime --test managed
```

After the native gate passes, that test runs the dependency-free
`Disunity.Tests` console checks against the compiled managed assembly: generated
parameter and result types, entry points, native image, calling conventions,
and record field types, offsets, and sizes. Native
and runtime Rust layout tests independently check sizes, alignment, and offsets.

The managed test suite builds `Disunity.IdentifierTests` with warnings as errors
and runs its CPU-only subprocess checks in a separate integration target, without
booting a native host. The fixture covers persistence across fresh processes, scope
switching, deletion reset, barrier-synchronized concurrent creation, invalid stored
GUIDs and storage errors.

The managed console tests also derive a behaviour from the separate PortableFixture
assembly and export native state after each step. Commands from Start, Update,
and LateUpdate must be visible immediately; Start must run only once.

The triangle fixture and two failure fixtures verify 100
load/step/export/unload cycles with deterministic native object state, reused
world slots, and zero native worlds and managed GCHandle tokens after each
unload. It also tests missing fixtures, duplicate hosts/worlds, and active-host
cleanup. The host receives native library, managed assembly, runtimeconfig, and
bridge type paths explicitly through `ManagedHostConfig`; the managed resolver
uses the native image selected by that host. SDK discovery settings belong to the
invoking build/test environment, not engine policy.

`crates/disunity-runtime/tests/original_scenes.rs` plays the original games from
their scenes. It fetches the three games with `original-games`, writes their
plans with `unity-import plan`, compiles each game's scripts unchanged with the
`csharp/OriginalGames` projects, then runs `Disunity.SceneTests` against the
native library. Those tests check Unity's field binding rules on synthetic
plans, then load each game and play it: Pong's launch, paddle axes and scoring
through its scene-wired event, Breakout's prefab instantiation and tile walls,
and Snake's configured grid and tick rate, steering and growth.

`Disunity.Play` is the bridge the `play` command (`crates/play`) boots. Its `Run`
reads a request naming a scene plan, the compiled game scripts and font files,
loads the game assembly into the bridge's own load context so it binds the same
`Disunity.Engine`, and calls `DisunityHost.LoadScene`. The Rust host then
delivers keys with `DisunityHost::push_key`, advances the world and renders each
frame; `Unload` disposes the scene's templates and unloads the world. A request
with a `player` data directory starts a built Unity player instead: the player's
assemblies resolve in the same context through `PlayerAssemblies`, its startup
hooks run in Unity's phase order around the scene, and scripts in the plan are
found by assembly and full class name (`SceneLoadOptions.Assemblies`). When the
request names a `callback_cache` directory, `PlayerAssemblies` loads each listed
game assembly that passes `[MonoPInvokeCallback]` static methods to delegates from a
copy rewritten with Mono.Cecil (`PlayerCallbackRewriter`), so those delegates are made
once and kept (`NativeCallbacks.Keep`) and native code can call them after the game
has dropped them.
`Disunity.Layouts` writes a player's script layouts for `unity-import`.

CI defines Linux, Windows, and macOS managed jobs. A local Linux result does not
establish Windows or macOS compatibility.

## Native advancement and managed execution

`UnityEngine` names are implemented directly in `Disunity.Engine`. Hosting helpers
are in `Disunity.Hosting`; generated imports are in `Disunity.Native`. There is no
second managed object API. Object and script wrappers identify native storage.
Transform reads and Time properties query that storage rather than a managed cache.

Native advancement uses `advance` and `resume`. Native state selects Awake, enable/disable, Start, fixed updates and physics,
dynamic updates, LateUpdate and destruction. The host receives one owned work
record, returns from the ABI call, invokes managed code, then resumes the checked
ticket. Nested activation uses the same native dispatch stack. Invalid tickets,
invalid UTF-8, unsupported outcomes and revision mismatches are rejected explicitly.
A managed exception is returned as a failed callback and faults the native advance
after its teardown callbacks. There is no alternate execution policy.

The FFI build also generates `crates/disunity-runtime/src/native.g.rs` and numbered parts such as
`native.2.g.rs` from the same C-layout definitions and rejects checked-in drift.
Whole records are packed into files below 750 lines and re-exported by the first
part. Maintain all generated parts with
the explicit copy workflow above. Owned diagnostic buffer handles survive world
unload until freed; bounded copy-out reports the required UTF-8 byte count without
partial writes. Managed error translation frees its diagnostic buffer in finally.

`Object.Instantiate` allocates the native subtree first. Bounded clone rows identify
object and script mappings; managed wrappers and serialized fields bind before
native activation starts. Internal GameObject, Transform and script references,
arrays and lists remap to their cloned identities. External references retain
their identity. Nested Instantiate drains Awake and OnEnable before returning.
Constructor or field-binding failure discards the unpublished native subtree.

`DisunityHost.BeginConstruction` builds many objects whose scripts wake together, as
a Unity scene load does. Objects created in a construction take components and
scripts through the ordinary `AddComponent` calls, and fields bind freely: no
`Awake` or `OnEnable` runs and searches do not see the objects until `Activate`.
Activation runs every eligible `Awake`, then every `OnEnable`; `Start` follows before
the next update. Disposing an unactivated construction removes its objects, scripts
and native components. Clones and constructions share one unpublished mechanism in
the native world. Discarding a construction does not retire unrelated objects
already pending `Destroy`; they remain visible until frame cleanup. Native
`create_object_in` takes a `Construction`, never an object handle. The ABI rejects
an object handle passed as a construction, including a pending-destroyed object.
A pending object remains a valid hierarchy parent until cleanup. Parenting a
constructed child under it makes the child inherit retirement, suppresses its
lifecycle, and retires both at frame cleanup, matching the pending-parent clone
contract.

A construction that is never activated holds prefab templates, as a scene load does
for the prefabs its scripts reference. Its objects and scripts take part in nothing:
no messages, searches, rendering or physics. `Instantiate` of a template object or
component clones it into a new published object that wakes like any clone, with
references inside the prefab remapped. The clone's root is named after the original
with a `(Clone)` suffix, as in Unity. Disposing the construction destroys the
templates and keeps their clones.

`AddComponent` wakes a new script only when its object is active in the hierarchy.
It never changes whether the object is active; an inactive object's scripts wake
when it is activated.

`Object.Instantiate` copies children and components. Under the pending-visible destruction contract,
cloning copies every still-existing component, including pending components, and
remaps references before activation. An independent clone does not inherit the
source's pending destruction. A clone attached to a pending parent does inherit
that parent's retirement and cannot activate or tick. Capacity preflight counts
all copied components, including each required Transform.
`Transform.SetParent` and `GameObject.SetActive` also drain native lifecycle work.
Script messages are resolved by name, including private methods, without requiring
virtual overrides. Start may also return an IEnumerator, which begins synchronously on native Start delivery.

The build derives `NativeLayouts.g.cs` and native layout assertions from the C ABI
records. Its generated expectations check every record's size and field offsets;
Rust additionally checks alignment. Maintain the checked managed expectations
with the same explicit-copy procedure as the bindings. `UnityConsumer` references
only `Disunity.Engine` and compiles UnityEngine names with warnings as errors.
The managed console-test build includes that consumer, while architecture tests
check its dependency edges and reject duplicate APIs and JSON request transport.

## Contacts, coroutines, input and UI

Generated callback records carry native coroutine identity and contact data. Native
physics selects contact recipients and Enter/Stay/Exit order before dynamic Update.
Managed dispatch resolves one collider wrapper per native component and invokes
collision or trigger methods with typed arguments. Rigidbody2D properties and
box/circle colliders read and write the native physics components. Raycast returns
the nearest native hit; OverlapCircle returns the first native collider identity.
Native component clone rows bind physics and UI wrappers before script activation.

StartCoroutine checks native target eligibility before executing the iterator to
its first yield immediately. Native state
owns next-frame, scaled/unscaled time and fixed-update readiness. Managed code
executes iterator bodies, including nested iterators, and returns a typed wait for
the suspended native callback. Completion and cancellation dispose iterators.
MonoBehaviour.enabled delegates eligibility changes to native lifecycle scopes.
Deactivation, destruction and callback-failure cancellation notifications transfer
once. Unload releases the world even when an iterator disposer throws; disposal
failures remain errors. `UnloadWorld` destroys the scenes' objects and then the
DontDestroyOnLoad objects and assets, and runs each cleanup through
`disunity_world_advance_teardown`, so no `Update` or other frame message runs while
the world unloads. Coroutine joins, end-of-frame and custom yields are not
supported.

Loading a world installs Unity's main thread `SynchronizationContext`
(`UnitySynchronizationContext`), which unloading removes. Work posted to it from
any thread, including `await` continuations, runs on the main thread once a frame
in the native delayed-tasks phase, after Update coroutines and before LateUpdate.
`disunity_world_delayed_tasks` names the identity that receives that phase as
message 14. Work posted while the queue runs waits for the next frame, and a
`Send` from another thread waits until a frame runs it. Work that throws is logged
with `Debug.LogException` and the rest of the queue runs, as in Unity. Unloading drops
queued work, fails waiting `Send` callers with `ObjectDisposedException`, and drops
later posts. LoadWorld, Step and UnloadWorld must run on one thread, the main thread.

Host PushKey, PushAxis and PushFocus accept timestamped input. Input.GetKey,
GetKeyDown and GetKeyUp query native phase views. Fixed and dynamic edges are
independent. The host converts external seconds once to integer nanoseconds,
including the actual value represented by a float frame delta.

Register project keyboard axes before advancing the world with
`DisunityHost.RegisterInputAxis(ushort id, InputAxisDefinition definition)`.
Definitions contain Name, Negative/Positive, AltNegative/AltPositive, Gravity,
Sensitivity, Dead, Snap and Invert. IDs identify entries; replacing an ID resets
its values. No names are implicitly registered. Input.GetAxis reads the native
smoothed value; GetAxisRaw bypasses smoothing. Missing names throw ArgumentException.
Duplicate names select the greatest absolute value, with the lowest ID breaking
ties. PushAxis resolves the lowest matching ID. Smoothing advances once per
projection using unscaled elapsed time, independently for fixed and dynamic
views. Repeated reads never advance it. Simultaneous opposite keys cancel.
Focus loss clears held keys and raw/smoothed axis values. Registry state belongs
to the world and is released on unload. Mouse movement and joystick configuration
are not represented by this keyboard registry.

Canvas and RectTransform use native layout. SetViewport supplies presentation
extent. RegisterFont accepts explicit font bytes; CreateText attaches native text
using that selected font. No implicit replacement font is selected. Text.text,
fontSize and the style properties (font, color, alignment, lineSpacing, both
overflow modes, fontStyle, supportRichText, resizeTextForBestFit) update native
TextRun values. Unsupported styles and text coverage fail without changing the old
text. Canvas.scaleFactor reports the factor for the current viewport;
DisunityHost.ScaleWithScreenSize applies a CanvasScaler's scale-with-screen-size
settings. The test font consists of generated triangle outlines and
metrics, without external font data. Screen Canvas and rectangle resize, text
updates and cloned UI wrappers are exercised through production imports.

Object name/tag properties and active flags read and write native object state.
Active-object and component lookup preserves existing wrapper identities. Transform
exposes local position, scale and quaternion rotation, plus native world rotation.
Instantiate world-pose overloads apply their pose before Awake and OnEnable.
Vector2/Vector3 conversions, mutable coordinates, Vector2Int arithmetic, Quaternion
identity, Mathf, random values, logging and generic UnityEvent listeners are managed
value/delegate utilities. Keyboard held state delegates to native input. Nested
lifecycle callbacks inherit their suspended parent's fixed/dynamic input view.
Time.fixedDeltaTime reads the native fixed step.

Rigidbody2D.MovePosition submits a native target for the next nonzero physics step.
Physics derives movement velocity and applies its existing continuous collision
solver rather than teleporting. Invalid targets are rejected before mutation;
retiring a body releases its target.

ScriptableObject.CreateInstance creates a standalone native-owned script asset,
without a scene object or transform. Creation delivers Awake and OnEnable;
Object.Destroy suppresses further ordinary callbacks immediately. Pending scene
objects remain readable, writable, searchable and non-null until final frame
cleanup, after Update, coroutines and LateUpdate and before presentation. Cleanup
runs OnDisable then OnDestroy once in component order and drains nested Destroy
requests before retiring handles. Destroy(obj, delay) uses scaled world seconds;
duplicate requests retain the earliest deadline. World unload also retires live
assets. Assets do not receive frame-update messages.

Pending AudioListener, Light2D, GUILayer, FlareLayer and additional-camera-data
components remain readable, writable and cloneable until their owner's retirement.
Pending listeners are excluded from the active-listener count immediately, even
if re-enabled, so a replacement can become the single active listener before
cleanup. The zero/multiple-listener diagnostics count only non-pending, enabled
listeners on active published objects. Components attached beneath a pending
owner inherit its retirement; independent clones do not inherit it.

Rigidbody2D.Sleep, IsSleeping and WakeUp use native body state. Sleeping bodies
skip gravity and integration; collision impulses and explicit wake resume motion.

Sprite and SpriteRenderer now use native immutable content identities and renderer
state. Camera exposes its orthographic lens, clip planes, clear flags, background
colour, depth and enabled state; perspective projection is unsupported. Missing
surface includes
object constructors, delayed component destruction, body forces, broader
colliders/queries, materials/resources, script tile/grid APIs, application/scene controls,
the CanvasScaler component, bold/italic text, rich text markup, best fit and generic
AddComponent<Text>. The three pinned game script trees compile unchanged against
Disunity through `csharp/OriginalGames`. The scene and window tests described above
exercise their loading and play behavior; this does not establish arbitrary game
compatibility.

### Callback failures and dependency checks

Scene scripts and standalone script assets use the same native callback tickets,
nested scopes and resume path. A failed callback suppresses ordinary work and
attempts each required cleanup callback once, including live assets. Cleanup
failures do not replace the first diagnostic. Suspended ancestor pumps check
callback admission before resuming and receive an AbortedScope reason after a
nested failure. Cancellation disposal belongs to the current callback dispatch.

Position reads and writes are world space. Local position remains local TRS.
Foreign clones remain unpublished until bindings and serialized fields are
complete. Wrapper lifetime follows native subtree invalidation, including
inactive descendants without scripts.

DisunityException exposes Code and Reason. Stable reason tags and diagnostic
limits are generated from the ABI. Valid UTF-8 failure diagnostics are clipped
at a character boundary before native ownership, so oversized messages cannot
prevent failure teardown. The Rust host retains the full managed diagnostic.

Regeneration covers these checked outputs: NativeErrors.g.cs,
crates/disunity-runtime/src/native.g.rs and its numbered parts, NativeLayouts.g.cs, every NativeBindings
part and NativeRecords.g.cs.
Build first, inspect the generated output named by the drift failure, copy that
artifact explicitly, and rebuild until every drift check passes. Builds do not
rewrite these files. Managed imports and records are checked first, followed by
reasons, native records and layouts.

Dependency regressions inspect cargo metadata's resolved runtime edges,
evaluated MSBuild project/package references, resolved assembly paths and the
built consumer/engine assembly references. Imported managed references and
aliased native dependencies have negative fixtures. Source policy searches are
additional checks rather than dependency-graph evidence.

Rigidbody2D.constraints supports FreezePositionX, FreezePositionY, FreezeRotation,
FreezePosition and FreezeAll with Unity's flag values. Position masks constrain
integration, MovePosition, continuous sweeps, impulses and penetration correction.
The freezeRotation property preserves the position constraints.
