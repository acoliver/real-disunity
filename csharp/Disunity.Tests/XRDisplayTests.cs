using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.XR;

/// <summary>The XR display subsystem: absent from the registry, with data structs and device calls that throw.</summary>
internal static class XRDisplayTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            RegistryHasNoDisplay();
            DescriptorHasNoDevice();
            InstanceMembersNeedADevice();
            RenderPassIsData();
            MirrorBlitIsData();
            Constants();
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: XRDisplaySubsystem is absent from the registry and its device members throw NotSupportedException");
    }

    private static void RegistryHasNoDisplay()
    {
        var displays = new List<XRDisplaySubsystem> { new() };
        SubsystemManager.GetSubsystems(displays);
        Expect.Equal(0, displays.Count, "XR display subsystems");
        var descriptors = new List<XRDisplaySubsystemDescriptor> { new() };
        SubsystemManager.GetSubsystemDescriptors(descriptors);
        Expect.Equal(0, descriptors.Count, "XR display descriptors");
        var instances = new List<XRDisplaySubsystem>();
        SubsystemManager.GetInstances(instances);
        Expect.Equal(0, instances.Count, "XR display instances");
    }

    private static void DescriptorHasNoDevice()
    {
        var descriptor = new XRDisplaySubsystemDescriptor();
        Expect.That(!descriptor.disablesLegacyVr && !descriptor.enableBackBufferMSAA, "descriptor flags");
        Expect.Equal(string.Empty, descriptor.id, "id");
        var error = Expect.Catch<NotSupportedException>(() => descriptor.Create(), "Create");
        Expect.That(error.Message.Contains("XRDisplaySubsystemDescriptor.Create"), error.Message);
    }

    private static void InstanceMembersNeedADevice()
    {
        var display = new XRDisplaySubsystem();
        Expect.That(!display.running, "never running");
        var camera = new GameObject("xr").AddComponent<Camera>();
        var calls = new (string Name, Action Call)[]
        {
            ("zNear", () => _ = display.zNear), ("zNear", () => display.zNear = 1),
            ("zFar", () => _ = display.zFar), ("zFar", () => display.zFar = 1),
            ("sRGB", () => _ = display.sRGB), ("sRGB", () => display.sRGB = true),
            ("occlusionMaskScale", () => _ = display.occlusionMaskScale), ("occlusionMaskScale", () => display.occlusionMaskScale = 1),
            ("singlePassRenderingDisabled", () => _ = display.singlePassRenderingDisabled),
            ("singlePassRenderingDisabled", () => display.singlePassRenderingDisabled = true),
            ("disableLegacyRenderer", () => _ = display.disableLegacyRenderer),
            ("disableLegacyRenderer", () => display.disableLegacyRenderer = true),
            ("contentProtectionEnabled", () => _ = display.contentProtectionEnabled),
            ("contentProtectionEnabled", () => display.contentProtectionEnabled = true),
            ("foveatedRenderingLevel", () => _ = display.foveatedRenderingLevel),
            ("foveatedRenderingLevel", () => display.foveatedRenderingLevel = 1),
            ("textureLayout", () => _ = display.textureLayout),
            ("textureLayout", () => display.textureLayout = XRDisplaySubsystem.TextureLayout.SingleTexture2D),
            ("reprojectionMode", () => _ = display.reprojectionMode),
            ("reprojectionMode", () => display.reprojectionMode = XRDisplaySubsystem.ReprojectionMode.None),
            ("supportedTextureLayouts", () => _ = display.supportedTextureLayouts),
            ("displayOpaque", () => _ = display.displayOpaque),
            ("scaleOfAllViewports", () => display.scaleOfAllViewports = 1),
            ("scaleOfAllRenderTargets", () => display.scaleOfAllRenderTargets = 1),
            ("GetRenderPassCount", () => display.GetRenderPassCount()),
            ("GetRenderPass", () => display.GetRenderPass(0, out _)),
            ("GetCullingParameters", () => display.GetCullingParameters(camera, 0, out _)),
            ("GetRenderTextureForRenderPass", () => display.GetRenderTextureForRenderPass(0)),
            ("GetMirrorViewBlitDesc", () => display.GetMirrorViewBlitDesc(null!, out _)),
            ("GetMirrorViewBlitDesc", () => display.GetMirrorViewBlitDesc(null!, out _, XRDisplaySubsystem.XRMirrorViewBlitMode.SideBySide)),
            ("AddGraphicsThreadMirrorViewBlit", () => display.AddGraphicsThreadMirrorViewBlit(null!, true)),
            ("AddGraphicsThreadMirrorViewBlit", () => display.AddGraphicsThreadMirrorViewBlit(null!, true, 0)),
            ("GetPreferredMirrorBlitMode", () => display.GetPreferredMirrorBlitMode()),
            ("SetPreferredMirrorBlitMode", () => display.SetPreferredMirrorBlitMode(0)),
            ("GetFocusPlane", () => display.GetFocusPlane(out _, out _, out _)),
            ("SetFocusPlane", () => display.SetFocusPlane(Vector3.zero, Vector3.up, Vector3.zero)),
            ("SetMSAALevel", () => display.SetMSAALevel(2)), ("GetMSAALevel", () => display.GetMSAALevel()),
            ("MarkTransformLateLatched", () => display.MarkTransformLateLatched(camera.transform, XRDisplaySubsystem.LateLatchNode.Head)),
        };
        foreach (var (name, call) in calls)
        {
            var error = Expect.Catch<NotSupportedException>(call, name);
            Expect.That(error.Message.Contains($"XRDisplaySubsystem.{name}:") && error.Message.Contains("no XR device"), error.Message);
        }
        Expect.Throws<NotSupportedException>(display.Start, "Start");
    }

    private static void RenderPassIsData()
    {
        var pass = new XRDisplaySubsystem.XRRenderPass
        {
            renderPassIndex = 2, cullingPassIndex = 3, hasMotionVectorPass = true, shouldFillOutDepth = true,
            renderTargetDesc = new RenderTextureDescriptor(8, 4),
        };
        Expect.Equal(2, pass.renderPassIndex, "renderPassIndex");
        Expect.Equal(3, pass.cullingPassIndex, "cullingPassIndex");
        Expect.That(pass.hasMotionVectorPass && pass.shouldFillOutDepth, "flags");
        Expect.Equal(8, pass.renderTargetDesc.width, "renderTargetDesc");
        Expect.Equal(0, pass.GetRenderParameterCount(), "a pass has no render parameters");
        var camera = new GameObject("pass").AddComponent<Camera>();
        Expect.Throws<ArgumentOutOfRangeException>(() => pass.GetRenderParameter(camera, 0, out _), "index 0");
        var parameter = new XRDisplaySubsystem.XRRenderParameter
        {
            view = Matrix4x4.identity, projection = Matrix4x4.identity, viewport = new Rect(0, 0, 1, 1),
            textureArraySlice = 1, isPreviousViewValid = true,
        };
        Expect.That(parameter.viewport == new Rect(0, 0, 1, 1) && parameter.isPreviousViewValid, "XRRenderParameter data");
        Expect.Equal(1, parameter.textureArraySlice, "textureArraySlice");
        Expect.That(default(XRDisplaySubsystem.XRRenderParameter).occlusionMesh == null, "no occlusion mesh by default");
    }

    private static void MirrorBlitIsData()
    {
        var desc = new XRDisplaySubsystem.XRMirrorViewBlitDesc { nativeBlitAvailable = true, nativeBlitInvalidStates = true, blitParamsCount = 1 };
        Expect.That(desc.nativeBlitAvailable && desc.nativeBlitInvalidStates, "desc flags");
        Expect.Equal(1, desc.blitParamsCount, "blitParamsCount");
        Expect.Throws<ArgumentOutOfRangeException>(() => desc.GetBlitParameter(0, out _), "no blit parameters are held");
        var blit = new XRDisplaySubsystem.XRBlitParams { srcTexArraySlice = 1, srcRect = new Rect(0, 0, 2, 2), destRect = new Rect(1, 1, 3, 3) };
        Expect.That(blit.srcRect.width == 2f && blit.destRect.x == 1f && blit.srcTex == null, "XRBlitParams data");
        Expect.Equal(IntPtr.Zero, blit.foveatedRenderingInfo, "foveatedRenderingInfo");
    }

    private static void Constants()
    {
        Expect.Equal(0, XRDisplaySubsystem.XRMirrorViewBlitMode.None, "None");
        Expect.Equal(-1, XRDisplaySubsystem.XRMirrorViewBlitMode.Left, "Left");
        Expect.Equal(-2, XRDisplaySubsystem.XRMirrorViewBlitMode.Right, "Right");
        Expect.Equal(-3, XRDisplaySubsystem.XRMirrorViewBlitMode.SideBySide, "SideBySide");
        Expect.Equal(1, (int)XRDisplaySubsystem.TextureLayout.Texture2DArray, "Texture2DArray");
        Expect.Equal(2, (int)XRDisplaySubsystem.TextureLayout.SeparateTexture2Ds, "SeparateTexture2Ds");
        Expect.Equal(4, (int)XRDisplaySubsystem.TextureLayout.SingleTexture2D, "SingleTexture2D");
        Expect.Equal(3, (int)XRDisplaySubsystem.ReprojectionMode.OrientationOnly, "OrientationOnly");
        Expect.Equal(2, (int)XRDisplaySubsystem.LateLatchNode.RightHand, "RightHand");
    }
}
