using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

public static partial class Input
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
    public static float GetAxis(string name) => DisunityHost.ReadAxis(name, false);
    public static float GetAxisRaw(string name) => DisunityHost.ReadAxis(name, true);
}
