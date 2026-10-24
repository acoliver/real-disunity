using System.Text.Json;

namespace CompatibilityBridge;

internal static class NativeReadinessProfile
{
    internal const string Name = "native_readiness_single_v1";
    internal static bool Selected(JsonElement manifest, JsonElement native)
    {
        if (!manifest.TryGetProperty("requirements", out var requirements)
            || !requirements.TryGetProperty("scheduler_profile", out _)) return false;
        Validate(requirements, native);
        return true;
    }
    internal static void Validate(JsonElement requirements, JsonElement native)
    {
        if (requirements.GetProperty("scheduler_profile").GetString() == ContextualCleanupProfile.Name)
        { ContextualCleanupProfile.Validate(requirements, native); return; }
        foreach (var field in new[] { "scheduler_profile", "execution_profile" })
            if (requirements.EnumerateObject().Count(value => value.Name == field) != 1)
                throw BridgeProtocol.Invalid("bounded readiness requires one " + field);
        var profile = requirements.GetProperty("scheduler_profile").GetString();
        if (profile == NestedStartProfile.Name) NestedStartProfile.Validate(native);
        else RequireText(requirements, "scheduler_profile", Name);
        RequireText(requirements, "execution_profile", PhaseContexts.Profile);
        if (!native.GetProperty("capabilities").EnumerateArray().Any(v => v.GetString() == "native_readiness_primitives_v1"))
            throw BridgeProtocol.Invalid("native readiness primitives capability is unavailable");
        if (!native.TryGetProperty("native_readiness", out var descriptor))
            throw BridgeProtocol.Invalid("native readiness descriptor is unavailable");
        RequireText(descriptor, "capability", "native_readiness_primitives_v1");
        RequireText(descriptor, "requires_profile", PhaseContexts.Profile);
        RequireText(descriptor, "iterator_model", "single_iterator_per_routine");
        foreach (var name in new[] { "max_claims_per_root", "max_scripts", "max_routines" })
            if (!descriptor.TryGetProperty(name, out var limit) || !limit.TryGetInt32(out var value) || value != 4096)
                throw BridgeProtocol.Invalid("incompatible native readiness limit " + name);
        foreach (var wait in new[] { "next", "scaled", "unscaled", "fixed", "end_frame", "join" })
            if (!descriptor.GetProperty("waits").EnumerateArray().Any(v => v.GetString() == wait))
                throw BridgeProtocol.Invalid("native readiness wait unavailable: " + wait);
    }
    private static void RequireText(JsonElement root, string field, string expected)
    {
        if (!root.TryGetProperty(field, out var value) || value.ValueKind != JsonValueKind.String || value.GetString() != expected)
            throw BridgeProtocol.Invalid("incompatible " + field);
    }
    internal static object Describe() => new
    {
        profile = Name, requires_execution_profile = PhaseContexts.Profile,
        native_capability = "native_readiness_primitives_v1", iterator_model = "single_iterator_per_routine",
        max_claims_per_root = 4096, max_scripts = 4096, max_routines = 4096,
        lifecycle_eligibility = "native", wait_readiness = "native_relative_descriptors",
        nested_stacks = false, predicates = false, immediate_start_inside_ticket = false
    };
}
