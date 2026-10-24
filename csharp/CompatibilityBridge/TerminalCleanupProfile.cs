using System.Text.Json;

namespace CompatibilityBridge;

internal static class TerminalCleanupProfile
{
    internal const string Managed = "managed_terminal_cleanup_v1";
    private const string Native = "native_terminal_cleanup_v1";
    internal static bool Selected(JsonElement manifest, JsonElement native)
    {
        if (!manifest.TryGetProperty("requirements", out var requirements)) return false;
        var provider = Requires(requirements, "native_capabilities", Native);
        var consumer = Requires(requirements, "bridge_capabilities", Managed);
        if (provider != consumer) throw BridgeProtocol.Invalid("terminal cleanup requires paired native/managed capabilities");
        if (!provider) return false;
        if (!requirements.TryGetProperty("execution_profile", out var profile) || profile.GetString() != PhaseContexts.Profile)
            throw BridgeProtocol.Invalid("terminal cleanup requires strict execution");
        if (!native.GetProperty("capabilities").EnumerateArray().Any(v => v.GetString() == Native))
            throw BridgeProtocol.Invalid("cleanup_unsupported: native terminal capability is unavailable");
        return true;
    }
    private static bool Requires(JsonElement requirements, string field, string capability) =>
        requirements.TryGetProperty(field, out var values) && values.ValueKind == JsonValueKind.Array
        && values.EnumerateArray().Any(v => v.ValueKind == JsonValueKind.String && v.GetString() == capability);
}
