using Unity.Profiling.LowLevel;
using Unity.Profiling.LowLevel.Unsafe;

namespace Unity.Profiling
{
    /// <summary>One recorded value and how many raw values it sums.</summary>
    public readonly struct ProfilerRecorderSample
    {
        private readonly long _value;
        private readonly long _count;

        internal ProfilerRecorderSample(long value, long count) { _value = value; _count = count; }

        public long Value => _value;
        public long Count => _count;
    }

    /// <summary>
    /// Reads values the profiler publishes. The engine has no profiler and publishes no markers or counters, so
    /// every recorder, whichever way it is built, is not <see cref="Valid"/>: it never runs, has no samples and
    /// reads 0, as a Unity recorder does for a statistic that does not exist. <see cref="GetSample"/> throws
    /// ArgumentOutOfRangeException because no index is below the zero sample count (this is the engine's reading
    /// of Unity's behaviour; the Unity page does not state the exception for an invalid recorder).
    /// Start, Stop, Reset and Dispose do nothing, so disposing twice is safe.
    /// </summary>
    public readonly struct ProfilerRecorder : IDisposable
    {
        public ProfilerRecorder(string statName, int capacity = 1, ProfilerRecorderOptions options = ProfilerRecorderOptions.Default) => Check(statName);

        public ProfilerRecorder(string categoryName, string statName, int capacity = 1, ProfilerRecorderOptions options = ProfilerRecorderOptions.Default) => Check(statName);

        public ProfilerRecorder(ProfilerCategory category, string statName, int capacity = 1, ProfilerRecorderOptions options = ProfilerRecorderOptions.Default) => Check(statName);

        public unsafe ProfilerRecorder(ProfilerCategory category, char* statName, int statNameLen, int capacity = 1, ProfilerRecorderOptions options = ProfilerRecorderOptions.Default) { }

        public ProfilerRecorder(ProfilerMarker marker, int capacity = 1, ProfilerRecorderOptions options = ProfilerRecorderOptions.Default) { }

        public ProfilerRecorder(ProfilerRecorderHandle statHandle, int capacity = 1, ProfilerRecorderOptions options = ProfilerRecorderOptions.Default) { }

        public static ProfilerRecorder StartNew(ProfilerCategory category, string statName, int capacity = 1, ProfilerRecorderOptions options = ProfilerRecorderOptions.Default) => new(category, statName, capacity, options);

        public static ProfilerRecorder StartNew(ProfilerMarker marker, int capacity = 1, ProfilerRecorderOptions options = ProfilerRecorderOptions.Default) => new(marker, capacity, options);

        private static void Check(string statName) => ArgumentNullException.ThrowIfNull(statName);

        public bool Valid => false;
        public ProfilerMarkerDataType DataType => 0;
        public ProfilerMarkerDataUnit UnitType => ProfilerMarkerDataUnit.Undefined;
        public void Start() { }
        public void Stop() { }
        public void Reset() { }
        public long CurrentValue => 0;
        public double CurrentValueAsDouble => 0;
        public long LastValue => 0;
        public double LastValueAsDouble => 0;
        public int Capacity => 0;
        public int Count => 0;
        public bool IsRunning => false;
        public bool WrappedAround => false;

        public ProfilerRecorderSample GetSample(int index) =>
            throw new ArgumentOutOfRangeException(nameof(index), index, "The recorder holds no samples");

        public void CopyTo(List<ProfilerRecorderSample> outSamples, bool reset = false) => ArgumentNullException.ThrowIfNull(outSamples);

        public unsafe int CopyTo(ProfilerRecorderSample* dest, int destSize, bool reset = false) => 0;

        public ProfilerRecorderSample[] ToArray() => [];

        public void Dispose() { }
    }
}

namespace Unity.Profiling.LowLevel.Unsafe
{
    /// <summary>Identifies a statistic the profiler publishes. The engine publishes none, so no handle is valid.</summary>
    public readonly struct ProfilerRecorderHandle
    {
        public bool Valid => false;

        public static ProfilerRecorderDescription GetDescription(ProfilerRecorderHandle handle) =>
            throw new ArgumentException("The handle does not name a statistic; the engine publishes none", nameof(handle));

        public static void GetAvailable(List<ProfilerRecorderHandle> outRecorderHandleList)
        {
            ArgumentNullException.ThrowIfNull(outRecorderHandleList);
            outRecorderHandleList.Clear();
        }
    }

    public readonly struct ProfilerRecorderDescription
    {
        public ProfilerCategory Category => default;
        public MarkerFlags Flags => MarkerFlags.Default;
        public ProfilerMarkerDataType DataType => 0;
        public ProfilerMarkerDataUnit UnitType => ProfilerMarkerDataUnit.Undefined;
        public int NameUtf8Len => 0;
        public unsafe byte* NameUtf8 => null;
        public string Name => "";
    }
}
