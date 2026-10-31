using Disunity.Hosting;
using UnityEngine;

/// <summary>SpriteRenderer.drawMode and size: stored on the component, read back and copied by a clone.</summary>
internal static class SpriteDrawModeTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var renderer = DisunityHost.CreateGameObject().AddComponent<SpriteRenderer>();
            Expect.Equal(SpriteDrawMode.Simple, renderer.drawMode, "a sprite renderer starts in simple mode");
            Expect.Equal(Vector2.zero, renderer.size, "and with size zero");
            renderer.drawMode = SpriteDrawMode.Tiled;
            renderer.size = new Vector2(3, 4);
            Expect.That(renderer.drawMode == SpriteDrawMode.Tiled && renderer.size == new Vector2(3, 4), "the mode and size read back");
            renderer.drawMode = SpriteDrawMode.Sliced;
            Expect.Equal(SpriteDrawMode.Sliced, renderer.drawMode, "the mode changes");
            var clone = UnityEngine.Object.Instantiate(renderer);
            Expect.That(clone.drawMode == SpriteDrawMode.Sliced && clone.size == new Vector2(3, 4), "a clone keeps the mode and size");
            clone.drawMode = SpriteDrawMode.Simple;
            Expect.Equal(SpriteDrawMode.Sliced, renderer.drawMode, "editing the clone leaves the source");
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: SpriteRenderer.drawMode and size are stored, read back and copied by a clone");
    }
}
