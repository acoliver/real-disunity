namespace UnityEngine.Rendering;

/// <summary>
/// Camera setup on a command buffer. The engine's choice is that <see cref="SetupCameraProperties(Camera)"/> records the camera's
/// view and projection the way <see cref="SetViewProjectionMatrices"/> does, read when the member is called, so the draws after
/// it in the buffer use the camera's matrices. It names no camera to the world, leaves the bound targets as they are and sets
/// no shader globals of its own. A null camera throws <see cref="ArgumentNullException"/> and a destroyed one
/// <see cref="InvalidOperationException"/>.
/// </summary>
public sealed partial class CommandBuffer
{
    public void SetupCameraProperties(Camera camera)
    {
        ThrowIfReleased();
        ArgumentNullException.ThrowIfNull(camera);
        var (view, projection, _) = camera.SetupValues();
        SetViewProjectionMatrices(view, projection);
    }
}
