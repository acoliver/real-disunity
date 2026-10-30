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
/// is ignored, and data for a field whose type cannot be loaded fails with the field named and the
/// innermost cause. A nested serializable class is built by <see cref="SerializableInstance"/>. A
/// reference to an <see cref="UnbuiltComponent"/> binds null and reports a difference, when the
/// binder has somewhere to report it.
/// </summary>
internal sealed class FieldBinder(Func<JsonProperty, object> resolve, Action<string>? difference = null)
{
    /// <summary>Sets the fields, then calls <see cref="ISerializationCallbackReceiver.OnAfterDeserialize"/>
    /// as Unity does once an object's data is loaded.</summary>
    public void Bind(object target, JsonElement fields, string where)
    {
        foreach (var field in DisunityHost.SerializedFields(target.GetType()))
            if (fields.TryGetProperty(field.Name, out var value))
                field.SetValue(target, Value(value, field.FieldType, $"{where}.{field.Name}"));
        (target as ISerializationCallbackReceiver)?.OnAfterDeserialize();
    }

    private object? Value(JsonElement value, Type type, string path)
    {
        try { return Convert(value, type, path); }
        catch (Exception error) when (error is not SceneLoadException)
        {
            throw new SceneLoadException($"{path}: cannot read {value} as {type.Name}: {Cause(error)}", error);
        }
    }

    /// <summary>The innermost exception, which is the failure itself; reflection and type
    /// initialization wrap it in exceptions that say nothing about the cause.</summary>
    private static string Cause(Exception error)
    {
        while (error.InnerException is { } inner) error = inner;
        return $"{error.GetType().Name}: {error.Message}";
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
        if (ValueReader.Read(value, type) is { } builtin) return builtin;
        if (type.IsArray) return Array(value, type.GetElementType()!, path);
        if (type.IsGenericType && type.GetGenericTypeDefinition() == typeof(List<>)) return List(value, type, path);
        if (typeof(IPersistentCalls).IsAssignableFrom(type)) return Event(value, type, path);
        if (SerializableInstance.IsSerializedClass(type))
        {
            var nested = SerializableInstance.Create(type);
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

    private object? Reference(JsonProperty reference, Type type, string path)
    {
        var target = resolve(reference);
        if (target is UnbuiltComponent unbuilt && difference is not null)
        {
            if (!unbuilt.CanBeHeldBy(type))
                throw new SceneLoadException($"{path}: a {type.Name} field references a {unbuilt.Class}");
            difference($"{path}: references a {unbuilt.Class} component, which is not built yet");
            return null;
        }
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
