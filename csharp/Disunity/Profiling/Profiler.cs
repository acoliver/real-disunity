namespace UnityEngine.Profiling;

/// <summary>
/// Memory figures of the managed runtime. The engine has no profiler, so the sizes come from the .NET
/// garbage collector: that is the engine's choice of what the Mono heap numbers mean here.
/// </summary>
public static class Profiler
{
    /// <summary>The size of the managed heap in bytes, as the garbage collector last measured it.</summary>
    public static long GetMonoHeapSizeLong() => GC.GetGCMemoryInfo().HeapSizeBytes;

    /// <summary>The managed memory in use in bytes, without forcing a collection.</summary>
    public static long GetMonoUsedSizeLong() => GC.GetTotalMemory(forceFullCollection: false);

    /// <summary>The managed memory in use in bytes; the same figure as <see cref="GetMonoUsedSizeLong"/>.</summary>
    public static long usedHeapSizeLong => GetMonoUsedSizeLong();
}
