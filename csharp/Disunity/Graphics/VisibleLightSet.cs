using Disunity.Hosting;
using Unity.Collections;
using Unity.Collections.LowLevel.Unsafe;

namespace UnityEngine.Rendering;

/// <summary>
/// The lights one <see cref="ScriptableRenderContext.Cull"/> found visible, and the light index map a pipeline stored for
/// them. The lights are listed with the directional ones first, then the others, each group in the order the lights were
/// created. The list lives on the pinned object heap, so the <see cref="NativeArray{T}"/> over it keeps its address for as
/// long as the set is reachable; a <see cref="CullingResults"/> and every copy of it hold the set.
/// </summary>
internal sealed class VisibleLightSet
{
    private const int PlaneCount = 6;
    private const int NearPlane = 4;
    private const int FarPlane = 5;

    private int[]? _indexMap;

    private VisibleLightSet(VisibleLight[] lights, Light[] owners)
    {
        Lights = lights;
        Owners = owners;
    }

    /// <summary>The visible lights, in the order described on the class.</summary>
    public VisibleLight[] Lights { get; }

    /// <summary>The light objects of <see cref="Lights"/>. Each <see cref="VisibleLight"/> holds its light by instance ID
    /// only, which keeps the struct plain data; this array keeps the objects alive so the ID still finds them.</summary>
    public Light[] Owners { get; }

    public int Count => Lights.Length;

    /// <summary>A native array over the lights, valid while the set is reachable. An empty set gives the default array.</summary>
    public NativeArray<VisibleLight> AsNativeArray() =>
        Lights.Length == 0 ? default : NativeArrayUnsafeUtility.ConvertExistingDataToNativeArray<VisibleLight>(Lights, Allocator.None);

    /// <summary>A copy of the stored index map in a new array, or the identity map when none was stored.</summary>
    public NativeArray<int> GetIndexMap(Allocator allocator)
    {
        var map = new NativeArray<int>(Count, allocator, NativeArrayOptions.UninitializedMemory);
        for (var index = 0; index < Count; index++) map[index] = _indexMap is null ? index : _indexMap[index];
        return map;
    }

    public void SetIndexMap(NativeArray<int> map) => _indexMap = map.ToArray();

    /// <summary>Finds the lights the parameters see: an enabled light on an active GameObject whose culling mask shares a layer
    /// with the parameters' culling mask, and that is directional or whose sphere (position and range) is not wholly outside a
    /// plane of the culling matrix. A light lights only the renderers on its culling mask's layers, so one that shares no layer
    /// with the camera lights nothing the camera draws. The light's own GameObject layer does not hide it: a light on a layer
    /// the camera does not draw still lights what it draws, as Old World's scene lights do.</summary>
    public static VisibleLightSet Collect(in ScriptableCullingParameters parameters)
    {
        var planes = GeometryUtility.CalculateFrustumPlanes(parameters.cullingMatrix);
        var directional = new List<(Light Light, VisibleLight Data)>();
        var others = new List<(Light Light, VisibleLight Data)>();
        foreach (var light in DisunityHost.NativeComponentsOf<Light>())
        {
            if (!light || !light.isActiveAndEnabled || !LightsAnyLayerOf(light, parameters.cullingMask)) continue;
            var type = light.type;
            if (type == LightType.Directional) directional.Add((light, Describe(light, type, VisibleLightFlags.None)));
            else if (TrySphere(planes, light, out var flags)) others.Add((light, Describe(light, type, flags)));
        }
        var ordered = directional.Concat(others).ToArray();
        var lights = GC.AllocateUninitializedArray<VisibleLight>(ordered.Length, pinned: true);
        for (var index = 0; index < ordered.Length; index++) lights[index] = ordered[index].Data;
        return new VisibleLightSet(lights, ordered.Select(entry => entry.Light).ToArray());
    }

    private static bool LightsAnyLayerOf(Light light, uint cameraMask) => (unchecked((uint)light.cullingMask) & cameraMask) != 0;

    /// <summary>Whether the sphere at the light's position, with its range as radius, is outside none of the planes, and the
    /// near and far planes it crosses. The plane normals point into the frustum, so a negative distance beyond the radius is outside.</summary>
    private static bool TrySphere(Plane[] planes, Light light, out VisibleLightFlags flags)
    {
        flags = VisibleLightFlags.None;
        var (center, radius) = (light.transform.position, light.range);
        for (var index = 0; index < PlaneCount; index++)
        {
            var distance = planes[index].GetDistanceToPoint(center);
            if (distance < -radius) return false;
            if (distance >= radius) continue;
            if (index == NearPlane) flags |= VisibleLightFlags.IntersectsNearPlane;
            else if (index == FarPlane) flags |= VisibleLightFlags.IntersectsFarPlane;
        }
        return true;
    }

    private static VisibleLight Describe(Light light, LightType type, VisibleLightFlags flags) => new()
    {
        lightType = type,
        finalColor = FinalColor(light),
        screenRect = new Rect(0f, 0f, 1f, 1f),
        localToWorldMatrix = light.transform.localToWorldMatrix,
        range = light.range,
        spotAngle = light.spotAngle,
        light = light,
        flags = flags,
        occlusionMaskChannel = -1,
    };

    /// <summary>The light's color times its intensity, with the color converted to linear first when the project is in linear
    /// color space and lights use linear intensity. Alpha is the light's own.</summary>
    private static Color FinalColor(Light light)
    {
        var color = light.color;
        if (QualitySettings.activeColorSpace == ColorSpace.Linear && GraphicsSettings.lightsUseLinearIntensity) color = color.linear;
        var intensity = light.intensity;
        return new Color(color.r * intensity, color.g * intensity, color.b * intensity, color.a);
    }
}
