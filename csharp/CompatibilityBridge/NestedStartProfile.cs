using System.Text.Json;

namespace CompatibilityBridge;

internal static class NestedStartProfile
{
    internal const string Name = "native_readiness_nested_start_v1";
    internal const string Consumer = "managed_immediate_nested_start_consumer_v1";
    internal static bool Required(JsonElement manifest) =>
        manifest.TryGetProperty("requirements", out var requirements)
        && requirements.TryGetProperty("scheduler_profile", out var profile)
        && profile.ValueKind == JsonValueKind.String && profile.GetString() == Name;
    internal static void Validate(JsonElement native)
    {
        if (!native.GetProperty("capabilities").EnumerateArray().Any(v => v.GetString() == Name)
            || !native.TryGetProperty("native_readiness_nested_start", out var descriptor))
            throw BridgeProtocol.Invalid("native immediate nested-start capability is unavailable");
        foreach (var field in new[] { "capability", "requires_profile" })
            if (descriptor.GetProperty(field).GetString() != Name)
                throw BridgeProtocol.Invalid("incompatible native nested-start " + field);
        foreach (var (field, expected) in new[] { ("max_child_depth", 32), ("max_claims_per_root", 4096), ("claim_reservation_per_child_scope", 3) })
            if (descriptor.GetProperty(field).GetInt32() != expected)
                throw BridgeProtocol.Invalid("incompatible native nested-start " + field);
        foreach (var field in new[] { "immediate_child_starts", "start_inside_lifecycle", "start_inside_movenext" })
            if (!descriptor.GetProperty(field).GetBoolean()) throw BridgeProtocol.Invalid("native nested-start unavailable: " + field);
        foreach (var field in new[] { "start_inside_current", "start_inside_dispose", "yielded_ienumerator", "predicates" })
            if (descriptor.GetProperty(field).GetBoolean()) throw BridgeProtocol.Invalid("incompatible native nested-start " + field);
    }
    internal static object Describe() => new
    {
        profile = Name, consumer_capability = Consumer, requires_execution_profile = PhaseContexts.Profile,
        native_capability = Name, max_child_depth = 32, max_claims_per_root = 4096,
        claim_reservation_per_child_scope = 3, immediate_start_inside_ticket = true,
        lifecycle_eligibility = "native", wait_readiness = "native_relative_descriptors",
        nested_stacks = false, predicates = false, start_inside_current = false, start_inside_dispose = false,
        callback_batch_attestation = false
    };
}
