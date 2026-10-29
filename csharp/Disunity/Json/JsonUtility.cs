using System.Collections;
using System.Reflection;
using Disunity.Hosting;

namespace UnityEngine;

/// <summary>
/// Converts objects to JSON text and back. <see cref="ToJson"/> writes the fields the engine
/// serializes in scenes and clones: public and <c>[SerializeField]</c> instance fields that are not
/// <c>readonly</c> or <c>[NonSerialized]</c>, base class first and each class in declaration order.
/// Integers and floats are JSON numbers (floats in the shortest form that reads back exactly), enums
/// are their integer value, a null string is <c>""</c>, a null array or list is <c>[]</c>, and a null
/// class field is an object of its default field values. A field that refers to a
/// <see cref="Object"/> is not supported and throws <see cref="NotSupportedException"/> naming it, as
/// does a float that JSON cannot hold (NaN or infinity). Object nesting deeper than 32 levels,
/// such as a class that contains itself, throws <see cref="ArgumentException"/>.
/// <see cref="FromJson{T}"/> builds a <typeparamref name="T"/> with its constructor and sets the
/// fields the JSON names; unknown names are ignored and missing fields keep their constructor values.
/// </summary>
public static class JsonUtility
{
    public static string ToJson(object obj) => obj == null ? "" : JsonFieldWriter.Write(obj);

    public static T FromJson<T>(string json) => JsonFieldReader.Read<T>(json);
}

/// <summary>What the writer and the reader agree on: which fields count and which types are lists.</summary>
internal static class JsonShapes
{
    public const int MaxDepth = 32;

    /// <summary>The serialized fields, base class first so a derived class's fields follow it.</summary>
    public static IEnumerable<FieldInfo> OrderedFields(Type type) =>
        DisunityHost.SerializedFields(type).OrderBy(field => Depth(field.DeclaringType));

    public static bool IsList(Type type) => type.IsGenericType && type.GetGenericTypeDefinition() == typeof(List<>);

    public static Type ElementType(Type type) => type.IsArray ? type.GetElementType()! : type.GetGenericArguments()[0];

    public static bool IsEngineObject(Type type) => typeof(Object).IsAssignableFrom(type);

    public static NotSupportedException ObjectReference(string operation, string path) =>
        new($"JsonUtility.{operation}: {path} refers to a UnityEngine.Object, which is not supported");

    public static object NewInstance(Type type, string path)
    {
        try
        {
            return Activator.CreateInstance(type, nonPublic: true)!;
        }
        catch (MissingMemberException error)
        {
            throw new ArgumentException($"JsonUtility: {path} has type {type.Name}, which has no constructor without parameters", error);
        }
    }

    private static int Depth(Type? type)
    {
        var depth = 0;
        for (var current = type?.BaseType; current != null; current = current.BaseType) depth++;
        return depth;
    }
}
