using System.Buffers;
using System.Collections;
using System.Globalization;
using System.Text;
using System.Text.Json;

namespace UnityEngine;

/// <summary>Writes an object's serialized fields as compact JSON.</summary>
internal static class JsonFieldWriter
{
    public static string Write(object value)
    {
        var buffer = new ArrayBufferWriter<byte>();
        using (var writer = new Utf8JsonWriter(buffer))
        {
            var type = value.GetType();
            if (value is Object) WriteFields(writer, value, type, type.Name, 0);
            else WriteValue(writer, value, type, type.Name, 0);
        }
        return Encoding.UTF8.GetString(buffer.WrittenSpan);
    }

    private static void WriteValue(Utf8JsonWriter writer, object? value, Type type, string path, int depth)
    {
        if (depth > JsonShapes.MaxDepth)
            throw new ArgumentException($"JsonUtility.ToJson: {path} is nested more than {JsonShapes.MaxDepth} levels deep");
        if (JsonShapes.IsEngineObject(type)) throw JsonShapes.ObjectReference("ToJson", path);
        if (type.IsEnum) value = Convert.ChangeType(value!, Enum.GetUnderlyingType(type), CultureInfo.InvariantCulture);
        if (WritePrimitive(writer, value, type.IsEnum ? Enum.GetUnderlyingType(type) : type, path)) return;
        if (type.IsArray || JsonShapes.IsList(type)) WriteList(writer, value, type, path, depth);
        else WriteFields(writer, value ?? JsonShapes.NewInstance(type, path), type, path, depth);
    }

    private static bool WritePrimitive(Utf8JsonWriter writer, object? value, Type type, string path)
    {
        switch (Type.GetTypeCode(type))
        {
            case TypeCode.Boolean: writer.WriteBooleanValue((bool)value!); return true;
            case TypeCode.String: writer.WriteStringValue((string?)value ?? ""); return true;
            case TypeCode.Char: writer.WriteStringValue(value!.ToString()); return true;
            case TypeCode.SByte or TypeCode.Int16 or TypeCode.Int32 or TypeCode.Int64:
                writer.WriteNumberValue(Convert.ToInt64(value, CultureInfo.InvariantCulture)); return true;
            case TypeCode.Byte or TypeCode.UInt16 or TypeCode.UInt32 or TypeCode.UInt64:
                writer.WriteNumberValue(Convert.ToUInt64(value, CultureInfo.InvariantCulture)); return true;
            case TypeCode.Single: writer.WriteNumberValue((float)Finite((float)value!, path)); return true;
            case TypeCode.Double: writer.WriteNumberValue(Finite((double)value!, path)); return true;
            default:
                if (type.IsPrimitive || type == typeof(decimal))
                    throw new NotSupportedException($"JsonUtility.ToJson: {path} has type {type.Name}, which is not supported");
                return false;
        }
    }

    private static double Finite(double number, string path) =>
        double.IsFinite(number)
            ? number
            : throw new NotSupportedException($"JsonUtility.ToJson: {path} is {number.ToString(CultureInfo.InvariantCulture)}, which JSON cannot hold");

    private static void WriteList(Utf8JsonWriter writer, object? value, Type type, string path, int depth)
    {
        if (type.IsArray && type.GetArrayRank() != 1)
            throw new NotSupportedException($"JsonUtility.ToJson: {path} is a multidimensional array, which is not supported");
        var element = JsonShapes.ElementType(type);
        writer.WriteStartArray();
        if (value != null)
            foreach (var item in (IEnumerable)value) WriteValue(writer, item, element, path + "[]", depth + 1);
        writer.WriteEndArray();
    }

    private static void WriteFields(Utf8JsonWriter writer, object value, Type type, string path, int depth)
    {
        writer.WriteStartObject();
        foreach (var field in JsonShapes.OrderedFields(type))
        {
            writer.WritePropertyName(field.Name);
            WriteValue(writer, field.GetValue(value), field.FieldType, $"{type.Name}.{field.Name}", depth + 1);
        }
        writer.WriteEndObject();
    }
}
