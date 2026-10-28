using System.Reflection;
using System.Runtime.ExceptionServices;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Events;

/// <summary>
/// UnityEvent with zero to four arguments: listener order and removal, persistent calls in each
/// mode with the event's arguments, call states, method lookup and exceptions. Persistent calls
/// are added the way the scene loader adds them.
/// </summary>
internal static class UnityEventTests
{
    private sealed class Receiver : ScriptableObject
    {
        public readonly List<string> Calls = new();
        public void Ping() => Calls.Add("ping");
        public void Count(int value) => Calls.Add($"count {value}");
        public void Pair(int a, string b) => Calls.Add($"pair {a} {b}");
        public void Four(int a, int b, int c, int d) => Calls.Add($"four {a + b + c + d}");
        public void Flag(bool value) => Calls.Add($"flag {value}");
        public void Fail() => throw new InvalidOperationException("listener failed");
        public static void Shared() { }
        public void Measure(float value) => Calls.Add($"float {value}");
        public void Measure(double value) => Calls.Add($"double {value}");
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            ListenersRunInOrderAfterPersistentCalls();
            EventsTakeUpToFourArguments();
            PersistentCallsUseTheirMode();
            CallStatesAndCounts();
            MethodLookupMatchesUnity();
            ExceptionsAreNotWrapped();
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
        Console.WriteLine("PASS: UnityEvent listeners and persistent calls with zero to four arguments");
    }

    /// <summary>A persistent call as the scene loader builds one; its fields are internal to the
    /// engine, so they are set through reflection.</summary>
    private static PersistentCall Call(Receiver target, string method, PersistentListenerMode mode, bool flag = false)
    {
        var call = new PersistentCall();
        foreach (var (name, value) in new (string, object)[] { ("Target", target), ("MethodName", method), ("Mode", mode), ("BoolArgument", flag) })
            typeof(PersistentCall).GetField(name, BindingFlags.Instance | BindingFlags.NonPublic)!.SetValue(call, value);
        return call;
    }

    /// <summary>Adds a call through the engine's internal interface, as the scene loader does.</summary>
    private static void Add(UnityEventBase target, PersistentCall call)
    {
        var add = typeof(UnityEventBase).Assembly.GetType("UnityEngine.Events.IPersistentCalls")!.GetMethod("AddPersistentCall")!;
        try { add.Invoke(target, [call]); }
        catch (TargetInvocationException wrapped) when (wrapped.InnerException is { } inner) { ExceptionDispatchInfo.Capture(inner).Throw(); }
    }

    private static void ListenersRunInOrderAfterPersistentCalls()
    {
        var receiver = ScriptableObject.CreateInstance<Receiver>();
        var signal = new UnityEvent();
        Add(signal, Call(receiver, nameof(Receiver.Ping), PersistentListenerMode.Void));
        UnityAction first = () => receiver.Calls.Add("first");
        UnityAction second = () => receiver.Calls.Add("second");
        signal.AddListener(first);
        signal.AddListener(second);
        signal.AddListener(first);
        signal.Invoke();
        Require(string.Join(",", receiver.Calls) == "ping,first,second,first", $"invocation order: {string.Join(",", receiver.Calls)}");
        receiver.Calls.Clear();
        signal.RemoveListener(first);
        signal.Invoke();
        Require(string.Join(",", receiver.Calls) == "ping,second", "RemoveListener removes every occurrence");
        receiver.Calls.Clear();
        UnityAction late = () => receiver.Calls.Add("late");
        signal.AddListener(() => signal.AddListener(late));
        signal.Invoke();
        Require(!receiver.Calls.Contains("late"), "a listener added during an invocation waits for the next one");
        signal.RemoveAllListeners();
        receiver.Calls.Clear();
        signal.Invoke();
        Require(string.Join(",", receiver.Calls) == "ping", "RemoveAllListeners keeps persistent calls");
    }

    private static void EventsTakeUpToFourArguments()
    {
        var receiver = ScriptableObject.CreateInstance<Receiver>();
        var one = new UnityEvent<int>();
        one.AddListener(receiver.Count);
        one.Invoke(3);
        var two = new UnityEvent<int, string>();
        two.AddListener(receiver.Pair);
        two.Invoke(1, "a");
        var three = new UnityEvent<int, int, int>();
        three.AddListener((a, b, c) => receiver.Calls.Add($"three {a + b + c}"));
        three.Invoke(1, 2, 3);
        var four = new UnityEvent<int, int, int, int>();
        four.AddListener(receiver.Four);
        four.Invoke(1, 2, 3, 4);
        Require(string.Join(",", receiver.Calls) == "count 3,pair 1 a,three 6,four 10", $"arguments: {string.Join(",", receiver.Calls)}");
        Require(new UnityEvent().ToString().EndsWith(" UnityEngine.Events.UnityEvent", StringComparison.Ordinal), "ToString names the event type");
    }

