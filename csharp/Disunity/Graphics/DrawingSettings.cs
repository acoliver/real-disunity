using System.Runtime.CompilerServices;

namespace UnityEngine.Rendering;

/// <summary>
/// How a draw call sorts: the camera's view matrix and position (copied when the settings are made), the sorting
/// criteria, the distance metric and the axis for a custom metric. Plain data; the engine does not sort draws yet.
/// </summary>
public struct SortingSettings : IEquatable<SortingSettings>
{
    public Matrix4x4 worldToCameraMatrix { get; set; }
    public Vector3 cameraPosition { get; set; }
    public Vector3 customAxis { get; set; }
    public SortingCriteria criteria { get; set; }
    public DistanceMetric distanceMetric { get; set; }

    public SortingSettings(Camera camera)
    {
        ArgumentNullException.ThrowIfNull(camera);
        worldToCameraMatrix = camera.worldToCameraMatrix;
        cameraPosition = camera.transform.position;
        customAxis = Vector3.zero;
        criteria = SortingCriteria.None;
        distanceMetric = DistanceMetric.Perspective;
    }

    public readonly bool Equals(SortingSettings other) =>
        worldToCameraMatrix.Equals(other.worldToCameraMatrix) && cameraPosition.Equals(other.cameraPosition)
        && customAxis.Equals(other.customAxis) && criteria == other.criteria && distanceMetric == other.distanceMetric;

    public override readonly bool Equals(object? obj) => obj is SortingSettings other && Equals(other);
    public override readonly int GetHashCode() => HashCode.Combine(worldToCameraMatrix, cameraPosition, customAxis, criteria, distanceMetric);
    public static bool operator ==(SortingSettings left, SortingSettings right) => left.Equals(right);
    public static bool operator !=(SortingSettings left, SortingSettings right) => !left.Equals(right);
}

[InlineArray(DrawingSettings.maxShaderPasses)]
internal struct ShaderPassNames
{
    private ShaderTagId _first;
}

/// <summary>
/// Which shader passes a draw call uses and how it is set up. Plain data; the engine does not draw with it yet.
/// A new value has the given pass name in slot 0, no name in the other slots, instancing on, dynamic batching
/// off, no per-object data and main light index -1.
/// </summary>
public struct DrawingSettings
{
    public const int maxShaderPasses = 16;

    private ShaderPassNames _passes;

    public SortingSettings sortingSettings { get; set; }
    public PerObjectData perObjectData { get; set; }
    public int mainLightIndex { get; set; }
    public bool enableDynamicBatching { get; set; }
    public bool enableInstancing { get; set; }
    public int lodCrossFadeStencilMask { get; set; }
    public Material? overrideMaterial { get; set; }
    public int overrideMaterialPassIndex { get; set; }
    public Shader? overrideShader { get; set; }
    public int overrideShaderPassIndex { get; set; }
    public Material? fallbackMaterial { get; set; }

    public DrawingSettings(ShaderTagId shaderPassName, SortingSettings sortingSettings)
    {
        this.sortingSettings = sortingSettings;
        mainLightIndex = -1;
        enableInstancing = true;
        for (var slot = 0; slot < maxShaderPasses; slot++) _passes[slot] = ShaderTagId.none;
        _passes[0] = shaderPassName;
    }

    public readonly ShaderTagId GetShaderPassName(int index) => _passes[CheckSlot(index)];
    public void SetShaderPassName(int index, ShaderTagId shaderPassName) => _passes[CheckSlot(index)] = shaderPassName;

    private static int CheckSlot(int index) =>
        (uint)index < maxShaderPasses ? index : throw new ArgumentOutOfRangeException(nameof(index), $"shader pass {index} is outside 0 to {maxShaderPasses - 1}");
}
