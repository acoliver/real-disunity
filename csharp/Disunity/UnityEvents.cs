using System.Reflection;
using System.Runtime.ExceptionServices;

namespace UnityEngine.Events;

/// <summary>
/// The persistent calls and runtime listeners every UnityEvent has. <see cref="Invoke"/> runs
/// the persistent calls in authored order, then the runtime listeners in the order they were
/// added. A listener added or removed during an invocation takes effect at the next one.
/// </summary>
[Serializable]
public abstract class UnityEventBase : IPersistentCalls
{
    [SerializeField] private List<PersistentCall> m_PersistentCalls = new();
    private readonly List<Delegate> _listeners = new();

    /// <summary>The event's argument types, which a persistent call in EventDefined mode takes.</summary>
    private protected abstract Type[] ArgumentTypes { get; }

    public int GetPersistentEventCount() => m_PersistentCalls.Count;
    public Object? GetPersistentTarget(int index) => m_PersistentCalls[index].Target;
    public string GetPersistentMethodName(int index) => m_PersistentCalls[index].MethodName;
    public void SetPersistentListenerState(int index, UnityEventCallState state) => m_PersistentCalls[index].CallState = state;
    public UnityEventCallState GetPersistentListenerState(int index) => m_PersistentCalls[index].CallState;

    /// <summary>Removes the listeners added at runtime; persistent calls stay, as in Unity.</summary>
    public void RemoveAllListeners() => _listeners.Clear();

    public override string ToString() => base.ToString() + " " + GetType().FullName;

    public static MethodInfo? GetValidMethodInfo(object obj, string functionName, Type[] argumentTypes)
    {
        ArgumentNullException.ThrowIfNull(obj);
        return GetValidMethodInfo(obj.GetType(), functionName, argumentTypes);
    }

    /// <summary>
    /// The instance or static method named <paramref name="functionName"/> on the type or its
    /// bases whose parameters take <paramref name="argumentTypes"/>, searching the most derived
    /// type first; primitive parameters must match exactly.
    /// </summary>
    public static MethodInfo? GetValidMethodInfo(Type objectType, string functionName, Type[] argumentTypes)
    {
        const BindingFlags Flags = BindingFlags.Instance | BindingFlags.Static | BindingFlags.Public
            | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;
        for (var type = objectType; type != null && type != typeof(object); type = type.BaseType)
        {
            foreach (var method in type.GetMethods(Flags))
                if (method.Name == functionName && Accepts(method.GetParameters(), argumentTypes)) return method;
        }
        return null;
    }

    private static bool Accepts(ParameterInfo[] parameters, Type[] argumentTypes)
    {
        if (parameters.Length != argumentTypes.Length) return false;
        for (var i = 0; i < parameters.Length; i++)
        {
            var parameter = parameters[i].ParameterType;
            var argument = argumentTypes[i];
            if (parameter.IsPrimitive ? parameter != argument : !parameter.IsAssignableFrom(argument)) return false;
        }
        return true;
    }

    /// <summary>Adds an authored call, checking that its target has the method.</summary>
    void IPersistentCalls.AddPersistentCall(PersistentCall call)
    {
        call.Method(ArgumentTypes);
        m_PersistentCalls.Add(call);
    }

    private protected void Add(Delegate listener)
    {
        ArgumentNullException.ThrowIfNull(listener);
        _listeners.Add(listener);
    }

    /// <summary>Removes every occurrence of the listener, as Unity's RemoveListener does.</summary>
    private protected void Remove(Delegate listener) => _listeners.RemoveAll(added => added.Equals(listener));

    /// <summary>Runs the calls. A call that throws stops the invocation with its own exception,
    /// not a reflection wrapper, as when Unity calls the method directly.</summary>
    private protected void Invoke(object?[] values)
    {
        try
        {
            foreach (var call in m_PersistentCalls.ToArray()) call.Invoke(ArgumentTypes, values);
            foreach (var listener in _listeners.ToArray()) listener.DynamicInvoke(values);
        }
        catch (TargetInvocationException wrapped) when (wrapped.InnerException is { } inner)
        {
            ExceptionDispatchInfo.Capture(inner).Throw();
        }
    }
}

[Serializable]
public class UnityEvent : UnityEventBase
{
    public UnityEvent() { }
    private protected override Type[] ArgumentTypes => Type.EmptyTypes;
    public void AddListener(UnityAction call) => Add(call);
    public void RemoveListener(UnityAction call) => Remove(call);
    public void Invoke() => Invoke([]);
}

[Serializable]
public class UnityEvent<T0> : UnityEventBase
{
    public UnityEvent() { }
    private protected override Type[] ArgumentTypes => [typeof(T0)];
    public void AddListener(UnityAction<T0> call) => Add(call);
    public void RemoveListener(UnityAction<T0> call) => Remove(call);
    public void Invoke(T0 arg0) => Invoke([arg0]);
}

[Serializable]
public class UnityEvent<T0, T1> : UnityEventBase
{
    public UnityEvent() { }
    private protected override Type[] ArgumentTypes => [typeof(T0), typeof(T1)];
    public void AddListener(UnityAction<T0, T1> call) => Add(call);
    public void RemoveListener(UnityAction<T0, T1> call) => Remove(call);
    public void Invoke(T0 arg0, T1 arg1) => Invoke([arg0, arg1]);
}

[Serializable]
public class UnityEvent<T0, T1, T2> : UnityEventBase
{
    public UnityEvent() { }
    private protected override Type[] ArgumentTypes => [typeof(T0), typeof(T1), typeof(T2)];
    public void AddListener(UnityAction<T0, T1, T2> call) => Add(call);
    public void RemoveListener(UnityAction<T0, T1, T2> call) => Remove(call);
    public void Invoke(T0 arg0, T1 arg1, T2 arg2) => Invoke([arg0, arg1, arg2]);
}

[Serializable]
public class UnityEvent<T0, T1, T2, T3> : UnityEventBase
{
    public UnityEvent() { }
    private protected override Type[] ArgumentTypes => [typeof(T0), typeof(T1), typeof(T2), typeof(T3)];
    public void AddListener(UnityAction<T0, T1, T2, T3> call) => Add(call);
    public void RemoveListener(UnityAction<T0, T1, T2, T3> call) => Remove(call);
    public void Invoke(T0 arg0, T1 arg1, T2 arg2, T3 arg3) => Invoke([arg0, arg1, arg2, arg3]);
}
