using System.Text;
using System.Text.Json;
using Disunity.Hosting;
using UnityEngine;

/// <summary>
/// Script layouts from assembly metadata. The scripts are classes in this assembly, read
/// through their metadata with Disunity.Engine and the .NET class library, as a player's
/// scripts are read with its Managed directory.
/// </summary>
internal static class ScriptLayoutTests
{
    public enum Small : byte { A, B }

    public enum Wide : long { A }

    public enum WideUnsigned : ulong { A }

    public interface IShape { }

    [Serializable] public struct Pair { public int Count = 1; public float Weight = 1; public Pair() { } }

    [Serializable] public class Node { public string Name = ""; public List<Node> Children = new(); public Node? Next = null; }

    [Serializable] public class GenericBase<T> { public T Item = default!; }

    [Serializable] public class FromGeneric : GenericBase<int> { public int Extra = 1; }

    [Serializable] public class Loose { public int Value = 1; }

    [Serializable] public class Circle : IShape { public float Radius = 1; }

    [Serializable] public class Square : Circle { public float Side = 1; }

    [Serializable] public abstract class AbstractValue { public int Lost = 1; }

    public class Plain { public int Lost = 1; }

    [Serializable] public class SerialMap<TKey, TValue> : Dictionary<TKey, TValue> where TKey : notnull { [SerializeField] List<TKey> m_Keys = new(); [SerializeField] List<TValue> m_Values = new(); }

    [Serializable] public class SerialTable : Dictionary<string, int> { public List<string> names = new(); }

    [Serializable] public class Box<T> { public T Item = default!; }

    public class Base : MonoBehaviour { [SerializeField] protected int baseValue = 1; }

#pragma warning disable CS0169, CS0414, CS0649
    public sealed class LayoutScript : Base
    {
        public int number;
        [SerializeField] private bool flag;
        private int hidden;
        [NonSerialized] public int skipped;
        public static int shared;
        public readonly int fixedValue;
        public const int Constant = 1;
        public Small small;
        public Wide wide;
        public WideUnsigned wideUnsigned;
        public DayOfWeek day;
        public char letter;
        public Vector3 position;
        public GameObject? target;
        public int[] numbers = Array.Empty<int>();
        public List<Pair> pairs = new();
        public List<int[]> nested = new();
        public Dictionary<int, int> map = new();
        public SerialMap<int, Vector3> serialMap = new();
        public SerialTable table = new();
        public HashSet<int> set = new();
        public Node tree = new();
        public Plain plain = new();
        public AbstractValue? abstractValue;
        public Box<string> boxed = new();
        [SerializeReference] public IShape? shape;
        [SerializeReference] public List<IShape> shapes = new();
        [SerializeReference] public object? any;
        public FromGeneric fromGeneric = new();
        public Action? callback;
        public object? anything;
    }
#pragma warning restore CS0169, CS0414, CS0649

    public static void Run()
    {
        using var layouts = Load();
        var key = layouts.Script(Self, typeof(LayoutScript).FullName!.Replace('+', '/'))
            ?? throw new InvalidOperationException("the test script was not found");
        SerializesUnityFieldsInDeclarationOrderBaseFirst(layouts, key);
        DescribesNestedAndGenericTypes(layouts);
        AddsLayoutsForReferenceTargets(layouts);
        ListsScriptClasses(layouts);
        WritesJson(layouts, key);
        Console.WriteLine("PASS: script layouts follow Unity's serialization rules");
    }

    private static readonly string Self = typeof(ScriptLayoutTests).Assembly.GetName().Name!;

    private static ScriptLayouts Load()
    {
        // The assemblies this one references, as a player's Managed directory holds those its
        // scripts reference, and the core library their type forwarders lead to.
        var self = typeof(ScriptLayoutTests).Assembly;
        var paths = self.GetReferencedAssemblies()
            .Select(name => System.Reflection.Assembly.Load(name).Location)
            .Append(self.Location)
            .Append(typeof(object).Assembly.Location)
            .Where(path => path.Length > 0)
            .Distinct();
        return new ScriptLayouts(paths.Select(path => (Path.GetFileNameWithoutExtension(path), File.ReadAllBytes(path))));
    }

    private static string Nested(string name) => $"{Self}:{typeof(ScriptLayoutTests).FullName}/{name}";

    private static void SerializesUnityFieldsInDeclarationOrderBaseFirst(ScriptLayouts layouts, string key)
    {
        var fields = layouts.Types[key].Fields;
        Require(string.Join(" ", fields.Select(field => field.Name))
            == "baseValue number flag small day letter position target numbers pairs serialMap table tree boxed shape shapes any fromGeneric",
            $"serialized fields: {string.Join(" ", fields.Select(field => field.Name))}");
        Require(Type(fields, "flag") == new ScriptPrimitive("bool"), "a [SerializeField] bool");
        Require(Type(fields, "small") == new ScriptPrimitive("UInt8"), "an enum is its underlying type");
        Require(Type(fields, "day") == new ScriptPrimitive("int"), "a class library enum is serialized");
        Require(Type(fields, "letter") == new ScriptPrimitive("char"), "a char");
        Require(Type(fields, "any") == new ScriptReference("System.Object"), "a reference to any class");
        Require(Type(fields, "position") == new ScriptBuiltin("UnityEngine.Vector3"), "Vector3 is builtin");
        Require(Type(fields, "target") == new ScriptObject("UnityEngine.GameObject"), "an object reference");
        Require(Type(fields, "numbers") == new ScriptArray(new ScriptPrimitive("int")), "an int array");
        Require(Type(fields, "pairs") == new ScriptArray(new ScriptComposite(Nested("Pair"))), "a list of structs");
        Require(Type(fields, "shape") == new ScriptReference(typeof(IShape).FullName!.Replace('+', '/')), "a reference");
        Require(Type(fields, "shapes") is ScriptArray { Element: ScriptReference }, "a list of references");
        DescribesCollectionSubclasses(layouts, fields);
    }

