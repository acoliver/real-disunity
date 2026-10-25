using System.Text.Json;
using System.Text.Json.Nodes;
using CompatibilityBridge;
using UnityEngine;

internal static class DescriptorRejection
{
    internal static void Assert(JsonNode manifest, string capability, string diagnostic)
    {
        using var before = NativeWorld.Request("snapshot");
        using var response = NativeWorld.Request("describe");
        var descriptor = JsonNode.Parse(NativeWorld.Data(response).GetRawText())!;
        var capabilities = descriptor["capabilities"]!.AsArray();
        var selected = capabilities.Single(value => value!.GetValue<string>() == capability);
        capabilities.Remove(selected);
        using var native = JsonDocument.Parse(descriptor.ToJsonString());
        using var fixture = JsonDocument.Parse(manifest.ToJsonString());
        var rejected = false;
        try { BridgeProtocol.ValidateManifest(fixture.RootElement, native.RootElement); }
        catch (InvalidDataException error) { rejected = error.Message.Contains(diagnostic, StringComparison.Ordinal); }
        BinderBehavior.Check(rejected, "missing capability must reject with its specific diagnostic");
        using var after = NativeWorld.Request("snapshot");
        BinderBehavior.Check(before.RootElement.GetRawText() == after.RootElement.GetRawText(), "descriptor rejection mutates no native state");
    }
}
