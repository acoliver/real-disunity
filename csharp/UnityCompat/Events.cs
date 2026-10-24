namespace UnityEngine.Events;

public delegate void UnityAction();
public delegate void UnityAction<in T>(T value);
public interface ISerializedEvent
{
    void AddPersistentListener(object target, System.Reflection.MethodInfo method, object?[]? arguments);
    void CopyPersistentTo(ISerializedEvent destination, Func<object?, object?> remap);
}
public class UnityEvent<T> : ISerializedEvent
{
    private readonly List<UnityAction<T>> _listeners = new();
    private readonly List<(object Target, System.Reflection.MethodInfo Method, object?[]? Arguments)> _persistent = new();
    public void AddListener(UnityAction<T> listener) => _listeners.Add(listener);
    public void RemoveListener(UnityAction<T> listener) => _listeners.Remove(listener);
    public void RemoveAllListeners() => _listeners.Clear();
    public void AddPersistentListener(object target, System.Reflection.MethodInfo method, object?[]? arguments) => _persistent.Add((target, method, arguments));
    public void CopyPersistentTo(ISerializedEvent destination, Func<object?, object?> remap)
    {
        foreach (var item in _persistent) destination.AddPersistentListener(remap(item.Target)!, item.Method, item.Arguments?.Select(remap).ToArray());
    }
    public void Invoke(T value)
    {
        foreach (var item in _persistent.ToArray())
            item.Method.Invoke(item.Target, item.Arguments ?? [value]);
        foreach (var listener in _listeners.ToArray()) listener(value);
    }
}
public class UnityEvent : ISerializedEvent
{
    private readonly List<UnityAction> _listeners = new();
    private readonly List<(object Target, System.Reflection.MethodInfo Method, object?[]? Arguments)> _persistent = new();
    public void AddListener(UnityAction listener) => _listeners.Add(listener);
    public void RemoveListener(UnityAction listener) => _listeners.Remove(listener);
    public void RemoveAllListeners() => _listeners.Clear();
    public void AddPersistentListener(object target, System.Reflection.MethodInfo method, object?[]? arguments) => _persistent.Add((target, method, arguments));
    public void CopyPersistentTo(ISerializedEvent destination, Func<object?, object?> remap)
    {
        foreach (var item in _persistent) destination.AddPersistentListener(remap(item.Target)!, item.Method, item.Arguments?.Select(remap).ToArray());
    }
    public void Invoke()
    {
        foreach (var item in _persistent.ToArray()) item.Method.Invoke(item.Target, item.Arguments ?? []);
        foreach (var listener in _listeners.ToArray()) listener();
    }
}
