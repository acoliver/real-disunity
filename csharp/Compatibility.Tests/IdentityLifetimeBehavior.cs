using System.Runtime.CompilerServices;
using CompatibilityBridge;
using UnityEngine;
using UObject = UnityEngine.Object;

internal static class IdentityLifetimeBehavior
{
    public static void Run()
    {
        var actor = new GameObject("identity-root");
        using var payload = TestGraph.Payload([new("probe", actor, "BridgeProbe", new { })]);
        using var session = new ScriptSession(payload.RootElement); session.Initialize();
        var references = CloneAndDestroy(actor);
        GC.Collect(); GC.WaitForPendingFinalizers(); GC.Collect();
        BinderBehavior.Check(!references.Object.IsAlive && !references.Script.IsAlive, "destroyed native clone must release cached managed wrapper/script roots");
        var stale = actor.transform;
        var oldHandle = actor.NativeHandle;
        UObject.Destroy(actor);
        var replacement = new GameObject("replacement");
        BinderBehavior.Check(replacement.NativeHandle != oldHandle && stale == null, "native slot reuse never revives stale generation");
        Console.WriteLine("PASS identity lifetime: destroyed clone roots collect and native generation reuse preserves Unity null");
    }
    [MethodImpl(MethodImplOptions.NoInlining)]
    private static (WeakReference Object, WeakReference Script) CloneAndDestroy(GameObject original)
    {
        var clone = UObject.Instantiate(original);
        var script = clone.GetComponent<MonoBehaviour>()!;
        var result = (new WeakReference(clone), new WeakReference(script));
        UObject.Destroy(clone);
        return result;
    }
}
