using UnityEngine;

/// <summary>The engine measures no frame timings: the manager reports its feature off and returns no frames.</summary>
internal static class FrameTimingTests
{
    public static void Run()
    {
        Expect.That(!FrameTimingManager.IsFeatureEnabled(), "frame timing capture is off");
        FrameTimingManager.CaptureFrameTimings();
        var timings = new FrameTiming[4];
        timings[0].cpuFrameTime = 7;
        Expect.Equal(0u, FrameTimingManager.GetLatestTimings(4, timings), "no frame was measured");
        Expect.Equal(7d, timings[0].cpuFrameTime, "the array is left alone");
        Expect.Throws<ArgumentNullException>(() => FrameTimingManager.GetLatestTimings(1, null!), "a null array");
        Expect.Throws<ArgumentException>(() => FrameTimingManager.GetLatestTimings(5, timings), "more frames than the array holds");
        Expect.Near(0f, FrameTimingManager.GetVSyncsPerSecond(), "no display rate is reported");
        Expect.Equal(0UL, FrameTimingManager.GetGpuTimerFrequency(), "no GPU timer");
        Expect.Equal(0UL, FrameTimingManager.GetCpuTimerFrequency(), "no CPU timer");
        var frame = default(FrameTiming);
        Expect.That(frame.cpuFrameTime == 0 && frame.gpuFrameTime == 0 && frame.syncInterval == 0, "a new timing is all zero");
        Console.WriteLine("PASS: FrameTimingManager reports no frame timings and validates its array");
    }
}
