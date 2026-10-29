using System.Collections.Concurrent;
using Unity.Profiling.LowLevel;
using UnityEngine;

namespace Unity.Profiling
{
    /// <summary>
    /// A named profiler scope. The engine has no profiler, so Begin, End and Auto record nothing and have
    /// no effect. <see cref="Handle"/> is
    /// a non-zero number that is the same for every marker of one name in this process (numbers count up
    /// from 1 in the order names are first seen); a default-constructed marker has handle zero.
    /// </summary>
    public readonly struct ProfilerMarker
    {
        private static readonly ConcurrentDictionary<string, nint> Handles = new(StringComparer.Ordinal);
        private static long _lastHandle;

        private readonly nint _handle;

        public IntPtr Handle => _handle;

        public ProfilerMarker(string name) : this(default, name, MarkerFlags.Default) { }

        public unsafe ProfilerMarker(char* name, int nameLen) : this(default, new string(name, 0, nameLen), MarkerFlags.Default) { }

        public ProfilerMarker(ProfilerCategory category, string name) : this(category, name, MarkerFlags.Default) { }

        public unsafe ProfilerMarker(ProfilerCategory category, char* name, int nameLen) : this(category, new string(name, 0, nameLen), MarkerFlags.Default) { }

        public ProfilerMarker(string name, MarkerFlags flags) : this(default, name, flags) { }

        public unsafe ProfilerMarker(char* name, int nameLen, MarkerFlags flags) : this(default, new string(name, 0, nameLen), flags) { }

        public unsafe ProfilerMarker(ProfilerCategory category, char* name, int nameLen, MarkerFlags flags) : this(category, new string(name, 0, nameLen), flags) { }

        public ProfilerMarker(ProfilerCategory category, string name, MarkerFlags flags)
        {
            ArgumentNullException.ThrowIfNull(name);
            _handle = Handles.GetOrAdd(name, static _ => (nint)Interlocked.Increment(ref _lastHandle));
        }

        public void Begin() { }

        public void Begin(UnityEngine.Object contextUnityObject) { }

        public void End() { }

        public AutoScope Auto() => new(_handle);

        public readonly struct AutoScope : IDisposable
        {
            private readonly nint _marker;

            internal AutoScope(nint marker) { _marker = marker; }

            public void Dispose() { }
        }
    }
}

namespace Unity.Profiling.Memory
{
    /// <summary>Metadata the Memory Profiler attaches to a snapshot; the engine takes no snapshots and stores
    /// only the description. Unity declares this type internal; it is public here because the player's
    /// Memory Profiler assembly cannot see engine internals.</summary>
    public class MemorySnapshotMetadata
    {
        public string Description { get; set; } = "";

        public MemorySnapshotMetadata() { }
    }
}
