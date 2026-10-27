using System.Reflection;
using System.Runtime.Loader;
using Disunity.Hosting;

/// <summary>
/// Hosting a built player's assemblies, with synthetic stand-ins: a <c>UnityEngine.TestModule</c>
/// that defines one type Disunity.Engine implements (<c>GameObject</c>) and one it does not,
/// and a game assembly whose fields and member references use both.
/// </summary>
internal static class PlayerAssemblyTests
{
    private const string Module = "UnityEngine.TestModule";
    private static readonly Assembly Engine = typeof(DisunityHost).Assembly;

    public static void Run()
    {
        EngineModuleNames();
        ReadsAndForwardsOnlyImplementedTypes();
        ResolvesGameReferencesThroughFacades();
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
        module.DefineClass("UnityEngine", "NotImplementedThing");
        return module.Build();
    }

    /// <summary>A game assembly with a field of each module type and member references to both.</summary>
    private static byte[] Game()
    {
        var game = new SyntheticAssembly("Game");
        var module = game.Reference(Module);
        var gameObject = game.TypeRef(module, "UnityEngine", "GameObject");
        var missing = game.TypeRef(module, "UnityEngine", "NotImplementedThing");
        game.DefineClass("Game", "Holder", ("owner", gameObject), ("thing", missing));
        game.MethodRef(gameObject, "SetActive", 1);
        game.MethodRef(gameObject, "SetActive", 3);
        game.MethodRef(gameObject, "DoesNotExist", 0);
        game.FieldRef(gameObject, "missingField");
        game.MethodRef(missing, "Anything", 0);
        return game.Build();
    }

    private static void EngineModuleNames()
    {
        foreach (var name in new[] { "UnityEngine", "UnityEngine.CoreModule", Module })
            Require(ModuleFacades.IsEngineModule(name), $"{name} is an engine module");
        foreach (var name in new[] { "UnityEngine.UI", "Unity.Burst", "Assembly-CSharp", "UnityEngineModule" })
            Require(!ModuleFacades.IsEngineModule(name), $"{name} is not an engine module");
    }

    private static void ReadsAndForwardsOnlyImplementedTypes()
    {
        var module = ModuleFacades.Read(new MemoryStream(TestModule()));
        Require(module.Name.Name == Module, "module name is read");
        Require(module.Types.SequenceEqual(new[] { "UnityEngine.GameObject", "UnityEngine.NotImplementedThing" }), "module types are read");
        Require(ModuleFacades.Implemented(module, Engine).SequenceEqual(new[] { "UnityEngine.GameObject" }), "only implemented types forward");

        var context = new AssemblyLoadContext("facade", isCollectible: true);
        var facade = context.LoadFromStream(new MemoryStream(ModuleFacades.Build(module, Engine)));
        Require(facade.GetName().Name == Module, "the facade keeps the module's name");
        Require(facade.GetType("UnityEngine.GameObject") == typeof(UnityEngine.GameObject), "an implemented type forwards to Disunity.Engine");
        Require(facade.GetType("UnityEngine.NotImplementedThing") is null, "a missing type does not resolve");
        context.Unload();
    }

    private static void ResolvesGameReferencesThroughFacades()
    {
        var managed = Directory.CreateTempSubdirectory("player-managed");
        try
        {
            File.WriteAllBytes(Path.Combine(managed.FullName, Module + ".dll"), TestModule());
            File.WriteAllBytes(Path.Combine(managed.FullName, "Game.dll"), Game());
            var context = new AssemblyLoadContext("player", isCollectible: true);
            new PlayerAssemblies(managed.FullName, Engine).Attach(context);
            var holder = context.LoadFromAssemblyName(new AssemblyName("Game")).GetType("Game.Holder", throwOnError: true)!;
            Require(holder.GetField("owner")!.FieldType == typeof(UnityEngine.GameObject), "a game field binds to Disunity.Engine's type");
            try
            {
                _ = holder.GetField("thing")!.FieldType;
                throw new InvalidOperationException("a missing engine type must not load");
            }
            catch (TypeLoadException error)
            {
                Require(error.Message.Contains("NotImplementedThing"), $"the failure names the type: {error.Message}");
            }
            Require(context.Assemblies.Count(a => a.GetName().Name == Module) == 1, "the facade loads once");
            context.Unload();
        }
        finally { managed.Delete(recursive: true); }
    }

    private static void ReportsMissingTypesAndMembers()
    {
        var report = PlayerApiReport.Build(new[] { ("Game", (Stream)new MemoryStream(Game())) }, Engine);
        var lines = report.Select(m => $"{m.Module} {m.Type} {m.Member ?? "-"} {string.Join(",", m.ReferencedBy)}").ToList();
        var expected = new[]
        {
            $"{Module} UnityEngine.GameObject DoesNotExist(0 parameters) Game",
            $"{Module} UnityEngine.GameObject SetActive(3 parameters) Game",
            $"{Module} UnityEngine.GameObject missingField Game",
            $"{Module} UnityEngine.NotImplementedThing - Game",
        };
        Require(lines.SequenceEqual(expected), "report:\n" + string.Join("\n", lines));
    }
}
