using Disunity.Hosting;
using UnityEngine;

/// <summary>
/// A serializable class that derives from a collection is loaded and cloned through its own serialized
/// fields, and receives its callbacks like any other serialized object. A collection type with no
/// serialized fields of its own cannot be loaded.
/// </summary>
internal static class SerializedCollectionTests
{
    [Serializable]
    public sealed class SerialDictionary<TKey, TValue> : Dictionary<TKey, TValue>, ISerializationCallbackReceiver where TKey : notnull
    {
        [SerializeField] private List<TKey> m_Keys = new();
        [SerializeField] private List<TValue> m_Values = new();
        [NonSerialized] public int afterCalls;

        public void OnBeforeSerialize()
        {
            m_Keys.Clear();
            m_Values.Clear();
            foreach (var (key, value) in this)
            {
                m_Keys.Add(key);
                m_Values.Add(value);
            }
        }

        public void OnAfterDeserialize()
        {
            afterCalls++;
            Clear();
            for (var i = 0; i < m_Keys.Count; i++) this[m_Keys[i]] = m_Values[i];
        }
    }

    [Serializable]
    public sealed class Holder : ISerializationCallbackReceiver
    {
        public string tag = "";
        public SerialDictionary<string, Vector3> nested = new();
        [NonSerialized] public int nestedCountWhenLoaded = -1;
        public void OnBeforeSerialize() { }
        public void OnAfterDeserialize() => nestedCountWhenLoaded = nested.Count;
    }

    public sealed class Data : ScriptableObject
    {
        public SerialDictionary<string, int> table = new();
        public Holder holder = new();
    }

    public sealed class BareSet : ScriptableObject { public HashSet<int> set = new(); }

    public sealed class BareTable : ScriptableObject { public Dictionary<string, int> table = new(); }

    public sealed class CloneData : MonoBehaviour
    {
        public SerialDictionary<string, int> table = new();
        public Holder holder = new();
    }

    public static void Run()
    {
        PlanHarness.Asset[] assets =
        [
            new(typeof(Data), "data", """
                {"table": {"m_Keys": ["a", "b"], "m_Values": ["1", "2"]},
                 "holder": {"tag": "t", "nested": {"m_Keys": ["k"], "m_Values": [{"x": "1", "y": "2", "z": "3"}]}}}
                """),
            new(typeof(BareSet), "set", """{"set": {}}"""),
            new(typeof(BareTable), "bare", """{"table": {}}"""),
        ];
        PlanHarness.With(assets, () =>
        {
            LoadsADictionaryThroughItsSerializedFields(Resources.Load<Data>(PlanHarness.PathOf(assets[0]))!);
            RejectsCollectionsWithoutSerializedFields(assets);
            ClonesADictionaryThroughItsSerializedFields();
        });
        Console.WriteLine("PASS: classes deriving from collections load and clone through their serialized fields; bare collections are rejected");
    }

    private static void LoadsADictionaryThroughItsSerializedFields(Data data)
    {
        Expect.That(data.table.Count == 2 && data.table["a"] == 1 && data.table["b"] == 2, "a script field of a dictionary subclass is filled from its key and value lists");
        Expect.Equal(1, data.table.afterCalls, "OnAfterDeserialize of the dictionary runs once");
        Expect.That(data.holder.tag == "t" && data.holder.nested["k"] == new Vector3(1, 2, 3), "a dictionary nested in a serializable class is filled too");
        Expect.Equal(1, data.holder.nested.afterCalls, "OnAfterDeserialize of the nested dictionary runs once");
        Expect.Equal(1, data.holder.nestedCountWhenLoaded, "the nested dictionary is complete before its owner's OnAfterDeserialize runs");
    }

    private static void RejectsCollectionsWithoutSerializedFields(PlanHarness.Asset[] assets)
    {
        Fails(assets[1], "set: System.Collections.Generic.HashSet`1[System.Int32] fields cannot be loaded");
        Fails(assets[2], "bare.table: System.Collections.Generic.Dictionary`2[System.String,System.Int32] fields cannot be loaded");
    }

    private static void Fails(PlanHarness.Asset asset, string message)
    {
        var error = Expect.Catch<SceneLoadException>(() => Resources.Load(PlanHarness.PathOf(asset)), $"{asset.Name} must not load");
        Expect.That(error.Message.EndsWith(message, StringComparison.Ordinal) || error.Message.Contains(message, StringComparison.Ordinal), error.Message);
    }

    private static void ClonesADictionaryThroughItsSerializedFields()
    {
        var source = DisunityHost.AddComponent(new CloneData());
        source.table["x"] = 5;
        source.holder.nested["y"] = new Vector3(4, 5, 6);
        var clone = UnityEngine.Object.Instantiate(source);
        Expect.That(clone.table.Count == 1 && clone.table["x"] == 5 && !ReferenceEquals(clone.table, source.table), "Instantiate copies the dictionary's entries into a new dictionary");
        Expect.That(clone.holder.nested["y"] == new Vector3(4, 5, 6) && !ReferenceEquals(clone.holder.nested, source.holder.nested), "and does the same for a dictionary nested in a serializable class");
        Expect.Equal(1, clone.holder.nestedCountWhenLoaded, "the clone's nested dictionary is complete before its owner's OnAfterDeserialize runs");
        source.table["z"] = 6;
        Expect.That(clone.table.Count == 1, "the clone does not share entries with the source");
    }
}
