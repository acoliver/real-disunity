using System.Buffers;
using System.Text.Json;
using UnityEngine;

namespace CompatibilityBridge;

internal static class ContextualAccept
{
    internal static JsonDocument Send(ContextualEnvelope envelope)
    {
        var bytes = new ArrayBufferWriter<byte>();
        using (var writer = new Utf8JsonWriter(bytes))
        {
            writer.WriteStartObject();
            writer.WriteNumber("version", 1);
            writer.WriteString("op", "callbacks.accept");
            writer.WritePropertyName("context");
            writer.WriteStartObject();
            foreach (var field in envelope.Lease.EnumerateObject())
            {
                writer.WritePropertyName(field.Name);
                if (field.Name is "world" or "frame" or "fixed_tick" or "sequence") writer.WriteNumberValue(ContextualEnvelopeCodec.Integer(field.Value));
                else field.Value.WriteTo(writer);
            }
            writer.WriteEndObject();
            writer.WritePropertyName("envelope");
            writer.WriteRawValue(envelope.Raw, skipInputValidation: true);
            writer.WriteEndObject();
        }
        if (bytes.WrittenCount > BridgeProtocol.RequestLimit) throw new InvalidDataException("contextual acceptance length budget");
        if (NativeWorld.Current is not NativeJsonWorld native) throw new InvalidOperationException("contextual acceptance requires actual native transport");
        var response = native.RequestRaw(bytes.WrittenSpan);
        if (!response.RootElement.GetProperty("ok").GetBoolean())
        {
            response.Dispose();
            throw new InvalidDataException("native contextual acceptance rejected");
        }
        return response;
    }
}
