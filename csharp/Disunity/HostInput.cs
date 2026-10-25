using Disunity.Native;
using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    internal static uint InputPhase { get; private set; }
    public static void PushKey(double timestamp, KeyCode key, bool pressed) => PushInput(timestamp, 0, (uint)key, pressed ? 1 : 0);
    public static void PushFocus(double timestamp, bool focused) => PushInput(timestamp, 2, 0, focused ? 1 : 0);
    public static void PushAxis(double timestamp, string axis, float value) => PushInput(timestamp, 1,
        axis switch { "Horizontal" => 0u, "Vertical" => 1u, _ => throw new NotSupportedException($"input axis {axis}") }, value);
    private static void PushInput(double timestamp, uint kind, uint code, float value)
    {
        if (!double.IsFinite(timestamp) || timestamp < 0) throw new ArgumentOutOfRangeException(nameof(timestamp));
        ThrowIfError(NativeBindings.disunity_input_push(WorldHandle, checked((ulong)Math.Round(timestamp * 1_000_000_000)), 1_000_000_000, kind, code, value));
    }
}
