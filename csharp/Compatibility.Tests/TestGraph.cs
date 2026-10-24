using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;

internal sealed record ScriptSpec(string Id, GameObject Owner, string Type, object Fields);
internal static class TestGraph
{
    public static JsonDocument Payload(ScriptSpec[] scripts, Dictionary<string, UnityEngine.Object>? references = null, HashSet<ulong>? templates = null)
    {
        var handles = references?.ToDictionary(p => p.Key, p => p.Value.NativeHandle) ?? new Dictionary<string, ulong>();
        var nodes = scripts.Select(s => s.Owner).Concat((references?.Values ?? Enumerable.Empty<UnityEngine.Object>()).OfType<GameObject>()).DistinctBy(o => o.NativeHandle).ToArray();
        foreach (var node in nodes) handles.TryAdd("object-" + node.NativeHandle, node.NativeHandle);
        var components = scripts.Select(s =>
        {
            var handle = BinderBehavior.AddScript(s.Owner, s.Type);
            handles.Add(s.Id, handle);
            return new { id = s.Id, @object = "object-" + s.Owner.NativeHandle, script = new { type_name = s.Type, fields = s.Fields } };
        }).ToArray();
        return JsonDocument.Parse(JsonSerializer.Serialize(new
        {
            assemblies = new[] { typeof(TestGraph).Assembly.Location }, handles,
            graph = new
            {
                nodes = nodes.Select(n => new { id = "object-" + n.NativeHandle, template = templates?.Contains(n.NativeHandle) ?? false, active = true }).ToArray(),
                components, assets = Array.Empty<object>(),
                aliases = references?.Where(p => p.Value is GameObject).ToDictionary(p => p.Key, p => "object-" + p.Value.NativeHandle) ?? new Dictionary<string, string>()
            }, profile = new { id = "compat-6000.3" }
        }));
    }
    public static object Ref(string id) => new Dictionary<string, string> { ["$ref"] = id };
    public static void Advance(ScriptSession session, float dt, string phase = "Update")
    {
        NativeWorld.Request("frame.begin", ("dt", dt)).Dispose(); session.Dispatch(phase);
    }
}
