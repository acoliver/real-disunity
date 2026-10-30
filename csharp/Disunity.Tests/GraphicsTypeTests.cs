using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>The value types that the static Graphics API takes and returns.</summary>
internal static class GraphicsTypeTests
{
    public static void Run()
    {
        Params();
        Setup();
        Fences();
        Probes();
        Console.WriteLine("PASS: RenderParams, RenderTargetSetup, GraphicsFence, LightProbeProxyVolume and the probe and stage enums hold their documented defaults");
    }

    private static void Params()
    {
        var material = new Material(Material.SpriteUnlit);
        var p = new RenderParams(material);
        Expect.That(ReferenceEquals(p.material, material), "material");
        Expect.That(p.matProps == null && p.camera == null && p.lightProbeProxyVolume == null, "no block, camera or volume");
        Expect.Equal(0, p.layer, "layer");
        Expect.Equal(1u, p.renderingLayerMask, "renderingLayerMask");
        Expect.Equal(0, p.rendererPriority, "rendererPriority");
        Expect.Equal(ShadowCastingMode.On, p.shadowCastingMode, "shadowCastingMode");
        Expect.That(p.receiveShadows, "receiveShadows");
        Expect.Equal(MotionVectorGenerationMode.Camera, p.motionVectorMode, "motionVectorMode");
        Expect.Equal(LightProbeUsage.Off, p.lightProbeUsage, "lightProbeUsage");
        Expect.Equal(ReflectionProbeUsage.Off, p.reflectionProbeUsage, "reflectionProbeUsage");
        Expect.Equal(new Vector3(10000f, 10000f, 10000f), p.worldBounds.size, "worldBounds size");
        Expect.Equal(Vector3.zero, p.worldBounds.center, "worldBounds center");
        p.layer = 5;
        p.shadowCastingMode = ShadowCastingMode.Off;
        Expect.That(p.layer == 5 && p.shadowCastingMode == ShadowCastingMode.Off, "fields are writable");
    }

    private static void Setup()
    {
        var color = default(RenderBuffer);
        var depth = default(RenderBuffer);
        var one = new RenderTargetSetup(color, depth);
        Expect.Equal(1, one.color.Length, "one color");
        Expect.Equal(0, one.mipLevel, "mipLevel");
        Expect.Equal(CubemapFace.Unknown, one.cubemapFace, "cubemapFace");
        Expect.Equal(0, one.depthSlice, "depthSlice");
        Expect.Equal(RenderBufferLoadAction.Load, one.depthLoad, "depthLoad");
        Expect.Equal(RenderBufferStoreAction.Store, one.depthStore, "depthStore");
        Expect.Equal(RenderBufferLoadAction.Load, one.colorLoad[0], "colorLoad");
        Expect.Equal(RenderBufferStoreAction.Store, one.colorStore[0], "colorStore");
        var many = new RenderTargetSetup(new[] { color, color }, depth, 2, CubemapFace.NegativeY, 3);
        Expect.Equal(2, many.color.Length, "two colors");
        Expect.Equal(2, many.colorLoad.Length, "two load actions");
        Expect.Equal(2, many.colorStore.Length, "two store actions");
        Expect.That(many.mipLevel == 2 && many.cubemapFace == CubemapFace.NegativeY && many.depthSlice == 3, "mip, face and slice");
        var shortForm = new RenderTargetSetup(color, depth, 4, CubemapFace.PositiveX);
        Expect.That(shortForm.mipLevel == 4 && shortForm.cubemapFace == CubemapFace.PositiveX, "mip and face");
        Expect.Equal(7, new RenderTargetSetup(color, depth, 7).mipLevel, "mip only");
        Expect.Equal(6, new RenderTargetSetup(new[] { color }, depth, 6).mipLevel, "array with mip");
        Expect.Throws<ArgumentNullException>(() => new RenderTargetSetup((RenderBuffer[])null!, depth), "null colors");
    }

    private static void Fences()
    {
        var fence = default(GraphicsFence);
        Expect.That(fence.passed, "a fence that was never created has nothing pending");
        Expect.Equal(GraphicsFenceType.AsyncQueueSynchronisation, fence.fenceType, "fenceType");
        Expect.Equal(0, (int)SynchronisationStage.VertexProcessing, "VertexProcessing");
        Expect.Equal(1, (int)SynchronisationStage.PixelProcessing, "PixelProcessing");
    }

    private static void Probes()
    {
        Expect.Equal(0, (int)ReflectionProbeUsage.Off, "Off");
        Expect.Equal(1, (int)ReflectionProbeUsage.BlendProbes, "BlendProbes");
        Expect.Equal(2, (int)ReflectionProbeUsage.BlendProbesAndSkybox, "BlendProbesAndSkybox");
        Expect.Equal(3, (int)ReflectionProbeUsage.Simple, "Simple");
        Expect.That(typeof(LightProbeProxyVolume).BaseType == typeof(Behaviour), "LightProbeProxyVolume is a Behaviour");
        var volume = new LightProbeProxyVolume();
        Expect.That(volume.enabled, "enabled by default");
        DisunityHost.LoadWorld();
        try
        {
            var go = DisunityHost.CreateGameObject();
            var error = Expect.Catch<NotSupportedException>(() => go.AddComponent<LightProbeProxyVolume>(), "AddComponent");
            Expect.That(error.Message.Contains("LightProbeProxyVolume"), error.Message);
        }
        finally { DisunityHost.UnloadWorld(); }
    }
}
