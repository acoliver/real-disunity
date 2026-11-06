using UnityEngine.Rendering;

namespace UnityEngine;

public sealed partial class Camera
{
    private bool CullingParametersFilled(out ScriptableCullingParameters parameters)
    {
        parameters = default;
        var view = worldToCameraMatrix;
        var projection = projectionMatrix;
        var combined = projection * view;
        if (pixelWidth <= 0 || pixelHeight <= 0 || !IsFinite(combined)) return false;
        var planes = GeometryUtility.CalculateFrustumPlanes(combined);
        parameters.CameraHandle = Handle;
        parameters.cullingPlaneCount = planes.Length;
        for (var index = 0; index < planes.Length; index++) parameters.SetCullingPlane(index, planes[index]);
        parameters.cullingMask = unchecked((uint)cullingMask);
        parameters.sceneMask = ulong.MaxValue;
        parameters.isOrthographic = orthographic;
        parameters.cullingMatrix = combined;
        parameters.origin = transform.position;
        parameters.shadowDistance = MathF.Min(QualitySettings.shadowDistance, farClipPlane);
        parameters.cullingOptions = CullingOptions.None;
        parameters.stereoViewMatrix = view;
        parameters.stereoProjectionMatrix = projection;
        parameters.accurateOcclusionThreshold = -1f;
        parameters.lodParameters = new LODParameters
        {
            isOrthographic = orthographic, cameraPosition = transform.position, fieldOfView = fieldOfView, orthoSize = orthographicSize,
            cameraPixelHeight = pixelHeight,
        };
        var distances = layerCullDistances;
        for (var layer = 0; layer < distances.Length; layer++) parameters.SetLayerCullingDistance(layer, distances[layer]);
        return true;
    }

    /// <summary>The view, projection and target a context records when a pipeline sets the camera up. A destroyed camera throws
    /// <see cref="InvalidOperationException"/>.</summary>
    internal (Matrix4x4 View, Matrix4x4 Projection, CommandTarget Target) SetupValues()
    {
        ThrowIfDestroyed();
        return (worldToCameraMatrix, projectionMatrix, CommandTarget.OfCamera(targetTexture));
    }

    private static bool IsFinite(Matrix4x4 matrix)
    {
        for (var index = 0; index < 16; index++)
            if (!float.IsFinite(matrix[index])) return false;
        return true;
    }
}
