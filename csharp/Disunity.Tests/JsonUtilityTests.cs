using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>JsonUtility.ToJson and FromJson: which fields are written, how values are shaped, and what reading tolerates.</summary>
internal static class JsonUtilityTests
{
    private enum Mode { Off = 0, On = 5 }

    [Serializable]
    private sealed class Inner
    {
        public int Id;
        public string Label = "x";
    }

    [Serializable]
    private struct Pair
    {
        public float A;
        public float B;
    }

    [Serializable]
    private class SmallBase
    {
        public int B = 1;
    }

    [Serializable]
    private sealed class Small : SmallBase
    {
        public int I = 2;
        public Mode M = Mode.On;
        public float F = 0.1f;
        public bool Flag = true;
        public string? S;
        public int[]? A;
        public List<int> L = new() { 1, 2 };
        [NonSerialized] public int Skip = 9;
        public readonly int Ro = 3;
        [SerializeField] private int _p = 4;
        private int _unmarked = 5;
        public static int Shared = 6;
        public Pair P;

        public int Hidden => _p + _unmarked;
    }

    [Serializable]
    private sealed class Everything
    {
        public long Big = long.MaxValue;
        public uint Unsigned = uint.MaxValue;
        public double D = Math.PI;
        public float F = 1.1f;
        public Inner Nested = new() { Id = 7, Label = "seven" };
        public Inner? Absent;
        public List<Inner> Items = new() { new Inner { Id = 1 }, new Inner { Id = 2, Label = "two" } };
        public Inner[] Many = { new Inner { Id = 3 } };
        public List<string> Names = new() { "a", "b\"c" };
        public Vector2 V2 = new(1, 2);
        public Vector3 V3 = new(1, 2, 3);
        public Vector4 V4 = new(1, 2, 3, 4);
        public Quaternion Q = new(0, 0, 0, 1);
        public Color C = new(0.5f, 0.25f, 0.125f, 1);
        public Mode M = Mode.On;
    }

    [Serializable]
    private sealed class WithReference
    {
        public UnityEngine.Object? Target = null;
    }

    private sealed class WithNaN
    {
        public float X = float.NaN;
    }

    private abstract class Abstract;

    public static void Run()
    {
        Writing();
        RoundTrip();
        Reading();
        Malformed();
        Rejected();
        Console.WriteLine("PASS: JsonUtility field rules, value shapes, round trip, missing and unknown names, malformed JSON and rejected types");
    }

    private static void Writing()
    {
        Ensure(ModuleFacades.Forwards(typeof(DisunityHost).Assembly, "UnityEngine.JsonUtility"), "the module facades forward JsonUtility");
        Ensure(JsonUtility.ToJson(null!) == "", "null writes an empty string");
        Ensure(JsonUtility.ToJson(new Small()) == "{\"B\":1,\"I\":2,\"M\":5,\"F\":0.1,\"Flag\":true,\"S\":\"\",\"A\":[],\"L\":[1,2],\"_p\":4,\"P\":{\"A\":0,\"B\":0}}",
            "serialized fields in declaration order, base class first, enums as integers, null string and array empty, no NonSerialized, readonly, static or unmarked private fields");
        Ensure(JsonUtility.ToJson(new Inner { Id = 3 }) == "{\"Id\":3,\"Label\":\"x\"}", "a plain class");
        Ensure(JsonUtility.ToJson(new Vector3(1, 2.5f, 3)) == "{\"x\":1,\"y\":2.5,\"z\":3}", "a vector writes its fields");
    }

