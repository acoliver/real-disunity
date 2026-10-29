using System.Runtime.CompilerServices;
using Disunity.Hosting;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// What the machine and the graphics device provide. Processor, memory, operating system and device identity
/// come from the host (see <c>HostMachine</c>). The engine has no audio output, sensors or battery reporting,
/// so those are absent: batteryLevel -1 and BatteryStatus.Unknown, the values for a device without a
/// battery, and supportsAudio, supportsAccelerometer, supportsGyroscope, supportsLocationService and
/// supportsVibration false. deviceType is Desktop.
/// Graphics values come from the device a host registered with DisunityHost.SetGraphicsDevice. With none the
/// engine reports the null device, as a Unity player started with -nographics does: GraphicsDeviceType.Null,
/// "Null Device", and every capability, limit and format as absent (false or 0), because Unity does not
/// document the null device's values and the conservative answer cannot enable rendering the engine lacks.
/// </summary>
public sealed partial class SystemInfo
{
    public const string unsupportedIdentifier = "n/a";

    public SystemInfo() { }

    private static GraphicsDeviceInfo Device => DisunityHost.GraphicsDevice;

    private static bool Has(GraphicsCapabilities capability) => (Device.Capabilities & capability) != 0;

    private static int Limit([CallerMemberName] string name = "") =>
        Device.Limits is not null && Device.Limits.TryGetValue(name, out var value) ? value : 0;

    public static float batteryLevel => -1f;
    public static BatteryStatus batteryStatus => BatteryStatus.Unknown;
    public static string operatingSystem => HostMachine.OperatingSystem;
    public static OperatingSystemFamily operatingSystemFamily => HostMachine.Family;
    public static string processorType => HostMachine.ProcessorType;

    /// <summary>The same text as <see cref="processorType"/>; the engine has no separate model and manufacturer source other than the vendor field.</summary>
    public static string processorModel => HostMachine.ProcessorType;
    public static string processorManufacturer => HostMachine.ProcessorManufacturer;
    public static int processorFrequency => HostMachine.ProcessorFrequency;
    public static int processorCount => Environment.ProcessorCount;
    public static int systemMemorySize => HostMachine.SystemMemoryMB;
    public static string deviceUniqueIdentifier => HostMachine.DeviceUniqueIdentifier;
    public static string deviceName => HostMachine.DeviceName;
    public static string deviceModel => HostMachine.DeviceModel;
    public static bool supportsAccelerometer => false;
    public static bool supportsGyroscope => false;
    public static bool supportsLocationService => false;
    public static bool supportsVibration => false;
    public static bool supportsAudio => false;
    public static DeviceType deviceType => DeviceType.Desktop;

    public static int graphicsMemorySize => Device.MemoryMB;
    public static string graphicsDeviceName => Device.Name;
    public static string graphicsDeviceVendor => Device.Vendor;
    public static int graphicsDeviceID => Device.DeviceId;
    public static int graphicsDeviceVendorID => Device.VendorId;
    public static GraphicsDeviceType graphicsDeviceType => Device.Type;
    public static string graphicsDeviceVersion => Device.Version;
    public static int graphicsShaderLevel => Device.ShaderLevel;
    public static int maxTextureSize => Device.MaxTextureSize;
    public static RenderingThreadingMode renderingThreadingMode => Device.ThreadingMode;
    public static FoveatedRenderingCaps foveatedRenderingCaps => FoveatedRenderingCaps.None;
    public static NPOTSupport npotSupport => Device.NpotSupport;
    public static CopyTextureSupport copyTextureSupport => Device.CopyTextureSupport;
    public static HDRDisplaySupportFlags hdrDisplaySupportFlags => HDRDisplaySupportFlags.None;

