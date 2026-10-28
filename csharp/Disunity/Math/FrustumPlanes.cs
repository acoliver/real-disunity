namespace UnityEngine;

/// <summary>The six clipping planes of a perspective or orthographic projection: the left,
/// right, bottom and top extents on the near plane, and the near and far distances.</summary>
public struct FrustumPlanes
{
    public float left;
    public float right;
    public float bottom;
    public float top;
    public float zNear;
    public float zFar;
}
