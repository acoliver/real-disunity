using System.Collections;
using System.Globalization;
using System.Reflection;
using System.Text.Json;
using UnityEngine;
using UnityEngine.Events;

namespace Disunity.Hosting;

/// <summary>
/// Sets serialized script fields from a plan as Unity deserializes them. Public and
/// <c>[SerializeField]</c> fields bind; static, readonly and <c>[NonSerialized]</c> fields do not.
/// A field the data omits keeps its initializer, data for a field the script no longer declares
/// is ignored, and data for a field whose type cannot be loaded fails with the field named.
/// </summary>
internal sealed class FieldBinder(Func<JsonProperty, object> resolve)
{
    public void Bind(object target, JsonElement fields, string where)
    {
        foreach (var field in DisunityHost.SerializedFields(target.GetType()))
            if (fields.TryGetProperty(field.Name, out var value))
                field.SetValue(target, Value(value, field.FieldType, $"{where}.{field.Name}"));
    }

    private object? Value(JsonElement value, Type type, string path)
    {
        try { return Convert(value, type, path); }
        catch (Exception error) when (error is not SceneLoadException)
        {
            throw new SceneLoadException($"{path}: cannot read {value} as {type.Name}: {error.Message}", error);
        }
    }

    private object? Convert(JsonElement value, Type type, string path)
    {
        if (value.ValueKind == JsonValueKind.Null && !type.IsValueType) return null;
        if (AsReference(value) is { } reference) return Reference(reference, type, path);
        if (type == typeof(string)) return value.GetString();
        if (type == typeof(bool)) return Text(value) switch { "0" => false, "1" => true, var other => throw new FormatException(other) };
        if (type.IsEnum) return Enum.ToObject(type, long.Parse(Text(value), CultureInfo.InvariantCulture));
        if (type == typeof(float)) return float.Parse(Text(value), NumberStyles.Float, CultureInfo.InvariantCulture);
        if (type == typeof(double)) return double.Parse(Text(value), NumberStyles.Float, CultureInfo.InvariantCulture);
        // A ulong may exceed long's range.
        if (type == typeof(ulong)) return ulong.Parse(Text(value), CultureInfo.InvariantCulture);
        if (type.IsPrimitive) return System.Convert.ChangeType(long.Parse(Text(value), CultureInfo.InvariantCulture), type, CultureInfo.InvariantCulture);
        if (Vector(value, type) is { } vector) return vector;
        if (type.IsArray) return Array(value, type.GetElementType()!, path);
        if (type.IsGenericType && type.GetGenericTypeDefinition() == typeof(List<>)) return List(value, type, path);
        if (typeof(IPersistentCalls).IsAssignableFrom(type)) return Event(value, type, path);
        // Unity serializes only arrays and List<T> among collections, and no delegates, though
        // .NET marks many such types [Serializable].
        if (type.IsDefined(typeof(SerializableAttribute)) && !typeof(UnityEngine.Object).IsAssignableFrom(type)
            && !typeof(IEnumerable).IsAssignableFrom(type) && !typeof(Delegate).IsAssignableFrom(type))
        {
            var nested = Activator.CreateInstance(type, nonPublic: true)
                ?? throw new SceneLoadException($"{path}: cannot construct {type}");
            Bind(nested, value, path);
            return nested;
        }
        throw new SceneLoadException($"{path}: {type} fields cannot be loaded");
    }

    /// <summary>The reference a value is, when it is a one-member object such as <c>{"$object": 3}</c>.</summary>
    private static JsonProperty? AsReference(JsonElement value)
    {
        if (value.ValueKind != JsonValueKind.Object) return null;
        var members = value.EnumerateObject().Take(2).ToList();
        return members is [{ Name: ['$', ..] } only] ? only : null;
    }

    private static string Text(JsonElement value) => value.GetString() ?? throw new FormatException("expected a value");