    public static bool hasTiledGPU => Has(GraphicsCapabilities.TiledGPU);
    public static bool hasHiddenSurfaceRemovalOnGPU => Has(GraphicsCapabilities.HiddenSurfaceRemovalOnGPU);
    public static bool hasDynamicUniformArrayIndexingInFragmentShaders => Has(GraphicsCapabilities.DynamicUniformArrayIndexingInFragmentShaders);
    public static bool supportsShadows => Has(GraphicsCapabilities.Shadows);
    public static bool supportsRawShadowDepthSampling => Has(GraphicsCapabilities.RawShadowDepthSampling);
    public static bool supportsRenderTextures => Has(GraphicsCapabilities.RenderTextures);
    public static bool supportsMotionVectors => Has(GraphicsCapabilities.MotionVectors);
    public static bool supportsRenderToCubemap => Has(GraphicsCapabilities.RenderToCubemap);
    public static bool supportsImageEffects => Has(GraphicsCapabilities.ImageEffects);
    public static bool supports3DTextures => Has(GraphicsCapabilities.Textures3D);
    public static bool supportsCompressed3DTextures => Has(GraphicsCapabilities.Compressed3DTextures);
    public static bool supports2DArrayTextures => Has(GraphicsCapabilities.ArrayTextures2D);
    public static bool supports3DRenderTextures => Has(GraphicsCapabilities.RenderTextures3D);
    public static bool supportsCubemapArrayTextures => Has(GraphicsCapabilities.CubemapArrayTextures);
    public static bool supportsAnisotropicFilter => Has(GraphicsCapabilities.AnisotropicFilter);
    public static bool supportsComputeShaders => Has(GraphicsCapabilities.ComputeShaders);
    public static bool supportsGeometryShaders => Has(GraphicsCapabilities.GeometryShaders);
    public static bool supportsTessellationShaders => Has(GraphicsCapabilities.TessellationShaders);
    public static bool supportsRenderTargetArrayIndexFromVertexShader => Has(GraphicsCapabilities.RenderTargetArrayIndexFromVertexShader);
    public static bool supportsInstancing => Has(GraphicsCapabilities.Instancing);
    public static bool supportsHardwareQuadTopology => Has(GraphicsCapabilities.HardwareQuadTopology);
    public static bool supports32bitsIndexBuffer => Has(GraphicsCapabilities.Index32Bit);
    public static bool supportsSparseTextures => Has(GraphicsCapabilities.SparseTextures);
    public static bool supportsSeparatedRenderTargetsBlend => Has(GraphicsCapabilities.SeparatedRenderTargetsBlend);
    public static bool supportsMultisampled2DArrayTextures => Has(GraphicsCapabilities.Multisampled2DArrayTextures);
    public static bool supportsMultisampledBackBuffer => Has(GraphicsCapabilities.MultisampledBackBuffer);
    public static bool supportsMemorylessTextures => Has(GraphicsCapabilities.MemorylessTextures);
    public static bool supportsMultisampleAutoResolve => Has(GraphicsCapabilities.MultisampleAutoResolve);
    public static bool supportsMultisampledShaderResolve => Has(GraphicsCapabilities.MultisampledShaderResolve);
    public static bool usesReversedZBuffer => Has(GraphicsCapabilities.ReversedZBuffer);
    public static bool supportsVariableRateShading => Has(GraphicsCapabilities.VariableRateShading);
    public static bool supportsDynamicResolution => Has(GraphicsCapabilities.DynamicResolution);
    public static bool supportsAsyncCompute => Has(GraphicsCapabilities.AsyncCompute);
    public static bool supportsGpuRecorder => Has(GraphicsCapabilities.GpuRecorder);
    public static bool supportsGraphicsFence => Has(GraphicsCapabilities.GraphicsFence);
    public static bool supportsGPUFence => Has(GraphicsCapabilities.GraphicsFence);
    public static bool supportsAsyncGPUReadback => Has(GraphicsCapabilities.AsyncGPUReadback);
    public static bool supportsParallelPSOCreation => Has(GraphicsCapabilities.ParallelPSOCreation);
    public static bool supportsRayTracingShaders => Has(GraphicsCapabilities.RayTracingShaders);
    public static bool supportsRayTracing => Has(GraphicsCapabilities.RayTracing);
    public static bool supportsInlineRayTracing => Has(GraphicsCapabilities.InlineRayTracing);
    public static bool supportsIndirectDispatchRays => Has(GraphicsCapabilities.IndirectDispatchRays);
    public static bool supportsMachineLearning => Has(GraphicsCapabilities.MachineLearning);
    public static bool supportsSetConstantBuffer => Has(GraphicsCapabilities.SetConstantBuffer);
    public static bool minConstantBufferOffsetAlignment => Has(GraphicsCapabilities.MinConstantBufferOffsetAlignment);
    public static bool hasMipMaxLevel => Has(GraphicsCapabilities.MipMaxLevel);
    public static bool supportsMipStreaming => Has(GraphicsCapabilities.MipStreaming);
    public static bool usesLoadStoreActions => Has(GraphicsCapabilities.LoadStoreActions);
    public static bool supportsConservativeRaster => Has(GraphicsCapabilities.ConservativeRaster);
    public static bool supportsMultiview => Has(GraphicsCapabilities.Multiview);
    public static bool supportsStoreAndResolveAction => Has(GraphicsCapabilities.StoreAndResolveAction);
    public static bool supportsMultisampleResolveDepth => Has(GraphicsCapabilities.MultisampleResolveDepth);
    public static bool supportsMultisampleResolveStencil => Has(GraphicsCapabilities.MultisampleResolveStencil);
    public static bool supportsIndirectArgumentsBuffer => Has(GraphicsCapabilities.IndirectArgumentsBuffer);
    public static bool supportsDepthFetchInRenderPass => Has(GraphicsCapabilities.DepthFetchInRenderPass);
    public static bool supportsVertexPrograms => Has(GraphicsCapabilities.VertexPrograms);
    public static bool supportsRendering => Has(GraphicsCapabilities.Rendering);
    public static bool graphicsMultiThreaded => Has(GraphicsCapabilities.MultiThreaded);
    public static bool graphicsUVStartsAtTop => Has(GraphicsCapabilities.UVStartsAtTop);
    public static int maxTexture3DSize => Limit();
    public static int maxTextureArraySlices => Limit();
    public static int maxCubemapSize => Limit();
    public static int maxAnisotropyLevel => Limit();
    public static int maxComputeBufferInputsVertex => Limit();
    public static int maxComputeBufferInputsFragment => Limit();
    public static int maxComputeBufferInputsGeometry => Limit();
    public static int maxComputeBufferInputsDomain => Limit();
    public static int maxComputeBufferInputsHull => Limit();
    public static int maxComputeBufferInputsCompute => Limit();
    public static int maxComputeWorkGroupSize => Limit();
    public static int maxComputeWorkGroupSizeX => Limit();
    public static int maxComputeWorkGroupSizeY => Limit();
    public static int maxComputeWorkGroupSizeZ => Limit();
    public static int computeSubGroupSize => Limit();
    public static int supportedRenderTargetCount => Limit();
    public static int supportedRandomWriteTargetCount => Limit();
    public static int supportsMultisampledTextures => Limit();
    public static int supportsTextureWrapMirrorOnce => Limit();
    public static int supportsStencil => Limit();
    public static int maxTiledPixelStorageSize => Limit();
    public static int constantBufferOffsetAlignment => Limit();
    public static long maxGraphicsBufferSize => Limit();
    public static int maxConstantBufferSize => Limit();
    public static int graphicsPixelFillrate => Limit();
}
