using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>
/// Matrices and point conversions. Matrices use OpenGL conventions as in Unity: camera space looks
/// down negative Z, and depth maps to -1..1. The view matrix comes from the object's position and
/// rotation (scale is ignored, as Unity does), and a matrix set by a script replaces the derived one
/// until it is reset.
/// </summary>
public sealed partial class Camera
{
    private const uint ProjectionKind = 0;
    private const uint WorldToCameraKind = 1;

    private float? _aspectOverride;

    private unsafe Matrix4x4? ReadMatrix(uint kind)
    {
        var values = stackalloc float[16];
        uint isSet;
        DisunityHost.ThrowIfError(NativeBindings.disunity_camera_matrix_read(DisunityHost.WorldHandle, Handle, kind, values, &isSet));
        if (isSet == 0) return null;
        var matrix = new Matrix4x4();
        for (var i = 0; i < 16; i++) matrix[i] = values[i];
        return matrix;
    }

    private unsafe void WriteMatrix(uint kind, Matrix4x4? matrix)
    {
        if (matrix is not { } value)
        {
            DisunityHost.ThrowIfError(NativeBindings.disunity_camera_matrix_write(DisunityHost.WorldHandle, Handle, kind, null));
            return;
        }
        var values = stackalloc float[16];
        for (var i = 0; i < 16; i++) values[i] = value[i];
        DisunityHost.ThrowIfError(NativeBindings.disunity_camera_matrix_write(DisunityHost.WorldHandle, Handle, kind, values));
    }

    /// <summary>Width over height: the pixel rectangle's unless a script set it with the setter, until <see cref="ResetAspect"/>.</summary>
    public float aspect
    {
        get
        {
            if (_aspectOverride is { } set) return set;
            var size = pixelRect;
            return size.height > 0 ? size.width / size.height : 1f;
        }
        set
        {
            if (!float.IsFinite(value) || value <= 0) throw new ArgumentOutOfRangeException(nameof(value), "aspect must be a positive number");
            _aspectOverride = value;
        }
    }

    public void ResetAspect() => _aspectOverride = null;

    public Matrix4x4 projectionMatrix
    {
        get => ReadMatrix(ProjectionKind) ?? DerivedProjection();
        set => WriteMatrix(ProjectionKind, value);
    }

    /// <summary>The projection without jitter. The engine never jitters, so this is <see cref="projectionMatrix"/>, and setting it sets that.</summary>
    public Matrix4x4 nonJitteredProjectionMatrix
    {
        get => projectionMatrix;
        set => projectionMatrix = value;
    }

    public void ResetProjectionMatrix() => WriteMatrix(ProjectionKind, null);

    public Matrix4x4 worldToCameraMatrix
    {
        get => ReadMatrix(WorldToCameraKind) ?? DerivedView();
        set => WriteMatrix(WorldToCameraKind, value);
    }

    public void ResetWorldToCameraMatrix() => WriteMatrix(WorldToCameraKind, null);

    public Matrix4x4 cameraToWorldMatrix => worldToCameraMatrix.inverse;

    private Matrix4x4 DerivedProjection()
    {
        var c = Read();
        var ratio = aspect;
        if (c.orthographic != 1) return Matrix4x4.Perspective(c.field_of_view, ratio, c.near, c.far);
        var halfWidth = c.orthographic_size * ratio;
        return Matrix4x4.Ortho(-halfWidth, halfWidth, -c.orthographic_size, c.orthographic_size, c.near, c.far);
    }

    private Matrix4x4 DerivedView()
    {
        var pose = Matrix4x4.TRS(transform.position, transform.rotation, Vector3.one);
        return Matrix4x4.Scale(new Vector3(1f, 1f, -1f)) * pose.inverse;
    }

    private static void RequireMono(MonoOrStereoscopicEye eye, string member)
    {
        if (eye != MonoOrStereoscopicEye.Mono) throw new NotSupportedException($"Camera.{member} for the {eye} eye is not supported yet");
    }

