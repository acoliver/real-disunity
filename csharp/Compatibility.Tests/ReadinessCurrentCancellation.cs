using System.Collections;
using CompatibilityBridge;
using UnityEngine;

internal static class ReadinessCurrentCancellation
{
    internal static void Run(string path)
    {
        using var world = new ReadinessWorld(path);
        var manifest = world.Manifest([new("probe", new GameObject("current-cancellation"), nameof(ReadinessCurrentProbe), new { })]);
        BinderBehavior.Check(world.Load(manifest) == 0, "Current cancellation fixture load");
        var session = (ScriptSession)NativeIdentity.Runtime!;
        var probe = session.Resolve("probe");
        world.Frame();
        BinderBehavior.Check(session.CallbackFailureCount == 0, "native cancellation during Current must not become a false application failure");
        BinderBehavior.Check((int)probe.GetType().GetField("disposals")!.GetValue(probe)! == 1, "Current cancellation receives one native Dispose attempt");
        world.Cleanup();
        Console.WriteLine("PASS readiness cancellation latched inside Current skips suspension and disposes without a false application failure");
    }
}
public sealed class ReadinessCurrentProbe : MonoBehaviour
{
    public int disposals;
    private IEnumerator Start() => new Iterator(this);
    private sealed class Iterator(ReadinessCurrentProbe owner) : IEnumerator, IDisposable
    {
        public object? Current { get { owner.gameObject.SetActive(false); return null; } }
        public bool MoveNext() => true;
        public void Reset() => throw new NotSupportedException();
        public void Dispose() => owner.disposals++;
    }
}
