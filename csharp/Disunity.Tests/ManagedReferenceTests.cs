using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>
/// A <c>[SerializeReference]</c> value is built as the class the plan names, which may be any class that
/// fits the field's declared type, and its fields bind like any serialized class's.
/// </summary>
internal static class ManagedReferenceTests
{
    [Serializable]
    public class Group : IRenderPipelineGraphicsSettings
    {
        public int size = 1;
        public int version => 2;
    }

    [Serializable]
    public sealed class ResourceGroup : Group, IRenderPipelineResources
    {
        public string? label = null;
    }

    public abstract class AbstractGroup : IRenderPipelineGraphicsSettings
    {
        public int version => 1;
    }

    public sealed class Holder : ScriptableObject
    {
        public RenderPipelineGraphicsSettingsCollection settings = new();
        [SerializeReference] public Group? single = null;
    }

    private static string Value(Type type, string fields) =>
        "{\"$managed\": {\"class\": \"" + type.FullName!.Replace('+', '/') + "\", \"assembly\": \"" + type.Assembly.GetName().Name + "\", \"fields\": " + fields + "}}";

    public static void Run()
    {
        PlanHarness.Asset[] assets =
        [
            new(typeof(Holder), "holder",
                "{\"settings\": {\"m_SettingsList\": [" + Value(typeof(Group), "{\"size\": \"5\"}") + ", null, "
                + Value(typeof(ResourceGroup), "{\"size\": \"7\", \"label\": \"r\"}") + "]}, \"single\": " + Value(typeof(ResourceGroup), "{}") + "}"),
            new(typeof(Holder), "abstract", "{\"single\": " + Value(typeof(AbstractGroup), "{}") + "}"),
            new(typeof(Holder), "missing", """{"single": {"$managed": {"class": "Nope", "assembly": "x", "fields": {}}}}"""),
        ];
        PlanHarness.With(assets, () =>
        {
            var holder = UnityEngine.Resources.Load<Holder>(PlanHarness.PathOf(assets[0]))!;
            Expect.That(holder.settings.settingsList is [Group { size: 5 }, null, ResourceGroup { size: 7, label: "r" }], "a collection of settings groups holds the classes the plan names");
            Expect.That(holder.single is ResourceGroup { size: 1, version: 2 }, "a [SerializeReference] field holds a subclass of its declared type");
            Fails(assets[1], "single: a Group field cannot hold a ManagedReferenceTests/AbstractGroup");
            Fails(assets[2], "single: x has no class Nope");
        });
        Console.WriteLine("PASS: [SerializeReference] values load as the classes the plan names and fail when a class does not fit the field");
    }

    private static void Fails(PlanHarness.Asset asset, string message)
    {
        var error = Expect.Catch<SceneLoadException>(() => UnityEngine.Resources.Load(PlanHarness.PathOf(asset)), $"{asset.Name} must not load");
        Expect.That(error.Message.EndsWith(message, StringComparison.Ordinal), error.Message);
    }
}
