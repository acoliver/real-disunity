using Disunity.Hosting;
using UnityEngine;

/// <summary>Instantiate of a ScriptableObject: the clone is a new asset of the same class holding a copy of
/// the serialized fields, named like any clone. Nested serializable data is copied, not shared, while
/// references to other objects stay as they are. The clone enables after its fields are copied.</summary>
internal static class AssetCloneTests
{
    [Serializable]
    public sealed class Entry
    {
        public float weight = 0f;
        public List<string> tags = new();
    }

    public class BaseSettings : ScriptableObject
    {
        public int level = 0;
    }

    public sealed class Settings : BaseSettings
    {
        public string label = "";
        public Entry entry = new();
        public Entry[] entries = [];
        public List<int> numbers = new();
        public Settings? partner = null;
        [SerializeField] private float hidden = 0f;
        [NonSerialized] public int scratch = 7;
        public int enabledSaw = -1;
        public static int Enabled;

        public float Hidden { get => hidden; set => hidden = value; }
        private void OnEnable() { enabledSaw = level; Enabled++; }
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            CloneCopiesTheSerializedFields();
            CloneIsAnAssetOfItsOwn();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static Settings Original()
    {
        var partner = ScriptableObject.CreateInstance<Settings>();
        var original = ScriptableObject.CreateInstance<Settings>();
        original.name = "source";
        original.level = 3;
        original.label = "text";
        original.Hidden = 2.5f;
        original.entry.weight = 1.5f;
        original.entry.tags.Add("a");
        original.entries = [new Entry { weight = 4f }];
        original.numbers.AddRange([5, 6]);
        original.partner = partner;
        original.scratch = 99;
        return original;
    }

    private static void CloneCopiesTheSerializedFields()
    {
        var original = Original();
        var enabledBefore = Settings.Enabled;
        var clone = UnityEngine.Object.Instantiate(original);
        Expect.That(!ReferenceEquals(clone, original) && clone.name == "source(Clone)", "the clone is a new asset named like a clone");
        Expect.That(clone.level == 3 && clone.label == "text" && Math.Abs(clone.Hidden - 2.5f) < 1e-6f, "public, inherited and [SerializeField] fields are copied");
        Expect.That(clone.scratch == 7, "a [NonSerialized] field keeps what the constructor set");
        Expect.That(!ReferenceEquals(clone.entry, original.entry) && Math.Abs(clone.entry.weight - 1.5f) < 1e-6f && clone.entry.tags.SequenceEqual(["a"]), "nested serializable data is copied");
        Expect.That(!ReferenceEquals(clone.entry.tags, original.entry.tags), "including its lists");
        Expect.That(clone.entries.Length == 1 && !ReferenceEquals(clone.entries[0], original.entries[0]) && Math.Abs(clone.entries[0].weight - 4f) < 1e-6f, "arrays of serializable data are copied");
        Expect.That(clone.numbers.SequenceEqual([5, 6]) && !ReferenceEquals(clone.numbers, original.numbers), "lists are copied");
        Expect.That(ReferenceEquals(clone.partner, original.partner), "a reference to another asset stays a reference");
        Expect.That(Settings.Enabled == enabledBefore + 1 && clone.enabledSaw == 3, "the clone enables once, after its fields are copied");
        clone.entry.tags.Add("b");
        clone.numbers.Add(7);
        Expect.That(original.entry.tags.Count == 1 && original.numbers.Count == 2, "changing the clone leaves the original alone");
        Console.WriteLine("PASS: Instantiate copies a ScriptableObject's serialized fields into an asset of its own");
    }

    private static void CloneIsAnAssetOfItsOwn()
    {
        var original = Original();
        var clone = UnityEngine.Object.Instantiate(original);
        UnityEngine.Object.Destroy(clone);
        DisunityHost.Step(0);
        Expect.That(clone == null, "a destroyed clone is null");
        Expect.That(original != null, "and the original stays alive");
        Expect.That(UnityEngine.Object.Instantiate<ScriptableObject>(original) is Settings, "the clone has the original's class when asked for the base type");
        Console.WriteLine("PASS: a cloned ScriptableObject lives and dies on its own");
    }
}
