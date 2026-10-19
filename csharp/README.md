# Managed ABI and portable fixture

`Disunity/NativeBindings.g.cs` is checked in. The native FFI build generates a
separate file in Cargo's `OUT_DIR` and compares **all bytes**, including complete
signatures, calling conventions, and struct fields. A mismatch fails the build;
normal builds never update the checked file. Both ABI source files and the
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
   preserve its append-only ABI contract.
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
`Disunity.Tests` console checks against the compiled managed assembly: all 12
imports' complete parameter and result types, entry points, native image,
calling conventions, and all 3 structs' field types, offsets, and sizes. Native
and runtime Rust layout tests independently check sizes, alignment, and offsets.

The fixture uses only `csharp/PortableFixture/triangle.fixture`. It verifies 100
load/step/export/unload cycles with deterministic native object state, reused
world slots, and zero native worlds and managed GCHandle tokens after each
unload. It also tests missing fixtures, duplicate hosts/worlds, and active-host
cleanup. The host receives native library, managed assembly, runtimeconfig, and
bridge type paths explicitly through `ManagedHostConfig`; the managed resolver
uses the native image selected by that host. SDK discovery settings belong to the
invoking build/test environment, not engine policy.

CI defines Linux, Windows, and macOS managed jobs. A local Linux result does not
establish Windows or macOS compatibility.
