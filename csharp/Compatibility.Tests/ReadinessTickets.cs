using System.Reflection;
using System.Text.Json;
using UnityEngine;

internal static class ReadinessTickets
{
    internal static void Run(string path)
    {
        using var world = new ReadinessWorld(path);
        var script = new GameObject("linear-tickets").AddComponent<ReadinessTicketProbe>();
        var context = world.Open("Initialize");
        Reject(() => Call(context, "register", ("component", script.NativeHandle), ("callbacks", new[] { "Awake", "Update" })), "unclaimed registration");
        Reject(() => Call(context, "next"), "unclaimed selection");
        Claim(context, "Initialize");
        Call(context, "register", ("component", script.NativeHandle), ("callbacks", new[] { "Awake", "Update" })).Dispose();
        Call(context, "register", ("component", script.NativeHandle), ("callbacks", new[] { "Awake", "Update" })).Dispose();
        Reject(() => Call(context, "register", ("component", script.NativeHandle), ("callbacks", new[] { "Awake" })), "immutable callback mask");
        using var next = Call(context, "next");
        var ticket = NativeWorld.Data(next).GetProperty("ticket").GetUInt64();
        Reject(() => Call(context, "enter", ("ticket", ticket + 1)), "wrong ticket");
        BinderBehavior.Check(Task.Run(() => Rejected(context, ticket)).GetAwaiter().GetResult(), "wrong-thread native ticket entry rejects");
        BinderBehavior.Check(script.attempts == 0, "no callback before successful native entry");
        Execute(context, ticket, script, "Awake");
        Reject(() => Call(context, "enter", ("ticket", ticket)), "duplicate entry");
        Call(context, "complete", ("ticket", ticket), ("outcome", "ok")).Dispose();
        Call(context, "complete", ("ticket", ticket), ("outcome", "ok")).Dispose();
        Reject(() => Call(context, "complete", ("ticket", ticket), ("outcome", "failed")), "conflicting completion");
        Call(context, "next").Dispose();
        world.Finish(context);
        world.Control("frame.open", ("dt", new { num = 0, den = 1 })).Dispose();
        var update = world.Open("Update");
        Reject(() => Call(update, "enter", ("ticket", ticket)), "unclaimed/stale ticket");
        Claim(update, "Update");
        Reject(() => Call(context, "enter", ("ticket", ticket)), "revoked context");
        using var selected = Call(update, "next");
        var pending = NativeWorld.Data(selected).GetProperty("ticket").GetUInt64();
        NativeWorld.Request("component.set", ("component", script.NativeHandle), ("enabled", false)).Dispose();
        using var entered = Call(update, "enter", ("ticket", pending));
        BinderBehavior.Check(!NativeWorld.Data(entered).GetProperty("execute").GetBoolean() && script.attempts == 1, "entry revalidates native eligibility and skips mutated peer");
        Call(update, "next").Dispose(); world.Finish(update);
        foreach (var phase in new[] { "UpdateCoroutines", "LateUpdate", "EndOfFrame" }) EmptyPhase(world, phase);
        world.Control("frame.end", ("frame", 1UL)).Dispose();
        EmptyPhase(world, "Cleanup");
        NativeIdentity.Reset();
        Console.WriteLine("PASS readiness actual Cdecl protocol tickets: unclaimed/wrong/stale/duplicate/wrong-thread entry, immutable metadata, completion replay and execute:false mutation gate");
    }
    private static void Execute(JsonElement context, ulong ticket, ReadinessTicketProbe script, string callback)
    {
        using var entered = Call(context, "enter", ("ticket", ticket));
        if (NativeWorld.Data(entered).GetProperty("execute").GetBoolean())
            script.GetType().GetMethod(callback, BindingFlags.Instance | BindingFlags.NonPublic)!.Invoke(script, null);
    }
    private static bool Rejected(JsonElement context, ulong ticket)
    {
        try { Call(context, "enter", ("ticket", ticket)).Dispose(); return false; }
        catch (NativeWorldException) { return true; }
    }
    private static void EmptyPhase(ReadinessWorld world, string phase)
    {
        var context = world.Open(phase); Claim(context, phase); Call(context, "next").Dispose(); world.Finish(context);
    }
    private static void Claim(JsonElement context, string phase) => NativeWorld.Request("phase.claim", ("phase", phase), ("context", context)).Dispose();
    private static JsonDocument Call(JsonElement context, string operation, params (string key, object? value)[] fields) =>
        NativeWorld.Request("readiness." + operation, fields.Append(("context", (object?)context)).ToArray());
    private static void Reject(Func<JsonDocument> request, string reason)
    {
        try { using var result = request(); throw new Exception("accepted " + reason); }
        catch (NativeWorldException) { }
    }
}
public sealed class ReadinessTicketProbe : MonoBehaviour
{
    public int attempts;
    private void Awake() => attempts++;
    private void Update() => attempts++;
}
