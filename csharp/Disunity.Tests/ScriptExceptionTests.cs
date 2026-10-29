using System.Collections;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Events;

/// <summary>
/// An exception thrown by a script message, a coroutine step or a UnityEvent listener is logged
/// against the script and the frame goes on, as in Unity. A native error fails the step.
/// </summary>
internal static class ScriptExceptionTests
{
    private sealed class Logged : ILogHandler, IDisposable
    {
        private readonly ILogHandler _previous = Debug.unityLogger.logHandler;
        public readonly List<(Exception Error, UnityEngine.Object? Context)> Exceptions = new();
        public Logged() => Debug.unityLogger.logHandler = this;
        public void LogFormat(LogType logType, UnityEngine.Object? context, string format, params object?[] args) { }
        public void LogException(Exception exception, UnityEngine.Object? context) => Exceptions.Add((exception, context));
        public void Dispose() => Debug.unityLogger.logHandler = _previous;
    }

    /// <summary>Records every message and throws from the ones it is told to fail in.</summary>
    private sealed class Hooked : MonoBehaviour
    {
        public readonly List<string> Calls = new();
        private readonly HashSet<string> _fails;
        public Hooked() => _fails = new();
        public Hooked(params string[] fails) => _fails = new(fails);
        public int Count(string message) => Calls.Count(call => call == message);
        private void Run(string message)
        {
            Calls.Add(message);
            if (_fails.Contains(message)) throw new InvalidOperationException($"{message} failed");
        }
        private void Awake() => Run("Awake");
        private void OnEnable() => Run("OnEnable");
        private void Start() => Run("Start");
        private void FixedUpdate() => Run("FixedUpdate");
        private void Update() => Run("Update");
        private void LateUpdate() => Run("LateUpdate");
        private void OnDisable() => Run("OnDisable");
        private void OnDestroy() => Run("OnDestroy");
    }

    private sealed class Routines : MonoBehaviour
    {
        public readonly List<string> Calls = new();
        public IEnumerator Bad()
        {
            Calls.Add("bad-1");
            try
            {
                yield return null;
                Calls.Add("bad-2");
                throw new InvalidOperationException("bad coroutine");
            }
            finally { Calls.Add("bad-finally"); }
        }
        public IEnumerator Outer()
        {
            try
            {
                yield return Inner();
                Calls.Add("outer-after");
            }
            finally { Calls.Add("outer-finally"); }
        }
        private IEnumerator Inner()
        {
            yield return null;
            throw new InvalidOperationException("nested coroutine");
        }
        public IEnumerator Good()
        {
            while (true) { Calls.Add("good"); yield return null; }
        }
        public IEnumerator Immediate()
        {
            Calls.Add("immediate");
            if (Calls.Count > 0) throw new InvalidOperationException("immediate coroutine");
            yield break;
        }
    }

    private sealed class IteratorStart : MonoBehaviour
    {
        public readonly List<string> Calls = new();
        private IEnumerator Start()
        {
            Calls.Add("start-1");
            yield return null;
            Calls.Add("start-2");
            throw new InvalidOperationException("iterator Start failed");
        }
        private void Update() => Calls.Add("update");
    }

    private sealed class EventScript : MonoBehaviour
    {
        public readonly UnityEvent Event = new();
        public bool ReachedEnd;
        private void Update()
        {
            Event.Invoke();
            ReachedEnd = true;
        }
    }

    private sealed class Contact : MonoBehaviour
    {
        public readonly List<string> Calls = new();
        private void OnTriggerEnter2D(Collider2D other)
        {
            Calls.Add("enter");
            throw new InvalidOperationException("contact failed");
        }
        private void OnTriggerStay2D(Collider2D other) => Calls.Add("stay");
    }

    private sealed class Asset : ScriptableObject
    {
        public static readonly List<string> Calls = new();
        private void Awake() { Calls.Add("Awake"); throw new InvalidOperationException("asset Awake failed"); }
        private void OnEnable() => Calls.Add("OnEnable");
        private void OnDestroy() { Calls.Add("OnDestroy"); throw new InvalidOperationException("asset OnDestroy failed"); }
    }

    private sealed class CloneProbe : MonoBehaviour
    {
        public static bool Fail;
        public static int Sibling;
        private void Awake() { if (Fail) throw new InvalidOperationException("clone Awake failed"); }
    }

