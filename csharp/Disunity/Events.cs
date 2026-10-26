using System.Reflection;

namespace UnityEngine.Events;

public delegate void UnityAction<in T>(T value);

/// <summary>Unity's <c>PersistentListenerMode</c>: what a persistent call passes.</summary>
public enum PersistentListenerMode { EventDefined, Void, Object, Int, Float, String, Bool }

/// <summary>Unity's <c>UnityEventCallState</c>.</summary>
public enum UnityEventCallState { Off, EditorAndRuntime, RuntimeOnly }

/// <summary>
/// A call authored on a UnityEvent: a method on a target object, with the event's argument or
/// a fixed one. Calls are serialized data, so a cloned event calls the clone's targets.
/// </summary>
[Serializable]
public sealed class PersistentCall
{
    [SerializeField] internal Object? Target;
    [SerializeField] internal string MethodName = "";
    [SerializeField] internal PersistentListenerMode Mode;
    [SerializeField] internal UnityEventCallState CallState = UnityEventCallState.RuntimeOnly;
    [SerializeField] internal Object? ObjectArgument;
    [SerializeField] internal int IntArgument;
    [SerializeField] internal float FloatArgument;
    [SerializeField] internal string StringArgument = "";
    [SerializeField] internal bool BoolArgument;

    /// <summary>The target's method this call invokes, given the event's argument type.</summary>
    internal MethodInfo Method(Type eventArgument)
    {
        var parameters = Mode switch
        {
            PersistentListenerMode.EventDefined => [eventArgument],
            PersistentListenerMode.Void => Type.EmptyTypes,
            PersistentListenerMode.Object => [ObjectArgument?.GetType() ?? typeof(Object)],
            PersistentListenerMode.Int => [typeof(int)],
            PersistentListenerMode.Float => [typeof(float)],
            PersistentListenerMode.String => [typeof(string)],
            PersistentListenerMode.Bool => [typeof(bool)],
            _ => throw new NotSupportedException($"persistent listener mode {Mode}"),
        };
        var type = Target?.GetType() ?? throw new InvalidOperationException($"persistent call {MethodName} has no target");
        return type.GetMethod(MethodName, BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic, parameters)
            ?? throw new MissingMethodException($"{type.Name} has no method {MethodName}({string.Join(", ", parameters.Select(p => p.Name))})");
    }

    internal void Invoke<T>(T value)
    {
        if (CallState == UnityEventCallState.Off || Target == null) return;
        object?[] arguments = Mode switch
        {
            PersistentListenerMode.EventDefined => [value],
            PersistentListenerMode.Void => [],
            PersistentListenerMode.Object => [ObjectArgument],
            PersistentListenerMode.Int => [IntArgument],
            PersistentListenerMode.Float => [FloatArgument],
            PersistentListenerMode.String => [StringArgument],
            _ => [BoolArgument],
        };
        Method(typeof(T)).Invoke(Target, arguments);
    }
}

/// <summary>Lets the scene loader add authored calls to any UnityEvent.</summary>
internal interface IPersistentCalls
{
    void AddPersistentCall(PersistentCall call);
}

[Serializable]
public class UnityEvent<T> : IPersistentCalls
{
    [SerializeField] private List<PersistentCall> m_PersistentCalls = new();
    private event UnityAction<T>? Listeners;
    public void AddListener(UnityAction<T> listener) => Listeners += listener;
    public void RemoveListener(UnityAction<T> listener) => Listeners -= listener;
    /// <summary>Removes the listeners added at runtime; persistent calls stay, as in Unity.</summary>
    public void RemoveAllListeners() => Listeners = null;
    public int GetPersistentEventCount() => m_PersistentCalls.Count;
    public string GetPersistentMethodName(int index) => m_PersistentCalls[index].MethodName;
    public Object? GetPersistentTarget(int index) => m_PersistentCalls[index].Target;
    /// <summary>Adds an authored call, checking that its target has the method.</summary>
    void IPersistentCalls.AddPersistentCall(PersistentCall call)
    {
        call.Method(typeof(T));
        m_PersistentCalls.Add(call);
    }
    /// <summary>Calls the persistent calls in authored order, then the runtime listeners.</summary>
    public void Invoke(T value)
    {
        foreach (var call in m_PersistentCalls) call.Invoke(value);
        Listeners?.Invoke(value);
    }
}
