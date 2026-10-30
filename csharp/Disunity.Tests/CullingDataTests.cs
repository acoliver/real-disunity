using UnityEngine.Experimental.Rendering;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>VisibleLight, VisibleReflectionProbe, ShadowSplitData, CullingResults and AttachmentDescriptor.</summary>
internal static class CullingDataTests
{
    public static void Run()
    {
        Lights();
        Splits();
        Results();
        Attachments();
        Console.WriteLine("PASS: VisibleLight, VisibleReflectionProbe, ShadowSplitData, CullingResults and AttachmentDescriptor");
    }

    private static void Lights()
    {
        var light = new VisibleLight
        {
            lightType = LightType.Point, finalColor = new Color(1, 0.5f, 0.25f, 1), range = 7f, spotAngle = 30f,
            screenRect = new Rect(0, 0, 4, 4), flags = VisibleLightFlags.IntersectsFarPlane, occlusionMaskChannel = 2,
        };
        Expect.That(light.lightType == LightType.Point && light.range == 7f && light.spotAngle == 30f, "light values read back");
        Expect.That(light.flags == VisibleLightFlags.IntersectsFarPlane && light.occlusionMaskChannel == 2 && light.light == null, "flags, channel and no light object");
        var same = light;
        Expect.That(light == same && !(light != same) && light.GetHashCode() == same.GetHashCode(), "equal lights");
        same.range = 8f;
        Expect.That(light != same, "range takes part in equality");
        same = light;
        same.finalColor = Color.white;
        Expect.That(light != same, "color takes part in equality");
        same = light;
        same.flags = VisibleLightFlags.None;
        Expect.That(light != same, "flags take part in equality");
        Expect.That(default(VisibleLight).Equals((object)default(VisibleLight)) && !light.Equals("x"), "object equality");

        var probe = new VisibleReflectionProbe { importance = 3, blendDistance = 2f, isBoxProjection = true, center = new Vector3(1, 1, 1), bounds = new Bounds(Vector3.zero, Vector3.one) };
        Expect.That(probe.importance == 3 && probe.blendDistance == 2f && probe.isBoxProjection && probe.texture == null, "probe values read back");
        var other = probe;
        Expect.That(probe == other && probe.GetHashCode() == other.GetHashCode(), "equal probes");
        other.importance = 4;
        Expect.That(probe != other, "importance takes part in equality");
        other = probe;
        other.bounds = new Bounds(Vector3.one, Vector3.one);
        Expect.That(probe != other, "bounds take part in equality");
    }

    private static void Splits()
    {
        var split = new ShadowSplitData { cullingSphere = new Vector4(1, 2, 3, 4), cullingNearPlane = 0.5f, shadowCascadeBlendCullingFactor = 0.8f };
        Expect.Equal(0, split.cullingPlaneCount, "no planes at first");
        Expect.Throws<IndexOutOfRangeException>(() => split.GetCullingPlane(0), "a plane outside the count");
        split.cullingPlaneCount = 2;
        var plane = new Plane(Vector3.up, 3f);
        split.SetCullingPlane(1, plane);
        Expect.Equal(Vector3.up, split.GetCullingPlane(1).normal, "plane normal");
        Expect.Near(3f, split.GetCullingPlane(1).distance, "plane distance");
        Expect.Throws<IndexOutOfRangeException>(() => split.SetCullingPlane(2, plane), "plane 2 of 2");
        Expect.Throws<IndexOutOfRangeException>(() => split.GetCullingPlane(-1), "plane -1");
        Expect.Throws<ArgumentException>(() => split.cullingPlaneCount = 11, "11 planes");
        Expect.Throws<ArgumentException>(() => split.cullingPlaneCount = -1, "negative planes");
        split.cullingPlaneCount = 10;
        split.SetCullingPlane(9, plane);
        Expect.Equal(10, split.cullingPlaneCount, "ten planes");
        split.cullingPlaneCount = 2;

        var copy = split;
        Expect.That(copy == split && copy.GetHashCode() == split.GetHashCode() && copy.Equals((object)split), "equal splits");
        copy.SetCullingPlane(1, new Plane(Vector3.right, 3f));
        Expect.That(copy != split, "a plane takes part in equality");
        copy = split;
        copy.cullingSphere = Vector4.one;
        Expect.That(copy != split, "the sphere takes part in equality");
        copy = split;
        copy.cullingNearPlane = 1f;
        Expect.That(copy != split, "the near plane takes part in equality");
        copy = split;
        copy.cullingPlaneCount = 1;
        Expect.That(copy != split, "the plane count takes part in equality");
        copy = split;
        copy.cullingMatrix = Matrix4x4.Scale(new Vector3(2, 2, 2));
        Expect.That(copy != split, "the matrix takes part in equality");
    }

