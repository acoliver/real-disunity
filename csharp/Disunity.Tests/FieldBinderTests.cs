using Disunity.Hosting;
using UnityEngine;

/// <summary>
/// How loaded data builds a serializable class and reports a field it cannot read. A class
/// without a parameterless constructor is created without running any constructor, so its state
/// is its serialized data; a class with one runs it. A failing field names the innermost cause.
/// </summary>
internal static class FieldBinderTests
{
    [Serializable]
    public sealed class WithoutDefault
    {
        public int count = 0;
        public string? label = "initializer";
        [NonSerialized] public bool constructed;
        public WithoutDefault(int seed) { count = seed; constructed = true; }
    }

    [Serializable]
    public sealed class WithDefault
    {
        public int count = 0;
        public string? label = "initializer";
        public WithDefault() => count = -5;
    }

    [Serializable]
    public sealed class WithPrivateDefault
    {
        [NonSerialized] public bool constructed;
        public int count = 0;
        private WithPrivateDefault() => constructed = true;
    }

    [Serializable]
    public sealed class RefusesConstruction
    {
        public int count = 0;
        public RefusesConstruction() => throw new InvalidOperationException("construction refused");
    }

    [Serializable]
    public sealed class RefusesInitialization
    {
        public int count = 0;
        static RefusesInitialization() => throw new ArgumentException("initialization refused");
        public RefusesInitialization() { }
    }

    [Serializable]
    public sealed class RefusesInitializationWithoutDefault
    {
        public int count = 0;
        static RefusesInitializationWithoutDefault() => throw new ArgumentException("static refusal without default");
        public RefusesInitializationWithoutDefault(int seed) => count = seed;
    }

    public sealed class Data : ScriptableObject
    {
        public WithoutDefault? withoutDefault = null;
        public WithDefault? withDefault = null;
        public WithPrivateDefault? privateDefault = null;
        public List<WithoutDefault> list = new();
        public WithoutDefault[] array = [];
    }

    public sealed class RefusesConstructionData : ScriptableObject { public RefusesConstruction? value = null; }
    public sealed class RefusesInitializationData : ScriptableObject { public RefusesInitialization? value = null; }
    public sealed class RefusesStaticWithoutDefaultData : ScriptableObject { public RefusesInitializationWithoutDefault? value = null; }

    public sealed class CloneHolder : MonoBehaviour
    {
        public WithoutDefault? value = null;
        public List<WithoutDefault> list = new();
    }

    public static void Run()
    {
        PlanHarness.Asset[] assets =
        [
            new(typeof(Data), "data", """
                {"withoutDefault": {"count": "7"}, "withDefault": {"count": "3"}, "privateDefault": {"count": "4"},
                 "list": [{"count": "1", "label": "one"}, {}], "array": [{"count": "2"}]}
                """),
            new(typeof(RefusesConstructionData), "refuses", """{"value": {"count": "1"}}"""),
            new(typeof(RefusesInitializationData), "init", """{"value": {"count": "1"}}"""),
            new(typeof(RefusesStaticWithoutDefaultData), "staticOnly", """{"value": {"count": "1"}}"""),
        ];
        PlanHarness.With(assets, () =>
        {
            BuildsClassesWithoutParameterlessConstructorsFromData(Load<Data>(assets[0]));
            ReportsTheInnermostCause(assets);
            ClonesClassesWithoutParameterlessConstructors();
        });
        Console.WriteLine("PASS: loaded data builds serializable classes without a default constructor and names the innermost failure");
    }

    private static T Load<T>(PlanHarness.Asset asset) where T : UnityEngine.Object =>
        Resources.Load<T>(PlanHarness.PathOf(asset)) ?? throw new InvalidOperationException($"{asset.Name} did not load");

    private static void BuildsClassesWithoutParameterlessConstructorsFromData(Data data)
    {
        var value = data.withoutDefault!;
        Expect.That(value.count == 7 && !value.constructed, "a class without a parameterless constructor loads its fields and runs no constructor");
        Expect.That(value.label is null, "its omitted field is the type's default, as no initializer ran");
        Expect.That(data.withDefault is { count: 3, label: "initializer" }, "a class with a parameterless constructor runs it, so an omitted field keeps its initializer");
        Expect.That(data.privateDefault is { constructed: true, count: 4 }, "a private parameterless constructor runs too");
        Expect.That(data.list is [{ count: 1, label: "one", constructed: false }, { count: 0, label: null }] && data.array is [{ count: 2, constructed: false }],
            "list and array elements build the same way");
    }

    private static void ReportsTheInnermostCause(PlanHarness.Asset[] assets)
    {
        Fails(assets[1], "InvalidOperationException: construction refused");
        Fails(assets[2], "ArgumentException: initialization refused");
        Fails(assets[3], "ArgumentException: static refusal without default");
    }

    private static void Fails(PlanHarness.Asset asset, string cause)
    {
        try
        {
            Resources.Load(PlanHarness.PathOf(asset));
            throw new InvalidOperationException($"{asset.Name}: a field that cannot be built loaded");
        }
        catch (SceneLoadException error)
        {
            Expect.That(error.Message.Contains($"{asset.Name}.value: cannot read", StringComparison.Ordinal) && error.Message.EndsWith($": {cause}", StringComparison.Ordinal),
                $"the message names the field and the innermost cause: {error.Message}");
            Expect.That(!error.Message.Contains("target of an invocation", StringComparison.Ordinal), "the invocation wrapper is not the reported cause");
            Expect.That(Chain(error.InnerException).Any(inner => $"{inner.GetType().Name}: {inner.Message}" == cause),
                "the original exception stays in the inner exception chain");
        }
    }

    private static IEnumerable<Exception> Chain(Exception? error)
    {
        for (; error is not null; error = error.InnerException) yield return error;
    }

    private static void ClonesClassesWithoutParameterlessConstructors()
    {
        var source = DisunityHost.AddComponent(new CloneHolder());
        source.value = new WithoutDefault(9) { label = "copied" };
        source.list.Add(new WithoutDefault(8));
        var clone = UnityEngine.Object.Instantiate(source);
        Expect.That(clone.value is { count: 9, label: "copied", constructed: false } && !ReferenceEquals(clone.value, source.value),
            "Instantiate copies a serializable class without a parameterless constructor from its serialized fields only");
        Expect.That(clone.list is [{ count: 8, constructed: false }] && !ReferenceEquals(clone.list[0], source.list[0]), "and does the same inside a list");
    }
}
