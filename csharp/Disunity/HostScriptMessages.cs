using Disunity.Native;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    /// <summary>Tells the native world which per-frame messages the script bound to
    /// <paramref name="identity"/> defines, so the world sends it only those. The set depends on the
    /// script's type, which is computed once per type. A script that defines all three needs no
    /// report, because a native script starts with all three.</summary>
    private static void ReportScriptMessages(ulong identity, Type type)
    {
        var bits = ScriptMessages.PerFrameMessages(type);
        if (bits == 7) return;
        ThrowIfError(NativeBindings.disunity_script_messages(WorldHandle, identity, bits));
    }

    /// <summary>Tells the native world the execution order of the script bound to <paramref name="identity"/>,
    /// from its type. A script of order 0 needs no report, because a native script starts with order 0.</summary>
    private static void ReportScriptExecutionOrder(ulong identity, Type type)
    {
        var order = ScriptExecutionOrders.Of(type);
        if (order == 0) return;
        ThrowIfError(NativeBindings.disunity_script_execution_order(WorldHandle, identity, order));
    }
}