    private static void PersistentCallsUseTheirMode()
    {
        var receiver = ScriptableObject.CreateInstance<Receiver>();
        var pair = new UnityEvent<int, string>();
        Add(pair, Call(receiver, nameof(Receiver.Pair), PersistentListenerMode.EventDefined));
        Add(pair, Call(receiver, nameof(Receiver.Flag), PersistentListenerMode.Bool, flag: true));
        Add(pair, Call(receiver, nameof(Receiver.Ping), PersistentListenerMode.Void));
        pair.Invoke(5, "b");
        Require(string.Join(",", receiver.Calls) == "pair 5 b,flag True,ping", $"persistent modes: {string.Join(",", receiver.Calls)}");
        var none = new UnityEvent();
        Expect<MissingMethodException>(() => Add(none, Call(receiver, nameof(Receiver.Count), PersistentListenerMode.EventDefined)),
            "an EventDefined call needs a method taking the event's arguments");
        Add(none, Call(receiver, nameof(Receiver.Ping), PersistentListenerMode.EventDefined));
        Require(none.GetPersistentEventCount() == 1, "an EventDefined call on a zero-argument event takes no arguments");
    }

    private static void CallStatesAndCounts()
    {
        var receiver = ScriptableObject.CreateInstance<Receiver>();
        var signal = new UnityEvent();
        Add(signal, Call(receiver, nameof(Receiver.Ping), PersistentListenerMode.Void));
        Require(signal.GetPersistentEventCount() == 1 && signal.GetPersistentMethodName(0) == "Ping"
            && ReferenceEquals(signal.GetPersistentTarget(0), receiver), "persistent call accessors");
        Require(signal.GetPersistentListenerState(0) == UnityEventCallState.RuntimeOnly, "calls default to RuntimeOnly");
        signal.SetPersistentListenerState(0, UnityEventCallState.Off);
        signal.Invoke();
        Require(receiver.Calls.Count == 0, "an Off call does not run");
        signal.SetPersistentListenerState(0, UnityEventCallState.EditorAndRuntime);
        signal.Invoke();
        Require(receiver.Calls.Count == 1, "an EditorAndRuntime call runs in a player");
        UnityEngine.Object.Destroy(receiver);
        DisunityHost.Step(0);
        signal.Invoke();
        Require(receiver.Calls.Count == 1, "a call whose target is destroyed does not run");
    }

    private static void MethodLookupMatchesUnity()
    {
        var type = typeof(Receiver);
        Require(UnityEventBase.GetValidMethodInfo(type, "Count", [typeof(int)])?.Name == "Count", "an exact match");
        Require(UnityEventBase.GetValidMethodInfo(type, "Count", [typeof(long)]) is null, "primitive parameters match exactly");
        Require(UnityEventBase.GetValidMethodInfo(type, "Measure", [typeof(double)])?.GetParameters()[0].ParameterType == typeof(double),
            "the overload with the given parameter types");
        Require(UnityEventBase.GetValidMethodInfo(type, "Shared", Type.EmptyTypes)?.IsStatic == true, "static methods are found");
        Require(UnityEventBase.GetValidMethodInfo(type, "GetHashCode", Type.EmptyTypes)?.DeclaringType == typeof(UnityEngine.Object), "base class methods are found");
        Require(UnityEventBase.GetValidMethodInfo(ScriptableObject.CreateInstance<Receiver>(), "Ping", Type.EmptyTypes) is not null, "lookup by instance");
        Require(UnityEventBase.GetValidMethodInfo(type, "Missing", Type.EmptyTypes) is null, "a missing method is null");
    }

    private static void ExceptionsAreNotWrapped()
    {
        var receiver = ScriptableObject.CreateInstance<Receiver>();
        var signal = new UnityEvent();
        signal.AddListener(receiver.Fail);
        signal.AddListener(receiver.Ping);
        Expect<InvalidOperationException>(signal.Invoke, "a listener's exception propagates unwrapped");
        Require(receiver.Calls.Count == 0, "a throwing listener stops the invocation");
        var persistent = new UnityEvent();
        Add(persistent, Call(receiver, nameof(Receiver.Fail), PersistentListenerMode.Void));
        Expect<InvalidOperationException>(persistent.Invoke, "a persistent call's exception propagates unwrapped");
        Expect<ArgumentNullException>(() => signal.AddListener(null!), "a null listener is rejected");
    }

    private static void Expect<T>(Action action, string message) where T : Exception
    {
        try { action(); }
        catch (T) { return; }
        catch (TargetInvocationException) { throw new InvalidOperationException($"unity events: {message}: the exception was wrapped"); }
        throw new InvalidOperationException($"unity events: {message}: expected {typeof(T).Name}");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException($"unity events: {message}");
    }
}
