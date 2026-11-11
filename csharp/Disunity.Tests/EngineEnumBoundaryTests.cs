using UnityEngine.Experimental.Rendering;
using UnityEngine;

internal static class EngineEnumBoundaryTests
{
    public static void Run()
    {
        TextureStorageIds();
        TextureTargetIds();
        ObjectAndCameraMasks();
        DiscreteSelectors();
        TransformAndBufferMasks();
        AnimationAndInputIds();
        ExtendedTextureIds();
        IndividualMaskBits();
        BlockTextureNumbersMapToNativeContract();
        WideTextureNumbersConvertAndAllocate();
        HdrAstcNumbersCarryBlockLayouts();
        SignedWideTexturesAreRefused();
        RawRenderTextureNumbersConvert();
        RenderAndDisplaySelectors();
        Console.WriteLine("PASS: engine enum wire values, masks and selectors");
    }

    private static void TextureStorageIds()
    {
        Equal(4, (int)TextureFormat.RGBA32, "RGBA byte storage");
        Equal(10, (int)TextureFormat.DXT1, "DXT1 storage");
        Equal(12, (int)TextureFormat.DXT5, "DXT5 storage preserves the gap");
        Equal(24, (int)TextureFormat.BC6H, "BC6H storage");
        Equal(25, (int)TextureFormat.BC7, "BC7 storage");
        Equal(26, (int)TextureFormat.BC4, "BC4 follows BC7");
        Equal(27, (int)TextureFormat.BC5, "BC5 storage");
        Equal(28, (int)TextureFormat.DXT1Crunched, "crunched DXT1 storage");
        Equal(29, (int)TextureFormat.DXT5Crunched, "crunched DXT5 storage");
        Equal(63, (int)TextureFormat.R8, "single byte storage");
    }

    private static void TextureTargetIds()
    {
        Equal(0, (int)RenderTextureFormat.ARGB32, "default target storage");
        Equal(7, (int)RenderTextureFormat.Default, "default target sentinel");
        Equal(9, (int)RenderTextureFormat.DefaultHDR, "HDR target sentinel");
        Equal(11, (int)RenderTextureFormat.ARGBFloat, "float target storage");
        Equal(20, (int)RenderTextureFormat.BGRA32, "BGRA target storage");
        Equal(22, (int)RenderTextureFormat.RGB111110Float, "packed float target preserves the gap");
        Equal(0, (int)RenderTextureReadWrite.Default, "default color conversion");
        Equal(1, (int)RenderTextureReadWrite.Linear, "linear color conversion");
        Equal(2, (int)RenderTextureReadWrite.sRGB, "sRGB color conversion");
    }

    private static void ObjectAndCameraMasks()
    {
        Equal(52, (int)HideFlags.DontSave, "transient object save mask");
        Equal(61, (int)HideFlags.HideAndDontSave, "hidden transient object mask");
        Equal(0, (int)(HideFlags.HideAndDontSave & HideFlags.HideInInspector),
            "combined hidden mask does not hide the inspector");
        Equal(5, (int)(DepthTextureMode.Depth | DepthTextureMode.MotionVectors), "depth and motion request");
        Equal(7, (int)(DepthTextureMode.Depth | DepthTextureMode.DepthNormals | DepthTextureMode.MotionVectors),
            "all camera depth outputs");
        Equal(16, (int)CameraType.Reflection, "reflection camera discriminator");
        Equal(-1, (int)RenderingPath.UsePlayerSettings, "inherited rendering path sentinel");
        Equal(3, (int)RenderingPath.DeferredShading, "deferred shading discriminator");
    }

    private static void DiscreteSelectors()
    {
        Equal(1, (int)CameraClearFlags.Skybox, "skybox clear");
        Equal(2, (int)CameraClearFlags.SolidColor, "solid clear");
        Equal(4, (int)CameraClearFlags.Nothing, "no clear");
        Equal(1, (int)TextureWrapMode.Clamp, "clamped sampler");
        Equal(2, (int)TextureWrapMode.Mirror, "mirrored sampler");
        Equal(3, (int)TextureWrapMode.MirrorOnce, "single mirrored sampler");
        Equal(3, (int)PrimitiveType.Cube, "cube shape discriminator");
        Equal(5, (int)PrimitiveType.Quad, "quad shape discriminator");
        Equal(8, (int)TextAnchor.LowerRight, "last text anchor");
        Equal(0, (int)WrapMode.Default, "default animation wrap mode");
        Equal(1, (int)WrapMode.Once, "once animation wrap mode");
        Equal(1, (int)WrapMode.Clamp, "clamp is an alias of once");
        Equal(2, (int)WrapMode.Loop, "looping animation wrap mode");
        Equal(4, (int)WrapMode.PingPong, "ping-pong animation wrap mode");
        Equal(8, (int)WrapMode.ClampForever, "nonsequential animation wrap mode");
        Equal(1, (int)AnimatorUpdateMode.AnimatePhysics, "physics animation clock is an alias of fixed");
    }

