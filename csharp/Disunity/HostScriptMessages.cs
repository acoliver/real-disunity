using System.Text;
using Disunity.Native;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    /// <summary>Tells the native world which per-frame messages the script's type defines, before the
    /// script is registered, so the world sends it only those. The world names a script's type by its
    /// identity, so this is sent per script; a type that defines all three needs no report.</summary>
    private static unsafe void ReportScriptMessages(ulong identity, Type type)
    {
        var bits = ScriptMessages.PerFrameMessages(type);
        if (bits == 7) return;
        fixed (byte* assembly = Encoding.UTF8.GetBytes("Managed\0"))
        fixed (byte* name = Encoding.UTF8.GetBytes(identity + "\0"))
            ThrowIfError(NativeBindings.disunity_script_type_messages(WorldHandle, assembly, name, bits));
    }
}
