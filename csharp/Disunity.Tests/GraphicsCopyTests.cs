using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>Graphics.CopyTexture validates its textures, then records the copy; WorldGraphicsTests covers what is recorded.</summary>
internal static class GraphicsCopyTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var (source, destination) = (new Texture2D(4, 4), new RenderTexture(4, 4, 0));
            var copies = new (string, Action<Texture?, Texture?>)[]
            {
                ("whole texture", (s, d) => Graphics.CopyTexture(s!, d!)),
                ("element", (s, d) => Graphics.CopyTexture(s!, 0, d!, 0)),
                ("element and mip", (s, d) => Graphics.CopyTexture(s!, 0, 0, d!, 0, 0)),
            };
            foreach (var (_, copy) in copies) copy(source, destination);
            Console.WriteLine("PASS: every Graphics.CopyTexture overload the engine records returns");
            var all = copies.Append(("region", (s, d) => Graphics.CopyTexture(s!, 0, 0, 0, 0, 1, 1, d!, 0, 0, 0, 0))).ToArray();
            foreach (var (name, copy) in all)
            {
                Expect.Throws<ArgumentNullException>(() => copy(null, destination), $"{name} with a null source");
                Expect.Throws<ArgumentNullException>(() => copy(source, null), $"{name} with a null destination");
            }
            var destroyed = new Texture2D(2, 2);
            UnityEngine.Object.DestroyImmediate(destroyed);
            var destroyedTarget = new RenderTexture(2, 2, 0);
            UnityEngine.Object.DestroyImmediate(destroyedTarget);
            foreach (var (name, copy) in all)
            {
                Expect.Throws<ArgumentException>(() => copy(destroyed, destination), $"{name} with a destroyed source");
                Expect.Throws<ArgumentException>(() => copy(source, destroyedTarget), $"{name} with a destroyed destination");
            }
            Console.WriteLine("PASS: Graphics.CopyTexture throws for a null or destroyed texture");
            Expect.Throws<ArgumentException>(() => Graphics.CopyTexture(source, 0, 9, destination, 0, 0), "a source mip the texture lacks");
            Expect.Throws<ArgumentException>(() => Graphics.CopyTexture(source, 0, 0, destination, 0, 9), "a destination mip the texture lacks");
            Expect.Throws<ArgumentException>(() => Graphics.CopyTexture(source, -1, destination, 0), "a negative element");
            Console.WriteLine("PASS: Graphics.CopyTexture throws for a mip level or element the textures lack");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }
}
