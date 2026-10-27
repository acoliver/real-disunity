# Writing C# scripts

Real Disunity runs C# game code written against the `UnityEngine` API. The `Disunity.Engine` assembly in `csharp/Disunity` defines `UnityEngine.MonoBehaviour`, `GameObject`, `Transform`, `Rigidbody2D` and the other supported types. They are thin wrappers: every property read, write and lifecycle message goes through a C ABI to the same native world that a [Rust game](rust-games.md) uses.

This page covers writing scripts and hosting them today. Hosting is still low level, because the player that loads a Unity scene and runs it is not finished. See the [current state](../README.md#current-state).

## How scripts run

The native world decides when each message runs, using Unity's ordering: Awake and OnEnable when an object becomes active, Start before its first update, FixedUpdate and physics in each fixed step, collision and trigger messages after physics, Update, coroutine resumptions, LateUpdate, then OnDisable and OnDestroy at the end of the frame. The C# side only invokes the method the native scheduler hands it. There is no second managed scheduler.

Script messages are found by name, including private methods, so the usual Unity style works without overrides. `Start` may return an `IEnumerator`.

A script that uses a type or member `Disunity.Engine` does not define fails to compile. A member that exists but is called with something unsupported throws an exception that names it: `Input.GetAxis("Jump")` throws `ArgumentException` when no `Jump` axis is registered, and a point light throws `UnsupportedLight2DException`. The [supported API list](unity-api.md) shows what exists.

## Set up a project

Use an SDK-style .NET 8 project that references `Disunity.Engine`:

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
    <GenerateRuntimeConfigurationFiles>true</GenerateRuntimeConfigurationFiles>
  </PropertyGroup>
  <ItemGroup>
    <ProjectReference Include="path/to/real-disunity/csharp/Disunity/Disunity.Engine.csproj" />
  </ItemGroup>
</Project>
```

Do not reference Unity's own assemblies. `Disunity.Engine` is the only `UnityEngine` the project should see. `GenerateRuntimeConfigurationFiles` produces the `.runtimeconfig.json` that the native host needs to start .NET.

## Write a script

This script compiles against the current `Disunity.Engine`. It moves a paddle with the arrow keys and flashes its sprite when something hits it:

```csharp
using System.Collections;
using UnityEngine;

public sealed class Paddle : MonoBehaviour
{
    [SerializeField] private float speed = 8f;
    private Rigidbody2D? body;
    private int hits;

    private void Awake() => body = GetComponent<Rigidbody2D>();

    private void FixedUpdate()
    {
        var direction = 0f;
        if (Input.GetKey(KeyCode.LeftArrow)) direction -= 1f;
        if (Input.GetKey(KeyCode.RightArrow)) direction += 1f;
        body!.velocity = new Vector2(direction * speed, 0f);
    }

    private void OnCollisionEnter2D(Collision2D collision)
    {
        hits++;
        Debug.Log($"hit {collision.gameObject.name} ({hits} total)");
        StartCoroutine(Flash());
    }

    private IEnumerator Flash()
    {
        var sprite = GetComponent<SpriteRenderer>()!;
        sprite.color = new Color(1f, 1f, 1f, 1f);
        yield return new WaitForSeconds(0.1f);
        sprite.color = new Color(0.2f, 0.6f, 1f, 1f);
    }
}
```

The math types are smaller than Unity's. `Vector3` has component access, `zero`, `one`, scalar multiplication and conversion to and from `Vector2`, but no vector addition yet. Check [the API list](unity-api.md) before relying on a helper.

## Host the scripts

A process runs C# scripts through `DisunityHost` in the `worldrunner-disunity-runtime` Rust crate. The host loads the engine's native library, starts .NET from your project's runtime configuration, and calls a small bridge class in your assembly.

1. **Build the native library.** `cargo build -p worldrunner-disunity-ffi` produces `libworldrunner_disunity_ffi.so` (Linux), `libworldrunner_disunity_ffi.dylib` (macOS) or `worldrunner_disunity_ffi.dll` (Windows) under `target/debug`. The C# side and the Rust host must load this same file, so both see one world.
2. **Add a bridge class.** The bridge is a static class with `[UnmanagedCallersOnly]` methods named `ConfigureNativeLibrary`, `Run`, `WorldHandle`, `Unload`, `Dispatch`, `Error`, `ReadPosition`, `TokenAlloc`, `TokenFree` and `TokenLive`. `csharp/PortableFixture/NativeBridge.cs` is a working bridge; copy it and change `Run`. `Run` receives a UTF-8 string from the host, whose meaning is up to you, and builds the starting world: call `DisunityHost.LoadWorld()`, create objects with `DisunityHost.CreateGameObject()`, and attach components with `gameObject.AddComponent<SpriteRenderer>()` or scripts with `DisunityHost.AddComponent(new Paddle())`. To create many objects whose scripts must wake together, as a scene load does, use `DisunityHost.BeginConstruction()` and call `Activate` once everything is in place.
3. **Boot the host from Rust.** Fill in a `ManagedHostConfig` with the native library path, your `.runtimeconfig.json`, your assembly and the bridge type name, for example `"MyGame.NativeBridge, MyGame"`. Then call `DisunityHost::boot`, `run_slice` with the string for `Run`, and `advance(1, 60)` once per frame. Only one host can exist per process.
4. **Draw it.** `host.export_presentation([width, height], next_texture)` returns the world's sprites, camera and UI glyphs. Pass the result and your texture bytes to `worldrunner::scene::plan::simulation::from_presentation`, then build a `WorldSceneContribution` and a `LiveFrame` exactly as a Rust game does in [Draw a 2D world in a window](rust-games.md#draw-a-2d-world-in-a-window).
5. **Shut down.** `unload_slice` destroys the world and releases every native and managed handle. You can then call `run_slice` again in the same process.

`crates/worldrunner/tests/native_sprite_equivalence.rs` is a complete example of steps 3 and 4. It builds the fixture assembly, boots the host, exports the C# world and checks that it renders the same quads as an equivalent world built in Rust.

Input from the window reaches C# through `DisunityHost.PushKey`, `PushAxis` and `PushFocus`, with timestamps in seconds. `Input.GetKey`, `GetKeyDown`, `GetKeyUp` and `GetAxis` read the native input state for the current fixed or dynamic phase. No input axes exist by default: register each one the scripts use, such as `Horizontal`, with `DisunityHost.RegisterInputAxis(id, new InputAxisDefinition("Horizontal", KeyCode.LeftArrow, KeyCode.RightArrow))` in `Run`.

## Unity projects

To run an existing Unity project's scripts, the scene they belong to has to be imported and built in the world before Awake runs. The importer side of that exists; the loader does not yet. [Importing Unity projects](unity-projects.md) describes what is available now.

## Built players

A built Unity player ships its game code compiled, together with Unity packages and third-party libraries, in `<Game>_Data/Managed`. Those assemblies reference engine types through Unity's module assemblies, for example `[UnityEngine.CoreModule]UnityEngine.Vector3`. Unity's module assemblies call into Unity's native player, so they are never loaded.

`PlayerAssemblies` resolves a player's assembly names in an `AssemblyLoadContext`:

```csharp
var player = new PlayerAssemblies("/path/to/Game_Data/Managed", typeof(DisunityHost).Assembly);
player.Attach(context);
var game = context.LoadFromAssemblyName(new AssemblyName("Assembly-CSharp"));
```

- An engine module name (`UnityEngine` or `UnityEngine.*Module`) resolves to a facade built in memory from that module's metadata in the player. The facade has the module's name and version and forwards each of the module's types that `Disunity.Engine` defines publicly, with their public nested types. Any other type stays unresolved, so code that uses it fails with a `TypeLoadException` naming the type and module.
- Other names the player ships resolve to its own assemblies, including packages such as `UnityEngine.UI`. Framework names (`mscorlib`, `netstandard`, `System` and `System.*`) are never taken from the player, whose copies are built for Unity's Mono runtime; .NET provides them, and one it lacks fails to load by name.
- One `PlayerAssemblies` serves one load context.

`PlayerApiReport.Build` lists the engine types and members a player's assemblies reference and the facades do not forward, with the assemblies that reference each. It reads metadata only. A nested type of a missing type is listed through its outer type. Members match by name, kind and parameter count, so overloads that differ only in parameter types are not told apart, and members referenced through a generic instantiation are not checked yet.

Loading assemblies does not run anything. Running a player's scripts needs its scenes, `RuntimeInitializeOnLoadMethod` hooks and main-thread rules; see [#172](https://github.com/acoliver/real-disunity/issues/172).