    private static void TransformAndBufferMasks()
    {
        Equal(-1, (int)DrivenTransformProperties.All, "all driven fields sentinel");
        Equal(14, (int)DrivenTransformProperties.AnchoredPosition3D, "driven position axes");
        Equal(224, (int)DrivenTransformProperties.Scale, "driven scale axes");
        Equal(3840, (int)DrivenTransformProperties.Anchors, "driven anchor bounds");
        Equal(12288, (int)DrivenTransformProperties.SizeDelta, "driven size axes");
        Equal(49152, (int)DrivenTransformProperties.Pivot, "driven pivot axes");
        Equal(0, (int)(DrivenTransformProperties.Anchors & DrivenTransformProperties.AnchoredPosition3D),
            "anchors and positions are independent masks");
        Equal(17, (int)(ComputeBufferType.Raw | ComputeBufferType.Structured), "buffer storage mask");
        Equal(256, (int)ComputeBufferType.IndirectArguments, "indirect buffer mask");
        Equal(256, (int)ComputeBufferType.DrawIndirect, "legacy indirect buffer name");
        Equal(4, (int)ComputeBufferMode.SubUpdates, "subupdate buffer mode");
    }

    private static void AnimationAndInputIds()
    {
        Equal(2, (int)AnimatorCullingMode.CullCompletely, "culled animation");
        Equal(1, (int)AnimatorUpdateMode.Fixed, "fixed animation clock");
        Equal(1, (int)DurationUnit.Normalized, "normalized animation duration");
        Equal(3, (int)WeightedMode.Both, "two weighted curve handles");
        Equal(2, (int)GradientMode.PerceptualBlend, "perceptual gradient");
        Equal(1, (int)EventType.MouseUp, "mouse release event");
        Equal(5, (int)EventType.KeyUp, "key release event");
        Equal(12, (int)EventType.Used, "consumed event");
        Equal(15, (int)EventType.DragExited, "last contiguous event");
        Equal(16, (int)EventType.ContextClick, "context click event");
        Equal(20, (int)EventType.MouseEnterWindow, "window enter event follows the gap");
        Equal(21, (int)EventType.MouseLeaveWindow, "window leave event");
        Equal(11, (int)(EventModifiers.Shift | EventModifiers.Control | EventModifiers.Command),
            "keyboard modifier mask");
        Equal(4, (int)TouchPhase.Canceled, "canceled touch");
        Equal(0, (int)ParticleSystemStopBehavior.StopEmittingAndClear, "clear stopped particles");
        Equal(1, (int)ParticleSystemStopBehavior.StopEmitting, "keep stopped particles");
        Equal(1, (int)FindObjectsInactive.Include, "inactive lookup selector");
        Equal(-1, (int)ColorSpace.Uninitialized, "uninitialized color space sentinel");
    }

    private static void ExtendedTextureIds()
    {
        Equal(48, (int)TextureFormat.ASTC_4x4, "small ASTC block");
        Equal(53, (int)TextureFormat.ASTC_12x12, "large ASTC block");
        Equal(66, (int)TextureFormat.ASTC_HDR_4x4, "small HDR ASTC block");
        Equal(67, (int)TextureFormat.ASTC_HDR_5x5, "5x5 HDR ASTC block");
        Equal(68, (int)TextureFormat.ASTC_HDR_6x6, "6x6 HDR ASTC block");
        Equal(69, (int)TextureFormat.ASTC_HDR_8x8, "8x8 HDR ASTC block");
        Equal(70, (int)TextureFormat.ASTC_HDR_10x10, "10x10 HDR ASTC block");
        Equal(71, (int)TextureFormat.ASTC_HDR_12x12, "large HDR ASTC block");
        Equal(72, (int)TextureFormat.RG32, "two ushort channels");
        Equal(73, (int)TextureFormat.RGB48, "three ushort channels");
        Equal(74, (int)TextureFormat.RGBA64, "four ushort channels");
        Equal(75, (int)TextureFormat.R8_SIGNED, "one signed byte channel");
        Equal(76, (int)TextureFormat.RG16_SIGNED, "two signed byte channels");
        Equal(77, (int)TextureFormat.RGB24_SIGNED, "three signed byte channels");
        Equal(78, (int)TextureFormat.RGBA32_SIGNED, "four signed byte channels");
        Equal(79, (int)TextureFormat.R16_SIGNED, "one signed short channel");
        Equal(80, (int)TextureFormat.RG32_SIGNED, "two signed short channels");
        Equal(81, (int)TextureFormat.RGB48_SIGNED, "three signed short channels");
        Equal(82, (int)TextureFormat.RGBA64_SIGNED, "four signed short channels");
        Equal(25, (int)RenderTextureFormat.RG16, "two byte target channels");
        Equal(28, (int)RenderTextureFormat.R16, "one ushort target channel");
    }

