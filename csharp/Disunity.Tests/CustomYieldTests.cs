using System.Collections;
using Disunity.Hosting;
using UnityEngine;

/// <summary>A coroutine that yields a <see cref="CustomYieldInstruction"/> stays suspended while
/// <c>keepWaiting</c> is true and resumes in the frame it turns false.</summary>
internal static class CustomYieldTests
{
    public static void Run()
    {
        ActsAsAnEnumeratorOverKeepWaiting();
        ResumesInTheFrameKeepWaitingTurnsFalse();
        ResumesAtOnceWhenNothingIsWaitedFor();
        StopsCheckingAfterTheCoroutineIsStopped();
        Console.WriteLine("PASS: CustomYieldInstruction suspends a coroutine until keepWaiting turns false");
    }

    private sealed class UntilFrame(Probe probe, int frame) : CustomYieldInstruction
    {
        public int Checks;

        public override bool keepWaiting
        {
            get
            {
                Checks++;
                return probe.Frames < frame;
            }
        }
    }

    private sealed class Probe : MonoBehaviour
    {
        public int Frames;
        public readonly List<string> Calls = new();
        private void Update() => Frames++;

        public IEnumerator Wait(CustomYieldInstruction instruction)
        {
            Calls.Add("before");
            yield return instruction;
            Calls.Add($"after at frame {Frames}");
        }
    }

    private sealed class Flag(bool waiting) : CustomYieldInstruction
    {
        public override bool keepWaiting => waiting;
    }

    private static void ActsAsAnEnumeratorOverKeepWaiting()
    {
        var waiting = new Flag(true);
        var done = new Flag(false);
        Expect.That(waiting.MoveNext() && waiting.Current is null, "a waiting instruction advances and yields nothing");
        Expect.That(!done.MoveNext(), "an instruction that is not waiting is finished");
        waiting.Reset();
        Expect.That(waiting.MoveNext(), "Reset leaves the wait as it was");
    }

    private static void ResumesInTheFrameKeepWaitingTurnsFalse()
    {
        DisunityHost.LoadWorld();
        var probe = DisunityHost.AddComponent(new Probe());
        var instruction = new UntilFrame(probe, 3);
        probe.StartCoroutine(probe.Wait(instruction));
        Expect.Sequence(new[] { "before" }, probe.Calls, "the coroutine runs to its first yield");
        DisunityHost.Step(0.01f);
        DisunityHost.Step(0.01f);
        Expect.Sequence(new[] { "before" }, probe.Calls, "the coroutine is suspended while keepWaiting is true");
        var checksBefore = instruction.Checks;
        DisunityHost.Step(0.01f);
        Expect.Sequence(new[] { "before", "after at frame 3" }, probe.Calls, "the coroutine resumes in the frame keepWaiting turns false");
        DisunityHost.Step(0.01f);
        DisunityHost.Step(0.01f);
        Expect.Sequence(new[] { "before", "after at frame 3" }, probe.Calls, "the coroutine does not resume twice");
        Expect.Equal(checksBefore + 1, instruction.Checks, "keepWaiting is read once a frame and not after the coroutine resumed");
        DisunityHost.UnloadWorld();
    }

    private static void ResumesAtOnceWhenNothingIsWaitedFor()
    {
        DisunityHost.LoadWorld();
        var probe = DisunityHost.AddComponent(new Probe());
        probe.StartCoroutine(probe.Wait(new Flag(false)));
        Expect.Sequence(new[] { "before", "after at frame 0" }, probe.Calls, "an instruction that is not waiting does not suspend the coroutine");
        DisunityHost.UnloadWorld();
    }

    private static void StopsCheckingAfterTheCoroutineIsStopped()
    {
        DisunityHost.LoadWorld();
        var probe = DisunityHost.AddComponent(new Probe());
        var instruction = new UntilFrame(probe, 2);
        var routine = probe.StartCoroutine(probe.Wait(instruction));
        probe.StopCoroutine(routine);
        var checks = instruction.Checks;
        DisunityHost.Step(0.01f);
        DisunityHost.Step(0.01f);
        Expect.That(instruction.Checks == checks && probe.Calls.SequenceEqual(["before"]), "a stopped coroutine does not resume");
        DisunityHost.UnloadWorld();
    }
}
