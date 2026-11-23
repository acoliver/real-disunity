using Disunity.Hosting;
using UnityEngine;

namespace PortableFixture;

/// <summary>
/// What a legacy UI Text does with the engine's text generator: lay "AB" out at 20 pixels centred in a 64 by 64 rectangle
/// of a screen overlay canvas, build a mesh from the generated vertices (position, first UV and colour, two triangles
/// per quad) and give the canvas renderer the mesh, the font's material and the font's atlas texture.
/// </summary>
internal static class LegacyTextFixture
{
    internal static void Create()
    {
        var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
        camera.transform.position = new Vector3(0, 0, -10);
        var canvas = DisunityHost.CreateGameObject();
        var root = canvas.AddComponent<RectTransform>();
        canvas.AddComponent<Canvas>().renderMode = RenderMode.ScreenSpaceOverlay;
        var label = DisunityHost.CreateGameObject().AddComponent<RectTransform>();
        label.sizeDelta = new Vector2(64, 64);
        label.SetParent(root, false);
        var font = DisunityHost.RegisterFont(SyntheticFont.Bytes());
        var generator = new TextGenerator();
        if (!generator.Populate("AB", Settings(font))) throw new InvalidOperationException("the text generator could not lay out the text");
        var renderer = label.gameObject.AddComponent<CanvasRenderer>();
        renderer.SetMesh(TextMesh(generator.verts));
        renderer.materialCount = 1;
        renderer.SetMaterial(font.material, 0);
        renderer.SetTexture(font.material.mainTexture);
    }

    private static TextGenerationSettings Settings(Font font) => new()
    {
        font = font, color = new Color(0, 1, 0, 1), fontSize = 20, lineSpacing = 1, scaleFactor = 1,
        textAnchor = TextAnchor.MiddleCenter, generationExtents = new Vector2(64, 64), pivot = new Vector2(0.5f, 0.5f),
    };

    private static Mesh TextMesh(IList<UIVertex> verts)
    {
        var triangles = new List<int>();
        for (var quad = 0; quad < verts.Count; quad += 4)
            triangles.AddRange([quad, quad + 1, quad + 2, quad + 2, quad + 3, quad]);
        return new Mesh
        {
            vertices = verts.Select(v => v.position).ToArray(),
            uv = verts.Select(v => new Vector2(v.uv0.x, v.uv0.y)).ToArray(),
            colors32 = verts.Select(v => v.color).ToArray(),
            triangles = triangles.ToArray(),
        };
    }
}
