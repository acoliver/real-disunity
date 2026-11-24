using UnityEngine;
using Disunity.Hosting;
namespace PortableFixture;

/// <summary>A UI image that shows a Texture2D the script fills from the GPU: it blits a one texel colored texture into a render
/// texture with no material, so no shader is involved, and reads the render texture back with ReadPixels, which flushes and waits for the host's answer. The first color
/// is read while the scene loads and the second in the fourth Update, so a host that answers flushes shows the first color
/// from the first frame and the second from a later one, and a host that does not leaves the texture at its zero texels. The
/// colors use only the channel values 0 and 1, which survive any color-space encoding between the blit and the screen.</summary>
public sealed class ReadbackFixture : MonoBehaviour
{
    private const int Side = 4;
    private const int SecondColorUpdate = 4;
    private static readonly Color First = new(1f, 1f, 0f, 1f);
    private static readonly Color Second = new(0f, 1f, 1f, 1f);

    private Texture2D _source = null!;
    private RenderTexture _drawn = null!;
    private Texture2D _target = null!;
    private int _updates;

    public static void Create()
    {
        var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
        camera.transform.position = new Vector3(0, 0, -10);
        var canvas = DisunityHost.CreateGameObject();
        var root = canvas.AddComponent<RectTransform>();
        canvas.AddComponent<Canvas>().renderMode = RenderMode.ScreenSpaceOverlay;
        var rect = DisunityHost.CreateGameObject().AddComponent<RectTransform>();
        rect.SetParent(root, false);
        rect.sizeDelta = new Vector2(32, 32);
        var holder = DisunityHost.CreateGameObject();
        var pass = holder.AddComponent<ReadbackFixture>();
        pass._source = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
        pass._drawn = new RenderTexture(Side, Side, 0);
        pass._target = new Texture2D(Side, Side, TextureFormat.RGBA32, false, true);
        var renderer = rect.gameObject.AddComponent<CanvasRenderer>();
        renderer.SetMesh(Quad());
        renderer.SetTexture(pass._target);
        pass.Read(First);
    }

    private void Update()
    {
        _updates++;
        if (_updates == SecondColorUpdate) Read(Second);
    }

    private void Read(Color color)
    {
        _source.SetPixel(0, 0, color);
        _source.Apply();
        Graphics.Blit(_source, _drawn);
        var previous = RenderTexture.active;
        RenderTexture.active = _drawn;
        _target.ReadPixels(new Rect(0, 0, Side, Side), 0, 0);
        RenderTexture.active = previous;
        _target.Apply();
    }

    private static Mesh Quad() => new()
    {
        vertices = [new(-16, -16, 0), new(16, -16, 0), new(16, 16, 0), new(-16, 16, 0)],
        uv = [new(0, 1), new(1, 1), new(1, 0), new(0, 0)],
        triangles = [0, 1, 2, 0, 2, 3],
    };
}
