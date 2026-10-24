using System.Text.Json;

namespace CompatibilityBridge;

internal static class ContactEnvelope
{
    internal static void Validate(JsonElement request, JsonElement actual)
    {
        if (request.TryGetProperty("events", out var events)) RequireMatch(events, actual);
        if (!request.TryGetProperty("events_response", out var response)) return;
        using var document = JsonDocument.Parse(response.GetString()
            ?? throw new InvalidDataException("strict contacts response must be JSON text"));
        var root = document.RootElement;
        if (root.GetProperty("version").GetInt32() != 1 || !root.GetProperty("ok").GetBoolean())
            throw new InvalidDataException("strict contacts response must be successful protocol version 1");
        RequireMatch(root.GetProperty("data"), actual);
    }

    private static void RequireMatch(JsonElement supplied, JsonElement actual)
    {
        if (!BatchEquals(supplied, actual))
            throw new InvalidDataException("strict contacts must match the native event queue");
    }

    private static bool BatchEquals(JsonElement supplied, JsonElement actual)
    {
        if (supplied.ValueKind != JsonValueKind.Array || supplied.GetArrayLength() != actual.GetArrayLength()) return false;
        for (var i = 0; i < actual.GetArrayLength(); i++)
            if (!EventEquals(supplied[i], actual[i])) return false;
        return true;
    }

    private static bool EventEquals(JsonElement supplied, JsonElement actual)
    {
        if (supplied.ValueKind != JsonValueKind.Object) return false;
        var names = new HashSet<string>(StringComparer.Ordinal);
        foreach (var property in supplied.EnumerateObject())
        {
            if (!names.Add(property.Name) || !actual.TryGetProperty(property.Name, out var expected)) return false;
            var equal = property.Name is "normal" or "point"
                ? VectorEquals(property.Value, expected) : ScalarEquals(property.Value, expected);
            if (!equal) return false;
        }
        return names.Count == actual.EnumerateObject().Count();
    }

    private static bool VectorEquals(JsonElement supplied, JsonElement actual)
    {
        if (supplied.ValueKind != JsonValueKind.Array || supplied.GetArrayLength() != 2) return false;
        for (var i = 0; i < 2; i++)
        {
            if (supplied[i].ValueKind != JsonValueKind.Number || !supplied[i].TryGetDouble(out var value)
                || !double.IsFinite(value) || BitConverter.DoubleToInt64Bits(value)
                    != BitConverter.DoubleToInt64Bits(actual[i].GetDouble())) return false;
        }
        return true;
    }

    private static bool ScalarEquals(JsonElement supplied, JsonElement actual)
    {
        if (supplied.ValueKind != actual.ValueKind) return false;
        return actual.ValueKind switch
        {
            JsonValueKind.Number => supplied.TryGetUInt64(out var value) && value == actual.GetUInt64(),
            JsonValueKind.String => supplied.GetString() == actual.GetString(),
            JsonValueKind.Null or JsonValueKind.True or JsonValueKind.False => true,
            _ => false
        };
    }
}
