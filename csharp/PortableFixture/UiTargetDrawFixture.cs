using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

namespace PortableFixture;

/// <summary>A script that clears an 8 by 8 render texture to yellow with a command buffer in each of its first two Updates (the
/// first runs while the scene loads, the second in the first frame the host advances), under an overlay canvas whose one quad,
/// 32 pixels square at the canvas centre, shows that texture. A window of 64 pixels shows a yellow square over the stock scene.
/// The colour uses only the channel values 0 and 1, which survive any colour-space encoding between the clear and the window.</summary>
public sealed class UiTargetDrawFixture : MonoBehaviour
{
    private const int DrawingUpdates = 2;

    private CommandBuffer _buffer = null!;
    private int _updates;

    public static void Create()
    {
        var target = new RenderTexture(8, 8, 0);
        var pass = DisunityHost.CreateGameObject().AddComponent<UiTargetDrawFixture>();
        pass._buffer = new CommandBuffer();
        pass._buffer.SetRenderTarget(target);
        pass._buffer.ClearRenderTarget(true, true, new Color(1f, 1f, 0f, 1f));
        DisunityHost.CreateGameObject().AddComponent<Camera>().transform.position = new Vector3(0, 0, -10);
        var canvas = DisunityHost.CreateGameObject();
        var root = canvas.AddComponent<RectTransform>();
        canvas.AddComponent<Canvas>().renderMode = RenderMode.ScreenSpaceOverlay;
        var rect = DisunityHost.CreateGameObject().AddComponent<RectTransform>();
        rect.SetParent(root, false);
        rect.sizeDelta = new Vector2(32, 32);
        var mesh = new Mesh
        {
            vertices = [new(-16, -16, 0), new(-16, 16, 0), new(16, 16, 0), new(16, -16, 0)],
            uv = [new(0, 0), new(0, 1), new(1, 1), new(1, 0)],
            triangles = [0, 1, 2, 2, 3, 0],
        };
        var renderer = rect.gameObject.AddComponent<CanvasRenderer>();
        renderer.SetMesh(mesh);
        renderer.SetTexture(target);
    }

    private void Update()
    {
        _updates++;
        if (_updates <= DrawingUpdates) Graphics.ExecuteCommandBuffer(_buffer);
    }
}
