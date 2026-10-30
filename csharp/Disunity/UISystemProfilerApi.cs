namespace UnityEngine;

/// <summary>Markers UI code places around its layout and rendering work for a profiler. The engine has no
/// profiler capture, so each marker is accepted and dropped.</summary>
public static class UISystemProfilerApi
{
    public enum SampleType
    {
        Layout = 0,
        Render = 1,
    }

    public static void BeginSample(SampleType type)
    {
    }

    public static void EndSample(SampleType type)
    {
    }

    public static void AddMarker(string name, Object obj)
    {
    }
}
