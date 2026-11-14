using Disunity.Hosting;
using UnityEngine;
using UnityEngine.LowLevel;
using Phases = UnityEngine.PlayerLoop;

/// <summary>
/// The player loop: the engine's seven phases, a loop a game sets and reads back, and the delegates it adds running once a
/// frame at their phase around the scripts' own messages.
/// </summary>
internal static class PlayerLoopTests
{
    private sealed class Probe : MonoBehaviour
    {
        public static List<string>? Calls;
        private void Update() => Calls?.Add("update");
        private void LateUpdate() => Calls?.Add("late");
    }

    private struct Added { }

    private static readonly Type[] Order =
    [
        typeof(Phases.Initialization), typeof(Phases.EarlyUpdate), typeof(Phases.FixedUpdate), typeof(Phases.PreUpdate),
        typeof(Phases.Update), typeof(Phases.PreLateUpdate), typeof(Phases.PostLateUpdate),
    ];

    public static void Run()
    {
        DefaultLoopIsTheSevenPhases();
        SetLoopIsReadBackAsACopy();
        LoopsTheEngineCannotRunAreRefused();
        DisunityHost.LoadWorld();
        try
        {
            AddedDelegatesRunAtTheirPhase();
            AThrowingDelegateDoesNotStopTheOthers();
        }
        finally { DisunityHost.UnloadWorld(); }
        Expect.That(PlayerLoop.GetCurrentPlayerLoop().subSystemList!.All(phase => phase.subSystemList is null), "unloading the world restores the default loop");
        Console.WriteLine("PASS: the player loop holds the seven phases, a set loop reads back, and added delegates run once a frame at their phase");
    }

    private static void DefaultLoopIsTheSevenPhases()
    {
        var loop = PlayerLoop.GetDefaultPlayerLoop();
        Expect.That(loop.type is null && loop.updateDelegate is null, "the root has no type and no delegate");
        Expect.That(loop.subSystemList!.Select(phase => phase.type).SequenceEqual(Order), "the root holds the seven phases in order");
        Expect.That(PlayerLoop.GetCurrentPlayerLoop().subSystemList!.Select(phase => phase.type).SequenceEqual(Order), "the current loop starts as the default");
    }

    private static void SetLoopIsReadBackAsACopy()
    {
        var loop = WithAdded(typeof(Phases.Update), () => { });
        PlayerLoop.SetPlayerLoop(loop);
        loop.subSystemList![4].subSystemList![0].type = typeof(Probe);
        var current = PlayerLoop.GetCurrentPlayerLoop();
        Expect.That(current.subSystemList![4].subSystemList![0].type == typeof(Added), "the loop that was set reads back, unchanged by later edits to its arrays");
        current.subSystemList[4].subSystemList = null;
        Expect.That(PlayerLoop.GetCurrentPlayerLoop().subSystemList![4].subSystemList!.Length == 1, "editing what was read changes nothing until it is set");
        PlayerLoop.SetPlayerLoop(PlayerLoop.GetDefaultPlayerLoop());
    }

    private static void LoopsTheEngineCannotRunAreRefused()
    {
        var fixedStep = WithAdded(typeof(Phases.FixedUpdate), () => { });
        Refused(fixedStep, "FixedUpdate", "a delegate under FixedUpdate");
        var dropped = PlayerLoop.GetDefaultPlayerLoop();
        dropped.subSystemList = dropped.subSystemList![1..];
        Refused(dropped, "seven phases", "a loop without Initialization");
        var moved = PlayerLoop.GetDefaultPlayerLoop();
        (moved.subSystemList![0], moved.subSystemList[1]) = (moved.subSystemList[1], moved.subSystemList[0]);
        Refused(moved, "seven phases", "a loop with two phases swapped");
        var afterFixed = PlayerLoop.GetDefaultPlayerLoop();
        afterFixed.subSystemList = [.. afterFixed.subSystemList![..3], new() { type = typeof(Added), updateDelegate = () => { } }, .. afterFixed.subSystemList[3..]];
        Refused(afterFixed, "FixedUpdate", "a top-level system between FixedUpdate and PreUpdate");
        var twice = PlayerLoop.GetDefaultPlayerLoop();
        twice.subSystemList = [.. twice.subSystemList!, new() { type = typeof(Phases.Update) }];
        Refused(twice, "seven phases", "a phase listed twice");
        var rooted = PlayerLoop.GetDefaultPlayerLoop();
        rooted.updateDelegate = () => { };
        Refused(rooted, "root", "a delegate on the root");
        Expect.That(PlayerLoop.GetCurrentPlayerLoop().subSystemList!.All(phase => phase.subSystemList is null), "a refused loop leaves the current one");
    }

