using System.Collections;
using System.Reflection;
using System.Text.Json;
using UnityEngine;
using UnityEngine.Events;
using UObject = UnityEngine.Object;

namespace CompatibilityBridge;

internal sealed class SerializedFields(Func<JsonElement, UObject> resolve, JsonElement profile, List<string> diagnostics)
{
    public static IEnumerable<FieldInfo> Fields(Type type)
    {
        for (var t = type; t is not null && t != typeof(MonoBehaviour) && t != typeof(ScriptableObject) && t != typeof(UObject); t = t.BaseType)
            foreach (var field in t.GetFields(BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly))
                if (!field.IsInitOnly && !field.IsLiteral && !field.IsDefined(typeof(NonSerializedAttribute)) && (field.IsPublic || field.IsDefined(typeof(SerializeField)))) yield return field;
    }
    public void Bind(object target, JsonElement values, string path)
    {
        var fields = Fields(target.GetType()).ToDictionary(f => f.Name);
        foreach (var item in values.EnumerateObject())
        {
            var name = Alias(target.GetType(), item.Name);
            if (name != item.Name && values.TryGetProperty(name, out _)) continue;
            if (!fields.TryGetValue(name, out var field)) { Unknown(target.GetType(), item.Name, path); continue; }
            try { field.SetValue(target, Convert(item.Value, field.FieldType, path + "." + item.Name)); }
            catch (Exception error) { throw new InvalidDataException($"serialized field '{path}.{item.Name}' ({field.FieldType.FullName}): {error.Message}", error); }
        }
    }
    private string Alias(Type type, string name)
    {
        if (profile.ValueKind == JsonValueKind.Object && profile.TryGetProperty("field_aliases", out var aliases) && aliases.TryGetProperty(type.FullName + "." + name, out var alias)) return alias.GetString()!;
        return name;
    }
    private void Unknown(Type type, string name, string path)
    {
        var message = $"unknown serialized field '{type.FullName}.{name}' at '{path}'";
        if (profile.ValueKind == JsonValueKind.Object && profile.TryGetProperty("unknown_fields", out var policy) && policy.GetString() == "diagnose") diagnostics.Add(message);
        else throw new MissingFieldException(message);
    }
    public object? Convert(JsonElement value, Type type, string path)
    {
        if (value.ValueKind == JsonValueKind.Null) return type.IsValueType ? Activator.CreateInstance(type) : null;
        if (typeof(UObject).IsAssignableFrom(type))
        {
            var resolved = resolve(value);
            if (!type.IsInstanceOfType(resolved)) throw new InvalidDataException($"'{path}' resolved {resolved.GetType().FullName}, expected {type.FullName}");
            return resolved;
        }
        if (type == typeof(string)) return value.GetString();
        if (type == typeof(bool)) return value.ValueKind == JsonValueKind.Number ? value.GetInt32() != 0 : value.GetBoolean();
        if (type.IsEnum) return value.ValueKind == JsonValueKind.String ? Enum.Parse(type, value.GetString()!) : Enum.ToObject(type, value.GetInt64());
        if (type.IsPrimitive || type == typeof(decimal)) return JsonSerializer.Deserialize(value.GetRawText(), type);
        if (type.IsArray) return ArrayValue(value, type.GetElementType()!, path);
        if (type.IsGenericType && type.GetGenericTypeDefinition() == typeof(List<>))
        {
            var list = (IList)Activator.CreateInstance(type)!;
            foreach (var entry in value.EnumerateArray()) list.Add(Convert(entry, type.GetGenericArguments()[0], path));
            return list;
        }
        var result = Activator.CreateInstance(type, nonPublic: true) ?? throw new NotSupportedException($"Cannot construct serialized type '{type.FullName}'");
        if (result is ISerializedEvent evt) BindEvent(evt, value, type, path);
        else Bind(result, value, path);
        return result;
    }
    private Array ArrayValue(JsonElement value, Type elementType, string path)
    {
        var result = Array.CreateInstance(elementType, value.GetArrayLength());
        var index = 0;
        foreach (var item in value.EnumerateArray()) { result.SetValue(Convert(item, elementType, path + "[" + index + "]"), index); index++; }
        return result;
    }
    private void BindEvent(ISerializedEvent evt, JsonElement value, Type type, string path)
    {
        if (!value.TryGetProperty("m_PersistentCalls", out var persistent) || !persistent.TryGetProperty("m_Calls", out var calls)) return;
        foreach (var call in calls.EnumerateArray())
        {
            if (call.GetProperty("m_CallState").GetInt32() == 0) continue;
            var name = call.GetProperty("m_MethodName").GetString();
            var target = call.GetProperty("m_Target");
            if (target.ValueKind == JsonValueKind.Null || string.IsNullOrEmpty(name)) continue;
            var receiver = resolve(target);
            var mode = call.GetProperty("m_Mode").GetInt32();
            var dynamicTypes = EventArguments(type);
            var args = call.GetProperty("m_Arguments");
            var argumentTypes = mode switch
            {
                0 => dynamicTypes, 1 => Type.EmptyTypes, 2 => [ObjectArgumentType(args)],
                3 => [typeof(int)], 4 => [typeof(float)], 5 => [typeof(string)], 6 => [typeof(bool)],
                _ => throw new NotSupportedException($"UnityEvent listener mode {mode} at '{path}'")
            };
            var method = FindMethod(receiver.GetType(), name!, argumentTypes) ?? throw new MissingMethodException($"UnityEvent '{path}': {receiver.GetType().FullName}.{name}");
            object?[]? arguments = mode switch
            {
                0 => null, 1 => [], 2 => [Convert(args.GetProperty("m_ObjectArgument"), argumentTypes[0], path)],
                3 => [args.GetProperty("m_IntArgument").GetInt32()], 4 => [args.GetProperty("m_FloatArgument").GetSingle()],
                5 => [args.GetProperty("m_StringArgument").GetString()], 6 => [Convert(args.GetProperty("m_BoolArgument"), typeof(bool), path)],
                _ => throw new NotSupportedException($"UnityEvent listener mode {mode}")
            };
            evt.AddPersistentListener(receiver, method, arguments);
        }
    }
    private static Type[] EventArguments(Type type)
    {
        for (var t = type; t is not null; t = t.BaseType)
            if (t.IsGenericType && t.GetGenericTypeDefinition() == typeof(UnityEvent<>)) return t.GetGenericArguments();
        return Type.EmptyTypes;
    }
    private static Type ObjectArgumentType(JsonElement args)
    {
        var name = args.GetProperty("m_ObjectArgumentAssemblyTypeName").GetString()?.Split(',')[0];
        return string.IsNullOrEmpty(name) ? typeof(UObject) : typeof(UObject).Assembly.GetType(name) ?? throw new TypeLoadException($"UnityEvent object argument '{name}'");
    }
    internal static MethodInfo? FindMethod(Type type, string name, Type[] args)
    {
        for (var t = type; t is not null; t = t.BaseType)
        {
            var method = t.GetMethod(name, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance | BindingFlags.DeclaredOnly, null, args, null);
            if (method is not null) return method;
        }
        return null;
    }
}
