using System.Collections;
using Disunity.Hosting;
using UnityEngine;

/// <summary>
/// The main thread's synchronization context: installed with the world, drained once a frame
/// after <c>Update</c> coroutines and before <c>LateUpdate</c>, and removed with the world.
/// </summary>
internal static class SynchronizationContextTests
{
    private sealed class Probe : MonoBehaviour
    {
        public readonly List<string> Calls = new();
        private void Update() => Calls.Add("update");
        private void LateUpdate() => Calls.Add("late");
        public IEnumerator Next()
        {
            yield return null;
            Calls.Add("coroutine");
        }
    }

    public static void Run()
    {
        var before = SynchronizationContext.Current;
        DisunityHost.LoadWorld();
        try
        {
            var context = SynchronizationContext.Current;
            Require(context is not null && context.GetType().FullName == "UnityEngine.UnitySynchronizationContext",
                "loading a world installs Unity's main thread context");
            Require(TaskScheduler.FromCurrentSynchronizationContext() is not null, "the context serves as a task scheduler");
            RunsPostedWorkAfterUpdateCoroutines(context!);
            DefersWorkPostedWhileRunning(context!);
            SendsFromOtherThreadsThroughTheQueue(context!);
            ResumesAwaitsOnTheMainThread();
            LogsFailuresAndRunsTheRestInOrder(context!);
            RunsWorkWithItselfCurrentAndRestoresTheThread(context!);
        }
        finally { DisunityHost.UnloadWorld(); }
        Require(ReferenceEquals(SynchronizationContext.Current, before), "unloading the world restores the previous context");
        UnloadingReleasesWaitingSenders();
        Console.WriteLine("PASS: the main thread synchronization context runs posted work once a frame after Update coroutines");
    }

    private static void RunsPostedWorkAfterUpdateCoroutines(SynchronizationContext context)
    {
        var probe = DisunityHost.AddComponent(new Probe());
        probe.StartCoroutine(probe.Next());
        context.Post(_ => probe.Calls.Add("posted"), null);
        DisunityHost.Step(0.02f);
        Require(probe.Calls.SequenceEqual(["update", "coroutine", "posted", "late"]), string.Join(" ", probe.Calls));
        UnityEngine.Object.Destroy(probe.gameObject);
        DisunityHost.Step(0.02f);
    }

    private static void DefersWorkPostedWhileRunning(SynchronizationContext context)
    {
        var calls = new List<string>();
        context.Post(_ =>
        {
            calls.Add("first");
            SynchronizationContext.Current!.Post(_ => calls.Add("second"), null);
        }, null);
        DisunityHost.Step(0.02f);
        Require(calls.SequenceEqual(["first"]), "work posted while the queue runs waits for the next frame");
        DisunityHost.Step(0.02f);
        Require(calls.SequenceEqual(["first", "second"]), "deferred work runs the next frame");
        var main = Environment.CurrentManagedThreadId;
        var inline = 0;
        context.Send(_ => inline = Environment.CurrentManagedThreadId, null);
        Require(inline == main, "Send on the main thread runs at once");
    }

    private static void SendsFromOtherThreadsThroughTheQueue(SynchronizationContext context)
    {
        var main = Environment.CurrentManagedThreadId;
        var ranOn = 0;
        var sender = Task.Run(() => context.Send(_ => ranOn = Environment.CurrentManagedThreadId, null));
        for (var frame = 0; frame < 200 && !sender.IsCompleted; frame++)
        {
            DisunityHost.Step(0.02f);
            if (!sender.IsCompleted) Thread.Sleep(5);
        }
        Require(sender.Wait(TimeSpan.FromSeconds(5)) && ranOn == main, "Send from another thread runs on the main thread during a frame");
    }

    private static void ResumesAwaitsOnTheMainThread()
    {
        var main = Environment.CurrentManagedThreadId;
        var resumed = 0;
        async Task Continue()
        {
            await Task.Yield();
            resumed = Environment.CurrentManagedThreadId;
        }
        var task = Continue();
        Require(!task.IsCompleted, "the continuation waits for a frame");
        DisunityHost.Step(0.02f);
        Require(task.IsCompleted && resumed == main, "an await resumes on the main thread in the next frame");
    }

    private static void LogsFailuresAndRunsTheRestInOrder(SynchronizationContext context)
    {
        var calls = new List<string>();
        context.Post(_ => calls.Add("first"), null);
        context.Post(_ => throw new InvalidOperationException("posted work failed on purpose"), null);
        context.CreateCopy().Post(_ => calls.Add("copy"), null);
        DisunityHost.Step(0.02f);
        Require(calls.SequenceEqual(["first", "copy"]), "work after a failure runs, in posted order, and copies share the queue");
        context.Post(_ => calls.Add("next frame"), null);
        DisunityHost.Step(0.02f);
        Require(calls[^1] == "next frame", "a failure is logged and the world keeps running");
    }

    private static void RunsWorkWithItselfCurrentAndRestoresTheThread(SynchronizationContext context)
    {
        SynchronizationContext? during = null;
        context.Post(_ => during = SynchronizationContext.Current, null);
        SynchronizationContext.SetSynchronizationContext(null);
        try
        {
            DisunityHost.Step(0.02f);
            Require(ReferenceEquals(during, context) && SynchronizationContext.Current is null,
                "work runs with the context current, and the thread's own context comes back afterwards");
        }
        finally { SynchronizationContext.SetSynchronizationContext(context); }
    }

    private static void UnloadingReleasesWaitingSenders()
    {
        DisunityHost.LoadWorld();
        var context = SynchronizationContext.Current!;
        Task sender;
        try
        {
            sender = Task.Run(() => context.Send(_ => throw new InvalidOperationException("ran after unload"), null));
            Thread.Sleep(50);
        }
        finally { DisunityHost.UnloadWorld(); }
        try
        {
            Require(sender.Wait(TimeSpan.FromSeconds(5)), "a sender waiting when the world unloads is released");
            throw new InvalidOperationException("synchronization context: the unloaded sender's work ran");
        }
        catch (AggregateException error) when (error.InnerException is ObjectDisposedException) { }
        context.Post(_ => throw new InvalidOperationException("posted after unload"), null);
    }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException($"synchronization context: {message}");
    }
}
