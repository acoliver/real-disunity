using Disunity.Hosting;
using UnityEngine;

namespace PortableFixture;

/// <summary>A canvas renderer drawing a quad over a render texture, as a raw image over a camera's target does.</summary>
internal static class UiTargetFixture
{
    internal static void Create()
    {
        var root = DisunityHost.CreateGameObject();
        var canvas = root.AddComponent<RectTransform>();
        var screen = root.AddComponent<Canvas>();
        screen.renderMode = RenderMode.ScreenSpaceOverlay;
        screen.sortingOrder = 2;
        var rect = DisunityHost.CreateGameObject().AddComponent<RectTransform>();
        rect.SetParent(canvas, false);
        rect.sizeDelta = new Vector2(32, 32);
        var mesh = new Mesh
        {
            vertices = [new(-16, -16, 0), new(-16, 16, 0), new(16, 16, 0), new(16, -16, 0)],
            uv = [new(0, 0), new(0, 1), new(1, 1), new(1, 0)],
            triangles = [0, 1, 2, 2, 3, 0],
        };
        var renderer = rect.gameObject.AddComponent<CanvasRenderer>();
        renderer.SetMesh(mesh);
        renderer.SetTexture(new RenderTexture(16, 8, 0));
        var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
        camera.transform.position = new Vector3(0, 0, -10);
    }
}
