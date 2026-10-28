using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

/// <summary>SystemInfo host values, the null graphics device and a registered device.</summary>
internal static class SystemInfoTests
{
    public static void Run()
    {
        HostValues();
        NullDevice();
        try { RegisteredDevice(); }
        finally { DisunityHost.SetGraphicsDevice(null); }
        Expect.That(SystemInfo.graphicsDeviceType == GraphicsDeviceType.Null, "clearing the device restores the null device");
        Console.WriteLine("PASS: SystemInfo host values, null graphics device and SetGraphicsDevice");
    }

    private static void HostValues()
    {
        Expect.Equal(Environment.ProcessorCount, SystemInfo.processorCount, "processorCount");
        Expect.That(SystemInfo.systemMemorySize > 0, "systemMemorySize is positive");
        Expect.That(SystemInfo.deviceType == DeviceType.Desktop, "deviceType");
        Expect.That(SystemInfo.batteryLevel == -1f && SystemInfo.batteryStatus == BatteryStatus.Unknown, "no battery information");
        Expect.That(SystemInfo.processorType.Length > 0 && SystemInfo.deviceName.Length > 0 && SystemInfo.deviceModel.Length > 0, "texts are not empty");

        var prefix = SystemInfo.operatingSystemFamily switch
        {
            OperatingSystemFamily.Linux => "Linux ",
            OperatingSystemFamily.MacOSX => "Mac OS X ",
            OperatingSystemFamily.Windows => "Windows ",
            _ => "",
        };
        Expect.That(SystemInfo.operatingSystem.StartsWith(prefix, StringComparison.Ordinal) && SystemInfo.operatingSystem.Length > prefix.Length, $"operatingSystem '{SystemInfo.operatingSystem}' starts with '{prefix}'");
        Expect.That(OperatingSystem.IsLinux() == (SystemInfo.operatingSystemFamily == OperatingSystemFamily.Linux), "operatingSystemFamily matches the host");

        var id = SystemInfo.deviceUniqueIdentifier;
        Expect.That(id.Length == 40 && id.All(Uri.IsHexDigit) && id == id.ToLowerInvariant(), "deviceUniqueIdentifier is 40 lowercase hex digits");
        Expect.Equal(id, SystemInfo.deviceUniqueIdentifier, "deviceUniqueIdentifier is stable across reads");
        Expect.That(!SystemInfo.supportsAudio && !SystemInfo.supportsVibration && !SystemInfo.supportsGyroscope, "the engine has no audio or sensors");
        Expect.Equal("n/a", SystemInfo.unsupportedIdentifier, "unsupportedIdentifier");
        Expect.That(SystemInfo.processorFrequency >= 0, "processorFrequency is not negative");
    }

