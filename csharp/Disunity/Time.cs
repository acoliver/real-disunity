using System.Diagnostics;
using Disunity.Native;
using Disunity.Hosting;
namespace UnityEngine;

/// <summary>Views of the sole native clock. The native world applies the time scale and the
/// maximum frame length, so every value here already includes them.</summary>
public static class Time
{
    private const float MaximumTimeScale = 100f;
    private const float MinimumMaximumDeltaTime = 0.0001f;

    private static readonly long Started = Stopwatch.GetTimestamp();

    /// <summary>Starts the real-time clock if nothing has yet; the host calls it when a world loads.</summary>
    internal static void Start() => _ = Started;

    private static unsafe DisunityClock Read()
    {
        DisunityClock clock;
        DisunityHost.ThrowIfError(NativeBindings.disunity_world_clock(DisunityHost.WorldHandle, &clock));
        return clock;
    }

    private static unsafe DisunityTime ReadSettings()
    {
        DisunityTime time;
        DisunityHost.ThrowIfError(NativeBindings.disunity_world_time_read(DisunityHost.WorldHandle, &time));
        return time;
    }

    /// <summary>Sends <c>round(seconds * denominator) / denominator</c>; the native clock holds exact fractions.</summary>
    private static void Write(uint setting, double seconds, ulong denominator) =>
        DisunityHost.ThrowIfError(NativeBindings.disunity_world_time_write(DisunityHost.WorldHandle, setting,
            checked((ulong)Math.Round(seconds * denominator)), denominator));

    public static unsafe float fixedDeltaTime
    {
        get { double value; DisunityHost.ThrowIfError(NativeBindings.disunity_world_fixed_delta(DisunityHost.WorldHandle, &value)); return (float)value; }
    }
    public static float deltaTime => (float)Read().delta;
    public static float time => (float)Read().time;
    public static int frameCount => checked((int)Read().frame);
    public static float DeltaTime => (float)Read().delta;
    public static uint Frame => checked((uint)Read().frame);

    /// <summary>Frames rendered. The player renders every frame it advances, so this equals <see cref="frameCount"/>.</summary>
    public static int renderedFrameCount => frameCount;

    public static float unscaledDeltaTime => (float)ReadSettings().unscaled_delta;
    public static float unscaledTime => (float)ReadSettings().unscaled_time;

    /// <summary>
    /// A smoothed <see cref="deltaTime"/>: each frame moves it a fifth of the way from its last value to that frame's
    /// scaled delta, and the first frame sets it to that delta. Unity does not document its formula.
    /// </summary>
    public static float smoothDeltaTime => (float)ReadSettings().smooth_delta;

    /// <summary>
    /// How fast time passes, from 0 to 100, held to hundredths. It scales <see cref="deltaTime"/>, <see cref="time"/> and the
    /// fixed steps, which run on scaled time; unscaled values and real time do not change. A value outside 0 to 100 is
    /// brought to the nearest limit with a warning.
    /// </summary>
    public static float timeScale
    {
        get => (float)ReadSettings().time_scale;
        set
        {
            if (float.IsNaN(value)) throw new ArgumentOutOfRangeException(nameof(value), "Time.timeScale cannot be NaN");
            var clamped = Math.Clamp(value, 0f, MaximumTimeScale);
            if (clamped != value) Debug.LogWarning($"Time.timeScale {value} is outside 0 to {MaximumTimeScale} and was set to {clamped}");
            Write(0, clamped, 100);
        }
    }

    /// <summary>
    /// The longest frame the clock accepts, 1/3 s by default: a real frame longer than this counts as this long, for
    /// <see cref="deltaTime"/> (before scaling), <see cref="unscaledDeltaTime"/> and the fixed steps. A value below 0.0001 is raised to it.
    /// </summary>
    public static float maximumDeltaTime
    {
        get => (float)ReadSettings().maximum_delta;
        set
        {
            if (!float.IsFinite(value)) throw new ArgumentOutOfRangeException(nameof(value), "Time.maximumDeltaTime must be finite");
            Write(1, Math.Max(value, MinimumMaximumDeltaTime), 1_000_000_000);
        }
    }

    /// <summary>Seconds of real time since the host started, from a monotonic clock that the time scale does not touch.</summary>
    public static double realtimeSinceStartupAsDouble => Stopwatch.GetElapsedTime(Started).TotalSeconds;

    public static float realtimeSinceStartup => (float)realtimeSinceStartupAsDouble;
}
