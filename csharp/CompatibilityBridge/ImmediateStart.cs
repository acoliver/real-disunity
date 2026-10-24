using System.Text.Json;

namespace CompatibilityBridge;

internal sealed partial class NativeReadiness
{
    private void DrainImmediate(ulong id, JsonElement decision)
    {
        while (true)
        {
            if (decision.GetProperty("routine").GetUInt64() != id)
                throw new InvalidDataException("native immediate decision belongs to another routine");
            if (Execute(decision)) return;
            decision = Next() ?? throw new InvalidDataException("native immediate child pipeline ended without suspension or disposal");
        }
    }
}