    private static void IndividualMaskBits()
    {
        Equal(1, (int)HideFlags.HideInHierarchy, "hierarchy hide bit");
        Equal(2, (int)HideFlags.HideInInspector, "inspector hide bit");
        Equal(4, (int)HideFlags.DontSaveInEditor, "editor save bit");
        Equal(8, (int)HideFlags.NotEditable, "read-only bit");
        Equal(16, (int)HideFlags.DontSaveInBuild, "build save bit");
        Equal(32, (int)HideFlags.DontUnloadUnusedAsset, "unload bit");
        Equal(2, (int)DrivenTransformProperties.AnchoredPositionX, "driven position x");
        Equal(4, (int)DrivenTransformProperties.AnchoredPositionY, "driven position y");
        Equal(8, (int)DrivenTransformProperties.AnchoredPositionZ, "driven position z");
        Equal(16, (int)DrivenTransformProperties.Rotation, "driven rotation");
        Equal(32, (int)DrivenTransformProperties.ScaleX, "driven scale x");
        Equal(64, (int)DrivenTransformProperties.ScaleY, "driven scale y");
        Equal(128, (int)DrivenTransformProperties.ScaleZ, "driven scale z");
        Equal(256, (int)DrivenTransformProperties.AnchorMinX, "driven anchor min x");
        Equal(512, (int)DrivenTransformProperties.AnchorMinY, "driven anchor min y");
        Equal(1024, (int)DrivenTransformProperties.AnchorMaxX, "driven anchor max x");
        Equal(2048, (int)DrivenTransformProperties.AnchorMaxY, "driven anchor max y");
        Equal(4096, (int)DrivenTransformProperties.SizeDeltaX, "driven size x");
        Equal(8192, (int)DrivenTransformProperties.SizeDeltaY, "driven size y");
        Equal(16384, (int)DrivenTransformProperties.PivotX, "driven pivot x");
        Equal(32768, (int)DrivenTransformProperties.PivotY, "driven pivot y");
        Equal(1, (int)EventModifiers.Shift, "shift modifier");
        Equal(2, (int)EventModifiers.Control, "control modifier");
        Equal(4, (int)EventModifiers.Alt, "alt modifier");
        Equal(8, (int)EventModifiers.Command, "command modifier");
        Equal(16, (int)EventModifiers.Numeric, "numeric pad modifier");
        Equal(32, (int)EventModifiers.CapsLock, "caps lock modifier");
        Equal(64, (int)EventModifiers.FunctionKey, "function key modifier");
    }

    // The raw numbers below are what a player passes for TextureFormat, and the GraphicsFormat numbers are the
    // ones the native renderer expects (crates/play/src/offscreen/assets.rs).
    private static void BlockTextureNumbersMapToNativeContract()
    {
        Equal(97, GraphicsOf(10, false), "linear DXT1 maps to native format 97");
        Equal(96, GraphicsOf(10, true), "sRGB DXT1 maps to native format 96");
        Equal(101, GraphicsOf(12, false), "linear DXT5 maps to native format 101");
        Equal(100, GraphicsOf(12, true), "sRGB DXT5 maps to native format 100");
        Equal(106, GraphicsOf(24, false), "BC6H maps to native format 106");
        Equal(109, GraphicsOf(25, false), "linear BC7 maps to native format 109");
        Equal(108, GraphicsOf(25, true), "sRGB BC7 maps to native format 108");
        Equal(102, GraphicsOf(26, false), "linear BC4 maps to native format 102");
        Equal(102, GraphicsOf(26, true), "BC4 has no sRGB form");
        Equal(10, TextureOf(96), "sRGB DXT1 converts back to raw 10");
        Equal(10, TextureOf(97), "linear DXT1 converts back to raw 10");
        Equal(12, TextureOf(100), "sRGB DXT5 converts back to raw 12");
        Equal(12, TextureOf(101), "linear DXT5 converts back to raw 12");
        Equal(24, TextureOf(106), "native 106 converts back to BC6H");
        Equal(25, TextureOf(108), "sRGB native BC7 converts back to raw 25");
        Equal(25, TextureOf(109), "linear native BC7 converts back to raw 25");
        Equal(26, TextureOf(102), "native BC4 converts back to raw 26");
        var bc7 = (GraphicsFormat)109;
        Equal(16, (int)GraphicsFormatUtility.GetBlockSize(bc7), "BC7 has a 16-byte block");
        Equal(4, (int)GraphicsFormatUtility.GetBlockWidth(bc7), "BC7 has a four-texel block width");
        Equal(4, (int)GraphicsFormatUtility.GetBlockHeight(bc7), "BC7 has a four-texel block height");
        var bc4 = (GraphicsFormat)102;
        Equal(8, (int)GraphicsFormatUtility.GetBlockSize(bc4), "BC4 has an eight-byte block");
        Equal(1, (int)GraphicsFormatUtility.GetComponentCount(bc4), "BC4 stores one component");
    }

