using System.Collections;
using System.Text;
using System.Text.Json.Nodes;
using Disunity.Hosting;
using UnityEngine;

/// <summary>
/// Loading a player's resources from a resource plan: text assets from their byte ranges,
/// script assets with their fields bound before <c>Awake</c>, case-insensitive paths, type
/// filtering, unbuilt classes named, and asynchronous loads that finish in a later frame.
/// </summary>
internal static class ResourceTests
{
    public sealed class Config : ScriptableObject
    {
        public int level = -1;
        public TextAsset? notes = null;
        public string? awake;
        private void Awake() => awake = $"level={level} notes={notes?.name}";
    }

    public sealed class Linked : ScriptableObject
    {
        public int value = -1;
        public Linked? other = null;
        [NonSerialized] public int seen = -1;
        private void Awake() => seen = other is null ? 0 : other.value;
    }

    private sealed class Loader : MonoBehaviour
    {
        public readonly List<string> Calls = new();
        public IEnumerator Load(ResourceRequest request)
        {
            Calls.Add($"waiting done={request.isDone}");
            yield return request;
            Calls.Add($"resumed {(request.asset as TextAsset)?.name}");
        }
    }

    public static void Run()
    {
        var directory = Directory.CreateTempSubdirectory("disunity-resources-");
        try
        {
            DisunityHost.LoadResources(WritePlan(directory.FullName), _ => typeof(ResourceTests).Assembly);
            DisunityHost.LoadWorld();
            try
            {
                LoadsTextAndScriptAssetsOnce();
                DecodesEachByteOrderMark();
                BindsAssetsThatReferenceEachOtherBeforeAwake();
                NamesTextBytesOutsideTheFile();
                FiltersByTypeAndNamesUnbuiltClasses();
                FinishesAsynchronousLoadsInALaterFrame();
            }
            finally { DisunityHost.UnloadWorld(); }
        }
        finally
        {
            DisunityHost.UnloadResources();
            directory.Delete(recursive: true);
        }
        Require(Resources.Load("text/hello") is null, "without resources nothing loads");
        DecodesTextAsUnityDoes();
        Console.WriteLine("PASS: player resources load by path with their fields, types and frames");
    }

    /// <summary>The marked texts the plan holds after "hello": "é" in each Unicode encoding
    /// with its byte order mark.</summary>
    private static readonly (string Path, Encoding Encoding)[] Marked =
    [
        ("text/utf16le", new UnicodeEncoding(bigEndian: false, byteOrderMark: true)),
        ("text/utf16be", new UnicodeEncoding(bigEndian: true, byteOrderMark: true)),
        ("text/utf32le", new UTF32Encoding(bigEndian: false, byteOrderMark: true)),
        ("text/utf32be", new UTF32Encoding(bigEndian: true, byteOrderMark: true)),
    ];

    /// <summary>A player file holding text assets after three bytes of other data, and a
    /// resource plan with those texts, a script asset that references one, two script assets
    /// that reference each other, a text whose bytes lie outside the file, a path beside the
    /// config folder and an unbuilt sprite.</summary>
    private static string WritePlan(string directory)
    {
        var data = Path.Combine(directory, "resources.assets");
        var hello = new byte[] { 0xEF, 0xBB, 0xBF }.Concat(Encoding.UTF8.GetBytes("hello")).ToArray();
        var file = new List<byte> { 1, 2, 3 };
        var assets = new JsonArray(Text("hello", data, file.Count, hello.Length));
        file.AddRange(hello);
        var entries = new JsonArray(Entry("text/hello", "TextAsset", 0));
        foreach (var (path, encoding) in Marked)
        {
            var bytes = encoding.GetPreamble().Concat(encoding.GetBytes("é")).ToArray();
            entries.Add(Entry(path, "TextAsset", assets.Count));
            assets.Add(Text(path, data, file.Count, bytes.Length));
            file.AddRange(bytes);
        }
        File.WriteAllBytes(data, [.. file, 9]);
        Directory.CreateDirectory(Path.Combine(directory, "resources"));
        File.WriteAllText(Path.Combine(directory, "resources", "1.json"), """{"level": "3", "notes": {"$asset": 0}}""");
        var linkA = assets.Count + 1;
        File.WriteAllText(Path.Combine(directory, "resources", "a.json"), """{"value": "1", "other": {"$asset": """ + (linkA + 1) + "}}");
        File.WriteAllText(Path.Combine(directory, "resources", "b.json"), """{"value": "2", "other": {"$asset": """ + linkA + "}}");
        entries.Add(Entry("config/main", "MonoBehaviour", assets.Count));
        assets.Add(Script(nameof(Config), "main", "resources/1.json"));
        entries.Add(Entry("links/a", "MonoBehaviour", assets.Count));
        assets.Add(Script(nameof(Linked), "a", "resources/a.json"));
        entries.Add(Entry("links/b", "MonoBehaviour", assets.Count));
        assets.Add(Script(nameof(Linked), "b", "resources/b.json"));
        entries.Add(Entry("broken/text", "TextAsset", assets.Count));
        assets.Add(Text("broken", data, file.Count, 100));
        entries.Add(Entry("configx/other", "TextAsset", 0));
        entries.Add(Entry("art/icon", "Sprite", null));
        var plan = new JsonObject { ["entries"] = entries, ["assets"] = assets, ["differences"] = new JsonArray() };
        var planPath = Path.Combine(directory, "resources.json");
        File.WriteAllText(planPath, plan.ToJsonString());
        return planPath;
    }

