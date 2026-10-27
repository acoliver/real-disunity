using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.Loader;
using Disunity.Hosting;
using UnityEngine.Events;
using UnityEngine.Rendering.Universal;

/// <summary>
/// Hosting a built player's assemblies, with synthetic stand-ins: a <c>UnityEngine.TestModule</c>
/// whose types Disunity.Engine implements publicly (<c>GameObject</c>, <c>Light2D</c> with its
/// nested <c>LightType</c>, generic <c>UnityEvent`1</c>), implements internally
/// (<c>IPersistentCalls</c>) or lacks, and a game assembly that uses them.
/// </summary>
internal static class PlayerAssemblyTests
{
    private const string Module = "UnityEngine.TestModule";
    private static readonly Assembly Engine = typeof(DisunityHost).Assembly;

    private static readonly string[] Implemented =
    {
        "UnityEngine.GameObject",
        "UnityEngine.Rendering.Universal.Light2D",
        "UnityEngine.Events.UnityEvent`1",
    };

    public static void Run()
    {
        EngineModuleNames();
        ReadsDefinedAndForwardedTypes();
        ForwardsOnlyPublicImplementedTypes();
        ResolvesGameReferencesThroughFacades();
        ResolvesListedNamesForOneContextAndLeavesOthersToDotNet();
        ReportsMissingTypesAndMembers();
        Console.WriteLine("PASS: player module facades, assembly resolution and missing engine API report");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static byte[] TestModule()
    {
        var module = new SyntheticAssembly(Module);
        module.DefineClass("UnityEngine", "GameObject");
        module.DefineClass("UnityEngine.Rendering.Universal", "Light2D");
        module.DefineClass("UnityEngine.Events", "UnityEvent`1");
        module.DefineClass("UnityEngine.Events", "IPersistentCalls");
        module.DefineClass("UnityEngine", "NotImplementedThing");
        return module.Build();
    }

    /// <summary>A game assembly with fields of the module's types and member references to them.</summary>
    private static byte[] Game()
    {
        var game = new SyntheticAssembly("Game");
        var module = game.Reference(Module);
        var gameObject = game.TypeRef(module, "UnityEngine", "GameObject");
        var light = game.TypeRef(module, "UnityEngine.Rendering.Universal", "Light2D");
        var lightType = game.NestedTypeRef(light, "LightType");
        var unityEvent = game.TypeRef(module, "UnityEngine.Events", "UnityEvent`1");
        var missing = game.TypeRef(module, "UnityEngine", "NotImplementedThing");
        var internalType = game.TypeRef(module, "UnityEngine.Events", "IPersistentCalls");
        game.DefineClass("Game", "Holder",
            SyntheticField.Class("owner", gameObject),
            SyntheticField.Value("lightType", lightType),
            SyntheticField.GenericOfInt("changed", unityEvent),
            SyntheticField.Class("thing", missing),
            SyntheticField.Class("calls", internalType));
        game.MethodRef(gameObject, "SetActive", 1);
        game.MethodRef(gameObject, "SetActive", 3);
        game.MethodRef(gameObject, "DoesNotExist", 0);
        game.FieldRef(gameObject, "missingField");
        game.MethodRef(missing, "Anything", 0);
        // A nested type of an implemented type, and one of a missing type.
        game.NestedTypeRef(gameObject, "NotNested");
        game.NestedTypeRef(missing, "Inner");
        return game.Build();
    }

    private static void EngineModuleNames()
    {
        foreach (var name in new[] { "UnityEngine", "UnityEngine.CoreModule", Module })
            Require(ModuleFacades.IsEngineModule(name), $"{name} is an engine module");
        foreach (var name in new[] { "UnityEngine.UI", "Unity.Burst", "Assembly-CSharp", "UnityEngineModule" })
            Require(!ModuleFacades.IsEngineModule(name), $"{name} is not an engine module");
    }

    private static void ReadsDefinedAndForwardedTypes()
    {
        var module = ModuleFacades.Read(new MemoryStream(TestModule()));
        Require(module.Name.Name == Module, "module name is read");
        Require(module.Types.Count == 5 && module.Types[0] == "UnityEngine.GameObject", "defined types are read");

        // UnityEngine.dll defines nothing and forwards its types to the module assemblies.
        var forwarding = new SyntheticAssembly("UnityEngine");
        forwarding.Forward(forwarding.Reference("UnityEngine.CoreModule"), "UnityEngine", "GameObject");
        var read = ModuleFacades.Read(new MemoryStream(forwarding.Build()));
        Require(read.Types.SequenceEqual(new[] { "UnityEngine.GameObject" }), "forwarded types are read");
    }

    private static void ForwardsOnlyPublicImplementedTypes()
    {
        var module = ModuleFacades.Read(new MemoryStream(TestModule()));
        var forwarded = ModuleFacades.Implemented(module, Engine);
        Require(forwarded.SequenceEqual(Implemented), "forwarded: " + string.Join(", ", forwarded));
        Require(!ModuleFacades.Forwards(Engine, "UnityEngine.Events.IPersistentCalls"), "an internal type is not forwarded");
        Require(ModuleFacades.Forwards(Engine, "UnityEngine.Rendering.Universal.Light2D+LightType"), "a public nested type is forwarded");
    }

    private static void ResolvesGameReferencesThroughFacades()
    {
        WithManagedDirectory(directory =>
        {
            var holder = LoadHolder(directory, out var context);
            Require(FieldType(holder, "owner") == typeof(UnityEngine.GameObject), "a game field binds to Disunity.Engine's type");
            Require(FieldType(holder, "lightType") == typeof(Light2D.LightType), "a nested type forwards with its enclosing type");
            Require(FieldType(holder, "changed") == typeof(UnityEvent<int>), "a generic type forwards");
            RequireTypeLoadFailure(holder, "thing", "NotImplementedThing");
            RequireTypeLoadFailure(holder, "calls", "IPersistentCalls");
            return context;
        });
    }

    private static void ResolvesListedNamesForOneContextAndLeavesOthersToDotNet()
    {
        WithManagedDirectory(directory =>
        {
            // A listed package with a framework-like name, and Mono class libraries the player
            // ships without listing them.
            File.WriteAllBytes(Path.Combine(directory, "System.Package.dll"), new SyntheticAssembly("System.Package").Build());
            File.WriteAllBytes(Path.Combine(directory, "System.Core.dll"), new SyntheticAssembly("System.Core").Build());
            File.WriteAllBytes(Path.Combine(directory, "mscorlib.dll"), new SyntheticAssembly("mscorlib").Build());
            var context = new AssemblyLoadContext("player", isCollectible: true);
            var player = new PlayerAssemblies(directory, Engine, new[] { "Game", "System.Package", "Listed.Missing" });
            player.Attach(context);
            var first = player.Resolve(context, new AssemblyName(Module));
            Require(first is not null && ReferenceEquals(first, player.Resolve(context, new AssemblyName(Module))),
                "a module's facade is built once per context");
            Require(player.Resolve(context, new AssemblyName("System.Package"))?.GetName().Name == "System.Package",
                "a listed assembly loads from Managed whatever its name");
            Require(player.Resolve(context, new AssemblyName("System.Core")) is null, "an unlisted class library is left to .NET");
            Require(player.Resolve(context, new AssemblyName("mscorlib")) is null, "mscorlib is left to .NET");
            Require(player.Resolve(context, new AssemblyName("UnityEngine.AbsentModule")) is null, "an engine module the player lacks stays unresolved");
            try
            {
                player.Resolve(context, new AssemblyName("Listed.Missing"));
                throw new InvalidOperationException("a listed assembly the player lacks must fail by name");
            }
            catch (FileNotFoundException error) when (error.Message.Contains("Listed.Missing.dll")) { }
            var other = new AssemblyLoadContext("other", isCollectible: true);
            try
            {
                player.Attach(other);
                throw new InvalidOperationException("a second context must be rejected");
            }
            catch (InvalidOperationException error) when (error.Message.Contains("one load context")) { }
            other.Unload();
            return context;
        });
    }

    private static void ReportsMissingTypesAndMembers()
    {
        var report = PlayerApiReport.Build(new[] { ("Game", (Stream)new MemoryStream(Game())) }, Engine);
        var lines = report.Select(m => $"{m.Type} {m.Member ?? "-"} {string.Join(",", m.ReferencedBy)}").ToList();
        var expected = new[]
        {
            "UnityEngine.Events.IPersistentCalls - Game",
            "UnityEngine.GameObject DoesNotExist(0 parameters) Game",
            "UnityEngine.GameObject SetActive(3 parameters) Game",
            "UnityEngine.GameObject missingField Game",
            "UnityEngine.GameObject+NotNested - Game",
            "UnityEngine.NotImplementedThing - Game",
        };
        Require(lines.SequenceEqual(expected), "report:\n" + string.Join("\n", lines));
        Require(report.All(m => m.Module == Module), "entries name their module");
    }

    private static Type FieldType(Type holder, string field) =>
        holder.GetField(field)?.FieldType ?? throw new InvalidOperationException($"no field {field}");

    private static void RequireTypeLoadFailure(Type holder, string field, string type)
    {
        try
        {
            _ = FieldType(holder, field);
            throw new InvalidOperationException($"{field}: an engine type Disunity.Engine does not forward must not load");
        }
        catch (TypeLoadException error)
        {
            Require(error.Message.Contains(type), $"the failure names {type}: {error.Message}");
        }
    }

    private static Type LoadHolder(string directory, out AssemblyLoadContext context)
    {
        context = new AssemblyLoadContext("player", isCollectible: true);
        new PlayerAssemblies(directory, Engine, new[] { "Game" }).Attach(context);
        return context.LoadFromAssemblyName(new AssemblyName("Game")).GetType("Game.Holder", throwOnError: true)!;
    }

    /// <summary>
    /// Runs a test in a temporary Managed directory holding the test module and game, then
    /// unloads the context it returns and waits for collection, since Windows keeps loaded
    /// assembly files open until then.
    /// </summary>
    private static void WithManagedDirectory(Func<string, AssemblyLoadContext> test)
    {
        var managed = Directory.CreateTempSubdirectory("player-managed");
        try
        {
            var unloaded = RunAndUnload(managed.FullName, test);
            for (var attempt = 0; attempt < 10 && unloaded.IsAlive; attempt++)
            {
                GC.Collect();
                GC.WaitForPendingFinalizers();
            }
        }
        finally
        {
            try { managed.Delete(recursive: true); }
            catch (Exception error) when (error is IOException or UnauthorizedAccessException)
            {
                Console.Error.WriteLine($"could not remove {managed.FullName}: {error.Message}");
            }
        }
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static WeakReference RunAndUnload(string directory, Func<string, AssemblyLoadContext> test)
    {
        File.WriteAllBytes(Path.Combine(directory, Module + ".dll"), TestModule());
        File.WriteAllBytes(Path.Combine(directory, "Game.dll"), Game());
        var context = test(directory);
        context.Unload();
        return new WeakReference(context);
    }
}
