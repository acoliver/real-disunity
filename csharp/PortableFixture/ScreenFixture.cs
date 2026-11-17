using UnityEngine;
using UnityEngine.Rendering;
using Disunity.Hosting;
namespace PortableFixture;

/// <summary>A script that draws into the screen with a command buffer: it sets the camera target, which names the screen when no camera
/// renders, and clears it to yellow in each of its first two Updates (the first runs while the scene loads and the second in the
/// first frame the host advances), then leaves the screen alone. The colour uses only the channel values 0 and 1, which survive any
/// colour-space encoding between the clear and the window.</summary>
public sealed class ScreenFixture : MonoBehaviour
{
    private const int DrawingUpdates = 2;
    private static readonly Color Cleared = new(1f, 1f, 0f, 1f);

    private CommandBuffer _buffer = null!;
    private int _updates;

    public static void Create()
    {
        var pass = DisunityHost.CreateGameObject().AddComponent<ScreenFixture>();
        pass._buffer = new CommandBuffer();
        pass._buffer.SetRenderTarget(BuiltinRenderTextureType.CameraTarget);
        pass._buffer.ClearRenderTarget(true, true, Cleared);
    }

    private void Update()
    {
        _updates++;
        if (_updates <= DrawingUpdates) Graphics.ExecuteCommandBuffer(_buffer);
    }
}
