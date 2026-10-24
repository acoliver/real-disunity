using System.Collections;
using CompatibilityBridge;
using UnityEngine;

internal static class ReadinessWaitBehavior
{
    internal static void Run(string path, bool nestedSession = false)
    {
        using var world = new ReadinessWorld(path, nestedSession: nestedSession);
        var modes = new[] { 0, 1, 2, 3, 4 };
        var manifest = world.Manifest(modes.Select(mode => new ScriptSpec("wait" + mode,
            new GameObject("wait" + mode), nameof(ReadinessWaitProbe), new { mode })).ToArray());
        BinderBehavior.Check(world.Load(manifest) == 0, "wait profile load: " + BridgeBehavior.LastError());
        var session = (ScriptSession)NativeIdentity.Runtime!;
        var probes = modes.Select(mode => session.Resolve("wait" + mode)).ToArray();
        int Value(int mode, string field) => (int)probes[mode].GetType().GetField(field)!.GetValue(probes[mode])!;
        world.Frame();
        BinderBehavior.Check(modes.All(m => Value(m, "currents") == 1), "each initial Current evaluated exactly once");
        BinderBehavior.Check(Value(4, "disposals") == 1, "EndOfFrame resumes in its native same-frame lane");
        world.Control("session.configure", ("time_scale", new { num = 0, den = 1 })).Dispose();
        world.Frame(1, 10);
        BinderBehavior.Check(Value(0, "disposals") == 1 && Value(3, "moves") == 1, "next-frame progresses with no admitted fixed tick");
        BinderBehavior.Check(Value(1, "moves") == 1 && Value(2, "moves") == 1, "exact 0.1f exceeds rational 1/10, scaled pause retained");
        world.Frame(1, 100000000);
        BinderBehavior.Check(Value(2, "disposals") == 1 && Value(1, "moves") == 1, "native unscaled wait progresses while scaled paused");
        world.Control("session.configure", ("time_scale", new { num = 1, den = 1 })).Dispose();
        world.Frame(1, 10);
        BinderBehavior.Check(Value(1, "moves") == 1 && Value(3, "disposals") == 1, "fixed wait requires a native admitted tick; scaled float remains exact");
        world.Frame(1, 100000000);
        BinderBehavior.Check(Value(1, "disposals") == 1, "exact native scaled endpoint reaches exact IEEE wait");
        BinderBehavior.Check(modes.All(m => Value(m, "moves") == 2 && Value(m, "currents") == 1 && Value(m, "disposals") == 1), "single iterator has exactly two MoveNext, one Current and one Dispose");
        world.Cleanup();
        Console.WriteLine("PASS readiness exact IEEE scaled/unscaled waits, pause, no-tick, native fixed/EndOfFrame lanes, Current once and disposal");
    }
    internal static void Unsupported(string path)
    {
        using var world = new ReadinessWorld(path);
        var manifest = world.Manifest(new[] { 5, 6, 7, 8 }.Select(mode => new ScriptSpec("bad" + mode,
            new GameObject("bad" + mode), nameof(ReadinessWaitProbe), new { mode })).ToArray());
        BinderBehavior.Check(world.Load(manifest) == 0, "unsupported fixture load");
        world.Frame();
        var session = (ScriptSession)NativeIdentity.Runtime!;
        foreach (var mode in new[] { 5, 6, 7, 8 })
        {
            var probe = session.Resolve("bad" + mode);
            BinderBehavior.Check((int)probe.GetType().GetField("disposals")!.GetValue(probe)! == 1, "unsupported yield disposed under native permission");
        }
        BinderBehavior.Check(session.CallbackFailureCount == 4, "tiny exact ratio, nested/custom yields and immediate nested start each record failure");
        var errors = BridgeBehavior.Observe("""{"op":"managed.failures"}""");
        BinderBehavior.Check(errors.Contains("NotSupportedException") && errors.Contains("exact native ratio bounds") && errors.Contains("synchronously StartCoroutine"), "unsupported contracts have explicit diagnostics");
        world.Frame();
        BinderBehavior.Check(session.CallbackFailureCount == 4, "unsupported operations do not retry or quietly queue work");
        world.Cleanup();
        Console.WriteLine("PASS readiness unsupported exact-ratio/nested/custom/immediate-start contexts fail closed with isolated disposal");
    }
    internal static void IteratorFailures(string path)
    {
        using var world = new ReadinessWorld(path);
        var manifest = world.Manifest(new[] { 9, 10, 11 }.Select(mode => new ScriptSpec("failure" + mode,
            new GameObject("failure" + mode), nameof(ReadinessWaitProbe), new { mode })).ToArray());
        BinderBehavior.Check(world.Load(manifest) == 0, "iterator failure fixture load");
        var session = (ScriptSession)NativeIdentity.Runtime!;
        world.Frame(); world.Frame();
        BinderBehavior.Check(session.CallbackFailureCount == 3 && session.CoroutineCount == 0, "MoveNext/Current/Dispose failures each complete native permission and drain");
        var errors = BridgeBehavior.Observe("""{"op":"managed.failures"}""");
        foreach (var name in new[] { "MoveNext", "Current", "Dispose" })
            BinderBehavior.Check(errors.Contains("\"callback\":\"" + name + "\""), "failure preserves iterator operation " + name);
        world.Cleanup();
        Console.WriteLine("PASS readiness throwing MoveNext/Current/Dispose reports each outcome and drains native routines");
    }

}
public sealed class ReadinessWaitProbe : MonoBehaviour
{
    public int mode, moves, currents, disposals;
    private IEnumerator Start() => new CountingIterator(this);
    private sealed class CountingIterator(ReadinessWaitProbe owner) : IEnumerator, IDisposable
    {
        public bool MoveNext()
        {
            owner.moves++;
            if (owner.mode == 10) throw new ArgumentException("readiness MoveNext sentinel");
            if (owner.mode == 8) owner.StartCoroutine(UnsupportedChild());
            return owner.moves == 1;
        }
        public object? Current
        {
            get
            {
                owner.currents++;
                if (owner.mode == 9) throw new ArgumentException("readiness Current sentinel");
                return owner.mode switch
                {
                    0 => null, 1 => new WaitForSeconds(0.1f), 2 => new WaitForSecondsRealtime(0.1f),
                    3 => new WaitForFixedUpdate(), 4 => new WaitForEndOfFrame(),
                    5 => new WaitForSeconds(float.Epsilon), 6 => UnsupportedChild(), 7 => new object(), _ => null
                };
            }
        }
        public void Reset() => throw new NotSupportedException();
        public void Dispose()
        {
            owner.disposals++;
            if (owner.mode == 11) throw new ArgumentException("readiness explicit Dispose sentinel");
        }
    }
    private static IEnumerator UnsupportedChild() { yield return null; }
}
