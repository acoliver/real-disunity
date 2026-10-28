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
        }
        finally { DisunityHost.UnloadWorld(); }
        Require(ReferenceEquals(SynchronizationContext.Current, before), "unloading the world restores the previous context");
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

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException($"synchronization context: {message}");
    }
}
