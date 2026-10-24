using System.Collections;
using CompatibilityBridge;
using UnityEngine;

internal static class ReadinessContactBehavior
{
    internal static void Run(string path)
    {
        using var world = new ReadinessWorld(path);
        var owners = Enumerable.Range(0, 3).Select(i => new GameObject("contact" + i)).ToArray();
        foreach (var owner in owners)
        {
            owner.AddComponent<BoxCollider2D>().isTrigger = true;
            owner.AddComponent<Rigidbody2D>().gravityScale = 0;
        }
        var manifest = world.Manifest(owners.Select((owner, mode) => new ScriptSpec("probe" + mode,
            owner, nameof(ReadinessContactProbe), new { mode })).ToArray());
        BinderBehavior.Check(world.Load(manifest) == 0, "contact readiness load");
        var session = (ScriptSession)NativeIdentity.Runtime!;
        var probes = Enumerable.Range(0, 3).Select(i => session.Resolve("probe" + i)).ToArray();
        int Value(int mode, string field) => (int)probes[mode].GetType().GetField(field)!.GetValue(probes[mode])!;
        world.Frame(1, 50);
        BinderBehavior.Check(Value(0, "moves") == 1 && Value(0, "joins") == 0, "contact starts execute initial iterator synchronously without entered ticket");
        BinderBehavior.Check(Value(1, "disposals") == 1 && Value(1, "moves") == 1, "inactive/reactivated owner cancels and cannot revive routine");
        BinderBehavior.Check(Value(2, "disposals") == 1 && session.CallbackFailureCount == 1, "explicit cancellation attempts throwing Dispose once and isolates failure");
        world.Frame();
        BinderBehavior.Check(Value(0, "moves") == 2 && Value(0, "joins") == 1 && Value(0, "disposals") == 1, "disabled owner retains routine and join resumes only after disposal");
        var log = probes[0].GetType().GetField("log")!.GetValue(probes[0]);
        BinderBehavior.Check((string)log! == "DJ", "join observes target finally before continuing");
        BinderBehavior.Check(session.CoroutineCount == 0, "joiners drain and terminal records release");
        world.Cleanup();
        Console.WriteLine("PASS readiness real Contacts immediate outside-ticket start, native join/disposal ordering, disabled retention, inactivity latch and cancellation failure isolation");
    }
}
public sealed class ReadinessContactProbe : MonoBehaviour
{
    public int mode, moves, disposals, joins;
    public string log = "";
    private bool _contact;
    private void OnTriggerEnter2D(Collider2D other)
    {
        if (_contact) return;
        _contact = true;
        var target = StartCoroutine(Target());
        if (mode == 0) { StartCoroutine(Join(target)); enabled = false; }
        if (mode == 1)
        {
            NativeWorld.Request("object.set", ("object", gameObject.NativeHandle), ("active", false)).Dispose();
            NativeWorld.Request("object.set", ("object", gameObject.NativeHandle), ("active", true)).Dispose();
        }
        if (mode == 2) StopCoroutine(target);
    }
    private IEnumerator Target()
    {
        try { moves++; yield return null; moves++; }
        finally
        {
            disposals++; log += "D";
            if (mode == 2) throw new ArgumentException("readiness Dispose sentinel");
        }
    }
    private IEnumerator Join(Coroutine target) { yield return target; joins++; log += "J"; }
}
