// Small enums the engine's scene, playable, profiler, asynchronous read and lightmapping code names, written
// from the engine's own knowledge and from what the engine, its tests and Old World name. Only the members
// something names are declared.

namespace UnityEngine.SceneManagement
{
    public enum LoadSceneMode
    {
        Single = 0,
        Additive = 1,
    }
}

namespace UnityEngine.Playables
{
    public enum PlayState
    {
        Delayed = 2,
    }

    /// <summary>What a director does after its asset ends. No member is named yet.</summary>
    public enum DirectorWrapMode
    {
    }
}

namespace Unity.Profiling.LowLevel
{
    public enum ProfilerMarkerDataUnit
    {
        Undefined = 0,
    }
}

namespace Unity.IO.LowLevel.Unsafe
{
    public enum ReadStatus
    {
        Failed = 2,
    }

    public enum FileState
    {
        Absent = 0,
    }
}

namespace UnityEngine.Experimental.GlobalIllumination
{
    /// <summary>How a baked light fades with distance. No member is named yet.</summary>
    public enum FalloffType
    {
    }
}