    private static void Results()
    {
        var results = default(CullingResults);
        Expect.Equal(0, results.visibleLights.Length, "no visible lights");
        Expect.Equal(0, results.visibleReflectionProbes.Length, "no visible probes");
        Expect.Equal(0, results.visibleOffscreenVertexLights.Length, "no offscreen vertex lights");
        Expect.That(results == default, "default results are equal");
        Throws(() => results.GetLightIndexMap(Unity.Collections.Allocator.Temp), "GetLightIndexMap");
        Throws(() => results.SetLightIndexMap(default), "SetLightIndexMap");
        Throws(() => results.GetReflectionProbeIndexMap(Unity.Collections.Allocator.Temp), "GetReflectionProbeIndexMap");
        Throws(() => results.SetReflectionProbeIndexMap(default), "SetReflectionProbeIndexMap");
        Throws(() => results.GetShadowCasterBounds(0, out _), "GetShadowCasterBounds");
        Throws(() => _ = results.lightIndexCount, "lightIndexCount");
        Throws(() => _ = results.reflectionProbeIndexCount, "reflectionProbeIndexCount");
        Throws(() => _ = results.lightAndReflectionProbeIndexCount, "lightAndReflectionProbeIndexCount");
        Throws(() => results.ComputeSpotShadowMatricesAndCullingPrimitives(0, out _, out _, out _), "ComputeSpotShadowMatricesAndCullingPrimitives");
        Throws(() => results.ComputePointShadowMatricesAndCullingPrimitives(0, CubemapFace.PositiveX, 0f, out _, out _, out _), "ComputePointShadowMatricesAndCullingPrimitives");
        Throws(() => results.ComputeDirectionalShadowMatricesAndCullingPrimitives(0, 0, 1, Vector3.zero, 512, 0f, out _, out _, out _), "ComputeDirectionalShadowMatricesAndCullingPrimitives");
    }

    private static void Throws(Action action, string member)
    {
        var error = Expect.Catch<NotSupportedException>(action, member);
        Expect.That(error.Message.Contains(member, StringComparison.Ordinal), $"the message names {member}: {error.Message}");
    }

    private static void Attachments()
    {
        var color = new AttachmentDescriptor(GraphicsFormat.R8G8B8A8_UNorm);
        Expect.Equal(GraphicsFormat.R8G8B8A8_UNorm, color.graphicsFormat, "format");
        Expect.That(color.loadAction == RenderBufferLoadAction.DontCare && color.storeAction == RenderBufferStoreAction.DontCare, "loads and stores nothing");
        Expect.That(color.loadStoreTarget == RenderTargetIdentifier.Invalid && color.resolveTarget == RenderTargetIdentifier.Invalid, "invalid targets");
        Expect.That(color.clearColor == Color.clear && color.clearDepth == 1f && color.clearStencil == 0, "default clear values");
        Expect.Equal(GraphicsFormat.R8G8B8A8_SRGB, new AttachmentDescriptor(RenderTextureFormat.ARGB32).graphicsFormat, "a render texture format maps to its sRGB format in the default linear color space");

        var target = new RenderTargetIdentifier(BuiltinRenderTextureType.CameraTarget);
        color.ConfigureTarget(target, true, true);
        Expect.That(color.loadStoreTarget == target && color.loadAction == RenderBufferLoadAction.Load && color.storeAction == RenderBufferStoreAction.Store, "load and store");
        color.ConfigureTarget(target, false, false);
        Expect.Equal(RenderBufferStoreAction.DontCare, color.storeAction, "not storing undoes Store");
        Expect.Equal(RenderBufferLoadAction.Load, color.loadAction, "not loading leaves the load action");

        color.ConfigureClear(new Color(1, 0, 0, 1), 0.5f, 7);
        Expect.That(color.loadAction == RenderBufferLoadAction.Clear && color.clearColor == new Color(1, 0, 0, 1) && color.clearDepth == 0.5f && color.clearStencil == 7, "clear values");
        color.ConfigureTarget(target, true, false);
        Expect.Equal(RenderBufferLoadAction.Clear, color.loadAction, "loading does not replace Clear");

        var resolve = new AttachmentDescriptor(GraphicsFormat.R8G8B8A8_UNorm);
        resolve.ConfigureResolveTarget(target);
        Expect.That(resolve.resolveTarget == target && resolve.storeAction == RenderBufferStoreAction.Resolve, "a resolve target resolves");
        resolve.ConfigureTarget(target, false, true);
        Expect.Equal(RenderBufferStoreAction.StoreAndResolve, resolve.storeAction, "storing after a resolve target stores and resolves");
        resolve.ConfigureTarget(target, false, false);
        Expect.Equal(RenderBufferStoreAction.Resolve, resolve.storeAction, "not storing keeps the resolve");
        var stored = new AttachmentDescriptor(GraphicsFormat.R8G8B8A8_UNorm);
        stored.ConfigureTarget(target, false, true);
        stored.ConfigureResolveTarget(target);
        Expect.Equal(RenderBufferStoreAction.StoreAndResolve, stored.storeAction, "a resolve target after Store stores and resolves");

        var copy = stored;
        Expect.That(copy == stored && copy.GetHashCode() == stored.GetHashCode() && copy.Equals((object)stored), "equal descriptors");
        copy.clearStencil = 1;
        Expect.That(copy != stored, "stencil takes part in equality");
        copy = stored;
        copy.graphicsFormat = GraphicsFormat.R32_SFloat;
        Expect.That(copy != stored, "format takes part in equality");
    }
}
