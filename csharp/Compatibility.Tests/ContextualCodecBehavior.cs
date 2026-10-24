using System.Text;
using CompatibilityBridge;

internal static unsafe class ContextualCodecBehavior
{
    internal const string Context = """{"lease":{"world":1,"session":"s","frame":0,"fixed_tick":0,"phase":"Cleanup","sequence":1,"token":"t"},"root_phase":"Cleanup","phase":"destroy","flags":1,"time":{"numerator":0,"denominator":1},"dynamic_delta":0.0,"fixed_delta":0.02,"unscaled_delta":0.0,"callback_batch":1}""";
    internal static string Record(int i) => "{\"binding\":" + i + ",\"component\":" + i + ",\"kind\":\"OnDestroy\",\"payload\":{}}";
    internal static string Envelope(int count) => "{\"version\":1,\"context\":" + Context + ",\"callbacks\":[" + string.Join(',', Enumerable.Range(1, count).Select(Record)) + "]}";
    internal static void Run()
    {
        var valid = Envelope(4096);
        Check(valid, "no compatibility fixture is loaded");
        Check(Envelope(4097), "callback record limit");
        Check(valid.Replace("\"version\":1", "\"version\":1,\"ver\\u0073ion\":1"), "duplicate");
        Check(Last(valid, "\"payload\":{}", "\"payload\":{\"x\":0}"), "unknown");
        Check(Last(valid, "\"kind\":\"OnDestroy\"", "\"kind\":\"Update\""), "kind");
        Check(Last(valid, "\"payload\":{}", "\"payload\":{},\"payl\\u006fad\":{}"), "duplicate");
        Check(valid.Replace("\"flags\":1", "\"flags\":2"), "flags");
        foreach (var id in new[] { "0", "-1", "1e0", "1.0", "9007199254740992", "\"01\"", "\"+1\"" })
            Check(valid.Replace("\"binding\":4096", "\"binding\":" + id), "integer");
        Check(valid.Replace("\"binding\":4096", "\"binding\":\"18446744073709551615\""), "no compatibility fixture is loaded");
        Check(valid.Replace("\"component\":4096", "\"component\":1").Replace("\"binding\":4096", "\"binding\":1"), "duplicate callback");
        Check(valid.Replace("\"kind\":\"OnDestroy\"", "\"kind\":\"Update\""), "kind");
        Check(valid.Replace("\"dynamic_delta\":0.0", "\"dynamic_delta\":1e100"), "delta");
        Check(valid.Replace("\"denominator\":1", "\"denominator\":0"), "integer");
        Check(valid.Replace("\"context\":", "\"unknown\":0,\"context\":"), "unknown");
        Check(valid + "{}", "JSON");
        delegate* unmanaged[Cdecl]<byte*, nuint, int> dispatch = &ManagedBridge.DispatchPhase;
        BinderBehavior.Check(dispatch(null, (1U << 20) + 1) != 0 && BridgeBehavior.LastError().Contains("length"), "oversize rejected before pointer access");
        BinderBehavior.Check(dispatch(null, 1) != 0, "null nonempty rejected");
        BridgeBehavior.BeforeLoad();
        Console.WriteLine("PASS contextual raw Cdecl codec: 4096 recognition, 4097/last-item/schema/escaped duplicates/exact IDs/f32/byte limits; no native execution claim");
    }
    private static void Check(string json, string diagnostic)
    {
        var bytes = Encoding.UTF8.GetBytes(json);
        fixed (byte* pointer = bytes)
        {
            delegate* unmanaged[Cdecl]<byte*, nuint, int> dispatch = &ManagedBridge.DispatchPhase;
            BinderBehavior.Check(dispatch(pointer, (nuint)bytes.Length) != 0 && BridgeBehavior.LastError().Contains(diagnostic), "contextual codec expected " + diagnostic + ": " + BridgeBehavior.LastError());
        }
    }
    internal static string Last(string text, string oldValue, string newValue)
    {
        var offset = text.LastIndexOf(oldValue, StringComparison.Ordinal);
        if (offset < 0) throw new InvalidOperationException("test replacement missing");
        return text[..offset] + newValue + text[(offset + oldValue.Length)..];
    }
}