    /// <summary>A class deriving from a collection is described by its own fields; the collection's are not
    /// serialized, and a bare collection has no layout.</summary>
    private static void DescribesCollectionSubclasses(ScriptLayouts layouts, IReadOnlyList<ScriptField> fields)
    {
        var map = Type(fields, "serialMap") as ScriptComposite ?? throw new InvalidOperationException("a dictionary subclass has a layout");
        var mapFields = layouts.Types[map.Key].Fields;
        Require(mapFields.Select(f => f.Name).SequenceEqual(new[] { "m_Keys", "m_Values" })
            && mapFields[0].Type == new ScriptArray(new ScriptPrimitive("int")) && mapFields[1].Type == new ScriptArray(new ScriptBuiltin("UnityEngine.Vector3")),
            "a generic dictionary subclass lists its key and value lists with the type arguments applied");
        var table = Type(fields, "table") as ScriptComposite ?? throw new InvalidOperationException("a dictionary subclass has a layout");
        Require(layouts.Types[table.Key].Fields.Select(f => f.Name).SequenceEqual(new[] { "names" }), "a dictionary subclass keeps its own fields only");
    }

    private static void DescribesNestedAndGenericTypes(ScriptLayouts layouts)
    {
        var pair = layouts.Types[Nested("Pair")];
        Require(pair.ValueType && pair.Fields.Select(f => f.Name).SequenceEqual(new[] { "Count", "Weight" }), "Pair layout");
        var node = layouts.Types[Nested("Node")];
        Require(!node.ValueType && Type(node.Fields, "Children") == new ScriptArray(new ScriptComposite(Nested("Node"))),
            "a class with a list of itself refers to its own key");
        Require(node.Fields.All(f => f.Name != "Next"), "a field of the class's own type is left out");
        var derived = layouts.Types[Nested("FromGeneric")];
        Require(derived.Fields.Select(f => f.Name).SequenceEqual(new[] { "Item", "Extra" })
            && Type(derived.Fields, "Item") == new ScriptPrimitive("int"), "a generic base class's fields are bound");
        var boxed = layouts.Types.Keys.Single(k => k.StartsWith(Nested("Box`1"), StringComparison.Ordinal));
        Require(boxed.EndsWith("[String]", StringComparison.Ordinal), $"generic key {boxed}");
        Require(Type(layouts.Types[boxed].Fields, "Item") == new ScriptPrimitive("string"), "a generic argument is substituted");
    }

    private static void AddsLayoutsForReferenceTargets(ScriptLayouts layouts)
    {
        Require(!layouts.Types.ContainsKey(Nested("Circle")), "reference targets are added on request");
        layouts.AddReferenceTargets();
        var square = layouts.Types[Nested("Square")];
        Require(square.Fields.Select(f => f.Name).SequenceEqual(new[] { "Radius", "Side" }), "base class fields come first");
        Require(layouts.Types.ContainsKey(Nested("Circle")), "an implementing class gets a layout");
        Require(!layouts.Types.ContainsKey(Nested("AbstractValue")), "abstract classes are not serialized");
        Require(layouts.Types.ContainsKey(Nested("Loose")) && !layouts.Types.Keys.Any(k => k.StartsWith("System.", StringComparison.Ordinal)),
            "a System.Object reference reaches every serializable class outside the class library");
        Require(layouts.Unresolved.Count == 0, $"unresolved: {string.Join(", ", layouts.Unresolved)}");
    }

    private static void ListsScriptClasses(ScriptLayouts layouts)
    {
        var scripts = layouts.Scripts(Self);
        Require(scripts.Contains(Nested("LayoutScript")) && scripts.Contains(Nested("Base")), "MonoBehaviour classes are scripts");
        Require(!scripts.Contains(Nested("Node")), "a serializable class is not a script");
    }

    private static void WritesJson(ScriptLayouts layouts, string key)
    {
        using var stream = new MemoryStream();
        using (var writer = new Utf8JsonWriter(stream)) layouts.Write(writer, new[] { key });
        using var document = JsonDocument.Parse(Encoding.UTF8.GetString(stream.ToArray()));
        var root = document.RootElement;
        Require(root.GetProperty("scripts")[0].GetString() == key, "scripts are listed");
        var first = root.GetProperty("types").GetProperty(key).GetProperty("fields")[0];
        Require(first.GetProperty("name").GetString() == "baseValue"
            && first.GetProperty("type").GetProperty("primitive").GetString() == "int", "field JSON");
        var pairs = root.GetProperty("types").GetProperty(key).GetProperty("fields")[9].GetProperty("type");
        Require(pairs.GetProperty("array").GetProperty("composite").GetString() == Nested("Pair"), "nested type JSON");
    }

    private static ScriptFieldType Type(IReadOnlyList<ScriptField> fields, string name) =>
        fields.Single(field => field.Name == name).Type;

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException($"script layouts: {message}");
    }
}
