using System.Collections;
using CompatibilityBridge;
using UnityEngine;

internal static class ReadinessStartAdmission
{
    internal static void Run(string path)
    {
        using var world = new ReadinessWorld(path);
        var manifest = world.Manifest([
            new("factory", new GameObject("inactive-factory"), nameof(ReadinessInactiveFactory), new { }),
            new("peer", new GameObject("healthy-peer"), nameof(ReadinessProbe), new { })]);
        BinderBehavior.Check(world.Load(manifest) == 0, "Start factory fixture load");
        var session = (ScriptSession)NativeIdentity.Runtime!;
        world.Frame();
        BinderBehavior.Check(session.CallbackFailureCount == 1, "inactive IEnumerator Start factory is an explicit application failure");
        var peer = session.Resolve("peer");
        BinderBehavior.Check((int)peer.GetType().GetField("updates")!.GetValue(peer)! == 1, "unsupported Start factory does not abort peers or root");
        world.Cleanup();
        Console.WriteLine("PASS readiness inactive Start factory fails explicitly before native routine admission and preserves peer continuation");
    }
}
public sealed class ReadinessInactiveFactory : MonoBehaviour
{
    private IEnumerator Start() { gameObject.SetActive(false); return Child(); }
    private static IEnumerator Child() { yield return null; }
}
