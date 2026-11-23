using Disunity.Hosting;
using UnityEngine;

namespace PortableFixture;

/// <summary>The screen fixture's yellow screen with one overlay canvas over it and a camera, which the presentation needs to place
/// a canvas; the camera records no render of its own. The canvas has a single green rectangle, 16 pixels
/// square, whose lower left corner is 24 pixels left of and 0 pixels above the canvas centre, so a frame that shows the canvas
/// has green in the upper left of a screen twice as wide as it is high and yellow everywhere else. The colour uses only the
/// channel values 0 and 1, which survive any colour-space encoding.</summary>
internal static class ScreenUiFixture
{
    internal static void Create()
    {
        ScreenFixture.Create();
        AddOverlay();
    }

    /// <summary>The camera and the canvas with its green rectangle, which is returned so a script can move it.</summary>
    internal static RectTransform AddOverlay()
    {
        DisunityHost.CreateGameObject().AddComponent<Camera>().transform.position = new Vector3(0, 0, -10);
        var canvas = DisunityHost.CreateGameObject();
        var root = canvas.AddComponent<RectTransform>();
        canvas.AddComponent<Canvas>().renderMode = RenderMode.ScreenSpaceOverlay;
        var rect = DisunityHost.CreateGameObject().AddComponent<RectTransform>();
        rect.SetParent(root, false);
        rect.sizeDelta = new Vector2(16, 16);
        var mesh = new Mesh
        {
            vertices = [new(-24, 0, 0), new(-8, 0, 0), new(-8, 16, 0), new(-24, 16, 0)],
            uv = [new(0, 1), new(1, 1), new(1, 0), new(0, 0)],
            triangles = [0, 1, 2, 0, 2, 3],
        };
        var texture = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
        texture.SetPixel(0, 0, new Color(0, 1, 0, 1));
        texture.Apply();
        var renderer = rect.gameObject.AddComponent<CanvasRenderer>();
        renderer.SetMesh(mesh);
        renderer.SetTexture(texture);
        return rect;
    }
}
