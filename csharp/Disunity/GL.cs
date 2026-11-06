namespace UnityEngine;

/// <summary>
/// Low-level graphics state. The engine has no immediate-mode drawing, so the one member here is the projection conversion.
/// <see cref="GetGPUProjectionMatrix(Matrix4x4, bool)"/> returns the matrix unchanged: the engine keeps one convention for
/// every projection it records (camera space looks down negative Z, depth maps to -1 to 1, as <see cref="Camera.projectionMatrix"/>
/// has it) and the host that draws converts for its device (the engine's choice).
/// </summary>
public static class GL
{
    public static Matrix4x4 GetGPUProjectionMatrix(Matrix4x4 proj, bool renderIntoTexture) => proj;
}
