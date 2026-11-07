using Disunity.Hosting;
using UnityEngine;

namespace PortableFixture;

internal static class RetainedUiFixture
{
    internal static void Create()
    {
        var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
        camera.transform.position = new Vector3(0, 0, -10);
        camera.orthographic = false;
        camera.fieldOfView = 90;
        Background();
        var canvas = DisunityHost.CreateGameObject();
        var root = canvas.AddComponent<RectTransform>();
        canvas.AddComponent<Canvas>().sortingOrder = 7;
        // Allocate the front child first: hierarchy, not identity, must determine overlap.
        var front = Rectangle(null, 32);
        var back = Rectangle(root, 48);
        Graphic(back, new Color(0, 0, 1, 1));
        var label = Rectangle(back, 32);
        var text = DisunityHost.CreateText(label.gameObject,
            DisunityHost.RegisterFont(SyntheticFont.Bytes()));
        text.text = "A";
        text.fontSize = 32;
        text.color = new Color(1, 1, 1, 1);
        front.SetParent(label, false);
        var graphic = Graphic(front, new Color(0, 1, 0, 1));
        graphic.EnableRectClipping(new Rect(-8, -8, 8, 8));
        var behind = Rectangle(null, 64);
        behind.gameObject.AddComponent<Canvas>().sortingOrder = -3;
        Graphic(Rectangle(behind, 56), new Color(1, 1, 0, 1));
        var spatial = Rectangle(null, 8);
        spatial.gameObject.AddComponent<Canvas>().renderMode = RenderMode.WorldSpace;
        Graphic(Rectangle(spatial, 8), new Color(1, 0, 1, 1));
        UnavailableGraphics(root);
    }

    private static void UnavailableGraphics(Transform root)
    {
        Texture[] unavailable = [new RenderTexture(1, 1, 0),
            new Texture2D(1, 1, TextureFormat.Alpha8, false),
            new Texture2D(1, 1, TextureFormat.RGBAHalf, false)];
        foreach (var texture in unavailable)
        {
            var graphic = Graphic(Rectangle(root, 64), new Color(1, 0, 1, 1));
            graphic.SetTexture(texture);
        }
    }

    private static void Background()
    {
        var shown = DisunityHost.CreateGameObject();
        shown.AddComponent<MeshFilter>().sharedMesh = new Mesh
        {
            vertices = [new(-20, -20, 0), new(20, -20, 0), new(20, 20, 0), new(-20, 20, 0)],
            triangles = [0, 2, 1, 0, 3, 2],
        };
        var material = new Material(DisunityHost.RegisterShader("Fixture/Plain"));
        material.SetColor("_Color", new Color(1, 0, 0, 1));
        shown.AddComponent<MeshRenderer>().sharedMaterial = material;
    }

    private static RectTransform Rectangle(Transform? parent, float size)
    {
        var rect = DisunityHost.CreateGameObject().AddComponent<RectTransform>();
        rect.sizeDelta = new Vector2(size, size);
        if (parent != null) rect.SetParent(parent, false);
        return rect;
    }

    private static CanvasRenderer Graphic(RectTransform rect, Color color)
    {
        var half = rect.sizeDelta.x / 2;
        var mesh = new Mesh
        {
            vertices = [new(-half, -half, 0), new(half, -half, 0),
                new(half, half, 0), new(-half, half, 0)],
            uv = [new(0, 1), new(1, 1), new(1, 0), new(0, 0)],
            triangles = [0, 1, 2, 0, 2, 3],
        };
        var texture = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
        texture.SetPixel(0, 0, color);
        texture.Apply();
        var renderer = rect.gameObject.AddComponent<CanvasRenderer>();
        renderer.SetMesh(mesh);
        renderer.SetTexture(texture);
        return renderer;
    }
}
