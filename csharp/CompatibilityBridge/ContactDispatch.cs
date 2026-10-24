using System.Text.Json;
using UnityEngine;

namespace CompatibilityBridge;

public sealed partial class ScriptSession
{
    public void InvokeContact(JsonElement contact)
    {
        RequireLegacyEntry();
        InvokeContactCore(contact);
    }
    private void InvokeContactCore(JsonElement contact)
    {
        if (CompatibilityTrace.Enabled) CompatibilityTrace.Event("contact", contact.GetRawText());
        var phase = contact.GetProperty("phase").GetString();
        using var context = EnterPhase("contacts." + phase);
        var suffix = phase switch { "enter" => "Enter2D", "stay" => "Stay2D", "exit" => "Exit2D", _ => throw new NotSupportedException($"contact phase '{phase}'") };
        var trigger = contact.GetProperty("trigger").GetBoolean();
        var a = contact.GetProperty("a").GetUInt64();
        var b = contact.GetProperty("b").GetUInt64();
        var colliderA = contact.GetProperty("collider_a").GetUInt64();
        var colliderB = contact.GetProperty("collider_b").GetUInt64();
        var snapshot = new LifecycleSnapshot();
        if (!snapshot.ContainsComponent(colliderA) || !snapshot.ContainsComponent(colliderB))
        {
            Reconcile();
            return;
        }
        var ca = (Collider2D)NativeIdentity.Resolve(colliderA);
        var cb = (Collider2D)NativeIdentity.Resolve(colliderB);
        if (ca.gameObject.NativeHandle != a || cb.gameObject.NativeHandle != b) throw new InvalidDataException("contact collider/object ownership mismatch");
        var n = contact.GetProperty("normal"); var p = contact.GetProperty("point");
        var normal = new Vector2(n[0].GetSingle(), n[1].GetSingle()); var point = new Vector2(p[0].GetSingle(), p[1].GetSingle());
        var left = new ContactDelivery(ca, cb, normal, point, trigger, suffix);
        var right = new ContactDelivery(cb, ca, -normal, point, trigger, suffix);
        foreach (var owner in Recipients(contact, "body_a", a)) DispatchContact(owner, left, snapshot);
        foreach (var owner in Recipients(contact, "body_b", b)) DispatchContact(owner, right, snapshot);
        Reconcile();
    }
    private sealed record ContactDelivery(Collider2D Self, Collider2D Other, Vector2 Normal, Vector2 Point, bool Trigger, string Suffix);
    private static IEnumerable<ulong> Recipients(JsonElement contact, string bodyField, ulong owner)
    {
        yield return owner;
        if (contact.TryGetProperty(bodyField, out var body) && body.ValueKind == JsonValueKind.Number && body.GetUInt64() != owner) yield return body.GetUInt64();
    }
    private void DispatchContact(ulong owner, ContactDelivery contact, LifecycleSnapshot snapshot)
    {
        foreach (var state in _scripts.Values.Where(s => s.Instance.gameObject.NativeHandle == owner).ToArray())
        {
            if (!snapshot.ContainsComponent(contact.Self.NativeHandle) || !snapshot.ContainsComponent(contact.Other.NativeHandle)) return;
            if (!_scripts.ContainsKey(state.Instance.NativeHandle) || !snapshot.TryGet(state.Instance.NativeHandle, out var native) || !native.Active || !native.Enabled) continue;
            object argument = contact.Trigger ? contact.Other : new Collision2D(contact.Other, contact.Self, contact.Point, contact.Normal);
            Invoke(state, (contact.Trigger ? "OnTrigger" : "OnCollision") + contact.Suffix, argument, completed: snapshot.Refresh);
        }
    }
    public object Observe(JsonElement query)
    {
        var op = query.TryGetProperty("op", out var operation) ? operation.GetString() : "managed.snapshot";
        if (op == "managed.failures") return _failures.Observe(query);
        ObjectDisposedException.ThrowIf(_disposed, this);
        if (op == "world.snapshot")
        {
            using var result = NativeWorld.Request("snapshot");
            return result.RootElement.Clone();
        }
        if (op is "managed.query" or "managed.public") return QueryPublic(query);
        if (op != "managed.snapshot") throw new NotSupportedException($"Observe('{op}')");
        return new
        {
            version = 1, ok = true, data = new
            {
                loaded = !_disposed, scripts = _scripts.Count, coroutines = CoroutineCount,
                lifecycle_authority = _readiness is null ? "managed" : NativeReadinessProfile.Name,
                instances = _scripts.Values.Select(s => new
                {
                    component = s.Instance.NativeHandle, @object = s.Instance.gameObject.NativeHandle,
                    type = s.Instance.GetType().FullName, awake = _readiness is null ? (bool?)s.Awake : null,
                    enabled = _readiness is null ? (bool?)s.Enabled : null, started = _readiness is null ? (bool?)s.Started : null,
                    fields = SerializedFields.Fields(s.Instance.GetType()).ToDictionary(f => f.Name, f => ObserveValue(f.GetValue(s.Instance), 0))
                }).ToArray(), diagnostics = _diagnostics.ToArray()
            }
        };
    }
    private static object? ObserveValue(object? value, int depth)
    {
        if (value is null || value is string || value.GetType().IsPrimitive || value is decimal) return value;
        if (value is UnityEngine.Object obj) return new { handle = obj.NativeHandle, type = obj.GetType().FullName };
        if (value.GetType().IsEnum) return value.ToString();
        if (depth >= 4) return new { type = value.GetType().FullName, truncated = true };
        if (value is System.Collections.IEnumerable values) return values.Cast<object?>().Take(256).Select(v => ObserveValue(v, depth + 1)).ToArray();
        return SerializedFields.Fields(value.GetType()).ToDictionary(f => f.Name, f => ObserveValue(f.GetValue(value), depth + 1));
    }
}
