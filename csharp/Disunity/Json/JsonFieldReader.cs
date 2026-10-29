using System.Collections;
using System.Globalization;
using System.Reflection;
using System.Text.Json;

namespace UnityEngine;

/// <summary>Reads JSON into a new object by field name.</summary>
internal static class JsonFieldReader
{
    public static T Read<T>(string json)
    {
        if (string.IsNullOrEmpty(json)) return default!;
        var type = typeof(T);
        if (type.IsAbstract || type.IsInterface || JsonShapes.IsEngineObject(type))
            throw new ArgumentException($"JsonUtility.FromJson: {type.Name} cannot be created from JSON");
        using var document = Parse(json);
        return (T)ReadValue(document.RootElement, null, type, type.Name)!;
    }

    private static JsonDocument Parse(string json)
    {
        try
        {
            return JsonDocument.Parse(json);
        }
        catch (JsonException error)
        {
            throw new ArgumentException(
                $"JsonUtility.FromJson: malformed JSON at line {(error.LineNumber ?? 0) + 1}, position {error.BytePositionInLine ?? 0}: {error.Message}", error);
        }
    }

    private static object? ReadValue(JsonElement element, object? current, Type type, string path)
    {
        if (JsonShapes.IsEngineObject(type)) throw JsonShapes.ObjectReference("FromJson", path);
        if (type.IsEnum) return Enum.ToObject(type, ReadInteger(element, Enum.GetUnderlyingType(type), path));
        if (TryReadPrimitive(element, type, path, out var primitive)) return primitive;
        if (type.IsArray) return ReadArray(element, type, path);
        if (JsonShapes.IsList(type)) return ReadList(element, type, path);
        return ReadObject(element, current, type, path);
    }

    private static bool TryReadPrimitive(JsonElement element, Type type, string path, out object? value)
    {
        value = null;
        switch (Type.GetTypeCode(type))
        {
            case TypeCode.Boolean:
                value = element.ValueKind is JsonValueKind.True or JsonValueKind.False ? element.GetBoolean() : throw Mismatch(path, "true or false", element);
                return true;
            case TypeCode.String:
                value = element.ValueKind == JsonValueKind.Null ? "" : ReadText(element, path);
                return true;
            case TypeCode.Single:
                value = ExpectNumber(element, path).GetSingle();
                return true;
            case TypeCode.Double:
                value = ExpectNumber(element, path).GetDouble();
                return true;
            case TypeCode.SByte or TypeCode.Int16 or TypeCode.Int32 or TypeCode.Int64
                or TypeCode.Byte or TypeCode.UInt16 or TypeCode.UInt32 or TypeCode.UInt64:
                value = ReadInteger(element, type, path);
                return true;
            default:
                return false;
        }
    }

    private static string ReadText(JsonElement element, string path) =>
        element.ValueKind == JsonValueKind.String ? element.GetString()! : throw Mismatch(path, "a string", element);

    private static JsonElement ExpectNumber(JsonElement element, string path) =>
        element.ValueKind == JsonValueKind.Number ? element : throw Mismatch(path, "a number", element);

    private static object ReadInteger(JsonElement element, Type type, string path)
    {
        if (element.ValueKind == JsonValueKind.Number)
        {
            try
            {
                if (type == typeof(ulong)) { if (element.TryGetUInt64(out var unsigned)) return unsigned; }
                else if (element.TryGetInt64(out var signed)) return Convert.ChangeType(signed, type, CultureInfo.InvariantCulture);
            }
            catch (OverflowException)
            {
                // Falls through to the mismatch below, which names the field and the range.
            }
        }
        throw Mismatch(path, $"an integer within the range of {type.Name}", element);
    }

    private static Array ReadArray(JsonElement element, Type type, string path)
    {
        var items = ReadItems(element, JsonShapes.ElementType(type), path);
        var array = Array.CreateInstance(JsonShapes.ElementType(type), items.Count);
        for (var i = 0; i < items.Count; i++) array.SetValue(items[i], i);
        return array;
    }

    private static IList ReadList(JsonElement element, Type type, string path)
    {
        var list = (IList)JsonShapes.NewInstance(type, path);
        foreach (var item in ReadItems(element, JsonShapes.ElementType(type), path)) list.Add(item);
        return list;
    }

    private static List<object?> ReadItems(JsonElement element, Type itemType, string path)
    {
        if (element.ValueKind != JsonValueKind.Array) throw Mismatch(path, "an array", element);
        var items = new List<object?>();
        foreach (var item in element.EnumerateArray())
            items.Add(ReadValue(item, null, itemType, $"{path}[{items.Count}]"));
        return items;
    }

    private static object ReadObject(JsonElement element, object? current, Type type, string path)
    {
        if (element.ValueKind != JsonValueKind.Object) throw Mismatch(path, "an object", element);
        var target = current ?? JsonShapes.NewInstance(type, path);
        var fields = new Dictionary<string, FieldInfo>(StringComparer.Ordinal);
        foreach (var field in JsonShapes.OrderedFields(type)) fields[field.Name] = field;
        foreach (var property in element.EnumerateObject())
        {
            if (!fields.TryGetValue(property.Name, out var field)) continue;
            if (property.Value.ValueKind == JsonValueKind.Null && field.FieldType != typeof(string)) continue;
            var value = ReadValue(property.Value, field.GetValue(target), field.FieldType, $"{type.Name}.{field.Name}");
            field.SetValue(target, value);
        }
        return target;
    }

    private static ArgumentException Mismatch(string path, string expected, JsonElement element) =>
        new($"JsonUtility.FromJson: {path} needs {expected}, but the JSON has {element.ValueKind}");
}
