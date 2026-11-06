using System.Runtime.CompilerServices;

namespace UnityEngine.Rendering;

/// <summary>What the level of detail of a camera is chosen from. Plain data.</summary>
public struct LODParameters : IEquatable<LODParameters>
{
    public bool isOrthographic { get; set; }
    public Vector3 cameraPosition { get; set; }
    public float fieldOfView { get; set; }
    public float orthoSize { get; set; }
    public int cameraPixelHeight { get; set; }

    public readonly bool Equals(LODParameters other) =>
        isOrthographic == other.isOrthographic && cameraPosition.Equals(other.cameraPosition) && fieldOfView.Equals(other.fieldOfView)
        && orthoSize.Equals(other.orthoSize) && cameraPixelHeight == other.cameraPixelHeight;

    public override readonly bool Equals(object? obj) => obj is LODParameters other && Equals(other);
    public override readonly int GetHashCode() => HashCode.Combine(isOrthographic, cameraPosition, fieldOfView, orthoSize, cameraPixelHeight);
    public static bool operator ==(LODParameters left, LODParameters right) => left.Equals(right);
    public static bool operator !=(LODParameters left, LODParameters right) => !left.Equals(right);
}

[InlineArray(ScriptableCullingParameters.LayerCount)]
internal struct LayerDistances
{
    private float _first;
}

/// <summary>
/// What culling a camera's view needs, filled by <see cref="Camera.TryGetCullingParameters(out ScriptableCullingParameters)"/>:
/// the layer mask, the planes of the view volume (up to 10), the combined projection and view matrix, the camera position, the
/// level of detail parameters, the shadow distance and one cull distance for each of the 32 layers. The values are plain data a
/// script may change before <see cref="ScriptableRenderContext.Cull"/>, which sends them to the world. A value that did not come
/// from a camera cannot be culled.
/// </summary>
public struct ScriptableCullingParameters : IEquatable<ScriptableCullingParameters>
{
    internal const int LayerCount = 32;

    private CullingPlanes _planes;
    private int _planeCount;
    private LayerDistances _layers;

    /// <summary>The camera the parameters were filled from; 0 for a value that came from none.</summary>
    internal ulong CameraHandle;

    public uint cullingMask { get; set; }
    public ulong sceneMask { get; set; }
    public bool isOrthographic { get; set; }
    public LODParameters lodParameters { get; set; }
    public Matrix4x4 cullingMatrix { get; set; }
    public Vector3 origin { get; set; }
    public float shadowDistance { get; set; }
    public CullingOptions cullingOptions { get; set; }
    public ReflectionProbeSortingCriteria reflectionProbeSortingCriteria { get; set; }
    public Matrix4x4 stereoViewMatrix { get; set; }
    public Matrix4x4 stereoProjectionMatrix { get; set; }
    public float stereoSeparationDistance { get; set; }
    public float accurateOcclusionThreshold { get; set; }
    public int maximumPortalCullingJobs { get; set; }
    public bool conservativeEnclosingSphere { get; set; }
    public int numIterationsEnclosingSphere { get; set; }

    /// <summary>How many planes are in use, from 0 to 10; a larger value throws <see cref="ArgumentException"/>.</summary>
    public int cullingPlaneCount
    {
        readonly get => _planeCount;
        set => _planeCount = value is >= 0 and <= ShadowSplitData.MaxCullingPlanes
            ? value : throw new ArgumentException($"culling plane count {value} is outside 0 to {ShadowSplitData.MaxCullingPlanes}", nameof(value));
    }

    public readonly Plane GetCullingPlane(int index) => _planes[CheckPlane(index)];
    public void SetCullingPlane(int index, Plane plane) => _planes[CheckPlane(index)] = plane;

    /// <summary>The number of layers a cull distance is held for: 32.</summary>
    public readonly int layerCount => LayerCount;

    /// <summary>The distance beyond which renderers of the layer are culled; 0 means the far plane.</summary>
    public readonly float GetLayerCullingDistance(int layerIndex) => _layers[CheckLayer(layerIndex)];

    public void SetLayerCullingDistance(int layerIndex, float distance) => _layers[CheckLayer(layerIndex)] = distance;

    private readonly int CheckPlane(int index) =>
        (uint)index < (uint)_planeCount ? index : throw new IndexOutOfRangeException($"culling plane {index} is outside the {_planeCount} in use");

    private static int CheckLayer(int index) =>
        (uint)index < LayerCount ? index : throw new IndexOutOfRangeException($"layer {index} is outside 0 to {LayerCount - 1}");

    public readonly bool Equals(ScriptableCullingParameters other)
    {
        if (!SameSettings(other)) return false;
        for (var index = 0; index < _planeCount; index++)
            if (_planes[index].normal != other._planes[index].normal || _planes[index].distance != other._planes[index].distance) return false;
        for (var layer = 0; layer < LayerCount; layer++)
            if (!_layers[layer].Equals(other._layers[layer])) return false;
        return true;
    }

    private readonly bool SameSettings(ScriptableCullingParameters other) =>
        _planeCount == other._planeCount && CameraHandle == other.CameraHandle && cullingMask == other.cullingMask && sceneMask == other.sceneMask
        && isOrthographic == other.isOrthographic && lodParameters.Equals(other.lodParameters) && cullingMatrix.Equals(other.cullingMatrix)
        && origin.Equals(other.origin) && shadowDistance.Equals(other.shadowDistance) && cullingOptions == other.cullingOptions
        && reflectionProbeSortingCriteria == other.reflectionProbeSortingCriteria && stereoViewMatrix.Equals(other.stereoViewMatrix)
        && stereoProjectionMatrix.Equals(other.stereoProjectionMatrix) && stereoSeparationDistance.Equals(other.stereoSeparationDistance)
        && accurateOcclusionThreshold.Equals(other.accurateOcclusionThreshold) && maximumPortalCullingJobs == other.maximumPortalCullingJobs
        && conservativeEnclosingSphere == other.conservativeEnclosingSphere && numIterationsEnclosingSphere == other.numIterationsEnclosingSphere;

    public override readonly bool Equals(object? obj) => obj is ScriptableCullingParameters other && Equals(other);
    public override readonly int GetHashCode() => HashCode.Combine(_planeCount, CameraHandle, cullingMask, cullingMatrix, origin, shadowDistance, cullingOptions);
    public static bool operator ==(ScriptableCullingParameters left, ScriptableCullingParameters right) => left.Equals(right);
    public static bool operator !=(ScriptableCullingParameters left, ScriptableCullingParameters right) => !left.Equals(right);
}
