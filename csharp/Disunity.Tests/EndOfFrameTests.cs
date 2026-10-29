using System.Collections;
using Disunity.Hosting;
using UnityEngine;

/// <summary>A coroutine that yields WaitForEndOfFrame resumes once, after every LateUpdate of the frame.</summary>
internal static class EndOfFrameTests
{
    public static void Run()
    {
        ResumesAfterLateUpdateOfTheSameFrame();
        ResumesOncePerYield();
        Console.WriteLine("PASS: WaitForEndOfFrame resumes a coroutine after LateUpdate, once per yield");
    }

    private sealed class Probe : MonoBehaviour
    {
        public readonly List<string> Log = new();
        public readonly List<int> Frames = new();
        public int Yields = 1;
        private bool _started;

        private void Update()
        {
            Log.Add("update");
            if (_started) return;
            _started = true;
            StartCoroutine(Wait());
        }

        private void LateUpdate() => Log.Add("late");

        private IEnumerator Wait()
        {
            Log.Add("begin");
            Frames.Add(Time.frameCount);
            for (var i = 0; i < Yields; i++)
            {
                yield return new WaitForEndOfFrame();
                Log.Add("end");
                Frames.Add(Time.frameCount);
            }
        }
    }

    private static Probe Start()
    {
        DisunityHost.LoadWorld();
        return DisunityHost.AddComponent(new Probe());
    }

    private static void ResumesAfterLateUpdateOfTheSameFrame()
    {
        var probe = Start();
        try
        {
            DisunityHost.Step(0.01f);
            Expect.Sequence(new[] { "update", "begin", "late", "end" }, probe.Log, "the coroutine resumes after LateUpdate");
            Expect.Equal(probe.Frames[0], probe.Frames[1], "the coroutine resumes in the frame it yielded in");
            DisunityHost.Step(0.01f);
            Expect.Sequence(new[] { "update", "begin", "late", "end", "update", "late" }, probe.Log, "the finished coroutine does not run again");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void ResumesOncePerYield()
    {
        DisunityHost.LoadWorld();
        var probe = new Probe { Yields = 2 };
        DisunityHost.AddComponent(probe);
        try
        {
            DisunityHost.Step(0.01f);
            Expect.Sequence(new[] { "update", "begin", "late", "end" }, probe.Log, "the first yield resumes once");
            DisunityHost.Step(0.01f);
            Expect.Sequence(new[] { "update", "begin", "late", "end", "update", "late", "end" }, probe.Log, "a yield made at the end of a frame resumes at the end of the next one");
            Expect.Equal(probe.Frames[1] + 1, probe.Frames[2], "the second resume is one frame later");
        }
        finally { DisunityHost.UnloadWorld(); }
    }
}
