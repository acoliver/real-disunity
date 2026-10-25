using Disunity.Native;
using Disunity.Hosting;
namespace UnityEngine;

/// <summary>Read-only views of the sole native clock.</summary>
public static class Time
{
    private static unsafe DisunityClock Read()
    {
        DisunityClock clock;
        DisunityHost.ThrowIfError(NativeBindings.disunity_world_clock(DisunityHost.WorldHandle, &clock));
        return clock;
    }
    public static unsafe float fixedDeltaTime
    {
        get { double value; DisunityHost.ThrowIfError(NativeBindings.disunity_world_fixed_delta(DisunityHost.WorldHandle, &value)); return (float)value; }
    }
    public static float deltaTime => (float)Read().delta;
    public static float time => (float)Read().time;
    public static int frameCount => checked((int)Read().frame);
    public static float DeltaTime => (float)Read().delta;
    public static uint Frame => checked((uint)Read().frame);
}
