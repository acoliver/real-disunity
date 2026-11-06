using Disunity.Hosting;
using UnityEngine;

namespace PortableFixture;

internal static class UiTriangleFixture
{
    internal static void Create()
    {
        var root = DisunityHost.CreateGameObject();
        var canvas = root.AddComponent<RectTransform>();
        root.AddComponent<Canvas>().sortingOrder = -3;
        var rect = DisunityHost.CreateGameObject().AddComponent<RectTransform>();
        rect.SetParent(canvas, false);
        rect.sizeDelta = new Vector2(32, 32);
        var mesh = new Mesh
        {
            vertices = [new(-8, -4, 0), new(8, -4, 0), new(0, 12, 0)],
            uv = [new(0.125f, 0.25f), new(0.375f, 0.5f), new(0.625f, 0.75f)],
            triangles = [0, 1, 2],
        };
        var renderer = rect.gameObject.AddComponent<CanvasRenderer>();
        renderer.SetMesh(mesh);
        renderer.SetColor(new Color(0.25f, 0.5f, 0.75f, 1));
        renderer.EnableRectClipping(new Rect(-8, -4, 16, 16));
        var texture = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
        texture.SetPixel(0, 0, Color.white);
        texture.Apply();
        renderer.SetTexture(texture);
        var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
        camera.transform.position = new Vector3(0, 0, -10);
    }
}
