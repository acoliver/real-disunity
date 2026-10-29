# Writing C# scripts

Real Disunity runs C# game code written against the `UnityEngine` API. The `Disunity.Engine` assembly in `csharp/Disunity` defines `UnityEngine.MonoBehaviour`, `GameObject`, `Transform`, `Rigidbody2D` and the other supported types. They provide thin compatibility: world state and lifecycle use a C ABI to the same native world as a [Rust game](rust-games.md). Managed value types and delegate utilities can execute in C#.

This page covers scripts, low-level hosting and built-player loading. The [play command](../crates/play/README.md) already loads the pinned test games through `DisunityHost.LoadScene`; arbitrary Unity game compatibility remains incomplete. See the [current state](../README.md#current-state).

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

## Script exceptions

An exception that a script throws does not stop the frame. The engine catches it, logs it with `Debug.LogException` against the script (so `Application.logMessageReceived` sees a message of type `Exception`), and carries on with the next script and the next message, as Unity does. This covers `Awake`, `OnEnable`, `Start`, `FixedUpdate`, `Update`, `LateUpdate`, `OnDisable`, `OnDestroy`, collision and trigger messages, the same messages on a `ScriptableObject`, and a `UnityEvent` listener invoked from any of them. A listener that throws stops the remaining listeners of that one invocation and the rest of the message that invoked it.

A script that threw keeps receiving later messages: an `Update` that throws is called again next frame, and an `Awake` that threw is followed by `OnEnable` and `Start`. `Start` runs once even when it throws. A coroutine step that throws is logged and ends that coroutine, including the coroutines it nested, and its `finally` blocks run. Other coroutines on the same script are unaffected. Work posted to the main thread and `Application.quitting` handlers follow the same rule.

An error that comes from the engine rather than the game still fails the step. That includes a native error (`DisunityException`), a step with no world loaded, and a scene that cannot be built. A `DisunityException` that surfaces inside a script's own call into the engine counts as an engine error too, and is not logged and skipped.

The host counts the logged script exceptions in `DisunityHost.ScriptExceptionCount` and resets the count when a world loads. When a `play` run ends, the bridge writes `N script exceptions were logged; the first: <exception>` to standard error if any were logged. The run still succeeds, so read that line when a game misbehaves without failing.

## Unity projects

To run an existing Unity project's scripts, the scene they belong to has to be imported and built in the world before Awake runs. `unity-import plan` produces the plan and `DisunityHost.LoadScene` builds it before activation. The [play command](../crates/play/README.md) uses this path for the pinned games. [Importing Unity projects](unity-projects.md) describes supported content and gaps.

## Built players

A built Unity player ships its game code compiled, together with Unity packages and third-party libraries, in `<Game>_Data/Managed`. Those assemblies reference engine types through Unity's module assemblies, for example `[UnityEngine.CoreModule]UnityEngine.Vector3`. Engine module names are resolved through generated facades forwarding supported types to `Disunity.Engine`; listed package assemblies can still be loaded and executed, as described below.

Follow the [source-use policy](../CONTRIBUTING.md#permitted-references-and-inspection) before selecting player material. Runtime loading and metadata processing do not authorize agent inspection of Unity engine, library or package implementation. Only inspect game-specific material within its permissions, and stop uncertain manual inspection for Andrew's guidance.

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

`RuntimeHooks.Resolve(manifest.Hooks, name => context.LoadFromAssemblyName(new AssemblyName(name)))` finds each startup hook's method: a static, non-generic method with no parameters, public or not. Hooks Unity marks `isUnityClass` belong to Unity's packages and run like any other. `Run(phase)` runs one phase's hooks in listed order; Unity's phases run in `RuntimeHooks.PhaseOrder`: `SubsystemRegistration`, `AfterAssembliesLoaded`, `BeforeSplashScreen`, `BeforeSceneLoad` and `AfterSceneLoad`. A hook that cannot be resolved is kept in `Unresolved` with the reason, and running its phase fails with its name before any of the phase's hooks run. A hook that throws stops its phase with an exception naming it; later hooks and phases are not run. `Run(phase, failed)` behaves as Unity does instead: each unresolved or throwing hook is passed to `failed` and the rest run. The player runner logs them that way. `PlayerManifest` reports a malformed or unexpected manifest entry, including an assembly type other than 2 (engine module) or 16 (game or package), as an `InvalidDataException` naming the file.

### The player's environment

`play --player` gives the engine what a built player knows about itself before its first scene:

- The command line. `play --player <data directory> [--arg VALUE]...` makes `Environment.GetCommandLineArgs()` return the player's own command line before any startup hook runs: argument 0 is the player's executable path (the engine's choice: the data directory's parent joined with its name without the `_Data` suffix, plus `.exe` for a Windows player), followed by the `--arg` values, each one argument, in order. A game that reads a save or `-userdata` directory from its command line receives them this way. There is no public API to set a hosted runtime's command line, so `Disunity.Play` sets the runtime's private `s_commandLineArgs` field on `System.Environment` by reflection and fails the run naming the field when this runtime lacks it.

- `Application` reports the player's company, product name, version, Unity release and platform, read from its `PlayerSettings` when the player was built with the Unity release the decoder knows (6000.3), and otherwise from `app.info`, with an empty version. `dataPath` is `<Game>_Data`, `streamingAssetsPath` is under it, and paths use forward slashes. `Quit` asks the `wantsToQuit` handlers, raises `quitting` once, saves `PlayerPrefs` and cancels `exitCancellationToken`; a handler that throws is logged. The run goes on until the host stops it.
- `Debug` and `Debug.unityLogger` deliver each message to `Application.logMessageReceivedThreaded` at once, on the thread that logged it, and to `logMessageReceived` on the main thread: at once there, and from other threads in the next frame. `filterLogType` drops less severe messages. Messages format values with the invariant culture. Errors and warnings carry the caller's stack trace, and an exception is reported as Unity reports it, `InvalidOperationException: message`, with its own stack trace.
- `PlayerPrefs` keeps typed values. A value read as another type gives the default. `Save`, quitting and unloading the player write them to `PlayerPrefs.xml` in `persistentDataPath`, and configuring a player reads that file back. A file that cannot be read is ignored with a warning. `play --player` keeps each game's file under `target/play/player/persistent/<company>/<product>`.
- `Resources.Load`, `LoadAll` and `LoadAsync` read the resource plan that `unity-import player-resources` writes. Paths are case-insensitive. A text asset's `text` drops a UTF-8 byte order mark and its `bytes` keep it. A script asset is created with its fields bound before `Awake` and is loaded once. Script assets that reference each other are all bound before any runs `Awake`. A `Load` or `LoadAll` that reaches a path naming a class the engine cannot build fails with `NotSupportedException` naming the path and class. Resources load on the main thread only. A `ResourceRequest` completes in the next frame's delayed tasks, so a coroutine that yields it resumes the frame after.

Loading assemblies and resolving hooks call no hook methods. The [player runner](../crates/play/README.md#built-unity-players) loads the scene around the startup phases and advances it on the main thread. This path reports unsupported content and does not establish arbitrary game playability.

### Jobs and native containers

A player's startup code uses Unity's job system and native containers, so `Disunity.Engine` defines them.

- Jobs run synchronously. `Schedule`, `ScheduleParallel` and `Run` of `IJob`, `IJobParallelFor` and `IJobFor` execute the job on the calling thread before the call returns, after the jobs it depends on, which have already run. Every `JobHandle` is complete: `IsCompleted` is true, `Complete` and `CompleteAll` do nothing, and handles compare equal. A parallel job visits indexes `0` to `arrayLength - 1` in order, once each, whatever the batch size. A job runs on a copy of its struct, as in Unity, so only the native containers it holds carry results back. There are no worker threads: `JobsUtility.JobWorkerCount` is 0 and `ThreadIndexCount` is 1.
- A job producer written against `JobsUtility` works too. `CreateJobReflectionData` registers the function that runs the job, and `Schedule` and `ScheduleParallelFor` call it with a copy of the job data and a `JobRanges`. `GetWorkStealingRange` hands out the batches in order, each once. An exception thrown by a job reaches the code that scheduled it.
- Burst reports itself uninitialized (`BurstCompilerService.IsInitialized` is false) because the engine loads no Burst library. Code and jobs marked `[BurstCompile]` run as ordinary managed code.
- `NativeArray<T>`, `NativeSlice<T>` and `UnsafeUtility` allocate unmanaged memory with the process's aligned allocator, honouring the requested alignment. `NativeArray<T>` has Unity's fields and layout. Every allocator label allocates the same way, so `Allocator.Temp` memory is not freed at the end of a frame and must be disposed like the rest. As in a Unity player, disposing a disposed, default or `Allocator.None` array throws `InvalidOperationException`, and a disposed array has length 0.
- Nothing checks container use. There is no `AtomicSafetyHandle` or `DisposeSentinel`, so reads and writes that would race in Unity are not detected, and `[ReadOnly]`, `[WriteOnly]` and `[NativeDisableParallelForRestriction]` are attributes only. `UnsafeUtility.CheckForLeaks` always reports none.

Startup hooks that throw are logged with `Debug.LogException` and the phase goes on with the next hook, as in Unity. A hook that cannot be resolved is logged the same way.