    private sealed class CloneSibling : MonoBehaviour
    {
        private void Awake() => CloneProbe.Sibling++;
    }

    public static void Run()
    {
        UpdateThrowsEveryFrame();
        AwakeThrowingKeepsOnEnableAndStart();
        StartThrowingIsNotRetried();
        IteratorStartThrowingStopsItsCoroutine();
        CoroutineThrowingStopsOnlyItself();
        OnDestroyThrowingDoesNotStopTheOthers();
        UnityEventListenerThrowingStopsOnlyThatEvent();
        ContactMessageThrowing();
        AssetAndCloneMessages();
        PostedWorkIsCounted();
        NativeErrorsStillFailTheStep();
        CounterAndSummaryResetWithTheWorld();
        Console.WriteLine("PASS: script exceptions are logged and the frame goes on; native errors still fail the step");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static void UpdateThrowsEveryFrame()
    {
        DisunityHost.LoadWorld();
        var received = new List<(string Condition, LogType Type)>();
        Application.LogCallback listen = (condition, _, type) => received.Add((condition, type));
        Application.logMessageReceived += listen;
        try
        {
            var failing = DisunityHost.AddComponent(new Hooked("Update"));
            var other = DisunityHost.AddComponent(new Hooked());
            for (var frame = 0; frame < 3; frame++) DisunityHost.Step(0.02f);
            Require(received.Count == 3 && received.All(r => r.Type == LogType.Exception && r.Condition == "InvalidOperationException: Update failed"),
                "every throwing Update reaches logMessageReceived as an Exception");
            Require(failing.Count("Update") == 3 && failing.Count("LateUpdate") == 3 && failing.Count("FixedUpdate") > 0,
                "the throwing script keeps receiving Update, LateUpdate and FixedUpdate");
            Require(other.Count("Update") == 3 && other.Count("LateUpdate") == 3, "other scripts ran in each frame");
            Require(DisunityHost.ScriptExceptionCount == 3, "each logged exception is counted");
            Require(DisunityHost.ScriptExceptionSummary == "3 script exceptions were logged; the first: InvalidOperationException: Update failed",
                "the summary names the count and the first exception");
        }
        finally { Application.logMessageReceived -= listen; }
        using (var log = new Logged())
        {
            DisunityHost.Step(0.02f);
            var entry = log.Exceptions.Single();
            Require(entry.Error is InvalidOperationException { Message: "Update failed" }, "the logged exception is the game's, not a reflection wrapper");
            Require(entry.Context is Hooked, "the exception is logged against the script");
        }
        DisunityHost.UnloadWorld();
    }

    /// <summary>
    /// Each message is sent independently, so an Awake that throws does not stop OnEnable or Start
    /// for that script: the exception is logged and the component still enables and starts, which the
    /// native scheduler already does once the exception no longer fails the step.
    /// </summary>
    private static void AwakeThrowingKeepsOnEnableAndStart()
    {
        DisunityHost.LoadWorld();
        var script = DisunityHost.AddComponent(new Hooked("Awake"));
        Require(script.Calls.SequenceEqual(new[] { "Awake", "OnEnable" }), "OnEnable follows an Awake that threw");
        DisunityHost.Step(0.02f);
        Require(script.Count("Start") == 1 && script.Count("Update") == 1, "Start and Update follow an Awake that threw");
        Require(DisunityHost.ScriptExceptionCount == 1, "only the Awake failure was logged");
        DisunityHost.AddComponent(new Hooked("OnEnable"));
        DisunityHost.Step(0.02f);
        Require(DisunityHost.ScriptExceptionCount == 2, "a throwing OnEnable is logged");
        DisunityHost.UnloadWorld();
    }

    private static void StartThrowingIsNotRetried()
    {
        DisunityHost.LoadWorld();
        var script = DisunityHost.AddComponent(new Hooked("Start"));
        for (var frame = 0; frame < 3; frame++) DisunityHost.Step(0.02f);
        Require(script.Count("Start") == 1, "Start runs once although it threw");
        Require(script.Count("Update") == 3, "Update runs after a Start that threw");
        Require(DisunityHost.ScriptExceptionCount == 1, "the Start failure is logged once");
        DisunityHost.UnloadWorld();
    }

    private static void IteratorStartThrowingStopsItsCoroutine()
    {
        DisunityHost.LoadWorld();
        using var log = new Logged();
        var script = DisunityHost.AddComponent(new IteratorStart());
        for (var frame = 0; frame < 4; frame++) DisunityHost.Step(0.02f);
        Require(script.Calls.Count(call => call == "start-1") == 1 && script.Calls.Count(call => call == "start-2") == 1,
            "an iterator Start stops at the step that threw");
        Require(script.Calls.Count(call => call == "update") == 4, "Update continues after the Start coroutine threw");
        Require(log.Exceptions.Single().Error.Message == "iterator Start failed" && ReferenceEquals(log.Exceptions[0].Context, script),
            "the coroutine failure is logged against its script");
        DisunityHost.UnloadWorld();
    }

    private static void CoroutineThrowingStopsOnlyItself()
    {
        DisunityHost.LoadWorld();
        using var log = new Logged();
        var script = DisunityHost.AddComponent(new Routines());
        script.StartCoroutine(script.Good());
        script.StartCoroutine(script.Bad());
        for (var frame = 0; frame < 4; frame++) DisunityHost.Step(0.02f);
        var count = (string call) => script.Calls.Count(value => value == call);
        Require(count("bad-1") == 1 && count("bad-2") == 1 && count("bad-finally") == 1, "the throwing coroutine stopped and ran its finally block once");
        Require(count("good") == 5, $"the other coroutine kept running (ran {count("good")} steps)");
        Require(log.Exceptions.Count == 1 && ReferenceEquals(log.Exceptions[0].Context, script), "one exception, logged against the owner");
        script.Calls.Clear();
        script.StartCoroutine(script.Outer());
        DisunityHost.Step(0.02f);
        DisunityHost.Step(0.02f);
        Require(count("outer-finally") == 1 && count("outer-after") == 0, "an exception in a nested coroutine ends the whole coroutine");
        var routine = script.StartCoroutine(script.Immediate());
        Require(routine != null && log.Exceptions.Count == 3, "a first step that throws is logged and StartCoroutine still returns");
        DisunityHost.UnloadWorld();
    }

    private static void OnDestroyThrowingDoesNotStopTheOthers()
    {
        DisunityHost.LoadWorld();
        using var log = new Logged();
        var scripts = new[] { new Hooked("OnDestroy"), new Hooked(), new Hooked("OnDestroy") }.Select(DisunityHost.AddComponent).ToArray();
        foreach (var script in scripts) UnityEngine.Object.Destroy(script.gameObject);
        DisunityHost.Step(0.02f);
        Require(scripts.All(script => script.Count("OnDestroy") == 1), "every OnDestroy ran");
        Require(scripts.All(script => !script.gameObject), "every object was destroyed");
        Require(log.Exceptions.Count == 2, "the two failures were logged");
        DisunityHost.UnloadWorld();
    }

    private static void UnityEventListenerThrowingStopsOnlyThatEvent()
    {
        DisunityHost.LoadWorld();
        using var log = new Logged();
        var script = DisunityHost.AddComponent(new EventScript());
        var other = DisunityHost.AddComponent(new Hooked());
        var listeners = new List<string>();
        script.Event.AddListener(() => listeners.Add("first"));
        script.Event.AddListener(() => throw new InvalidOperationException("listener failed"));
        script.Event.AddListener(() => listeners.Add("third"));
        DisunityHost.Step(0.02f);
        Require(listeners.SequenceEqual(new[] { "first" }), "the listeners after the one that threw did not run");
        Require(!script.ReachedEnd, "the message that invoked the event stopped there");
        Require(log.Exceptions.Count == 1 && log.Exceptions[0].Error.Message == "listener failed", "the listener failure is logged once");
        Require(other.Count("Update") == 1 && other.Count("LateUpdate") == 1, "other scripts ran in the frame");
        DisunityHost.UnloadWorld();
    }

    private static void ContactMessageThrowing()
    {
        DisunityHost.LoadWorld();
        using var log = new Logged();
        var a = DisunityHost.AddComponent(new Contact());
        a.gameObject.AddComponent<Rigidbody2D>().gravityScale = 0;
        a.gameObject.AddComponent<BoxCollider2D>().isTrigger = true;
        var b = DisunityHost.AddComponent(new Contact());
        b.gameObject.AddComponent<BoxCollider2D>();
        b.transform.position = new Vector3(0.8f, 0, 0);
        DisunityHost.Step(0.02f);
        DisunityHost.Step(0.02f);
        Require(a.Calls.Contains("enter") && a.Calls.Contains("stay"), "trigger Stay is delivered after Enter threw");
        Require(log.Exceptions.Count > 0 && log.Exceptions.All(e => e.Error.Message == "contact failed" && e.Context is Contact), "contact failures are logged against their script");
        DisunityHost.UnloadWorld();
    }

    private static void AssetAndCloneMessages()
    {
        DisunityHost.LoadWorld();
        using var log = new Logged();
        Asset.Calls.Clear();
        var asset = ScriptableObject.CreateInstance<Asset>();
        Require(asset && Asset.Calls.SequenceEqual(new[] { "Awake", "OnEnable" }), "CreateInstance returns the asset after its Awake threw");
        UnityEngine.Object.Destroy(asset);
        DisunityHost.Step(0.02f);
        Require(Asset.Calls.Contains("OnDestroy") && !asset, "the asset is destroyed although OnDestroy threw");
        Require(log.Exceptions.Count == 2 && log.Exceptions.All(e => e.Context is Asset), "both asset failures are logged against the asset");

        var template = DisunityHost.AddComponent(new CloneProbe());
        template.gameObject.AddComponent<CloneSibling>();
        CloneProbe.Sibling = 0;
        CloneProbe.Fail = true;
        try
        {
            var clone = UnityEngine.Object.Instantiate(template);
            Require(clone && CloneProbe.Sibling == 1, "Instantiate returns the clone and runs its other scripts after an Awake threw");
        }
        finally { CloneProbe.Fail = false; }
        Require(log.Exceptions.Count == 3, "the clone's Awake failure is logged");
        DisunityHost.UnloadWorld();
    }

    private static void PostedWorkIsCounted()
    {
        DisunityHost.LoadWorld();
        using var log = new Logged();
        SynchronizationContext.Current!.Post(_ => throw new InvalidOperationException("posted failed"), null);
        DisunityHost.Step(0.02f);
        Require(log.Exceptions.Count == 1 && DisunityHost.ScriptExceptionCount == 1, "posted work that throws is logged and counted");
        DisunityHost.UnloadWorld();
    }

    private static void NativeErrorsStillFailTheStep()
    {
        DisunityHost.LoadWorld();
        using (var log = new Logged())
        {
            DisunityHost.AddComponent(new ErrorInUpdate());
            Expect.Throws<DisunityException>(() => DisunityHost.Step(0.02f), "a native error that surfaces in Update");
            Require(DisunityHost.ScriptExceptionCount == 0 && log.Exceptions.Count == 0, "the native error was not logged as a script exception");
        }
        DisunityHost.UnloadWorld();
        Expect.Throws<InvalidOperationException>(() => DisunityHost.Step(0.02f), "a step without a loaded world");
        DisunityHost.LoadWorld();
        Expect.Throws<DisunityException>(() => DisunityHost.Step(float.NaN), "an invalid delta");
        DisunityHost.UnloadWorld();
    }

    private sealed class ErrorInUpdate : MonoBehaviour
    {
        private void Update() => throw EngineFault.Make("native error in Update");
    }

    private static void CounterAndSummaryResetWithTheWorld()
    {
        DisunityHost.LoadWorld();
        Require(DisunityHost.ScriptExceptionCount == 0 && DisunityHost.ScriptExceptionSummary is null && DisunityHost.FirstScriptException is null,
            "a loaded world starts without logged exceptions");
        using (new Logged()) DisunityHost.AddComponent(new Hooked("Awake"));
        Require(DisunityHost.ScriptExceptionCount == 1, "the exception was counted");
        DisunityHost.UnloadWorld();
        Require(DisunityHost.ScriptExceptionCount == 1, "the count survives unload so the run can report it");
        DisunityHost.LoadWorld();
        Require(DisunityHost.ScriptExceptionCount == 0 && DisunityHost.FirstScriptException is null, "loading a world resets the count");
        DisunityHost.UnloadWorld();
    }
}
