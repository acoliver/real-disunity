using System.Reflection;
using System.Text.Json;

namespace CompatibilityBridge;

internal static class ContextualMetadata
{
    internal static void Validate(JsonElement graph, FixtureAssemblies assemblies)
    {
        foreach (var component in graph.GetProperty("components").EnumerateArray())
        {
            if (!component.TryGetProperty("script", out var script) || script.ValueKind == JsonValueKind.Null) continue;
            var type = assemblies.ResolveType(script.GetProperty("type_name").GetString()!);
            foreach (var name in new[] { "Awake", "OnEnable", "OnDisable", "OnDestroy" }) FindCallback(type, name);
            foreach (var name in new[] { "Start", "FixedUpdate", "Update", "LateUpdate" })
                if (SerializedFields.FindMethod(type, name, Type.EmptyTypes) is not null)
                    throw new NotSupportedException("contextual Cleanup does not support callback metadata " + name);
        }
    }
    internal static MethodInfo? FindCallback(Type type, string name)
    {
        var method = SerializedFields.FindMethod(type, name, Type.EmptyTypes);
        if (method is not null && (method.ContainsGenericParameters || method.IsAbstract || method.ReturnType != typeof(void)))
            throw new InvalidDataException("contextual callback requires a closed parameterless void instance method: " + type.FullName + "." + name);
        return method;
    }
}
