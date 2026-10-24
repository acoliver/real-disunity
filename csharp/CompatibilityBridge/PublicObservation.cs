using System.Collections;
using System.Reflection;
using System.Text;
using System.Text.Json;
using UnityEngine;
using UObject = UnityEngine.Object;

namespace CompatibilityBridge;

public sealed partial class ScriptSession
{
    private object QueryPublic(JsonElement query)
    {
        var type = query.GetProperty("type").GetString();
        if (string.IsNullOrWhiteSpace(type)) throw new InvalidDataException("public query requires a type");
        var path = query.TryGetProperty("member", out var member) ? member.GetString() : query.GetProperty("path").GetString();
        var parts = path?.Split('.') ?? [];
        if (parts.Length is 0 or > 8 || parts.Any(string.IsNullOrWhiteSpace)) throw new InvalidDataException("public query path requires 1..8 named members");
        IEnumerable<UObject> candidates = _stable.Values.Concat(_scripts.Values.Select(s => (UObject)s.Instance)).Distinct<UObject>(ReferenceEqualityComparer.Instance)
            .Where(o => (o.GetType().FullName == type || o.GetType().Name == type) && o.IsAlive);
        if (query.TryGetProperty("handle", out var handle)) candidates = candidates.Where(o => o.NativeHandle != 0 && o.NativeHandle == handle.GetUInt64());
        if (query.TryGetProperty("id", out var id))
        {
            var selected = Resolve(id.GetString()!);
            candidates = candidates.Where(o => ReferenceEquals(o, selected));
        }
        var matches = candidates.Take(2).ToArray();
        if (matches.Length == 0) throw new KeyNotFoundException($"no live bound public query target '{type}'");
        if (matches.Length > 1) throw new InvalidDataException($"ambiguous public query target '{type}'; supply a component handle or stable id");
        object? value = matches[0];
        foreach (var part in parts)
        {
            if (value is null) throw new InvalidDataException($"null encountered at public member '{part}' in '{path}'");
            value = ReadPublic(value, part);
        }
        var budget = new ObservationBudget(Limit(query, "max_depth", 4, 8), Limit(query, "max_items", 256, 256));
        var result = new { version = 1, ok = true, data = budget.Read(value, 0) };
        if (JsonSerializer.SerializeToUtf8Bytes(result).Length > 65536) throw new InvalidDataException("public observation byte budget exceeded (65536)");
        return result;
    }
    private static int Limit(JsonElement query, string name, int fallback, int maximum)
    {
        var value = query.TryGetProperty(name, out var limit) ? limit.GetInt32() : fallback;
        return value >= 1 && value <= maximum ? value : throw new InvalidDataException($"public observation {name} requires 1..{maximum}");
    }
    private static object? ReadPublic(object target, string name)
    {
        var type = target.GetType();
        const BindingFlags flags = BindingFlags.Instance | BindingFlags.Public;
        var field = type.GetField(name, flags);
        if (field is not null) return field.GetValue(target);
        var property = type.GetProperty(name, flags);
        if (property is not null && property.GetMethod is { IsPublic: true, IsStatic: false } && property.GetIndexParameters().Length == 0)
            return property.GetValue(target);
        var methods = type.GetMethods(flags).Where(m => m.Name == name).ToArray();
        var pure = methods.Where(m => !m.IsGenericMethod && m.ReturnType != typeof(void) && m.GetParameters().Length == 0 &&
            m.CustomAttributes.Any(a => a.AttributeType.FullName == "System.Diagnostics.Contracts.PureAttribute")).ToArray();
        if (pure.Length == 1) return pure[0].Invoke(target, null);
        if (methods.Length > 0) throw new InvalidDataException($"public member '{type.FullName}.{name}' is not a declared read-only getter; methods require PureAttribute, no arguments and a return value");
        throw new MissingMemberException($"no readable public field or property '{type.FullName}.{name}'");
    }
    private sealed class ObservationBudget(int depthLimit, int itemLimit)
    {
        private readonly HashSet<object> _ancestors = new(ReferenceEqualityComparer.Instance);
        private int _remaining = 1024;
        private int _textBytes = 32768;
        public object? Read(object? value, int depth)
        {
            if (--_remaining < 0) throw new InvalidDataException("public observation node budget exceeded (1024)");
            if (value is null) return null;
            if (value is string text) { Text(text); return text; }
            var type = value.GetType();
            if (type.IsPrimitive || value is decimal) return value;
            if (type.IsEnum) { var name = value.ToString()!; Text(name); return name; }
            if (value is UObject obj) return new { handle = obj.NativeHandle, type = obj.GetType().FullName };
            if (_ancestors.Contains(value)) return new { type = type.FullName, cycle = true };
            if (depth >= depthLimit) return new { type = type.FullName, truncated = true };
            _ancestors.Add(value);
            try
            {
                if (value is IEnumerable sequence)
                {
                    var result = new List<object?>();
                    var iterator = sequence.GetEnumerator();
                    try
                    {
                        while (result.Count < itemLimit && iterator.MoveNext()) result.Add(Read(iterator.Current, depth + 1));
                    }
                    finally { (iterator as IDisposable)?.Dispose(); }
                    return result;
                }
                var fields = type.GetFields(BindingFlags.Instance | BindingFlags.Public).OrderBy(f => f.Name, StringComparer.Ordinal).Take(itemLimit);
                var data = new Dictionary<string, object?>();
                foreach (var field in fields) { Text(field.Name); data.Add(field.Name, Read(field.GetValue(value), depth + 1)); }
                return data;
            }
            finally { _ancestors.Remove(value); }
        }
        private void Text(string value)
        {
            _textBytes -= Encoding.UTF8.GetByteCount(value);
            if (_textBytes < 0) throw new InvalidDataException("public observation text byte budget exceeded (32768)");
        }
    }
}
