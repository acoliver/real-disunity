using System.Collections;
using System.Text.Json;
using UnityEngine;
using UnityEngine.Events;
using UObject = UnityEngine.Object;

namespace CompatibilityBridge;

public sealed partial class ScriptSession
{
    public void Cloned(IReadOnlyDictionary<ulong, ulong> remap, ulong original)
    {
        RequireStrictScope();
        var objects = ClonedObjects(remap[original]);
        foreach (var obj in objects) NativeIdentity.GameObject(obj.GetProperty("handle").GetUInt64());
        foreach (var obj in objects)
        {
            var owner = NativeIdentity.GameObject(obj.GetProperty("handle").GetUInt64());
            foreach (var component in obj.GetProperty("components").EnumerateArray())
            {
                var bound = NativeIdentity.Component(component, owner);
                if (bound is MonoBehaviour script) _scripts.Add(script.NativeHandle, new ScriptState(script));
            }
        }
        var copied = new Dictionary<object, object>(ReferenceEqualityComparer.Instance);
        foreach (var pair in remap)
        {
            if (!_scripts.TryGetValue(pair.Key, out var source) || !_scripts.TryGetValue(pair.Value, out var target)) continue;
            foreach (var field in SerializedFields.Fields(source.Instance.GetType())) field.SetValue(target.Instance, CopyValue(field.GetValue(source.Instance), remap, copied));
        }
        foreach (var pair in remap)
            if (_templates.TryGetValue(pair.Key, out var active)) NativeWorld.Request("object.set", ("object", pair.Value), ("active", active)).Dispose();
        var wasReconciling = _reconciling;
        _reconciling = false;
        try { Reconcile(); }
        finally { _reconciling = wasReconciling; }
    }
    // The clone is exactly the new root and its transform descendants. Each object.get uses the
    // native object projection that snapshot uses, and slot-index order matches snapshot order,
    // without serializing the whole world for every Instantiate.
    private static JsonElement[] ClonedObjects(ulong root)
    {
        var found = new List<JsonElement>();
        var pending = new Queue<ulong>([root]);
        while (pending.Count > 0)
        {
            using var response = NativeWorld.Request("object.get", ("object", pending.Dequeue()));
            var obj = NativeWorld.Data(response).Clone();
            found.Add(obj);
            foreach (var child in obj.GetProperty("transform").GetProperty("children").EnumerateArray())
                pending.Enqueue(child.GetUInt64());
        }
        return found.OrderBy(o => o.GetProperty("handle").GetUInt64() & 0xFFFF_FFFFUL).ToArray();
    }
    private static object? CopyValue(object? value, IReadOnlyDictionary<ulong, ulong> remap, Dictionary<object, object> copied)
    {
        if (value is null) return null;
        if (value is UObject reference) return remap.TryGetValue(reference.NativeHandle, out var mapped) ? NativeIdentity.Resolve(mapped) : reference;
        var type = value.GetType();
        if (type.IsPrimitive || type.IsEnum || type == typeof(string) || type == typeof(decimal)) return value;
        if (copied.TryGetValue(value, out var known)) return known;
        if (value is Array array)
        {
            var clone = Array.CreateInstance(type.GetElementType()!, array.Length);
            copied.Add(value, clone);
            for (var i = 0; i < array.Length; i++) clone.SetValue(CopyValue(array.GetValue(i), remap, copied), i);
            return clone;
        }
        var result = Activator.CreateInstance(type, nonPublic: true) ?? throw new NotSupportedException($"Cannot clone serialized type '{type.FullName}'");
        copied.Add(value, result);
        if (value is ISerializedEvent evt) evt.CopyPersistentTo((ISerializedEvent)result, item => CopyValue(item, remap, copied));
        else if (value is IList list)
            foreach (var item in list) ((IList)result).Add(CopyValue(item, remap, copied));
        else
            foreach (var field in SerializedFields.Fields(type)) field.SetValue(result, CopyValue(field.GetValue(value), remap, copied));
        return result;
    }
}
