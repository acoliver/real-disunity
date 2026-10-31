using System.Collections;
using Disunity.Hosting;
using UnityEngine;

/// <summary>StopCoroutine(IEnumerator) stops the coroutine that was started from that enumerator.</summary>
internal static class StopCoroutineTests
{
    private sealed class Probe : MonoBehaviour
    {
        public readonly List<string> Calls = new();

        public IEnumerator Run(string name)
        {
            try
            {
                while (true)
                {
                    Calls.Add(name);
                    yield return null;
                }
            }
            finally { Calls.Add(name + ":end"); }
        }
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            StopsTheRightOne();
            Console.WriteLine("PASS: StopCoroutine(IEnumerator) stops the coroutine started from that enumerator and leaves the others running");
            UnknownIsNoOp();
            Console.WriteLine("PASS: StopCoroutine(IEnumerator) of an enumerator that is not running, or whose coroutine ended, does nothing");
            NullThrows();
            Console.WriteLine("PASS: StopCoroutine(IEnumerator) rejects null");
            OtherOwner();
            Console.WriteLine("PASS: StopCoroutine(IEnumerator) does not stop a coroutine of another script");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void StopsTheRightOne()
    {
        var probe = DisunityHost.AddComponent(new Probe());
        var a = probe.Run("a");
        var b = probe.Run("b");
        probe.StartCoroutine(a);
        probe.StartCoroutine(b);
        DisunityHost.Step(0.01f);
        probe.Calls.Clear();
        probe.StopCoroutine(a);
        Expect.Sequence(["a:end"], probe.Calls, "stopping a runs its finally block");
        probe.Calls.Clear();
        DisunityHost.Step(0.01f);
        Expect.Sequence(["b"], probe.Calls, "only b resumes");
        probe.StopCoroutine(b);
    }

    private static void UnknownIsNoOp()
    {
        var probe = DisunityHost.AddComponent(new Probe());
        var stranger = probe.Run("x");
        probe.StopCoroutine(stranger);
        Expect.That(probe.Calls.Count == 0, "an enumerator that never started does nothing");
        var a = probe.Run("a");
        probe.StartCoroutine(a);
        probe.StopCoroutine(a);
        probe.Calls.Clear();
        probe.StopCoroutine(a);
        Expect.That(probe.Calls.Count == 0, "stopping twice does nothing the second time");
    }

    private static void NullThrows()
    {
        var probe = DisunityHost.AddComponent(new Probe());
        Expect.Throws<ArgumentNullException>(() => probe.StopCoroutine((IEnumerator)null!), "a null enumerator");
    }

    private static void OtherOwner()
    {
        var owner = DisunityHost.AddComponent(new Probe());
        var other = DisunityHost.AddComponent(new Probe());
        var a = owner.Run("a");
        owner.StartCoroutine(a);
        other.StopCoroutine(a);
        owner.Calls.Clear();
        DisunityHost.Step(0.01f);
        Expect.Sequence(["a"], owner.Calls, "the owner's coroutine keeps running");
        owner.StopCoroutine(a);
    }
}
