using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

public enum KeyCode
{
    None, Space, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    UpArrow, DownArrow, LeftArrow, RightArrow, Return, Escape
}
public static class Input
{
    private static unsafe (uint Keys, float Axis) Read(uint code)
    {
        uint keys;
        float axis;
        DisunityHost.ThrowIfError(NativeBindings.disunity_input_read(DisunityHost.WorldHandle, DisunityHost.InputPhase, code, &keys, &axis));
        return (keys, axis);
    }
    public static bool GetKey(KeyCode key) => (Read((uint)key).Keys & 1) != 0;
    public static bool GetKeyDown(KeyCode key) => (Read((uint)key).Keys & 2) != 0;
    public static bool GetKeyUp(KeyCode key) => (Read((uint)key).Keys & 4) != 0;
    public static float GetAxis(string name) => Read(name switch
    {
        "Horizontal" => 0,
        "Vertical" => 1,
        _ => throw new NotSupportedException($"input axis {name}"),
    }).Axis;
    public static float GetAxisRaw(string name) => GetAxis(name);
}
