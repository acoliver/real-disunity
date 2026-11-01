namespace UnityEngine;

/// <summary>One vertex of a UI mesh.</summary>
public struct UIVertex
{
    public Vector3 position;
    public Vector3 normal;
    public Vector4 tangent;
    public Color32 color;
    public Vector4 uv0;
    public Vector4 uv1;
    public Vector4 uv2;
    public Vector4 uv3;

    /// <summary>A white vertex facing the camera: normal (0, 0, -1), tangent (1, 0, 0, -1), every other field zero. A field,
    /// because game code reads it as one; read-only so that no caller changes the shared value (the engine's choice).</summary>
    public static readonly UIVertex simpleVert = new()
    {
        color = new Color32(255, 255, 255, 255),
        normal = new Vector3(0f, 0f, -1f),
        tangent = new Vector4(1f, 0f, 0f, -1f),
    };
}