    private static JsonObject Text(string name, string file, int offset, int length) =>
        new() { ["type"] = "TextAsset", ["name"] = name, ["file"] = file, ["offset"] = offset, ["length"] = length };

    private static JsonObject Script(string type, string name, string fields) => new()
    {
        ["type"] = "Script", ["class"] = $"{nameof(ResourceTests)}/{type}",
        ["assembly"] = typeof(ResourceTests).Assembly.GetName().Name, ["name"] = name, ["fields"] = fields,
    };

    private static JsonObject Entry(string path, string type, int? asset) =>
        new() { ["path"] = path, ["class"] = type, ["asset"] = asset };

    private static void LoadsTextAndScriptAssetsOnce()
    {
        var text = Resources.Load<TextAsset>("Text/Hello");
        Require(text is { name: "hello", text: "hello", dataSize: 8 }, "a text asset loads its bytes and drops the byte order mark from its text");
        Require(ReferenceEquals(Resources.Load("text/hello/"), text), "a loaded asset is loaded once");
        var config = Resources.Load<Config>("config/main");
        Require(config is { level: 3, name: "main" } && ReferenceEquals(config.notes, text), "a script asset binds its fields and references");
        Require(config!.awake == "level=3 notes=hello", "fields bind before Awake");
    }

    private static void DecodesEachByteOrderMark()
    {
        foreach (var (path, _) in Marked)
            Require(Resources.Load<TextAsset>(path)?.text == "é", $"{path} decodes by its byte order mark");
    }

    private static void BindsAssetsThatReferenceEachOtherBeforeAwake()
    {
        var a = Resources.Load<Linked>("links/a")!;
        var b = Resources.Load<Linked>("links/b")!;
        Require(ReferenceEquals(a.other, b) && ReferenceEquals(b.other, a), "assets that reference each other resolve to each other");
        Require((a.seen, b.seen) == (2, 1), $"each Awake sees the other's fields: a={a.seen} b={b.seen}");
    }

    private static void NamesTextBytesOutsideTheFile()
    {
        try
        {
            Resources.Load("broken/text");
            throw new InvalidOperationException("resources: a text outside its file loaded");
        }
        catch (InvalidDataException error) when (error.Message.Contains("resource broken", StringComparison.Ordinal)) { }
    }

    private static void FiltersByTypeAndNamesUnbuiltClasses()
    {
        Require(Resources.LoadAll("config") is [Config], "a folder holds only the paths under it, not a sibling that shares its prefix");
        try
        {
            Task.Run(() => Resources.Load("text/hello")).Wait();
            throw new InvalidOperationException("resources: a worker thread loaded a resource");
        }
        catch (AggregateException error) when (error.InnerException is InvalidOperationException { Message: var message }
            && message.Contains("main thread", StringComparison.Ordinal)) { }
        Require(Resources.Load<TextAsset>("config/main") is null, "a load of another type finds nothing");
        Require(Resources.Load<TextAsset>("missing") is null, "a missing path finds nothing");
        Require(Resources.LoadAll<TextAsset>("text") is [{ name: "hello" }, ..] && Resources.LoadAll<TextAsset>("text").Length == 1 + Marked.Length,
            "LoadAll filters by type within a folder");
        Require(Resources.LoadAll<ScriptableObject>("config") is [Config], "LoadAll finds the assets of a folder");
        try
        {
            Resources.Load("art/icon");
            throw new InvalidOperationException("an unbuilt class loaded");
        }
        catch (NotSupportedException error) when (error.Message.Contains("art/icon is a Sprite", StringComparison.Ordinal)) { }
    }

    private static void FinishesAsynchronousLoadsInALaterFrame()
    {
        var loader = DisunityHost.AddComponent(new Loader());
        var request = Resources.LoadAsync<TextAsset>("text/hello");
        var completed = 0;
        request.completed += _ => completed++;
        loader.StartCoroutine(loader.Load(request));
        Require(!request.isDone && completed == 0 && request.progress < 1, "a request is not done in the frame it starts");
        DisunityHost.Step(0.02f);
        Require(request.isDone && completed == 1 && request.progress == 1, "a request completes in the next frame's delayed tasks");
        DisunityHost.Step(0.02f);
        Require(loader.Calls.SequenceEqual(["waiting done=False", "resumed hello"]), string.Join(" | ", loader.Calls));
        request.completed += _ => completed++;
        Require(completed == 2, "a handler added after completion runs at once");
    }

    private static void DecodesTextAsUnityDoes()
    {
        Require(new TextAsset("plain").text == "plain" && new TextAsset().text.Length == 0, "constructed text assets hold their text");
        Require(new TextAsset("é").bytes.Length == 2, "constructed text is stored as UTF-8");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException($"resources: {message}");
    }
}
