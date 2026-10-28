namespace UnityEngine
{
    public enum NPOTSupport { None = 0, Restricted = 1, Full = 2 }
}

namespace UnityEngine.Rendering
{
    public enum RenderingThreadingMode
    {
        Direct = 0, SingleThreaded = 1, MultiThreaded = 2, LegacyJobified = 3, NativeGraphicsJobs = 4,
        NativeGraphicsJobsWithoutRenderThread = 5, NativeGraphicsJobsSplitThreading = 6,
    }

    public enum VideoShadersIncludeMode { Never = 0, Referenced = 1, Always = 2 }

    public enum LightProbeOutsideHullStrategy { kLightProbeSearchTetrahedralHull = 0, kLightProbeUseAmbientProbe = 1 }
}