    private static void RoundTrip()
    {
        var source = new Everything { Absent = null };
        source.Items.Add(new Inner { Id = 3, Label = "three" });
        var copy = JsonUtility.FromJson<Everything>(JsonUtility.ToJson(source))!;
        Ensure(copy.Big == long.MaxValue && copy.Unsigned == uint.MaxValue && copy.D == Math.PI && copy.F == 1.1f, "numbers survive");
        Ensure(copy.Nested.Id == 7 && copy.Nested.Label == "seven", "a nested class survives");
        Ensure(copy.Absent is { Id: 0, Label: "x" }, "a null class field comes back as an object of default values");
        Ensure(copy.Items.Count == 3 && copy.Items[1].Label == "two" && copy.Items[2].Id == 3, "a list of classes survives");
        Ensure(copy.Many.Length == 1 && copy.Many[0].Id == 3, "an array of classes survives");
        Ensure(copy.Names.SequenceEqual(new[] { "a", "b\"c" }), "strings are escaped and survive");
        Ensure(copy.V2 == source.V2 && copy.V3 == source.V3 && copy.V4 == source.V4 && copy.Q == source.Q && copy.C == source.C, "vectors, quaternion and color survive");
        Ensure(copy.M == Mode.On, "an enum survives");
        var small = new Small { S = "text", A = new[] { 5, 6 }, P = new Pair { A = 1.5f, B = -2 } };
        var back = JsonUtility.FromJson<Small>(JsonUtility.ToJson(small))!;
        Ensure(back.S == "text" && back.A!.SequenceEqual(new[] { 5, 6 }) && back.P.A == 1.5f && back.P.B == -2 && back.Hidden == 9, "a struct and a private SerializeField survive");
    }

    private static void Reading()
    {
        var inner = JsonUtility.FromJson<Inner>("{\"Id\":4}")!;
        Ensure(inner.Id == 4 && inner.Label == "x", "a missing field keeps its constructor value");
        Ensure(JsonUtility.FromJson<Inner>("{\"Id\":4,\"Other\":[1,2,{\"a\":null}]}")!.Id == 4, "unknown names are ignored");
        var small = JsonUtility.FromJson<Small>("{\"Skip\":5,\"Ro\":6,\"Shared\":7,\"_unmarked\":8,\"M\":0,\"I\":-3}")!;
        Ensure(small.Skip == 9 && small.Ro == 3 && Small.Shared == 6 && small.Hidden == 9, "NonSerialized, readonly, static and unmarked fields are not read");
        Ensure(small.M == Mode.Off && small.I == -3, "serialized fields are read");
        var partial = JsonUtility.FromJson<Everything>("{\"Nested\":{\"Id\":9}}")!;
        Ensure(partial.Nested.Id == 9 && partial.Nested.Label == "seven", "a nested object keeps the nested constructor values it does not mention");
        Ensure(JsonUtility.FromJson<Inner>("") == null && JsonUtility.FromJson<Inner>(null!) == null, "empty or null text gives default(T)");
        Ensure(JsonUtility.FromJson<Pair>("").A == 0, "empty text gives a default struct");
        Ensure(JsonUtility.FromJson<Pair>("{\"B\":2}").B == 2, "a struct reads its fields");
    }

    private static void Malformed()
    {
        Throws<ArgumentException>(() => JsonUtility.FromJson<Inner>("{\"Id\":"), "truncated JSON", "malformed JSON");
        Throws<ArgumentException>(() => JsonUtility.FromJson<Inner>("{\n  \"Id\" 1}"), "a missing colon", "line 2");
        Throws<ArgumentException>(() => JsonUtility.FromJson<Inner>("{\"Id\":\"x\"}"), "a string for a number", "Id");
        Throws<ArgumentException>(() => JsonUtility.FromJson<Inner>("[1]"), "an array for an object", "object");
        Throws<ArgumentException>(() => JsonUtility.FromJson<Inner>("{\"Id\":99999999999}"), "a number out of range", "Id");
    }

    private static void Rejected()
    {
        Throws<NotSupportedException>(() => JsonUtility.ToJson(new WithReference()), "an object reference", "Target");
        Throws<NotSupportedException>(() => JsonUtility.FromJson<WithReference>("{\"Target\":1}"), "an object reference", "Target");
        Throws<ArgumentException>(() => JsonUtility.FromJson<Abstract>("{}"), "an abstract type");
        Throws<ArgumentException>(() => JsonUtility.FromJson<Texture2D>("{}"), "an engine object type");
        Throws<NotSupportedException>(() => JsonUtility.ToJson(new WithNaN()), "a field that is not a number", "NaN");
    }
}
