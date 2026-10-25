using System.Text.Json;

namespace CompatibilityBridge;

internal static class BridgeProtocol
{
    internal const int RequestLimit = 1 << 20;
    internal const int ResponseLimit = 16 << 20;
    private static readonly string[] Capabilities = ["describe_bridge", "fixture_manifest_v1", "cdecl_utf8", "private_callbacks", "public_observation", "native_phase_context_v1", "strict_contact_envelope_native_json_v1", NativeReadinessProfile.Name, TerminalCleanupProfile.Managed, ByteTransportProfile.Consumer, NestedStartProfile.Consumer, HostIdleMutationProfile.Consumer];
    internal static byte[] Describe() => JsonSerializer.SerializeToUtf8Bytes(new
    {
        descriptor_version = 1, bridge_revision = 1,
        native_abi_versions = new[] { 2 }, native_protocol_versions = new[] { 1 },
        manifest_schema_versions = new[] { 1 }, capabilities = Capabilities,
        endianness = BitConverter.IsLittleEndian ? "little" : "big", pointer_width = IntPtr.Size * 8,
        limits = new { max_request_bytes = RequestLimit, max_response_bytes = ResponseLimit },
        clock_policy = "float_accumulated", execution_profiles = new[] { "native_phase_context_v1" },
        transport_profiles = new[] { new { transport_profile = ByteTransportProfile.Name,
            consumer_capability = ByteTransportProfile.Consumer, max_request_bytes = RequestLimit,
            max_response_bytes = ByteTransportProfile.ReplyLimit, native_outstanding_bytes = 16 << 20,
            outstanding_scope = "native_world_generation_owned_response_bytes" } },
        native_scheduler = false, coroutine_readiness = "managed",
        scheduler_profiles = new[] { NativeReadinessProfile.Describe(), NestedStartProfile.Describe() }
    });

    internal static void ValidateNative(JsonElement native)
    {
        Equal(native, "descriptor_version", 1);
        Equal(native, "native_abi_version", 2);
        Equal(native, "pointer_width", 64);
        Equal(native, "endianness", BitConverter.IsLittleEndian ? "little" : "big");
        Equal(native, "clock_policy", "float_accumulated");
        Contains(native, "native_protocol_versions", 1);
        Contains(native, "bridge_revisions", 1);
        Contains(native, "manifest_schema_versions", 1);
        Contains(native, "capabilities", "describe");
        Contains(native, "capabilities", "owned_responses");
        foreach (var name in new[] { "max_request_bytes", "max_response_bytes", "max_live_response_bytes", "max_responses" })
            if (!native.GetProperty("limits").GetProperty(name).TryGetUInt64(out var limit) || limit == 0)
                throw Invalid($"invalid native limit {name}");
        if (IntPtr.Size != 8) throw Invalid("compatibility requires 64-bit pointers");
    }

    internal static ulong ValidateManifest(JsonElement root, JsonElement native)
    {
        if (root.ValueKind != JsonValueKind.Object) throw Invalid("manifest must be an object");
        OptionalEqual(root, "version", 1);
        OptionalEqual(root, "schema_version", 1);
        if (!root.TryGetProperty("world", out var node) || !node.TryGetUInt64(out var world) || world == 0)
            throw Invalid("manifest requires a nonzero host-owned world");
        if (root.TryGetProperty("requirements", out var requirements)) ValidateRequirements(requirements, native);
        TerminalCleanupProfile.Selected(root, native);
        return world;
    }

    private static void ValidateRequirements(JsonElement requirements, JsonElement native)
    {
        if (requirements.ValueKind != JsonValueKind.Object) throw Invalid("requirements must be an object");
        foreach (var item in requirements.EnumerateObject())
        {
            switch (item.Name)
            {
                case "native_abi_version": Equal(requirements, item.Name, 2); break;
                case "native_protocol_version":
                case "bridge_revision":
                case "manifest_schema_version": Equal(requirements, item.Name, 1); break;
                case "pointer_width": Equal(requirements, item.Name, 64); break;
                case "endianness": Equal(requirements, item.Name, BitConverter.IsLittleEndian ? "little" : "big"); break;
                case "clock_policy": Equal(requirements, item.Name, "float_accumulated"); break;
                case "execution_profile":
                    Equal(requirements, item.Name, "native_phase_context_v1");
                    Contains(native, "capabilities", "native_phase_context_native_v1");
                    break;
                case "host_mutation_profile": HostIdleMutationProfile.Validate(requirements, native); break;
                case "transport_profile": ByteTransportProfile.ValidateRequirement(requirements, native); break;
                case "scheduler_profile": NativeReadinessProfile.Validate(requirements, native); break;
                case "native_capabilities": RequireCapabilities(item.Value, native.GetProperty("capabilities")); break;
                case "bridge_capabilities":
                    using (var bridge = JsonDocument.Parse(Describe())) RequireCapabilities(item.Value, bridge.RootElement.GetProperty("capabilities"));
                    break;
                default: throw Invalid($"unsupported requirement {item.Name}");
            }
        }
    }

    private static void RequireCapabilities(JsonElement required, JsonElement supported)
    {
        if (required.ValueKind != JsonValueKind.Array) throw Invalid("capabilities must be an array");
        foreach (var item in required.EnumerateArray())
        {
            if (item.ValueKind != JsonValueKind.String) throw Invalid("capability must be a string");
            Contains(supported, item.GetString()!);
        }
    }
    private static void Contains<T>(JsonElement root, string key, T expected) => Contains(root.GetProperty(key), expected);
    private static void Contains<T>(JsonElement array, T expected)
    {
        var encoded = JsonSerializer.Serialize(expected);
        if (array.ValueKind != JsonValueKind.Array || !array.EnumerateArray().Any(v => Matches(v, expected)))
            throw Invalid($"unsupported required value {encoded}");
    }
    private static void OptionalEqual<T>(JsonElement root, string name, T expected)
    {
        if (root.TryGetProperty(name, out _)) Equal(root, name, expected);
    }
    private static void Equal<T>(JsonElement root, string name, T expected)
    {
        if (!root.TryGetProperty(name, out var value) || !Matches(value, expected))
            throw Invalid($"incompatible {name}");
    }
    private static bool Matches<T>(JsonElement value, T expected) => expected switch
    {
        string text => value.ValueKind == JsonValueKind.String && value.GetString() == text,
        int number => value.ValueKind == JsonValueKind.Number && value.TryGetInt32(out var actual) && actual == number,
        _ => false
    };
    internal static InvalidDataException Invalid(string message) => new("compatibility handshake: " + message);
}
