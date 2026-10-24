using System.Text.Json;

namespace CompatibilityBridge;

internal static class ContextualCleanupProfile
{
    internal const string Name = "native_contextual_lifecycle_batch_v1";
    internal const string Callback = "contextual_lifecycle_batch_v1";
    internal const string Consumer = "managed_contextual_cleanup_batch_consumer_v1";
    internal static bool Required(JsonElement manifest) => manifest.TryGetProperty("requirements", out var required)
        && required.TryGetProperty("scheduler_profile", out var profile) && profile.GetString() == Name;
    internal static void Validate(JsonElement required, JsonElement native)
    {
        foreach (var field in new[] { "native_capabilities", "bridge_capabilities" })
            if (required.TryGetProperty(field, out var capabilities) && capabilities.ValueKind == JsonValueKind.Array
                && capabilities.EnumerateArray().Any(value => value.ValueKind == JsonValueKind.String
                    && value.GetString() is "native_terminal_cleanup_v1" or "managed_terminal_cleanup_v1"))
                throw BridgeProtocol.Invalid("contextual Cleanup terminal recovery composition is unsupported");
        foreach (var (key, value) in new[] { ("scheduler_profile", Name), ("execution_profile", PhaseContexts.Profile), ("callback_profile", Callback) })
            if (required.EnumerateObject().Count(p => p.Name == key) != 1 || required.GetProperty(key).GetString() != value)
                throw BridgeProtocol.Invalid("contextual Cleanup requires exact " + key);
        RequireCapability(required, "native_capabilities", Name);
        RequireCapability(required, "bridge_capabilities", Consumer);
        RequireCapability(native, "capabilities", Name);
        var descriptor = native.GetProperty("native_contextual_lifecycle_batch");
        if (descriptor.GetProperty("profile").GetString() != Name
            || descriptor.GetProperty("wire_version").GetInt32() != 1
            || descriptor.GetProperty("max_records").GetInt32() != ContextualEnvelopeCodec.Limit
            || descriptor.GetProperty("max_envelope_bytes").GetInt32() != BridgeProtocol.RequestLimit
            || descriptor.GetProperty("flags").GetInt32() != 1
            || !descriptor.GetProperty("native_only").GetBoolean()
            || new[] { "managed_integration", "callback_batch_attestation", "contacts", "coroutines", "nesting" }
                .Any(key => descriptor.GetProperty(key).GetBoolean()))
            throw BridgeProtocol.Invalid("contextual Cleanup native descriptor mismatch");
    }
    private static void RequireCapability(JsonElement root, string field, string capability)
    {
        if (!root.TryGetProperty(field, out var values) || values.ValueKind != JsonValueKind.Array
            || !values.EnumerateArray().Any(value => value.ValueKind == JsonValueKind.String && value.GetString() == capability))
            throw BridgeProtocol.Invalid("contextual Cleanup requires capability " + capability);
    }
}
