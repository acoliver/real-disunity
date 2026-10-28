# Writing C# scripts

Real Disunity runs C# game code written against the `UnityEngine` API. The `Disunity.Engine` assembly in `csharp/Disunity` defines `UnityEngine.MonoBehaviour`, `GameObject`, `Transform`, `Rigidbody2D` and the other supported types. They are thin wrappers: every property read, write and lifecycle message goes through a C ABI to the same native world that a [Rust game](rust-games.md) uses.

This page covers writing scripts and hosting them today. Hosting is still low level, because the player that loads a Unity scene and runs it is not finished. See the [current state](../README.md#current-state).

## How scripts run

The native world decides when each message runs, using Unity's ordering: Awake and OnEnable when an object becomes active, Start before its first update, FixedUpdate and physics in each fixed step, collision and trigger messages after physics, Update, coroutine resumptions, work posted to the main thread's `SynchronizationContext` (including `await` continuations), LateUpdate, then OnDisable and OnDestroy at the end of the frame. The C# side only invokes the method the native scheduler hands it. There is no second managed scheduler.

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

`PlayerManifest.Read` reads what the player lists about its managed code from `<Game>_Data`: the game and package assemblies in `ScriptingAssemblies.json`, in load order, and the startup hooks in `RuntimeInitializeOnLoads.json`. `PlayerAssemblies` then resolves the player's assembly names in an `AssemblyLoadContext`:

```csharp
var manifest = PlayerManifest.Read("/path/to/Game_Data");
var player = new PlayerAssemblies("/path/to/Game_Data/Managed", typeof(DisunityHost).Assembly, manifest.GameAssemblies);
player.Attach(context);
var game = context.LoadFromAssemblyName(new AssemblyName("Assembly-CSharp"));
```

- An engine module name (`UnityEngine` or `UnityEngine.*Module`) resolves to a facade built in memory from that module's metadata in the player. The facade has the module's name and version and forwards each of the module's types that `Disunity.Engine` defines publicly, with their public nested types. Any other type stays unresolved, so code that uses it fails with a `TypeLoadException` naming the type and module.
- The game and package assemblies the player lists resolve to its `Managed` directory, including `UnityEngine.UI` and packages such as `System.IO.Hashing`. A listed name that .NET already provides, such as `System.Memory`, binds to .NET first. A listed assembly the directory lacks fails with its name.
- Any other name is left to .NET. A player also ships Mono class libraries such as `System.Core` and `Mono.Security` without listing them; they are built for Unity's Mono runtime and are never loaded.
- One `PlayerAssemblies` serves one load context.

`PlayerApiReport.Build` lists the engine types and members a player's assemblies reference and the facades do not forward, with the assemblies that reference each. It reads metadata only. A nested type of a missing type is listed through its outer type. Members match by name, kind and parameter count, so overloads that differ only in parameter types are not told apart, and members referenced through a generic instantiation are not checked yet.

`RuntimeHooks.Resolve(manifest.Hooks, name => context.LoadFromAssemblyName(new AssemblyName(name)))` finds each startup hook's method: a static, non-generic method with no parameters, public or not. Hooks Unity marks `isUnityClass` belong to Unity's packages and run like any other. `Run(phase)` runs one phase's hooks in listed order; Unity's phases run in `RuntimeHooks.PhaseOrder`: `SubsystemRegistration`, `AfterAssembliesLoaded`, `BeforeSplashScreen`, `BeforeSceneLoad` and `AfterSceneLoad`. A hook that cannot be resolved is kept in `Unresolved` with the reason, and running its phase fails with its name before any of the phase's hooks run. A hook that throws stops its phase with an exception naming it; later hooks and phases are not run. Unity instead logs the exception and continues. `PlayerManifest` reports a malformed or unexpected manifest entry, including an assembly type other than 2 (engine module) or 16 (game or package), as an `InvalidDataException` naming the file.

### The player's environment

`play --player` gives the engine what a built player knows about itself before its first scene:

- `Application` reports the player's company, product name, version, Unity release and platform, read from its `PlayerSettings` when the player was built with the Unity release the decoder knows (6000.3), and otherwise from `app.info`, with an empty version. `dataPath` is `<Game>_Data`, `streamingAssetsPath` is under it, and paths use forward slashes. `Quit` asks the `wantsToQuit` handlers, raises `quitting` once, saves `PlayerPrefs` and cancels `exitCancellationToken`; a handler that throws is logged. The run goes on until the host stops it.
- `Debug` and `Debug.unityLogger` deliver each message to `Application.logMessageReceivedThreaded` at once, on the thread that logged it, and to `logMessageReceived` on the main thread: at once there, and from other threads in the next frame. `filterLogType` drops less severe messages. Messages format values with the invariant culture. Errors and warnings carry the caller's stack trace, and an exception is reported as Unity reports it, `InvalidOperationException: message`, with its own stack trace.
- `PlayerPrefs` keeps typed values. A value read as another type gives the default. `Save`, quitting and unloading the player write them to `PlayerPrefs.xml` in `persistentDataPath`, and configuring a player reads that file back. A file that cannot be read is ignored with a warning. `play --player` keeps each game's file under `target/play/player/persistent/<company>/<product>`.
- `Resources.Load`, `LoadAll` and `LoadAsync` read the resource plan that `unity-import player-resources` writes. Paths are case-insensitive. A text asset's `text` drops a UTF-8 byte order mark and its `bytes` keep it. A script asset is created with its fields bound before `Awake` and is loaded once. Script assets that reference each other are all bound before any runs `Awake`. A `Load` or `LoadAll` that reaches a path naming a class the engine cannot build fails with `NotSupportedException` naming the path and class. Resources load on the main thread only. A `ResourceRequest` completes in the next frame's delayed tasks, so a coroutine that yields it resumes the frame after.

Loading assemblies and resolving hooks call no hook methods. Running a player needs its scenes loaded around the scene phases and main-thread rules; see [#172](https://github.com/acoliver/real-disunity/issues/172).
