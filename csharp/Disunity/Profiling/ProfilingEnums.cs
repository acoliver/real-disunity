namespace Unity.Profiling
{
    public enum ProfilerCategoryColor : ushort
    {
        Render = 0, Scripts = 1, BurstJobs = 2, Other = 3, Physics = 4, Animation = 5, Audio = 6, AudioJob = 7,
        AudioUpdateJob = 8, Lighting = 9, GC = 10, VSync = 11, Memory = 12, Internal = 13, UI = 14, Build = 15,
        Input = 16,
    }

    [Flags]
    public enum ProfilerRecorderOptions
    {
        None = 0, StartImmediately = 1, KeepAliveDuringDomainReload = 2, CollectOnlyOnCurrentThread = 4,
        WrapAroundWhenCapacityReached = 8, SumAllSamplesInFrame = 16, GpuRecorder = 64, Default = 24,
    }
}

namespace Unity.Profiling.LowLevel
{
    [Flags]
    public enum MarkerFlags : ushort
    {
        Default = 0, Script = 2, ScriptInvoke = 32, ScriptDeepProfiler = 64, AvailabilityEditor = 4,
        AvailabilityNonDevelopment = 8, Warning = 16, Counter = 128, SampleGPU = 256, VerbosityDebug = 1024,
        VerbosityInternal = 2048, VerbosityExternal = 3072, VerbosityAdvanced = 4096,
    }

    public enum ProfilerMarkerDataType : byte
    {
        InstanceId = 1, Int32 = 2, UInt32 = 3, Int64 = 4, UInt64 = 5, Float = 6, Double = 7, String16 = 9,
        Blob8 = 11, GfxResourceId = 12,
    }
}
