# Managed ABI and portable fixture

`Disunity/NativeBindings.g.cs` is checked in. The native FFI build generates a
separate file in Cargo's `OUT_DIR` and compares **all bytes**, including complete
signatures, calling conventions, and struct fields. A mismatch fails the build;
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
   Do not guess an old build directory: use the path from this invocation.
3. Update `crates/disunity-ffi/disunity_interface.json` for new exports and
   record the current ABI revision. Revision 2 removes the old direct frame-step export.
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

Revision 2 replaces the direct frame-step export with `advance` and `resume`.
Native state selects Awake, enable/disable, Start, fixed updates and physics,
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
GetKeyDown, GetKeyUp and the Horizontal/Vertical axes query native phase views.
Fixed and dynamic edges are independent. Focus loss releases native held keys.
The host converts external seconds once to integer nanoseconds, including the
actual value represented by a float frame delta.

Canvas and RectTransform use native layout. SetViewport supplies presentation
extent. RegisterFont accepts explicit font bytes; CreateText attaches native text
using that selected font. No implicit replacement font is selected. Text.text and
fontSize update native TextRun values, and unsupported text coverage fails without
changing the old text. The test font consists of generated triangle outlines and
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
Object.Destroy queues OnDisable and OnDestroy for frame-end retirement. World
unload also retires live assets. Assets do not receive frame-update messages.
Rigidbody2D.Sleep, IsSleeping and WakeUp use native body state. Sleeping bodies
skip gravity and integration; collision impulses and explicit wake resume motion.

Missing surface includes Sprite/SpriteRenderer, object constructors, component
destruction, body forces/constraints, broader colliders/queries,
Camera/material/resources, tilemaps, application/scene controls,
CanvasScaler/camera binding, Text color/alignment/font reassignment and generic
AddComponent<Text>. External pinned runtime scripts were compiled unchanged in
projects outside this repository, with only a Disunity reference. One project
compiles; the other two currently report only missing Sprite/SpriteRenderer types,
which belong to the presentation slice. Later diagnostic phases can reveal more
unsupported members. No game execution or content import is claimed, and no
placeholder component claims those missing capabilities.

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

Regeneration covers four checked outputs: NativeErrors.g.cs,
crates/disunity-runtime/src/native.g.rs, NativeLayouts.g.cs and NativeBindings.g.cs.
Build first, inspect the generated output named by the drift failure, copy that
artifact explicitly, and rebuild until every drift check passes. Builds do not
rewrite these files. The reason output is checked before native records,
layouts and managed imports.

Dependency regressions inspect cargo metadata's resolved runtime edges,
evaluated MSBuild project/package references, resolved assembly paths and the
built consumer/engine assembly references. Imported managed references and
aliased native dependencies have negative fixtures. Source policy searches are
additional checks rather than dependency-graph evidence.
