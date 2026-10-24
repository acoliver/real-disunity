using System.Text.Json;
using System.Text.Json.Nodes;
using CompatibilityBridge;
using UnityEngine;

internal static class SerializationBehavior
{
    public static void Run()
    {
        var actor = new GameObject("serialization");
        using var payload = TestGraph.Payload([new("probe", actor, "SerializationProbe", new { oldAmount = 12, hidden = 19, excluded = 100 })]);
        var value = JsonNode.Parse(payload.RootElement.GetRawText())!;
        value["profile"] = JsonSerializer.SerializeToNode(new { id = "compat-2017.1", field_aliases = new Dictionary<string, string> { ["SerializationProbe.oldAmount"] = "amount" }, unknown_fields = "diagnose" });
        using (var configured = JsonDocument.Parse(value.ToJsonString()))
        using (var session = new ScriptSession(configured.RootElement))
        {
            session.Initialize();
            BinderBehavior.Check(actor.transform.position == new Vector3(12, 19, 3), "profile alias/private fields bind before Awake while NonSerialized retains initializer");
            BinderBehavior.Check(session.Diagnostics.Count == 1 && session.Diagnostics[0].Contains("SerializationProbe.excluded"), "stale serialization fields produce exact profile-selected diagnostic");
        }
        value["profile"] = JsonSerializer.SerializeToNode(new { id = "compat-6000.3" });
        using var strict = JsonDocument.Parse(value.ToJsonString());
        try { using var unexpected = new ScriptSession(strict.RootElement); throw new Exception("unknown fields silently accepted"); }
        catch (MissingFieldException error) { BinderBehavior.Check(error.Message.Contains("SerializationProbe.oldAmount"), "strict missing field diagnostic identifies exact field"); }
        BinderBehavior.Check(NativeIdentity.Runtime is null, "failed field binding must clear managed roots");
        var malformed = JsonNode.Parse(payload.RootElement.GetRawText())!;
        malformed["graph"]!["components"]![0]!["script"]!["type_name"] = "WrongType";
        malformed["graph"]!["components"]![0]!["script"]!["fields"] = new JsonObject();
        using var mismatched = JsonDocument.Parse(malformed.ToJsonString());
        try { using var unexpected = new ScriptSession(mismatched.RootElement); throw new Exception("native/graph script type mismatch silently accepted"); }
        catch (InvalidDataException error) { BinderBehavior.Check(error.Message.Contains("script type mismatch"), "native/graph mismatch diagnostic is explicit"); }
        Console.WriteLine("PASS serialization: profile aliases, private fields, NonSerialized, strict diagnostics and failed binding cleanup/type validation");
    }
}
public class SerializationProbe : MonoBehaviour
{
    public int amount;
    [SerializeField] private int hidden = 2;
    [NonSerialized] public int excluded = 3;
    private void Awake() => transform.position = new Vector3(amount, hidden, excluded);
}
