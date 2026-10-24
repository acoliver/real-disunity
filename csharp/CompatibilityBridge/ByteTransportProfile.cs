using System.Text.Json;

namespace CompatibilityBridge;

internal static class ByteTransportProfile
{
    internal const string Name = "transport_byte_limits_v1";
    internal const string Consumer = "managed_transport_profile_consumer_v1";
    internal const int ReplyLimit = 8 << 20;
    internal static bool Active { get; set; }
    internal static int ResponseLimit => Active ? ReplyLimit : BridgeProtocol.ResponseLimit;

    internal static void ValidateRequirement(JsonElement requirements, JsonElement native)
    {
        if (requirements.EnumerateObject().Count(p => p.Name == "transport_profile") != 1)
            throw BridgeProtocol.Invalid("byte transport duplicate profile requirement");
        if (requirements.GetProperty("transport_profile").ValueKind != JsonValueKind.String
            || requirements.GetProperty("transport_profile").GetString() != Name
            || !requirements.TryGetProperty("execution_profile", out var phase)
            || phase.GetString() != PhaseContexts.Profile)
            throw BridgeProtocol.Invalid("byte transport requires explicit transport and execution profiles");
        if (!Supported(native)) throw BridgeProtocol.Invalid("byte transport native capability is unavailable");
    }

    internal static bool Selected(string path, ulong world, JsonElement native, JsonElement manifest)
    {
        var required = manifest.TryGetProperty("requirements", out var requirements)
            && requirements.TryGetProperty("transport_profile", out _);
        if (!Supported(native))
        {
            if (required) throw BridgeProtocol.Invalid("byte transport native capability is unavailable");
            return false;
        }
        using var response = NativeDescription.ReadTransport(path, world);
        var data = response.RootElement.GetProperty("data");
        var selected = data.GetProperty("transport_profile").GetString() == Name;
        if (selected != required) throw BridgeProtocol.Invalid("byte transport manifest must match actual native selection");
        if (required)
        {
            ValidateRequirement(requirements, native);
            var limits = data.GetProperty("limits");
            if (limits.GetProperty("max_request_bytes").GetInt32() != BridgeProtocol.RequestLimit
                || limits.GetProperty("max_response_bytes").GetInt32() != ReplyLimit
                || limits.GetProperty("max_outstanding_bytes").GetInt32() != 16 << 20
                || data.GetProperty("outstanding_scope").GetString() != "world_generation")
                throw BridgeProtocol.Invalid("byte transport effective limits must match 1/8/16 MiB");
        }
        return selected;
    }

    private static bool Supported(JsonElement native) => native.GetProperty("capabilities").EnumerateArray().Any(v => v.GetString() == Name);
}
