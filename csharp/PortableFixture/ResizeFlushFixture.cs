using UnityEngine;
using UnityEngine.Rendering;
using Disunity.Hosting;
namespace PortableFixture;

/// <summary>Flushes the screen command buffer through a managed texture readback on the first update.</summary>
public sealed class ResizeFlushFixture : MonoBehaviour
{
    private const int Side = 4;
    private static readonly Color Cleared = new(1f, 1f, 0f, 1f);
    private CommandBuffer _screen = null!;
    private Texture2D _source = null!;
    private RenderTexture _drawn = null!;
    private Texture2D _target = null!;
    private int _updates;

    public static void Create()
    {
        var pass = DisunityHost.CreateGameObject().AddComponent<ResizeFlushFixture>();
        pass._screen = new CommandBuffer();
        pass._screen.SetRenderTarget(BuiltinRenderTextureType.CameraTarget);
        pass._screen.ClearRenderTarget(true, true, Cleared);
        pass._source = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
        pass._drawn = new RenderTexture(Side, Side, 0);
        pass._target = new Texture2D(Side, Side, TextureFormat.RGBA32, false, true);
    }

    private void Update()
    {
        _updates++;
        Graphics.ExecuteCommandBuffer(_screen);
        if (_updates == 2) Flush();
    }

    private void Flush()
    {
        _source.SetPixel(0, 0, Cleared);
        _source.Apply();
        Graphics.Blit(_source, _drawn);
        var previous = RenderTexture.active;
        RenderTexture.active = _drawn;
        _target.ReadPixels(new Rect(0, 0, Side, Side), 0, 0);
        RenderTexture.active = previous;
        _target.Apply();
    }
}
