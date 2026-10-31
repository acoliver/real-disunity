using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>Graphics.CopyTexture validates its textures, then completes without copying until the native draw surface exists.</summary>
internal static class GraphicsCopyTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var (source, destination) = (new RenderTexture(4, 4, 0), new Texture2D(4, 4));
            var copies = new (string, Action<Texture?, Texture?>)[]
            {
                ("whole texture", (s, d) => Graphics.CopyTexture(s!, d!)),
                ("element", (s, d) => Graphics.CopyTexture(s!, 0, d!, 0)),
                ("element and mip", (s, d) => Graphics.CopyTexture(s!, 0, 0, d!, 0, 0)),
                ("region", (s, d) => Graphics.CopyTexture(s!, 0, 0, 0, 0, 1, 1, d!, 0, 0, 0, 0)),
            };
            foreach (var (_, copy) in copies) copy(source, destination);
            Console.WriteLine("PASS: every Graphics.CopyTexture overload returns");
            foreach (var (name, copy) in copies)
            {
                Expect.Throws<ArgumentNullException>(() => copy(null, destination), $"{name} with a null source");
                Expect.Throws<ArgumentNullException>(() => copy(source, null), $"{name} with a null destination");
            }
            var destroyed = new Texture2D(2, 2);
            UnityEngine.Object.DestroyImmediate(destroyed);
            foreach (var (name, copy) in copies)
            {
                Expect.Throws<ArgumentException>(() => copy(destroyed, destination), $"{name} with a destroyed source");
                Expect.Throws<ArgumentException>(() => copy(source, destroyed), $"{name} with a destroyed destination");
            }
            Console.WriteLine("PASS: Graphics.CopyTexture throws for a null or destroyed texture");
            NotesOnce(copies);
            Console.WriteLine("PASS: Graphics.CopyTexture logs one note per world");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static void NotesOnce((string, Action<Texture?, Texture?>)[] copies)
    {
        DisunityHost.UnloadWorld();
        DisunityHost.LoadWorld();
        var (src, dst) = (new RenderTexture(4, 4, 0), new Texture2D(4, 4));
        using var log = new LogRecorder();
        foreach (var (_, copy) in copies) copy(src, dst);
        foreach (var (_, copy) in copies) copy(src, dst);
        var note = "Warning: Graphics.CopyTexture draws nothing yet: the engine has no draw surface";
        Ensure(log.Messages.Count(m => m == note) == 1, $"one note: {string.Join("|", log.Messages)}");
    }
}