    /// <summary>Viewport coordinates (0 to 1 across the rectangle) of a world point; z is its distance in front of the camera, negative behind it.</summary>
    public Vector3 WorldToViewportPoint(Vector3 position)
    {
        var view = worldToCameraMatrix;
        var clip = projectionMatrix * (view * new Vector4(position.x, position.y, position.z, 1f));
        var distance = -view.MultiplyPoint3x4(position).z;
        return new Vector3(clip.x / clip.w * 0.5f + 0.5f, clip.y / clip.w * 0.5f + 0.5f, distance);
    }

    public Vector3 WorldToViewportPoint(Vector3 position, MonoOrStereoscopicEye eye)
    {
        RequireMono(eye, nameof(WorldToViewportPoint));
        return WorldToViewportPoint(position);
    }

    public Vector3 WorldToScreenPoint(Vector3 position) => ViewportToScreenPoint(WorldToViewportPoint(position));

    public Vector3 WorldToScreenPoint(Vector3 position, MonoOrStereoscopicEye eye)
    {
        RequireMono(eye, nameof(WorldToScreenPoint));
        return WorldToScreenPoint(position);
    }

    public Vector3 ScreenToViewportPoint(Vector3 position)
    {
        var pixels = pixelRect;
        return new Vector3((position.x - pixels.x) / pixels.width, (position.y - pixels.y) / pixels.height, position.z);
    }

    public Vector3 ViewportToScreenPoint(Vector3 position)
    {
        var pixels = pixelRect;
        return new Vector3(pixels.x + position.x * pixels.width, pixels.y + position.y * pixels.height, position.z);
    }

    /// <summary>The world point at viewport (x, y) and z units in front of the camera; z of 0 is the camera's own plane.</summary>
    public Vector3 ViewportToWorldPoint(Vector3 position)
    {
        var projection = projectionMatrix;
        var onNear = projection.inverse * new Vector4(position.x * 2f - 1f, position.y * 2f - 1f, -1f, 1f);
        var near = new Vector3(onNear.x, onNear.y, onNear.z) / onNear.w;
        var inView = IsOrthographicProjection(projection)
            ? new Vector3(near.x, near.y, -position.z)
            : near * (position.z / -near.z);
        return cameraToWorldMatrix.MultiplyPoint3x4(inView);
    }

    public Vector3 ViewportToWorldPoint(Vector3 position, MonoOrStereoscopicEye eye)
    {
        RequireMono(eye, nameof(ViewportToWorldPoint));
        return ViewportToWorldPoint(position);
    }

    public Vector3 ScreenToWorldPoint(Vector3 position) => ViewportToWorldPoint(ScreenToViewportPoint(position));

    public Vector3 ScreenToWorldPoint(Vector3 position, MonoOrStereoscopicEye eye)
    {
        RequireMono(eye, nameof(ScreenToWorldPoint));
        return ScreenToWorldPoint(position);
    }

    // An orthographic projection leaves w at 1; a perspective one copies depth into w.
    private static bool IsOrthographicProjection(Matrix4x4 projection) => projection.m32 == 0f && projection.m33 != 0f;

    /// <summary>
    /// The ray through a viewport point. A perspective ray starts at the camera's position; an
    /// orthographic ray starts on the camera's plane at the point and runs along the camera's forward axis.
    /// </summary>
    public Ray ViewportPointToRay(Vector3 position)
    {
        var toWorld = cameraToWorldMatrix;
        if (IsOrthographicProjection(projectionMatrix))
            return new Ray(ViewportToWorldPoint(new Vector3(position.x, position.y, 0f)), toWorld.MultiplyVector(new Vector3(0f, 0f, -1f)));
        var origin = toWorld.GetPosition();
        return new Ray(origin, ViewportToWorldPoint(new Vector3(position.x, position.y, 1f)) - origin);
    }

    public Ray ViewportPointToRay(Vector3 position, MonoOrStereoscopicEye eye)
    {
        RequireMono(eye, nameof(ViewportPointToRay));
        return ViewportPointToRay(position);
    }

    public Ray ScreenPointToRay(Vector3 position) => ViewportPointToRay(ScreenToViewportPoint(position));

    public Ray ScreenPointToRay(Vector3 position, MonoOrStereoscopicEye eye)
    {
        RequireMono(eye, nameof(ScreenPointToRay));
        return ScreenPointToRay(position);
    }
}