    private static void AddedDelegatesRunAtTheirPhase()
    {
        var calls = new List<string>();
        Probe.Calls = calls;
        var probe = DisunityHost.AddComponent(new Probe());
        var loop = PlayerLoop.GetDefaultPlayerLoop();
        foreach (var phase in Order.Where(phase => phase != typeof(Phases.FixedUpdate)))
            loop.subSystemList![Array.IndexOf(Order, phase)].subSystemList = [new() { type = typeof(Added), updateDelegate = () => calls.Add(phase.Name) }];
        loop.subSystemList![4].subSystemList![0].subSystemList = [new() { type = typeof(Added), updateDelegate = () => calls.Add("Update child") }];
        // Systems a package puts at the top level run with the phase before them, or first when no phase is before them.
        PlayerLoopSystem TopLevel(string name) => new() { type = typeof(Added), updateDelegate = () => calls.Add(name) };
        loop.subSystemList = [TopLevel("leading"), .. loop.subSystemList![..5], TopLevel("after Update"), .. loop.subSystemList[5..], TopLevel("last")];
        PlayerLoop.SetPlayerLoop(loop);
        DisunityHost.Step(0.02f);
        string[] frame = ["leading", "Initialization", "EarlyUpdate", "PreUpdate", "update", "Update", "Update child", "after Update", "late", "PreLateUpdate", "PostLateUpdate", "last"];
        Expect.Sequence(frame, calls, "a frame");
        calls.Clear();
        DisunityHost.Step(0.02f);
        Expect.Sequence(frame, calls, "the next frame");
        PlayerLoop.SetPlayerLoop(PlayerLoop.GetDefaultPlayerLoop());
        calls.Clear();
        DisunityHost.Step(0.02f);
        Expect.Sequence(["update", "late"], calls, "a frame after the default loop is set again");
        Probe.Calls = null;
        UnityEngine.Object.Destroy(probe.gameObject);
        DisunityHost.Step(0.02f);
    }

    private static void AThrowingDelegateDoesNotStopTheOthers()
    {
        var calls = new List<string>();
        var loop = PlayerLoop.GetDefaultPlayerLoop();
        loop.subSystemList![4].subSystemList =
        [
            new() { type = typeof(Added), updateDelegate = () => throw new InvalidOperationException("loop delegate failed") },
            new() { type = typeof(Added), updateDelegate = () => calls.Add("after") },
        ];
        PlayerLoop.SetPlayerLoop(loop);
        var logged = new List<string>();
        void Capture(string message, string stack, LogType type) { if (type == LogType.Exception) logged.Add(message); }
        Application.logMessageReceived += Capture;
        try { DisunityHost.Step(0.02f); }
        finally { Application.logMessageReceived -= Capture; }
        Expect.Sequence(["after"], calls, "the next delegate");
        Expect.That(logged.Any(message => message.Contains("loop delegate failed")), $"the failure is logged as an exception: {string.Join(" | ", logged)}");
        PlayerLoop.SetPlayerLoop(PlayerLoop.GetDefaultPlayerLoop());
    }

    private static PlayerLoopSystem WithAdded(Type phase, PlayerLoopSystem.UpdateFunction function)
    {
        var loop = PlayerLoop.GetDefaultPlayerLoop();
        loop.subSystemList![Array.IndexOf(Order, phase)].subSystemList = [new() { type = typeof(Added), updateDelegate = function }];
        return loop;
    }

    private static void Refused(PlayerLoopSystem loop, string text, string what)
    {
        try
        {
            PlayerLoop.SetPlayerLoop(loop);
        }
        catch (NotSupportedException error) when (error.Message.Contains("PlayerLoop.SetPlayerLoop") && error.Message.Contains(text))
        {
            return;
        }
        throw new InvalidOperationException($"{what}: SetPlayerLoop did not throw NotSupportedException naming {text}");
    }
}
