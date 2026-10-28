using System.Reflection;

namespace UnityEngine.Events;

public delegate void UnityAction();
public delegate void UnityAction<T0>(T0 arg0);
public delegate void UnityAction<T0, T1>(T0 arg0, T1 arg1);
public delegate void UnityAction<T0, T1, T2>(T0 arg0, T1 arg1, T2 arg2);
public delegate void UnityAction<T0, T1, T2, T3>(T0 arg0, T1 arg1, T2 arg2, T3 arg3);

/// <summary>How a persistent call gets its arguments: the event's own, none, or one fixed
/// argument of the given kind.</summary>
public enum PersistentListenerMode { EventDefined, Void, Object, Int, Float, String, Bool }

/// <summary>When a persistent call runs. A built player runs calls in both runtime states.</summary>
public enum UnityEventCallState { Off, EditorAndRuntime, RuntimeOnly }

/// <summary>
/// A call authored on a UnityEvent: a method on a target object, with the event's arguments or
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

    private Type[] Parameters(Type[] eventArguments) => Mode switch
    {
        PersistentListenerMode.EventDefined => eventArguments,
        PersistentListenerMode.Void => Type.EmptyTypes,
        PersistentListenerMode.Object => [ObjectArgument?.GetType() ?? typeof(Object)],
        PersistentListenerMode.Int => [typeof(int)],
        PersistentListenerMode.Float => [typeof(float)],
        PersistentListenerMode.String => [typeof(string)],
        PersistentListenerMode.Bool => [typeof(bool)],
        _ => throw new NotSupportedException($"persistent listener mode {Mode}"),
    };

    /// <summary>The target's method this call invokes, given the event's argument types.</summary>
    internal MethodInfo Method(Type[] eventArguments)
    {
        var parameters = Parameters(eventArguments);
        var type = Target?.GetType() ?? throw new InvalidOperationException($"persistent call {MethodName} has no target");
        return UnityEventBase.GetValidMethodInfo(type, MethodName, parameters)
            ?? throw new MissingMethodException($"{type.Name} has no method {MethodName}({string.Join(", ", parameters.Select(p => p.Name))})");
    }

    internal void Invoke(Type[] eventArguments, object?[] values)
    {
        if (CallState == UnityEventCallState.Off || Target == null) return;
        object?[] arguments = Mode switch
        {
            PersistentListenerMode.EventDefined => values,
            PersistentListenerMode.Void => [],
            PersistentListenerMode.Object => [ObjectArgument],
            PersistentListenerMode.Int => [IntArgument],
            PersistentListenerMode.Float => [FloatArgument],
            PersistentListenerMode.String => [StringArgument],
            _ => [BoolArgument],
        };
        Method(eventArguments).Invoke(Target, arguments);
    }
}

/// <summary>Lets the scene loader add authored calls to any UnityEvent.</summary>
internal interface IPersistentCalls
{
    void AddPersistentCall(PersistentCall call);
}
