using System.Collections;
using Disunity.Hosting;
using UnityEngine;

internal static class CoroutineInputTests
{
    private sealed class Probe : MonoBehaviour
    {
        public readonly List<string> Calls = new();
        public readonly List<(bool Held, bool Down, bool Up)> Keys = new();
        public readonly List<bool> FixedDown = new();
        private void FixedUpdate() => FixedDown.Add(Input.GetKeyDown(KeyCode.Space));
        public IEnumerator Run()
        {
            try
            {
                Calls.Add("start");
                yield return null;
                Calls.Add("next");
                yield return new WaitForSeconds(0.03f);
                Calls.Add("wait");
            }
            finally { Calls.Add("dispose"); }
        }
        private void Update() => Keys.Add((Input.GetKey(KeyCode.Space), Input.GetKeyDown(KeyCode.Space), Input.GetKeyUp(KeyCode.Space)));
    }
    public static void Run()
    {
        DisunityHost.LoadWorld();
        var probe = DisunityHost.AddComponent(new Probe());
        probe.StartCoroutine(probe.Run());
        Require(probe.Calls.SequenceEqual(new[] { "start" }), "coroutine starts synchronously");
        DisunityHost.PushKey(0, KeyCode.Space, true);
        DisunityHost.Step(0.01f);
        Require(probe.Calls.SequenceEqual(new[] { "start", "next" }), "null resumes on next frame");
        Require(probe.Keys[^1] == (true, true, false), "input held/down");
        DisunityHost.Step(0.01f);
        Require(probe.Keys[^1] == (true, false, false), "input down consumed");
        DisunityHost.PushFocus(0.025, false);
        DisunityHost.Step(0.01f);
        Require(probe.Keys[^1] == (false, false, true), "focus loss releases held key");
        DisunityHost.Step(0.02f);
        Require(probe.Calls.SequenceEqual(new[] { "start", "next", "wait", "dispose" }), "native scaled wait completes and disposes");
        probe.Calls.Clear();
        var stopped = probe.StartCoroutine(probe.Run());
        probe.StopCoroutine(stopped);
        Require(probe.Calls.SequenceEqual(new[] { "start", "dispose" }), "explicit cancellation disposes");
        probe.Calls.Clear();
        probe.StartCoroutine(probe.Run());
        probe.gameObject.SetActive(false);
        Require(probe.Calls.SequenceEqual(new[] { "start", "dispose" }), "native deactivation cancels and disposes");
        probe.Calls.Clear();
        try { probe.StartCoroutine(probe.Run()); throw new Exception("inactive coroutine accepted"); }
        catch (DisunityException) { }
        Require(probe.Calls.Count == 0, "native admission precedes the first iterator body");
        DisunityHost.UnloadWorld();
        TestPhaseEdges();
        TestFailureCancellation();
        TestNestedCancellation();
        TestCurrentCancellation();
        TestFaultedUnloadDisposal();
        Console.WriteLine("PASS: native coroutine waits, cancellation disposal and input focus release");
    }
    private sealed class IteratorStart : MonoBehaviour
    {
        public bool Disposed;
        public bool Fail;
        private IEnumerator Start()
        {
            try { yield return null; yield return new WaitForFixedUpdate(); }
            finally { Disposed = true; }
        }
        private void Update() { if (Fail) throw new InvalidOperationException("iterator owner failed"); }
    }
    private static void TestFailureCancellation()
    {
        DisunityHost.LoadWorld();
        var probe = DisunityHost.AddComponent(new IteratorStart());
        DisunityHost.Step(0.01f);
        probe.Fail = true;
        try { DisunityHost.Step(0.01f); throw new Exception("callback failure was lost"); }
        catch (DisunityException error) when (error.Message.Contains("iterator owner failed")) { }
        Require(probe.Disposed, "fault teardown disposes iterator-returning Start");
        DisunityHost.UnloadWorld();
    }
    private static void TestPhaseEdges()
    {
        DisunityHost.LoadWorld();
        var probe = DisunityHost.AddComponent(new Probe());
        DisunityHost.PushKey(0, KeyCode.Space, true);
        DisunityHost.Step(0.041f);
        Require(probe.FixedDown.SequenceEqual(new[] { true, false }), "fixed input edge consumed once across catch-up");
        Require(probe.Keys.Single() == (true, true, false), "dynamic input edge independent of fixed consumption");
        DisunityHost.UnloadWorld();
    }
    private static IEnumerator Nested(List<string> calls)
    {
        try { yield return Child(calls); }
        finally { calls.Add("outer-dispose"); }
    }
    private static IEnumerator Child(List<string> calls)
    {
        try { yield return new WaitForSecondsRealtime(1); }
        finally { calls.Add("inner-dispose"); }
    }
    private static void TestNestedCancellation()
    {
        DisunityHost.LoadWorld();
        var probe = DisunityHost.AddComponent(new Probe());
        probe.StartCoroutine(Nested(probe.Calls));
        probe.StopAllCoroutines();
        Require(probe.Calls.SequenceEqual(new[] { "inner-dispose", "outer-dispose" }), "nested cancellation disposes inner before outer");
        DisunityHost.UnloadWorld();
    }
    private sealed class ThrowingIterator : IEnumerator, IDisposable
    {
        public object Current => new WaitForSeconds(1);
        public bool MoveNext() => true;
        public void Reset() => throw new NotSupportedException();
        public void Dispose() => throw new InvalidOperationException("disposal failed");
    }
    private static void TestFaultedUnloadDisposal()
    {
        DisunityHost.LoadWorld();
        var probe = DisunityHost.AddComponent(new Probe());
        probe.StartCoroutine(new ThrowingIterator());
        DisunityHost.ReportFault();
        try { DisunityHost.UnloadWorld(); throw new Exception("disposal failure was lost"); }
        catch (AggregateException error) when (error.ToString().Contains("disposal failed")) { }
        Require(!DisunityHost.IsLoaded, "disposal failure must not leak the native world");
        DisunityHost.LoadWorld();
        DisunityHost.UnloadWorld();
    }
    private sealed class CancellingCurrent(MonoBehaviour owner) : IEnumerator, IDisposable
    {
        private int _step;
        public int Disposals;
        public bool DisposedInsideCurrent;
        public object? Current
        {
            get
            {
                if (_step == 2)
                {
                    owner.gameObject.SetActive(false);
                    DisposedInsideCurrent = Disposals != 0;
                }
                return null;
            }
        }
        public bool MoveNext() { _step++; return true; }
        public void Reset() => throw new NotSupportedException();
        public void Dispose() => Disposals++;
    }
    private static void TestCurrentCancellation()
    {
        DisunityHost.LoadWorld();
        var probe = DisunityHost.AddComponent(new Probe());
        var iterator = new CancellingCurrent(probe);
        probe.StartCoroutine(iterator);
        DisunityHost.Step(0.01f);
        Require(!iterator.DisposedInsideCurrent && iterator.Disposals == 1, "cancellation disposes after Current returns, exactly once");
        DisunityHost.UnloadWorld();
    }
    private static void Require(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }
}