    private static float Part(JsonElement value, string name) =>
        float.Parse(Text(value.GetProperty(name)), NumberStyles.Float, CultureInfo.InvariantCulture);

    private static object? Vector(JsonElement value, Type type)
    {
        if (type == typeof(Vector2)) return new Vector2(Part(value, "x"), Part(value, "y"));
        if (type == typeof(Vector3)) return new Vector3(Part(value, "x"), Part(value, "y"), Part(value, "z"));
        if (type == typeof(Vector2Int)) return new Vector2Int((int)Part(value, "x"), (int)Part(value, "y"));
        if (type == typeof(Quaternion)) return new Quaternion(Part(value, "x"), Part(value, "y"), Part(value, "z"), Part(value, "w"));
        if (type == typeof(Color)) return new Color(Part(value, "r"), Part(value, "g"), Part(value, "b"), Part(value, "a"));
        if (type == typeof(Rect)) return new Rect(Part(value, "x"), Part(value, "y"), Part(value, "width"), Part(value, "height"));
        if (type == typeof(LayerMask)) return (LayerMask)(int)uint.Parse(Text(value.GetProperty("m_Bits")), CultureInfo.InvariantCulture);
        return null;
    }

    private object Reference(JsonProperty reference, Type type, string path)
    {
        var target = resolve(reference);
        if (!type.IsInstanceOfType(target))
            throw new SceneLoadException($"{path}: a {type.Name} field references a {target.GetType().Name}");
        return target;
    }

    private System.Array Array(JsonElement value, Type element, string path)
    {
        var items = value.EnumerateArray().ToList();
        var array = System.Array.CreateInstance(element, items.Count);
        for (var i = 0; i < items.Count; i++) array.SetValue(Value(items[i], element, $"{path}[{i}]"), i);
        return array;
    }

    private IList List(JsonElement value, Type type, string path)
    {
        var list = (IList)Activator.CreateInstance(type)!;
        var element = type.GetGenericArguments()[0];
        var i = 0;
        foreach (var item in value.EnumerateArray()) list.Add(Value(item, element, $"{path}[{i++}]"));
        return list;
    }

    /// <summary>A UnityEvent with its authored calls. Each call's method is checked now, so a
    /// renamed method fails the load instead of the event.</summary>
    private object Event(JsonElement value, Type type, string path)
    {
        var target = (IPersistentCalls)Activator.CreateInstance(type, nonPublic: true)!;
        var calls = value.GetProperty("m_PersistentCalls").GetProperty("m_Calls").EnumerateArray().ToList();
        for (var i = 0; i < calls.Count; i++)
        {
            var call = calls[i];
            var arguments = call.GetProperty("m_Arguments");
            var at = $"{path} call {i}";
            var persistent = new PersistentCall
            {
                Target = (UnityEngine.Object?)Value(call.GetProperty("m_Target"), typeof(UnityEngine.Object), at),
                MethodName = Text(call.GetProperty("m_MethodName")),
                Mode = (PersistentListenerMode)int.Parse(Text(call.GetProperty("m_Mode")), CultureInfo.InvariantCulture),
                CallState = (UnityEventCallState)int.Parse(Text(call.GetProperty("m_CallState")), CultureInfo.InvariantCulture),
                ObjectArgument = (UnityEngine.Object?)Value(arguments.GetProperty("m_ObjectArgument"), typeof(UnityEngine.Object), at),
                IntArgument = (int)Value(arguments.GetProperty("m_IntArgument"), typeof(int), at)!,
                FloatArgument = (float)Value(arguments.GetProperty("m_FloatArgument"), typeof(float), at)!,
                StringArgument = Text(arguments.GetProperty("m_StringArgument")),
                BoolArgument = (bool)Value(arguments.GetProperty("m_BoolArgument"), typeof(bool), at)!,
            };
            try { target.AddPersistentCall(persistent); }
            catch (Exception error) { throw new SceneLoadException($"{at}: {error.Message}", error); }
        }
        return target;
    }
}