    private static void NullDevice()
    {
        Expect.That(SystemInfo.graphicsDeviceType == GraphicsDeviceType.Null, "graphicsDeviceType is Null");
        Expect.Equal("Null Device", SystemInfo.graphicsDeviceName, "graphicsDeviceName");
        Expect.That(SystemInfo.graphicsMemorySize == 0 && SystemInfo.graphicsShaderLevel == 0 && SystemInfo.maxTextureSize == 0, "numbers are 0");
        Expect.That(!SystemInfo.supportsRendering && !SystemInfo.supportsRenderTextures && !SystemInfo.supportsComputeShaders && !SystemInfo.supportsInstancing, "capabilities are absent");
        Expect.That(!SystemInfo.usesReversedZBuffer && !SystemInfo.graphicsUVStartsAtTop && !SystemInfo.graphicsMultiThreaded, "conventions are off");
        Expect.That(SystemInfo.npotSupport == NPOTSupport.None && SystemInfo.copyTextureSupport == CopyTextureSupport.None, "support levels are None");
        Expect.That(SystemInfo.renderingThreadingMode == RenderingThreadingMode.Direct, "renderingThreadingMode");
        Expect.That(!SystemInfo.IsFormatSupported(GraphicsFormat.R8G8B8A8_UNorm, GraphicsFormatUsage.Render), "no format is supported");
        Expect.That(!SystemInfo.IsFormatSupported(GraphicsFormat.R8G8B8A8_UNorm, FormatUsage.Sample), "no format is supported for FormatUsage either");
        Expect.That(!SystemInfo.SupportsTextureFormat(TextureFormat.RGBA32) && !SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGB32), "texture formats are unsupported");
        Expect.That(!SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.Depth) && !SystemInfo.SupportsVertexAttributeFormat(VertexAttributeFormat.Float32, 3), "depth and vertex formats too");
        Expect.Equal(GraphicsFormat.None, SystemInfo.GetCompatibleFormat(GraphicsFormat.R8G8B8A8_UNorm, GraphicsFormatUsage.Sample), "no compatible format");
        Expect.Equal(1, SystemInfo.GetRenderTextureSupportedMSAASampleCount(new RenderTextureDescriptor(4, 4) { msaaSamples = 8 }), "no multisampling");
        Expect.Equal(0, SystemInfo.GetTiledRenderTargetStorageSize(GraphicsFormat.R8G8B8A8_UNorm, 1), "no tiled storage");
        Expect.Equal(GraphicsFormat.R8G8B8A8_SRGB, SystemInfo.GetGraphicsFormat(DefaultFormat.LDR), "LDR default in a linear project");
        Expect.Equal(GraphicsFormat.R16G16B16A16_SFloat, SystemInfo.GetGraphicsFormat(DefaultFormat.HDR), "HDR default");
        Expect.Equal(GraphicsFormat.D24_UNorm_S8_UInt, SystemInfo.GetGraphicsFormat(DefaultFormat.DepthStencil), "depth stencil default");
        Expect.Equal(GraphicsFormat.D32_SFloat, SystemInfo.GetGraphicsFormat(DefaultFormat.Shadow), "shadow default");
        Expect.Throws<NotSupportedException>(() => SystemInfo.GetGraphicsFormat(DefaultFormat.Video), "video has no mapping");
    }

    private static void RegisteredDevice()
    {
        var device = new GraphicsDeviceInfo
        {
            Type = GraphicsDeviceType.Vulkan, Name = "Test GPU", Vendor = "Acme", VendorId = 0x1234, DeviceId = 7, Version = "Vulkan 1.3",
            MemoryMB = 8192, ShaderLevel = 50, MaxTextureSize = 16384, MaxMsaaSamples = 4, NpotSupport = NPOTSupport.Full,
            Capabilities = GraphicsCapabilities.Rendering | GraphicsCapabilities.RenderTextures | GraphicsCapabilities.ReversedZBuffer,
            Limits = new Dictionary<string, int> { ["maxCubemapSize"] = 8192 },
            FormatSupport = (format, usage) => format == GraphicsFormat.R8G8B8A8_SRGB && usage == GraphicsFormatUsage.Render,
        };
        DisunityHost.SetGraphicsDevice(device);
        Expect.That(SystemInfo.graphicsDeviceType == GraphicsDeviceType.Vulkan && SystemInfo.graphicsDeviceName == "Test GPU", "type and name");
        Expect.That(SystemInfo.graphicsDeviceVendor == "Acme" && SystemInfo.graphicsDeviceVendorID == 0x1234 && SystemInfo.graphicsDeviceID == 7, "vendor and ids");
        Expect.That(SystemInfo.graphicsDeviceVersion == "Vulkan 1.3" && SystemInfo.graphicsMemorySize == 8192 && SystemInfo.graphicsShaderLevel == 50, "version, memory and level");
        Expect.That(SystemInfo.maxTextureSize == 16384 && SystemInfo.maxCubemapSize == 8192 && SystemInfo.maxTexture3DSize == 0, "limits");
        Expect.That(SystemInfo.supportsRendering && SystemInfo.supportsRenderTextures && SystemInfo.usesReversedZBuffer && !SystemInfo.supportsComputeShaders, "capabilities");
        Expect.That(SystemInfo.npotSupport == NPOTSupport.Full, "npotSupport");
        Expect.That(SystemInfo.IsFormatSupported(GraphicsFormat.R8G8B8A8_SRGB, GraphicsFormatUsage.Render), "the device's format");
        Expect.That(SystemInfo.IsFormatSupported(GraphicsFormat.R8G8B8A8_SRGB, FormatUsage.Render), "FormatUsage.Render is the Render flag");
        Expect.That(!SystemInfo.IsFormatSupported(GraphicsFormat.R8G8B8A8_SRGB, FormatUsage.Blend), "another usage");
        Expect.That(SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGB32), "ARGB32 renders as R8G8B8A8_SRGB");
        Expect.That(!SystemInfo.SupportsBlendingOnRenderTextureFormat(RenderTextureFormat.ARGB32), "ARGB32 does not blend");
        Expect.Equal(GraphicsFormat.R8G8B8A8_SRGB, SystemInfo.GetCompatibleFormat(GraphicsFormat.R8G8B8A8_SRGB, GraphicsFormatUsage.Render), "compatible format");
        Expect.Equal(4, SystemInfo.GetRenderTextureSupportedMSAASampleCount(new RenderTextureDescriptor(4, 4) { msaaSamples = 8 }), "limited to the device maximum");
        Expect.Equal(2, SystemInfo.GetRenderTextureSupportedMSAASampleCount(new RenderTextureDescriptor(4, 4) { msaaSamples = 2 }), "a request below the maximum");
        Expect.Equal(2, SystemInfo.GetRenderTextureSupportedMSAASampleCount(new RenderTextureDescriptor(4, 4) { msaaSamples = 3 }), "3 samples round down to 2");
        DisunityHost.SetGraphicsDevice(device with { Capabilities = GraphicsCapabilities.TiledGPU });
        Expect.Throws<NotSupportedException>(() => SystemInfo.GetTiledRenderTargetStorageSize(GraphicsFormat.R8G8B8A8_UNorm, 1), "tiled storage sizes are unknown");
    }
}
