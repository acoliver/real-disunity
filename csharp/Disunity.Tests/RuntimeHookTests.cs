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
        public static void Second() => Calls.Add("second");
        public static void Loaded() => Calls.Add("loaded");
        public static void Splash() => Calls.Add("splash");
        public static void Generic<T>() => Calls.Add(typeof(T).Name);
        public static void BeforeScene() => Calls.Add("before-scene");
        private static void AfterScene() => Calls.Add("after-scene");
        public static void Fails() => throw new InvalidOperationException("hook body failed");
        public static void WithParameter(int value) => Calls.Add($"parameter {value}");
        public void Instance() => Calls.Add("instance");
    }

    internal static class BrokenInit
    {
        private static readonly int Value = Throw();
        private static int Throw() => throw new InvalidOperationException("static initializer failed");
        public static void Init() => HookTargets.Calls.Add($"init {Value}");
    }

    public static void Run()
    {
        ReadsGameAssembliesInOrder();
        RejectsAssemblyTypesThatContradictNames();
        ReadsHooksWithTheirPhases();
        ReportsMalformedManifestsByFile();
        ReadsBothFilesFromADataDirectory();
        RunsPhasesInUnityOrderAndHooksInListedOrder();
        KeepsUnresolvedHooksAndFailsTheirPhase();
        NamesTheHookThatThrows();
        ReportsFailuresAndContinuesWhenAsked();
        Console.WriteLine("PASS: player manifest and startup hooks in Unity's phase order");
    }

    /// <summary>With a failure handler a phase runs as in Unity: unresolved and throwing hooks
    /// are reported and the hooks after them still run.</summary>
    private static void ReportsFailuresAndContinuesWhenAsked()
    {
        HookTargets.Calls.Clear();
        var hooks = Resolve(("", Targets, "Missing", 1), ("", Targets, "Fails", 1), ("", Targets, "BeforeScene", 1));
        var failures = new List<string>();
        hooks.Run(RuntimeInitializeLoadType.BeforeSceneLoad, (hook, error) => failures.Add($"{hook.Method}: {error.Message}"));
        Require(failures.Count == 2 && failures[0].StartsWith("Missing: no static method", StringComparison.Ordinal)
            && failures[1] == "Fails: hook body failed", "failures: " + string.Join(" | ", failures));
        Require(HookTargets.Calls.SequenceEqual(["before-scene"]), "a hook after a failing one runs");
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

    private const string Targets = "RuntimeHookTests+HookTargets";

    /// <summary>
    /// One hook per phase, listed against phase order, and two in one phase listed against
    /// alphabetical order.
    /// </summary>
    private static readonly (string, string, string, int)[] Listed =
    {
        ("", Targets, "AfterScene", 0),
        ("", Targets, "BeforeScene", 1),
        ("", Targets, "Second", 2),
        ("", Targets, "Splash", 3),
        ("", Targets, "Loaded", 2),
        ("", Targets, "Early", 4),
    };

    private static void ReadsHooksWithTheirPhases()
    {
        var hooks = PlayerManifest.ReadHooks(Json(HooksJson(("Some.Space", "Starter", "Init", 3))));
        Require(hooks.Single() == new RuntimeHook(Self, "Some.Space.Starter", "Init", RuntimeInitializeLoadType.BeforeSplashScreen),
            $"hook: {hooks.Single()}");
        RequireInvalid(() => PlayerManifest.ReadHooks(Json(HooksJson(("", "Starter", "Init", 9)))), "unknown load type 9");
    }

    private static void ReportsMalformedManifestsByFile()
    {
        RequireInvalid(() => PlayerManifest.ReadHooks(Json("{}")), "RuntimeInitializeOnLoads.json is malformed");
        RequireInvalid(() => PlayerManifest.ReadHooks(Json("{\"root\":[{\"loadTypes\":\"one\"}]}")), "RuntimeInitializeOnLoads.json is malformed");
        RequireInvalid(() => PlayerManifest.ReadAssemblies(Json("{\"names\":[]}")), "ScriptingAssemblies.json is malformed");
        RequireInvalid(() => PlayerManifest.ReadAssemblies(Json("{\"names\":[")), "ScriptingAssemblies.json is malformed");
    }

    private static void ReadsBothFilesFromADataDirectory()
    {
        var data = Directory.CreateTempSubdirectory("player-data");
        try
        {
            File.WriteAllText(Path.Combine(data.FullName, "ScriptingAssemblies.json"), "{\"names\":[\"Game.dll\"],\"types\":[16]}");
            File.WriteAllText(Path.Combine(data.FullName, "RuntimeInitializeOnLoads.json"), HooksJson(("", Targets, "Early", 4)));
            var manifest = PlayerManifest.Read(data.FullName);
            Require(manifest.GameAssemblies.SequenceEqual(new[] { "Game" }) && manifest.Hooks.Single().Method == "Early",
                "both manifest files are read");
        }
        finally { data.Delete(recursive: true); }
    }

    private static RuntimeHooks Resolve(params (string, string, string, int)[] listed) =>
        RuntimeHooks.Resolve(PlayerManifest.ReadHooks(Json(HooksJson(listed))), name =>
            name == Self ? typeof(RuntimeHookTests).Assembly : throw new FileNotFoundException($"no assembly {name}"));

    private static void RunsPhasesInUnityOrderAndHooksInListedOrder()
    {
        HookTargets.Calls.Clear();
        var hooks = Resolve(Listed);
        Require(hooks.Unresolved.Count == 0, "every listed hook resolves, including a private one");
        var unityOrder = new[]
        {
            RuntimeInitializeLoadType.SubsystemRegistration, RuntimeInitializeLoadType.AfterAssembliesLoaded,
            RuntimeInitializeLoadType.BeforeSplashScreen, RuntimeInitializeLoadType.BeforeSceneLoad,
            RuntimeInitializeLoadType.AfterSceneLoad,
        };
        Require(RuntimeHooks.PhaseOrder.SequenceEqual(unityOrder), "phases run in Unity's order");
        foreach (var phase in RuntimeHooks.PhaseOrder) hooks.Run(phase);
        var expected = new[] { "early", "second", "loaded", "splash", "before-scene", "after-scene" };
        Require(HookTargets.Calls.SequenceEqual(expected), "calls: " + string.Join(", ", HookTargets.Calls));
    }

    private static void KeepsUnresolvedHooksAndFailsTheirPhase()
    {
        HookTargets.Calls.Clear();
        var hooks = RuntimeHooks.Resolve(
            PlayerManifest.ReadHooks(Json(HooksJson(
                ("", Targets, "Early", 4),
                ("", Targets, "Loaded", 1),
                ("", "Missing", "Init", 1),
                ("", Targets, "Absent", 1),
                ("", Targets, "WithParameter", 1),
                ("", Targets, "Instance", 1),
                ("", Targets, "Generic", 1)))).Append(new RuntimeHook("Not.Shipped", "X", "Init", RuntimeInitializeLoadType.BeforeSceneLoad)),
            name => name == Self ? typeof(RuntimeHookTests).Assembly : throw new FileNotFoundException($"no assembly {name}"));
        var reasons = hooks.Unresolved.Select(u => u.Reason).ToList();
        Require(reasons.Count == 6, "unresolved: " + string.Join("; ", reasons));
        Require(reasons[0].Contains("no type Missing") && reasons[1].Contains("no static method Absent()")
            && reasons[2].Contains("WithParameter") && reasons[3].Contains("Instance") && reasons[4].Contains("Generic is generic")
            && reasons[5].Contains("no assembly Not.Shipped"),
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
        Require(HookTargets.Calls.SequenceEqual(new[] { "early" }), "no hook of a phase with an unresolved hook ran");
        try
        {
            hooks.Run((RuntimeInitializeLoadType)9);
            throw new InvalidOperationException("an undefined phase must be rejected");
        }
        catch (ArgumentOutOfRangeException) { }
    }

    private static void NamesTheHookThatThrows()
    {
        var broken = Resolve(("", "RuntimeHookTests+BrokenInit", "Init", 2));
        try
        {
            broken.Run(RuntimeInitializeLoadType.AfterAssembliesLoaded);
            throw new InvalidOperationException("a hook whose type fails to initialize must fail its phase");
        }
        catch (InvalidOperationException error) when (error.Message.StartsWith("startup hook"))
        {
            Require(error.Message.Contains("BrokenInit.Init") && error.InnerException is TypeInitializationException,
                $"the failure names the hook and keeps the initializer failure: {error.Message}");
        }
        var hooks = Resolve(("", Targets, "Fails", 1));
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
