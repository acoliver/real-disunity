using System.Runtime.CompilerServices;
using System.Runtime.Loader;
using System.Security.Cryptography;
using Disunity.Hosting;
using Mono.Cecil;
using Mono.Cecil.Cil;

/// <summary>
/// A game passes a fresh delegate for a static callback to native code, which keeps the function pointer and
/// calls it later. A player's own runtime keeps that pointer callable; the host does so by rewriting the player's
/// assemblies at load so such a delegate is made once (<see cref="PlayerCallbackRewriter"/>,
/// <see cref="NativeCallbacks"/>), and loads the rewritten image from a cache directory.
/// </summary>
internal static class NativeCallbackTests
{
    private static readonly System.Reflection.Assembly Engine = typeof(DisunityHost).Assembly;

    public static void Run()
    {
        RewritesMarkedStaticCallbackDelegatesToOneKeptInstance();
        LeavesInstanceTargetsAndUnmarkedMethodsAlone();
        ReportsAnAssemblyWithoutMarkedMethodsAsUnchanged();
        LoadsARewrittenAssemblyFromTheCacheUnderItsOwnFileName();
        ReusesACachedImageAndLoadsOriginalsWhenNothingChanges();
        FindsTypesTheAssemblyReferencesInItsOwnDirectoryOrLeavesItUnchanged();
        KeepsOneDelegatePerTypeAndMethod();
        Console.WriteLine("PASS: native callbacks made from static methods stay callable (load-time rewrite and cache)");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static Type LoadHolder(byte[] image, AssemblyLoadContext context) =>
        context.LoadFromStream(new MemoryStream(image)).GetType("Game.Holder", throwOnError: true)!;

    private static Delegate Call(Type holder, string name, params object?[] arguments) =>
        (Delegate)holder.GetMethod(name)!.Invoke(null, arguments)!;

    private static int Invoke(Delegate callback, int value) => (int)callback.DynamicInvoke(value)!;

    private static CallbackRewrite RewriteBuilt(bool marked = true) => PlayerCallbackRewriter.Rewrite(CallbackAssembly.Build(marked));

    private static void RewritesMarkedStaticCallbackDelegatesToOneKeptInstance()
    {
        var rewrite = RewriteBuilt();
        Require(rewrite.Image is not null && rewrite.Sites == 1, $"one site is rewritten, got {rewrite.Sites}");
        var holder = LoadHolder(rewrite.Image!, new AssemblyLoadContext("callbacks", isCollectible: false));
        var first = Call(holder, "Make");
        Require(ReferenceEquals(first, Call(holder, "Make")), "a marked static callback is one delegate however often it is made");
        Require(Invoke(first, 41) == 42, "the kept delegate still reaches the method");
        var plain1 = Call(holder, "MakePlain");
        Require(!ReferenceEquals(plain1, Call(holder, "MakePlain")), "an unmarked static method still gets a new delegate each time");
        Require(Invoke(plain1, 40) == 42, "an unmarked delegate still reaches the method");
        var weak = MakeWeak(holder);
        GC.Collect();
        GC.WaitForPendingFinalizers();
        GC.Collect();
        Require(weak.IsAlive, "a kept delegate is not collected, so its native thunk stays valid");
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static WeakReference MakeWeak(Type holder) => new(Call(holder, "Make"));

    private static void LeavesInstanceTargetsAndUnmarkedMethodsAlone()
    {
        var image = RewriteBuilt().Image!;
        using var module = ModuleDefinition.ReadModule(new MemoryStream(image));
        var holder = module.GetType("Game.Holder");
        Require(Creates(holder, "Make") is null && Keeps(holder, "Make"), "Make calls Keep in place of the delegate constructor");
        foreach (var name in new[] { "MakePlain", "MakeNullInstance", "MakeInstance" })
            Require(Creates(holder, name) is not null && !Keeps(holder, name), $"{name} keeps its delegate constructor");
    }

    private static Instruction? Creates(TypeDefinition holder, string method) =>
        holder.Methods.Single(m => m.Name == method).Body.Instructions.SingleOrDefault(i => i.OpCode == OpCodes.Newobj);

    private static bool Keeps(TypeDefinition holder, string method) =>
        holder.Methods.Single(m => m.Name == method).Body.Instructions
            .Any(i => i.OpCode == OpCodes.Call && i.Operand is MethodReference { Name: "Keep" } reference
                && reference.DeclaringType.FullName == "Disunity.Hosting.NativeCallbacks");

    private static void ReportsAnAssemblyWithoutMarkedMethodsAsUnchanged()
    {
        var rewrite = RewriteBuilt(marked: false);
        Require(rewrite.Image is null && rewrite.Sites == 0, "an assembly with no marked method is unchanged");
    }

    private static string Hash(byte[] image) => Convert.ToHexString(SHA256.HashData(image)).ToLowerInvariant();

    private static PlayerAssemblies Player(string managed, string? cache, List<string> log, AssemblyLoadContext context)
    {
        var player = new PlayerAssemblies(managed, Engine, new[] { "Game" }, callbackCache: cache, log: log.Add);
        player.Attach(context);
        return player;
    }

    private static void LoadsARewrittenAssemblyFromTheCacheUnderItsOwnFileName()
    {
        WithDirectories((managed, cache) =>
        {
            var original = CallbackAssembly.Build();
            File.WriteAllBytes(Path.Combine(managed, "Game.dll"), original);
            File.WriteAllBytes(Path.Combine(managed, "Game.pdb"), [1, 2, 3]);
            var log = new List<string>();
            var context = new AssemblyLoadContext("cached", isCollectible: false);
            Player(managed, cache, log, context);
            var assembly = context.LoadFromAssemblyName(new System.Reflection.AssemblyName("Game"));
            var expected = Path.Combine(cache, Hash(original), "Game.dll");
            Require(assembly.Location == expected, $"the rewritten assembly loads from {expected}, got {assembly.Location}");
            Require(File.Exists(expected) && File.ReadAllBytes(Path.Combine(cache, Hash(original), "Game.pdb")).Length == 3,
                "the cache holds the rewritten image and the original's symbols");
            Require(log.SequenceEqual(new[] { "native callbacks kept: Game (1 sites)" }), "one line is logged: " + string.Join(" | ", log));
            var holder = assembly.GetType("Game.Holder", throwOnError: true)!;
            Require(ReferenceEquals(Call(holder, "Make"), Call(holder, "Make")), "the cached assembly keeps its callback delegate");
        });
    }

    private static void ReusesACachedImageAndLoadsOriginalsWhenNothingChanges()
    {
        WithDirectories((managed, cache) =>
        {
            var original = CallbackAssembly.Build();
            File.WriteAllBytes(Path.Combine(managed, "Game.dll"), original);
            var cached = Path.Combine(cache, Hash(original), "Game.dll");
            var first = new AssemblyLoadContext("first", isCollectible: false);
            Player(managed, cache, [], first).Resolve(first, new System.Reflection.AssemblyName("Game"));
            var stamp = new DateTime(2001, 2, 3, 4, 5, 6, DateTimeKind.Utc);
            File.SetLastWriteTimeUtc(cached, stamp);
            var second = new AssemblyLoadContext("second", isCollectible: false);
            var log = new List<string>();
            var reused = Player(managed, cache, log, second).Resolve(second, new System.Reflection.AssemblyName("Game"));
            Require(reused?.Location == cached && File.GetLastWriteTimeUtc(cached) == stamp, "a cache file for the same image is reused");
            Require(log.SequenceEqual(new[] { "native callbacks kept: Game (cached)" }), "a reused image is logged: " + string.Join(" | ", log));

            var unmarked = CallbackAssembly.Build(marked: false);
            File.WriteAllBytes(Path.Combine(managed, "Game.dll"), unmarked);
            var third = new AssemblyLoadContext("third", isCollectible: false);
            var plain = Player(managed, cache, log, third).Resolve(third, new System.Reflection.AssemblyName("Game"));
            Require(plain?.Location == Path.Combine(managed, "Game.dll"), "an unchanged assembly loads from its original path");
            Require(!Directory.Exists(Path.Combine(cache, Hash(unmarked))), "an unchanged assembly writes nothing to the cache");

            // A loaded file stays mapped, so the original image goes to a directory of its own.
            var bare = Directory.CreateDirectory(Path.Combine(managed, "..", "Bare")).FullName;
            File.WriteAllBytes(Path.Combine(bare, "Game.dll"), original);
            var fourth = new AssemblyLoadContext("fourth", isCollectible: false);
            var off = new PlayerAssemblies(bare, Engine, new[] { "Game" });
            off.Attach(fourth);
            Require(off.Resolve(fourth, new System.Reflection.AssemblyName("Game"))?.Location == Path.Combine(bare, "Game.dll"),
                "without a cache directory the original loads");
        });
    }

    private static void FindsTypesTheAssemblyReferencesInItsOwnDirectoryOrLeavesItUnchanged()
    {
        WithDirectories((managed, cache) =>
        {
            // Writing a rewritten assembly needs the definition of an enum its parameter default names.
            File.WriteAllBytes(Path.Combine(managed, "Dep.dll"), CallbackAssembly.BuildDependency());
            var image = CallbackAssembly.Build(enumDefault: true);
            var found = PlayerCallbackRewriter.Rewrite(image, managed);
            Require(found.Image is not null && found.Sites == 1 && found.Failure is null, "the search directory supplies the enum: " + found.Failure);
            var missing = PlayerCallbackRewriter.Rewrite(image);
            Require(missing.Image is null && missing.Sites == 1 && missing.Failure is { Length: > 0 } && missing.Failure.Contains("Dep"),
                "without the enum's assembly the image is unchanged and says why: " + missing.Failure);

            var bare = Directory.CreateDirectory(Path.Combine(managed, "..", "Lonely")).FullName;
            File.WriteAllBytes(Path.Combine(bare, "Game.dll"), image);
            var log = new List<string>();
            var context = new AssemblyLoadContext("lonely", isCollectible: false);
            var game = Player(bare, cache, log, context).Resolve(context, new System.Reflection.AssemblyName("Game"));
            Require(game?.Location == Path.Combine(bare, "Game.dll"), "an assembly that cannot be rewritten loads from its original path");
            Require(log.Count == 1 && log[0].StartsWith("native callbacks not kept: Game (1 sites): "), "the failure is logged: " + string.Join(" | ", log));
        });
    }

    private static void KeepsOneDelegatePerTypeAndMethod()
    {
        var plus = Pointer(nameof(Plus));
        var first = NativeCallbacks.Keep<Func<int, int>>(null, plus);
        Require(ReferenceEquals(first, NativeCallbacks.Keep<Func<int, int>>(null, plus)), "one delegate per delegate type and method");
        Require(first(1) == 11, "the kept delegate calls the method");
        var other = NativeCallbacks.Keep<Converter<int, int>>(null, plus);
        Require(other(1) == 11 && !ReferenceEquals(first, other), "another delegate type with the same method gets its own delegate");
        var minus = NativeCallbacks.Keep<Func<int, int>>(null, Pointer(nameof(Minus)));
        Require(minus(1) == -9 && !ReferenceEquals(first, minus), "another method gets its own delegate");

        var instance = new Counter(100);
        var method = typeof(Counter).GetMethod(nameof(Counter.Add))!.MethodHandle.GetFunctionPointer();
        var one = NativeCallbacks.Keep<Func<int, int>>(instance, method);
        var two = NativeCallbacks.Keep<Func<int, int>>(instance, method);
        Require(!ReferenceEquals(one, two) && one(1) == 101 && two(2) == 102, "a delegate with a target is new each time and uses the target");
    }

    private static IntPtr Pointer(string name) => typeof(NativeCallbackTests)
        .GetMethod(name, System.Reflection.BindingFlags.Static | System.Reflection.BindingFlags.NonPublic)!.MethodHandle.GetFunctionPointer();

    private static int Plus(int value) => value + 10;

    private static int Minus(int value) => value - 10;

    private sealed class Counter(int start)
    {
        public int Add(int value) => start + value;
    }

    private static void WithDirectories(Action<string, string> test)
    {
        var root = Directory.CreateTempSubdirectory("native-callbacks");
        try
        {
            var managed = Directory.CreateDirectory(Path.Combine(root.FullName, "Managed")).FullName;
            test(managed, Path.Combine(root.FullName, "cache"));
        }
        finally
        {
            try { root.Delete(recursive: true); }
            catch (Exception error) when (error is IOException or UnauthorizedAccessException)
            {
                Console.Error.WriteLine($"could not remove {root.FullName}: {error.Message}");
            }
        }
    }
}