    private static void WideTextureNumbersConvertAndAllocate()
    {
        var wide = new[] { (72, GraphicsFormat.R16G16_UNorm, 2),
            (73, GraphicsFormat.R16G16B16_UNorm, 3), (74, GraphicsFormat.R16G16B16A16_UNorm, 4) };
        foreach (var (raw, graphics, channels) in wide)
        {
            Equal((int)graphics, GraphicsOf(raw, false), $"raw {raw} converts to its ushort layout");
            Equal((int)graphics, GraphicsOf(raw, true), $"raw {raw} has no sRGB variant");
            Equal(raw, TextureOf((int)graphics), $"{graphics} converts back to raw {raw}");
            Equal(channels, (int)GraphicsFormatUtility.GetComponentCount(graphics), $"raw {raw} channel count");
            Equal(6 * channels * 2, AllocatedBytes(raw, 3, 2), $"raw {raw} stores {channels} ushorts per texel");
        }
        var signed = new[] { (75, GraphicsFormat.R8_SNorm, 1),
            (76, GraphicsFormat.R8G8_SNorm, 2), (77, GraphicsFormat.R8G8B8_SNorm, 3),
            (78, GraphicsFormat.R8G8B8A8_SNorm, 4), (79, GraphicsFormat.R16_SNorm, 2) };
        foreach (var (raw, graphics, bytes) in signed)
        {
            Equal((int)graphics, GraphicsOf(raw, false), $"raw {raw} converts to its signed layout");
            Equal(raw, TextureOf((int)graphics), $"{graphics} converts back to raw {raw}");
            Equal(6 * bytes, AllocatedBytes(raw, 3, 2), $"raw {raw} stores {bytes} bytes per texel");
        }
    }

    private static void HdrAstcNumbersCarryBlockLayouts()
    {
        // A 36x36 texture needs a different block count for every block width, so each raw number is identified
        // by its allocation size as well as by the block dimensions of its graphics format.
        var layouts = new[] { (66, 4, 1296), (67, 5, 1024), (68, 6, 576),
            (69, 8, 400), (70, 10, 256), (71, 12, 144) };
        foreach (var (raw, block, bytes) in layouts)
        {
            var graphics = (GraphicsFormat)GraphicsOf(raw, false);
            Equal(block, (int)GraphicsFormatUtility.GetBlockWidth(graphics), $"raw {raw} block width");
            Equal(block, (int)GraphicsFormatUtility.GetBlockHeight(graphics), $"raw {raw} block height");
            Equal(16, (int)GraphicsFormatUtility.GetBlockSize(graphics), $"raw {raw} block bytes");
            Equal(bytes, AllocatedBytes(raw, 36, 36), $"raw {raw} level size");
            Equal(raw, TextureOf((int)graphics), $"raw {raw} converts back from its graphics format");
        }
    }

    // RG32_SIGNED, RGB48_SIGNED and RGBA64_SIGNED have byte sizes but no GraphicsFormat conversion, so the
    // engine refuses them at the conversion and allocation boundary. This pins the refusal, not support.
    private static void SignedWideTexturesAreRefused()
    {
        Equal(80, (int)TextureFormat.RG32_SIGNED, "two signed short channels");
        foreach (var raw in new[] { 80, 81, 82 })
        {
            Refused(() => GraphicsFormatUtility.GetGraphicsFormat((TextureFormat)raw, false), $"raw {raw} conversion");
            Refused(() => new Texture2D(2, 2, (TextureFormat)raw, false), $"raw {raw} allocation");
        }
    }

