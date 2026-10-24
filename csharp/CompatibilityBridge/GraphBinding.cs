using System.Text.Json;
using UnityEngine;
using UObject = UnityEngine.Object;

namespace CompatibilityBridge;

public sealed partial class ScriptSession
{
    private void BindGraph(JsonElement graph, JsonElement handles)
    {
        foreach (var node in graph.GetProperty("nodes").EnumerateArray())
        {
            var id = node.GetProperty("id").GetString()!;
            var owner = NativeIdentity.GameObject(Handle(handles, id));
            _stable.Add(id, owner);
            if (node.GetProperty("template").GetBoolean()) _templates.Add(owner.NativeHandle, node.GetProperty("active").GetBoolean());
        }
        AddSprites(graph);
        AddFallbackSprites(graph);
        var components = graph.GetProperty("components").EnumerateArray().ToArray();
        foreach (var component in components) CreateBinding(component, handles);
        if (graph.TryGetProperty("aliases", out var aliases))
            foreach (var item in aliases.EnumerateObject())
                if (_stable.TryGetValue(item.Value.GetString()!, out var value)) _stable.TryAdd(item.Name, value);
        foreach (var component in components)
        {
            if (!component.TryGetProperty("script", out var script) || script.ValueKind == JsonValueKind.Null) continue;
            var target = Resolve(component.GetProperty("id").GetString()!);
            if (target is MonoBehaviour or ScriptableObject) _fields.Bind(target, script.GetProperty("fields"), component.GetProperty("id").GetString()!);
        }
    }
    private static ulong Handle(JsonElement handles, string id)
    {
        if (!handles.TryGetProperty(id, out var value) || !value.TryGetUInt64(out var handle)) throw new InvalidDataException($"missing native handle for imported identity '{id}'");
        return handle;
    }
    private void CreateBinding(JsonElement component, JsonElement handles)
    {
        var id = component.GetProperty("id").GetString()!;
        var hasScript = component.TryGetProperty("script", out var script) && script.ValueKind != JsonValueKind.Null;
        if (component.GetProperty("object").ValueKind == JsonValueKind.Null)
        {
            if (!hasScript) return;
            var type = ResolveType(script.GetProperty("type_name").GetString()!);
            if (!typeof(ScriptableObject).IsAssignableFrom(type)) throw new InvalidDataException($"data document '{id}' type '{type.FullName}' must derive from ScriptableObject");
            var asset = (UObject)Activator.CreateInstance(type)!;
            NativeIdentity.MarkData(asset, id);
            _stable.Add(id, asset);
            return;
        }
        if (!handles.TryGetProperty(id, out var native))
        {
            if (hasScript) throw new InvalidDataException($"missing native script component '{id}'");
            return;
        }
        var handle = native.GetUInt64();
        using var result = NativeWorld.Request("component.get", ("component", handle));
        var snapshot = NativeWorld.Data(result);
        var owner = (GameObject)Resolve(component.GetProperty("object").GetString()!);
        if (snapshot.GetProperty("object").GetUInt64() != owner.NativeHandle) throw new InvalidDataException($"component '{id}' owner handle mismatch");
        if (hasScript && snapshot.GetProperty("kind").GetString() == "Script")
        {
            var expected = script.GetProperty("type_name").GetString();
            var actual = snapshot.GetProperty("fields").GetProperty("type").GetString();
            if (actual != expected) throw new InvalidDataException($"script type mismatch at '{id}': graph '{expected}', native '{actual}'");
        }
        var instance = NativeIdentity.Component(snapshot, owner);
        _stable.Add(id, instance);
        if (instance is MonoBehaviour behaviour) _scripts.Add(handle, new ScriptState(behaviour));
    }
    private void AddSprites(JsonElement graph)
    {
        if (!graph.TryGetProperty("assets", out var assets)) return;
        foreach (var asset in assets.EnumerateArray())
        {
            if (!asset.TryGetProperty("sprites", out var sprites)) continue;
            foreach (var sprite in sprites.EnumerateArray())
            {
                var id = asset.GetProperty("guid").GetString() + ":" + sprite.GetProperty("file_id").GetInt64();
                var r = sprite.GetProperty("rect");
                _stable.TryAdd(id, new Sprite(id, new Rect(r[0].GetSingle(), r[1].GetSingle(), r[2].GetSingle(), r[3].GetSingle()), sprite.GetProperty("pixels_per_unit").GetSingle()));
            }
        }
    }
    private void AddFallbackSprites(JsonElement graph)
    {
        if (!graph.TryGetProperty("fallback_sprites", out var sprites)) return;
        foreach (var fallback in sprites.EnumerateArray())
        {
            var id = fallback.GetProperty("name").GetString()!;
            var sprite = fallback.GetProperty("sprite");
            var rect = sprite.GetProperty("rect");
            _stable.Add(id, new Sprite(id, new Rect(rect[0].GetSingle(), rect[1].GetSingle(), rect[2].GetSingle(), rect[3].GetSingle()), sprite.GetProperty("pixels_per_unit").GetSingle()));
        }
    }
    private UObject ResolveReference(JsonElement reference)
    {
        if (reference.TryGetProperty("$builtin", out var builtin)) return Resolve(builtin.GetString()!);
        if (!reference.TryGetProperty("$ref", out var id)) throw new InvalidDataException($"unresolved serialized reference {reference.GetRawText()}");
        return id.ValueKind == JsonValueKind.Number ? NativeIdentity.Resolve(id.GetUInt64()) : Resolve(id.GetString()!);
    }
}
