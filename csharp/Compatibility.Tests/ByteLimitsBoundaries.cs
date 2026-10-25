using System.Text;
using System.Text.Json;
using UnityEngine;

internal static class ByteLimitsBoundaries
{
    internal static void Run(string path)
    {
        using var world = new ReadinessWorld(path, openSession: false);
        RequestBoundary();
        ReplyBoundary();
        Console.WriteLine("PASS byte consumer actual Cdecl exact1MiB request, plus-one predispatch rejection, exact8MiB reply repeated beyond16MiB cumulative, native oversize rollback");
    }

    private static void RequestBoundary()
    {
        var fields = new Dictionary<string, object?> { ["version"] = 1, ["op"] = "object.create", ["name"] = "" };
        var overhead = JsonSerializer.SerializeToUtf8Bytes(fields).Length;
        fields["name"] = new string('x', (1 << 20) - overhead);
        using var exact = JsonDocument.Parse(JsonSerializer.SerializeToUtf8Bytes(fields));
        using var accepted = NativeWorld.Current.Request(exact);
        BinderBehavior.Check(accepted.RootElement.GetProperty("ok").GetBoolean(), "exact1MiB managed request");
        fields["name"] = new string('x', (1 << 20) + 1 - overhead);
        using var tooLarge = JsonDocument.Parse(JsonSerializer.SerializeToUtf8Bytes(fields));
        var rejected = false;
        try { NativeWorld.Current.Request(tooLarge).Dispose(); }
        catch (InvalidDataException) { rejected = true; }
        BinderBehavior.Check(rejected, "1MiB+1 serialization rejects before native dispatch");
        using var snapshot = NativeWorld.Request("snapshot");
        BinderBehavior.Check(NativeWorld.Data(snapshot).GetProperty("objects").GetArrayLength() == 1, "oversize request created no object");
    }

    private static void ReplyBoundary()
    {
        using var large = NativeWorld.Request("object.create", ("name", new string('x', 600_000)));
        using var tail = NativeWorld.Request("object.create");
        var operations = Enumerable.Repeat<object>(new { op = "object.get", @object = NativeWorld.Data(large).GetProperty("handle").GetUInt64() }, 13).ToList();
        var handle = NativeWorld.Data(tail).GetProperty("handle").GetUInt64();
        operations.Add(new { op = "object.set", @object = handle, name = "" });
        using var baseRequest = JsonDocument.Parse(JsonSerializer.Serialize(new { version = 1, op = "transaction", operations }));
        using var baseResponse = NativeWorld.Current.Request(baseRequest);
        var baseSize = Encoding.UTF8.GetByteCount(baseResponse.RootElement.GetRawText());
        var name = new string('p', (8 << 20) - baseSize);
        operations[13] = new { op = "object.set", @object = handle, name };
        using var exact = JsonDocument.Parse(JsonSerializer.Serialize(new { version = 1, op = "transaction", operations }));
        for (var i = 0; i < 3; i++)
        {
            using var response = NativeWorld.Current.Request(exact);
            BinderBehavior.Check(Encoding.UTF8.GetByteCount(response.RootElement.GetRawText()) == 8 << 20, "inclusive8MiB native reply copy");
        }
        using var before = NativeWorld.Request("snapshot");
        operations[13] = new { op = "object.set", @object = handle, name = name + "x" };
        using var overflow = JsonDocument.Parse(JsonSerializer.Serialize(new { version = 1, op = "transaction", operations }));
        using var error = NativeWorld.Current.Request(overflow);
        BinderBehavior.Check(!error.RootElement.GetProperty("ok").GetBoolean(), "native refuses oversized semantic reply");
        using var after = NativeWorld.Request("snapshot");
        BinderBehavior.Check(before.RootElement.GetRawText() == after.RootElement.GetRawText(), "native oversized reply rolled back mutation");
    }
}
