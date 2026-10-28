using System.Collections;
using System.Reflection;
using Disunity.Native;
using UnityEngine;
using EngineObject = UnityEngine.Object;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    internal static unsafe T Instantiate<T>(T original, Transform? parent, Vector3? position = null, Quaternion? rotation = null) where T : EngineObject
    {
        EnsureWorld();
        ulong root;
        uint count;
        ThrowIfError(NativeBindings.disunity_clone_create(WorldHandle, original.Owner.NativeHandle,
            parent?.gameObject.NativeHandle ?? 0, &root, &count));
        var rows = new DisunityCloneRow[count];
        uint written;
        fixed (DisunityCloneRow* pointer = rows)
            ThrowIfError(NativeBindings.disunity_clone_copy(WorldHandle, root, pointer, count, &written));
        if (written != count) throw new InvalidOperationException("clone binding count changed");
        var remap = new Dictionary<object, object>(ReferenceEqualityComparer.Instance);
        var objects = new Dictionary<ulong, GameObject>();
        foreach (var row in rows.Where(row => row.kind == 0))
        {
            var source = ResolveObject(row.source);
            var target = Track(new GameObject(row.target));
            if (source.transform is RectTransform) target.Transform = new RectTransform { gameObject = target };
            objects.Add(row.target, target);
            remap.Add(source, target);
            remap.Add(source.transform, target.transform);
        }
        try
        {
            foreach (var row in rows.Where(row => row.kind == 2))
            {
                var source = NativeComponents[row.source];
                var owner = objects[row.owner];
                var target = source is RectTransform ? owner.transform : (Component)(Activator.CreateInstance(source.GetType(), nonPublic: true)
                    ?? throw new NotSupportedException($"cannot construct {source.GetType()}"));
                target.Handle = row.target;
                target.gameObject = owner;
                NativeComponents.Add(row.target, target);
                remap[source] = target;
            }
            var scripts = new List<(MonoBehaviour Source, MonoBehaviour Target)>();
            foreach (var row in rows.Where(row => row.kind == 1))
            {
                var source = Scripts[row.source];
                var target = (MonoBehaviour)(Activator.CreateInstance(source.GetType(), nonPublic: true)
                    ?? throw new NotSupportedException($"cannot construct {source.GetType()}"));
                target.gameObject = objects[row.owner];
                var identity = checked(_nextScript++);
                ThrowIfError(NativeBindings.disunity_clone_bind(WorldHandle, root, row.target, identity));
                Scripts.Add(identity, target);
                remap.Add(source, target);
                scripts.Add((source, target));
            }
            if (position is { } targetPosition) objects[root].transform.position = targetPosition;
            if (rotation is { } targetRotation) objects[root].transform.rotation = targetRotation;
            var copied = new Dictionary<object, object>(ReferenceEqualityComparer.Instance);
            foreach (var (source, target) in scripts)
                foreach (var field in SerializedFields(source.GetType()))
                    field.SetValue(target, CopyField(field.GetValue(source), remap, copied));
        }
        catch (Exception failure)
        {
            try { ThrowIfError(NativeBindings.disunity_clone_abort(WorldHandle, root)); }
            catch (Exception rollback) { failure.Data["CloneRollbackFailure"] = rollback; }
            Release(objects.Values);
            throw;
        }
        DisunityAdvance work;
        ThrowIfError(NativeBindings.disunity_clone_activate(WorldHandle, root, &work));
        Drain(work);
        return (T)remap[original];
    }

    internal static IEnumerable<FieldInfo> SerializedFields(Type type)
    {
        for (var current = type; current != null && current != typeof(MonoBehaviour); current = current.BaseType)
            foreach (var field in current.GetFields(BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly))
                if (!field.IsStatic && !field.IsInitOnly && !field.IsDefined(typeof(NonSerializedAttribute))
                    && (field.IsPublic || field.IsDefined(typeof(SerializeField)))) yield return field;
    }

    private static object? CopyField(object? value, Dictionary<object, object> remap, Dictionary<object, object> copied)
    {
        if (value == null) return null;
        if (value is Sprite or Font or Material) return value;
        if (value is EngineObject) return remap.GetValueOrDefault(value, value);
        var type = value.GetType();
        if (value is Vector3 or Vector2 or Vector4 or Vector2Int or Vector3Int or Quaternion or Rect or RectInt or Color or Color32 or Bounds or BoundsInt or Matrix4x4 or Plane or Ray || type.IsPrimitive || type.IsEnum || type == typeof(string) || type == typeof(decimal)) return value;
        if (copied.TryGetValue(value, out var known)) return known;
        if (value is Array array)
        {
            if (array.Rank != 1) throw new NotSupportedException("only one-dimensional serialized arrays are supported");
            var result = Array.CreateInstance(type.GetElementType()!, array.Length);
            copied.Add(value, result);
            for (var i = 0; i < array.Length; i++) result.SetValue(CopyField(array.GetValue(i), remap, copied), i);
            return result;
        }
        var fields = SerializedFields(type).ToArray();
        var isList = type.IsGenericType && type.GetGenericTypeDefinition() == typeof(List<>);
        if (!isList && (fields.Length == 0 || value is IEnumerable or Delegate))
            throw new NotSupportedException($"unsupported serialized field type {type}");
        object clone;
        try { clone = Activator.CreateInstance(type, nonPublic: true)
            ?? throw new NotSupportedException($"cannot clone serialized field {type}"); }
        catch (MissingMethodException error) { throw new NotSupportedException($"cannot clone serialized field {type}", error); }
        copied.Add(value, clone);
        if (isList)
            foreach (var item in (IList)value) ((IList)clone).Add(CopyField(item, remap, copied));
        else
            foreach (var field in fields) field.SetValue(clone, CopyField(field.GetValue(value), remap, copied));
        return clone;
    }
}