    private static void RawRenderTextureNumbersConvert()
    {
        Equal((int)GraphicsFormat.R8G8_UNorm, RenderGraphicsOf(25, false), "raw 25 is a two byte target");
        Equal((int)GraphicsFormat.R8G8_SRGB, RenderGraphicsOf(25, true), "raw 25 has an sRGB form");
        Equal((int)GraphicsFormat.R16_UNorm, RenderGraphicsOf(28, false), "raw 28 is a one ushort target");
        Equal((int)GraphicsFormat.R16_UNorm, RenderGraphicsOf(28, true), "raw 28 has no sRGB form");
        Equal(25, (int)GraphicsFormatUtility.GetRenderTextureFormat(GraphicsFormat.R8G8_UNorm), "two byte target number");
        Equal(25, (int)GraphicsFormatUtility.GetRenderTextureFormat(GraphicsFormat.R8G8_SRGB), "sRGB two byte target number");
        Equal(28, (int)GraphicsFormatUtility.GetRenderTextureFormat(GraphicsFormat.R16_UNorm), "one ushort target number");
        Equal(2, (int)GraphicsFormatUtility.GetComponentCount((GraphicsFormat)RenderGraphicsOf(25, false)),
            "raw 25 has two channels");
        Equal(1, (int)GraphicsFormatUtility.GetComponentCount((GraphicsFormat)RenderGraphicsOf(28, false)),
            "raw 28 has one channel");
    }

    private static void RenderAndDisplaySelectors()
    {
        Equal(2, (int)MeshTopology.Quads, "quad topology skips the reserved value");
        Equal(5, (int)MeshTopology.Points, "point topology");
        Equal(4, (int)SkinQuality.Bone4, "four bone skinning");
        Equal(-1, (int)CubemapFace.Unknown, "unspecified cube face sentinel");
        Equal(5, (int)CubemapFace.NegativeZ, "last cube face");
        Equal(4, (int)LightmapBakeType.Realtime, "realtime light bake selector");
        Equal(2, (int)LightmapBakeType.Baked, "baked light selector");
        Equal(1, (int)LightmapBakeType.Mixed, "mixed light selector");
        Equal(7, (int)(RenderTextureMemoryless.Color | RenderTextureMemoryless.Depth | RenderTextureMemoryless.MSAA),
            "memoryless attachment mask");
        Equal(17, (int)(AdditionalCanvasShaderChannels.TexCoord1 | AdditionalCanvasShaderChannels.Tangent),
            "extra canvas stream mask");
        Equal(3, (int)StereoTargetEyeMask.Both, "both stereo targets");
        Equal(3, (int)VRTextureUsage.DeviceSpecific, "device-specific target usage");
        Equal(3, (int)FullScreenMode.Windowed, "windowed display selector");
        Equal(2, (int)CursorLockMode.Confined, "confined cursor selector");
        Equal(3, (int)DeviceType.Desktop, "desktop device selector");
        Equal(3, (int)OperatingSystemFamily.Linux, "Linux platform family");
        Equal(4, (int)BatteryStatus.Full, "full battery status");
        Equal(3, (int)TouchScreenKeyboardType.URL, "URL keyboard selector");
        Equal(13, (int)AvatarMaskBodyPart.LastBodyPart, "body mask bound");
        if (((DepthTextureMode)5).ToString() == "5" || ((ComputeBufferType)17).ToString() == "17")
            throw new InvalidOperationException("Combined depth and buffer masks must retain named flag formatting");
    }

    private static int GraphicsOf(int rawTexture, bool srgb) =>
        (int)GraphicsFormatUtility.GetGraphicsFormat((TextureFormat)rawTexture, srgb);

    private static int RenderGraphicsOf(int rawTarget, bool srgb) =>
        (int)GraphicsFormatUtility.GetGraphicsFormat((RenderTextureFormat)rawTarget, srgb);

    private static int TextureOf(int rawGraphics) =>
        (int)GraphicsFormatUtility.GetTextureFormat((GraphicsFormat)rawGraphics);

    private static int AllocatedBytes(int rawTexture, int width, int height) =>
        new Texture2D(width, height, (TextureFormat)rawTexture, false).GetRawTextureData().Length;

    private static void Refused(Action action, string context)
    {
        try
        {
            action();
        }
        catch (NotSupportedException)
        {
            return;
        }
        throw new InvalidOperationException($"{context}: expected NotSupportedException");
    }

    private static void Equal(int expected, int actual, string context)
    {
        if (actual != expected)
            throw new InvalidOperationException($"{context}: expected {expected}, got {actual}");
    }
}
