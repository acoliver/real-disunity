# Managed ABI and portable fixture

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
   `native.g.rs` and `NativeLayouts.g.cs` (`NativeAbi.Version`), so the runtime and
   managed checks never repeat it. If those files drift too, copy them the same way.
   Revision 5 retains revision 4 batch construction, object wake and body position
   constraints, and moves canvas scale out of the Canvas record into
   `disunity_canvas_scale_read`, `_write` and `_factor`. Native and managed hosts
   require revision 5. Rebuild all peers.
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

The FFI build also generates `crates/disunity-runtime/src/native.g.rs` from the
same C-layout definitions and rejects checked-in drift. Maintain that file with
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

[Unity documents Instantiate](https://docs.unity3d.com/ScriptReference/Object.Instantiate.html)
as copying children and components. Under the pending-visible destruction contract,
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
failures remain errors. Coroutine joins, end-of-frame and custom yields are not
supported.

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
object constructors, component destruction, body forces, broader
colliders/queries, materials/resources, script tile/grid APIs, application/scene controls,
the CanvasScaler component, bold/italic text, rich text markup, best fit and generic
AddComponent<Text>. Three pinned external runtime-script trees compile unchanged
against Disunity in projects outside this repository. This establishes source
compilation, without claiming game execution or content import.

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
crates/disunity-runtime/src/native.g.rs, NativeLayouts.g.cs, every NativeBindings
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
