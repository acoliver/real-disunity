using System.Runtime.CompilerServices;
using Disunity.Hosting;
using UnityEngine;

internal static class ReviewRetentionTests
{
    private sealed class Payload
    {
        public byte[] Data = new byte[4096];
    }
    private sealed class Retained : MonoBehaviour
    {
        public Payload Serialized = new();
        public Rigidbody2D? Body;
        public static int Destroyed;
        private void OnDestroy() => Destroyed++;
    }
    public static void Run()
    {
        DisunityHost.LoadWorld();
        var world = DisunityHost.WorldHandle;
        Retained.Destroyed = 0;
        for (var cycle = 0; cycle < 32; cycle++)
        {
            var weak = SpawnAndRetire(cycle % 2 == 0);
            GC.Collect();
            GC.WaitForPendingFinalizers();
            GC.Collect();
            if (weak.Any(reference => reference.IsAlive)) throw new InvalidOperationException("B2 retired wrappers or serialized payload retained in loaded world");
            if (DisunityHost.WorldHandle != world || !DisunityHost.IsLoaded) throw new InvalidOperationException("B2 collectibility requires the same loaded world");
        }
        if (Retained.Destroyed != 32) throw new InvalidOperationException("B2 native script cleanup did not complete");
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: B2 repeated in-world wrapper and serialized payload collectibility including inactive scriptless objects");
    }
    [MethodImpl(MethodImplOptions.NoInlining)]
    private static WeakReference[] SpawnAndRetire(bool inactive)
    {
        var script = DisunityHost.AddComponent(new Retained());
        var owner = script.gameObject;
        script.Body = owner.AddComponent<Rigidbody2D>();
        var other = DisunityHost.CreateGameObject();
        other.transform.SetParent(owner.transform, false);
        var collider = other.AddComponent<BoxCollider2D>();
        if (inactive) other.SetActive(false);
        var weak = new[] {
            new WeakReference(owner), new WeakReference(owner.transform),
            new WeakReference(script), new WeakReference(script.Body),
            new WeakReference(script.Serialized), new WeakReference(script.Serialized.Data),
            new WeakReference(other), new WeakReference(other.transform), new WeakReference(collider)
        };
        UnityEngine.Object.Destroy(owner);
        DisunityHost.Step(0);
        return weak;
    }
}
