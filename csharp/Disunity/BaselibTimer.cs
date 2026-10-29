using System.Diagnostics;

namespace Unity.Baselib.LowLevel;

/// <summary>
/// Baselib's timers, which Unity Transport uses for timeouts and heartbeats. Ticks come from the
/// process's high-resolution clock, and startup is when the engine first read a timer.
/// </summary>
public static unsafe partial class Binding
{
    private static readonly long StartupTicks = Stopwatch.GetTimestamp();

    public static readonly Baselib_Timer_TickToNanosecondConversionRatio Baselib_Timer_HighPrecisionTimerTickToNanosecondConversionRatio =
        new() { ticksToNanosecondsNumerator = 1_000_000_000, ticksToNanosecondsDenominator = (ulong)Stopwatch.Frequency };

    public static Baselib_Timer_TickToNanosecondConversionRatio Baselib_Timer_GetTicksToNanosecondsConversionRatio() =>
        Baselib_Timer_HighPrecisionTimerTickToNanosecondConversionRatio;

    public static ulong Baselib_Timer_GetHighPrecisionTimerTicks() => (ulong)Stopwatch.GetTimestamp();

    public static double Baselib_Timer_GetTimeSinceStartupInSeconds() =>
        (Stopwatch.GetTimestamp() - StartupTicks) / (double)Stopwatch.Frequency;

    public static void Baselib_Timer_WaitForAtLeast(uint timeInMilliseconds) => Thread.Sleep((int)Math.Min(timeInMilliseconds, int.MaxValue));
}
