namespace UnityEngine;

/// <summary>The measured times of one frame. Plain data: the engine measures none, so every value is zero.</summary>
public struct FrameTiming
{
    public ulong cpuTimePresentCalled;
    public double cpuFrameTime;
    public ulong cpuTimeFrameComplete;
    public double gpuFrameTime;
    public double cpuMainThreadFrameTime;
    public double cpuMainThreadPresentWaitTime;
    public double cpuRenderThreadFrameTime;
    public double cpuRenderThreadPresentWaitTime;
    public ulong frameStartTimestamp;
    public ulong firstSubmitTimestamp;
    public ulong presentFrameTimestamp;
    public ulong frameCompleteTimestamp;
    public float heightScale;
    public float widthScale;
    public uint syncInterval;
}

/// <summary>
/// Frame timing capture. The engine measures no frame, so <see cref="IsFeatureEnabled"/> is false, capturing does nothing,
/// <see cref="GetLatestTimings"/> returns 0 frames and leaves the array alone, and the timer frequencies and the display
/// rate are 0 (the engine's choice).
/// </summary>
public static class FrameTimingManager
{
    public static bool IsFeatureEnabled() => false;

    public static void CaptureFrameTimings() { }

    /// <summary>Returns 0; <paramref name="timings"/> must hold at least <paramref name="numFrames"/> entries.</summary>
    public static uint GetLatestTimings(uint numFrames, FrameTiming[] timings)
    {
        ArgumentNullException.ThrowIfNull(timings);
        if (numFrames > timings.Length) throw new ArgumentException("the array holds fewer entries than the frames asked for", nameof(timings));
        return 0;
    }

    public static float GetVSyncsPerSecond() => 0f;

    public static ulong GetGpuTimerFrequency() => 0;

    public static ulong GetCpuTimerFrequency() => 0;
}
