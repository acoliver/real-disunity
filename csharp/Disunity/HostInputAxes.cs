using System.Text;
using Disunity.Native;
using UnityEngine;

namespace Disunity.Hosting;

public sealed record InputAxisDefinition(string Name, KeyCode Negative = KeyCode.None, KeyCode Positive = KeyCode.None)
{
    public KeyCode AltNegative { get; init; }
    public KeyCode AltPositive { get; init; }
    public float Gravity { get; init; } = 3;
    public float Sensitivity { get; init; } = 3;
    public float Dead { get; init; } = 0.001f;
    public bool Snap { get; init; } = true;
    public bool Invert { get; init; }
}

public static partial class DisunityHost
{
    /// <summary>The registered axes by name, for GetButton, which reads an axis's keys.</summary>
    internal static readonly Dictionary<string, InputAxisDefinition> AxisDefinitions = new();

    private static uint AxisKey(KeyCode key) => key == KeyCode.None ? 65536u : (uint)key;
    public static unsafe void RegisterInputAxis(ushort id, InputAxisDefinition axis)
    {
        ArgumentNullException.ThrowIfNull(axis);
        ArgumentException.ThrowIfNullOrEmpty(axis.Name);
        var bytes = Encoding.UTF8.GetBytes(axis.Name);
        AxisDefinitions[axis.Name] = axis;
        var config = new DisunityInputAxis {
            negative = AxisKey(axis.Negative), positive = AxisKey(axis.Positive),
            alt_negative = AxisKey(axis.AltNegative), alt_positive = AxisKey(axis.AltPositive),
            gravity = axis.Gravity, sensitivity = axis.Sensitivity, dead = axis.Dead,
            flags = (axis.Snap ? 1u : 0u) | (axis.Invert ? 2u : 0u),
        };
        fixed (byte* name = bytes)
            ThrowIfError(NativeBindings.disunity_input_axis_register(WorldHandle, id, name, (uint)bytes.Length, &config));
    }
    internal static unsafe uint AxisId(string axis)
    {
        ArgumentException.ThrowIfNullOrEmpty(axis);
        var bytes = Encoding.UTF8.GetBytes(axis);
        uint id;
        fixed (byte* name = bytes)
            AxisError(NativeBindings.disunity_input_axis_id(WorldHandle, name, (uint)bytes.Length, &id), axis);
        return id;
    }
    internal static unsafe float ReadAxis(string axis, bool raw)
    {
        ArgumentException.ThrowIfNullOrEmpty(axis);
        var bytes = Encoding.UTF8.GetBytes(axis);
        float value;
        fixed (byte* name = bytes)
            AxisError(NativeBindings.disunity_input_axis_read(WorldHandle, InputPhase, name, (uint)bytes.Length, raw ? 1u : 0u, &value), axis);
        return value;
    }
    private static void AxisError(DisunityResult result, string axis)
    {
        if (result.code == 0) return;
        var message = FetchErrorMessage();
        if ((DisunityErrorReason)result.detail == DisunityErrorReason.InvalidArgument)
            throw new ArgumentException($"Input axis '{axis}' is not registered or its name is invalid: {message}", nameof(axis));
        throw new DisunityException(result.code, (DisunityErrorReason)result.detail, message);
    }
}
