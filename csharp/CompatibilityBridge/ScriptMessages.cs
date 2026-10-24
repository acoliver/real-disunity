using System.Reflection;
using System.Runtime.ExceptionServices;
using UnityEngine;

namespace CompatibilityBridge;

internal sealed class ScriptState(MonoBehaviour instance)
{
    public MonoBehaviour Instance { get; } = instance;
    public bool Awake { get; set; }
    public bool Enabled { get; set; }
    public bool Started { get; set; }
}
internal sealed class ScriptMessages
{
    private readonly Dictionary<(Type Type, string Name, Type? Argument), MethodInfo?> _cache = new();
    public int BindingCount => _cache.Count;
    public object? Invoke(MonoBehaviour instance, string name, object? argument = null, Action? completed = null, Action<Exception>? failed = null)
    {
        var key = (instance.GetType(), name, argument?.GetType());
        if (!_cache.TryGetValue(key, out var method))
        {
            method = SerializedFields.FindMethod(key.Item1, name, argument is null ? Type.EmptyTypes : [argument.GetType()]);
            _cache.Add(key, method);
        }
        if (method is null) return null;
        using var trace = CompatibilityTrace.Begin("callback", CompatibilityTrace.Enabled ? $"{instance.GetType().FullName}.{name} component={instance.NativeHandle}" : string.Empty);
        try
        {
            return method.Invoke(instance, argument is null ? null : [argument]);
        }
        catch (TargetInvocationException error) when (error.InnerException is not null)
        {
            if (failed is null)
                throw new InvalidOperationException($"script callback {instance.GetType().FullName}.{name} (component {instance.NativeHandle}): {error.InnerException.Message}", error.InnerException);
            failed(error.InnerException);
            return null;
        }
        finally { completed?.Invoke(); }
    }
    public void Clear() => _cache.Clear();
}
