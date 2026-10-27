using System.Reflection;
using System.Text;
using Disunity.Hosting;
using UnityEngine;

/// <summary>
/// Reading a player's manifest files and running its startup hooks. The hooks are static
/// methods of <see cref="HookTargets"/> in this assembly, named the way a player's
/// <c>RuntimeInitializeOnLoads.json</c> names them.
/// </summary>
internal static class RuntimeHookTests
{
    private static readonly string Self = typeof(RuntimeHookTests).Assembly.GetName().Name!;

    internal sealed class HookTargets
    {
        public static readonly List<string> Calls = new();
        public static void Early() => Calls.Add("early");
        public static void Loaded() => Calls.Add("loaded");
        public static void BeforeScene() => Calls.Add("before-scene");
        private static void AfterScene() => Calls.Add("after-scene");
        public static void Fails() => throw new InvalidOperationException("hook body failed");
        public static void WithParameter(int value) => Calls.Add($"parameter {value}");
        public void Instance() => Calls.Add("instance");
    }

    public static void Run()
    {
        ReadsGameAssembliesInOrder();
        RejectsAssemblyTypesThatContradictNames();
        ReadsHooksWithTheirPhases();
        RunsPhasesInUnityOrderAndHooksInListedOrder();
        KeepsUnresolvedHooksAndFailsTheirPhase();
        NamesTheHookThatThrows();
        Console.WriteLine("PASS: player manifest and startup hooks in Unity's phase order");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static Stream Json(string text) => new MemoryStream(Encoding.UTF8.GetBytes(text));

    private static void RequireInvalid(Func<object> read, string fragment)
    {
        try
        {
            read();
            throw new InvalidOperationException($"expected a manifest error containing '{fragment}'");
        }
        catch (InvalidDataException error)
        {
            Require(error.Message.Contains(fragment), $"error names {fragment}: {error.Message}");
        }
    }

    private static void ReadsGameAssembliesInOrder()
    {
        var names = PlayerManifest.ReadAssemblies(Json(
            """{"names":["UnityEngine.dll","Game-firstpass.dll","UnityEngine.CoreModule.dll","UnityEngine.UI.dll","Game.dll"],"types":[2,16,2,16,16]}"""));
        Require(names.SequenceEqual(new[] { "Game-firstpass", "UnityEngine.UI", "Game" }), "game assemblies: " + string.Join(", ", names));
    }

    private static void RejectsAssemblyTypesThatContradictNames()
    {
        RequireInvalid(() => PlayerManifest.ReadAssemblies(Json("""{"names":["Game.dll"],"types":[2]}""")), "Game.dll with type 2");
        RequireInvalid(() => PlayerManifest.ReadAssemblies(Json("""{"names":["UnityEngine.CoreModule.dll"],"types":[16]}""")), "type 16");
        RequireInvalid(() => PlayerManifest.ReadAssemblies(Json("""{"names":["Game.dll"],"types":[4]}""")), "type 4");
        RequireInvalid(() => PlayerManifest.ReadAssemblies(Json("""{"names":["Game.dll","B.dll"],"types":[16]}""")), "2 names and 1 types");
        RequireInvalid(() => PlayerManifest.ReadAssemblies(Json("""{"names":["Game.exe"],"types":[16]}""")), "not a .dll");
    }

    private static string HooksJson(params (string Space, string Type, string Method, int Phase)[] hooks) =>
        "{\"root\":[" + string.Join(",", hooks.Select(h =>
            $"{{\"assemblyName\":\"{Self}\",\"nameSpace\":\"{h.Space}\",\"className\":\"{h.Type}\",\"methodName\":\"{h.Method}\",\"loadTypes\":{h.Phase},\"isUnityClass\":false}}")) + "]}";

    /// <summary>The hook targets as listed: deliberately out of phase order.</summary>
    private static readonly (string, string, string, int)[] Listed =
    {
        ("", "RuntimeHookTests+HookTargets", "AfterScene", 0),
        ("", "RuntimeHookTests+HookTargets", "BeforeScene", 1),
        ("", "RuntimeHookTests+HookTargets", "Loaded", 2),
        ("", "RuntimeHookTests+HookTargets", "Early", 4),
        ("", "RuntimeHookTests+HookTargets", "Loaded", 2),
    };

    private static void ReadsHooksWithTheirPhases()
    {
        var hooks = PlayerManifest.ReadHooks(Json(HooksJson(("Some.Space", "Starter", "Init", 3))));
        Require(hooks.Single() == new RuntimeHook(Self, "Some.Space.Starter", "Init", RuntimeInitializeLoadType.BeforeSplashScreen),
            $"hook: {hooks.Single()}");
        RequireInvalid(() => PlayerManifest.ReadHooks(Json(HooksJson(("", "Starter", "Init", 9)))), "unknown load type 9");
    }

    private static RuntimeHooks Resolve(params (string, string, string, int)[] listed) =>
        RuntimeHooks.Resolve(PlayerManifest.ReadHooks(Json(HooksJson(listed))), name =>
            name == Self ? typeof(RuntimeHookTests).Assembly : throw new FileNotFoundException($"no assembly {name}"));

    private static void RunsPhasesInUnityOrderAndHooksInListedOrder()
    {
        HookTargets.Calls.Clear();
        var hooks = Resolve(Listed);
        Require(hooks.Unresolved.Count == 0, "every listed hook resolves, including a private one");
        foreach (var phase in RuntimeHooks.PhaseOrder) hooks.Run(phase);
        var expected = new[] { "early", "loaded", "loaded", "before-scene", "after-scene" };
        Require(HookTargets.Calls.SequenceEqual(expected), "calls: " + string.Join(", ", HookTargets.Calls));
    }

    private static void KeepsUnresolvedHooksAndFailsTheirPhase()
    {
        HookTargets.Calls.Clear();
        const string targets = "RuntimeHookTests+HookTargets";
        var hooks = RuntimeHooks.Resolve(
            PlayerManifest.ReadHooks(Json(HooksJson(
                ("", targets, "Early", 4),
                ("", "Missing", "Init", 1),
                ("", targets, "Absent", 1),
                ("", targets, "WithParameter", 1),
                ("", targets, "Instance", 1)))).Append(new RuntimeHook("Not.Shipped", "X", "Init", RuntimeInitializeLoadType.BeforeSceneLoad)),
            name => name == Self ? typeof(RuntimeHookTests).Assembly : throw new FileNotFoundException($"no assembly {name}"));
        var reasons = hooks.Unresolved.Select(u => u.Reason).ToList();
        Require(reasons.Count == 5, "unresolved: " + string.Join("; ", reasons));
        Require(reasons[0].Contains("no type Missing") && reasons[1].Contains("no static method Absent()")
            && reasons[2].Contains("WithParameter") && reasons[3].Contains("Instance") && reasons[4].Contains("no assembly Not.Shipped"),
            "reasons: " + string.Join("; ", reasons));
        hooks.Run(RuntimeInitializeLoadType.SubsystemRegistration);
        try
        {
            hooks.Run(RuntimeInitializeLoadType.BeforeSceneLoad);
            throw new InvalidOperationException("a phase with unresolved hooks must not run");
        }
        catch (InvalidOperationException error) when (error.Message.StartsWith("BeforeSceneLoad startup hooks cannot run"))
        {
            Require(error.Message.Contains("Missing.Init"), $"the failure names the hook: {error.Message}");
        }
        Require(HookTargets.Calls.SequenceEqual(new[] { "early" }), "only the resolved phase ran");
    }

    private static void NamesTheHookThatThrows()
    {
        var hooks = Resolve(("", "RuntimeHookTests+HookTargets", "Fails", 1));
        try
        {
            hooks.Run(RuntimeInitializeLoadType.BeforeSceneLoad);
            throw new InvalidOperationException("a failing hook must fail its phase");
        }
        catch (InvalidOperationException error) when (error.Message.StartsWith("startup hook"))
        {
            Require(error.Message.Contains("HookTargets.Fails") && error.InnerException?.Message == "hook body failed",
                $"the failure names the hook and keeps its cause: {error.Message}");
        }
    }
}
