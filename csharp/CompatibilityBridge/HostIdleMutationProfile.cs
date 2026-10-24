using System.Text.Json;

namespace CompatibilityBridge;

internal static class HostIdleMutationProfile
{
    internal const string Name = "native_host_idle_rename_v1";
    internal const string Consumer = "managed_host_idle_rename_consumer_v1";
    internal static bool Required(JsonElement manifest) => manifest.TryGetProperty("requirements", out var requirements)
        && requirements.TryGetProperty("host_mutation_profile", out var profile) && profile.GetString() == Name;
    internal static void Validate(JsonElement requirements, JsonElement native)
    {
        foreach (var field in new[] { "host_mutation_profile", "execution_profile" })
            if (requirements.EnumerateObject().Count(value => value.Name == field) != 1)
                throw BridgeProtocol.Invalid("host Idle rename requires one " + field);
        if (requirements.GetProperty("host_mutation_profile").GetString() != Name
            || requirements.GetProperty("execution_profile").GetString() != PhaseContexts.Profile
            || requirements.TryGetProperty("scheduler_profile", out _)
            || requirements.TryGetProperty("callback_profile", out _))
            throw BridgeProtocol.Invalid("host Idle rename requires exact profile and managed scheduling");
        if (!native.GetProperty("capabilities").EnumerateArray().Any(value => value.GetString() == Name))
            throw BridgeProtocol.Invalid("native host Idle rename provider is unavailable");
    }
}
