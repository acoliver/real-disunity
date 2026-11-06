using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>Graphics.Blit validates its material and pass, then records the blit; WorldGraphicsTests covers what is recorded.</summary>
internal static class GraphicsBlitTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var calls = Calls();
            EveryRecordedOverloadReturns(calls);
            Console.WriteLine("PASS: every Graphics.Blit overload the engine records returns and leaves RenderTexture.active alone");
            MaterialAndPassAreValidated(calls);
            Console.WriteLine("PASS: Graphics.Blit throws for a null or destroyed material and for a pass outside the material's passes");
            UndeclaredShaderAcceptsAnyPass();
            Console.WriteLine("PASS: a material whose shader declares no pass count accepts any pass from -1");
            DestroyedTexturesThrow();
            Console.WriteLine("PASS: Graphics.Blit throws for a destroyed source or destination and takes a null source");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private sealed record BlitCall(string Member, bool TakesMaterial, Action<Material?, int> Run);

    private static Material TwoPassMaterial() => new(DisunityHost.DeclareShader("Test/Blit", 2000, 2, 1, _ => { }));

    private static List<BlitCall> Calls()
    {
        var source = new Texture2D(4, 4);
        var dest = new RenderTexture(8, 8, 0);
        RenderTexture.active = dest;
        return new List<BlitCall>
        {
            new("Graphics.Blit", false, (_, _) => Graphics.Blit(source, dest)),
            new("Graphics.Blit", false, (_, _) => Graphics.Blit(source, dest, Vector2.one, Vector2.zero)),
            new("Graphics.Blit", false, (_, _) => Graphics.Blit(source, dest, Vector2.one, Vector2.zero, 0, 0)),
            new("Graphics.Blit", true, (m, p) => Graphics.Blit(source, dest, m!, p)),
            new("Graphics.Blit", true, (m, p) => Graphics.Blit(source, dest, m!, p, 0)),
            new("Graphics.Blit", true, (m, p) => Graphics.Blit(source, m!, p)),
            new("Graphics.Blit", true, (m, p) => Graphics.Blit(source, m!, p, 0)),
        };
    }

    private static void EveryRecordedOverloadReturns(List<BlitCall> calls)
    {
        var material = TwoPassMaterial();
        var active = RenderTexture.active;
        foreach (var call in calls) call.Run(material, 1);
        Ensure(RenderTexture.active == active, "RenderTexture.active is unchanged");
        Graphics.Blit(null!, new RenderTexture(2, 2, 0));
    }

    private static void MaterialAndPassAreValidated(List<BlitCall> calls)
    {
        var material = TwoPassMaterial();
        var destroyed = TwoPassMaterial();
        UnityEngine.Object.DestroyImmediate(destroyed);
        foreach (var call in calls.Where(c => c.TakesMaterial))
        {
            Expect.Throws<ArgumentNullException>(() => call.Run(null, -1), $"{call.Member} with a null material");
            Expect.Throws<ArgumentException>(() => call.Run(destroyed, -1), $"{call.Member} with a destroyed material");
            Expect.Throws<ArgumentException>(() => call.Run(material, -2), $"{call.Member} with pass -2");
            Expect.Throws<ArgumentException>(() => call.Run(material, 2), $"{call.Member} with pass 2 of 2");
            foreach (var pass in new[] { -1, 0, 1 }) call.Run(material, pass);
        }
    }

    private static void UndeclaredShaderAcceptsAnyPass()
    {
        var material = new Material(DisunityHost.RegisterShader("Test/BlitUndeclared"));
        var source = new Texture2D(2, 2);
        RenderTexture.active = new RenderTexture(2, 2, 0);
        Graphics.Blit(source, material, 7);
        Graphics.Blit(source, material, -1);
        Expect.Throws<ArgumentException>(() => Graphics.Blit(source, material, -2), "pass -2 on an undeclared shader");
    }

    private static void DestroyedTexturesThrow()
    {
        var destroyed = new RenderTexture(2, 2, 0);
        UnityEngine.Object.DestroyImmediate(destroyed);
        Expect.Throws<ArgumentException>(() => Graphics.Blit(new Texture2D(2, 2), destroyed), "a destroyed destination");
        var goneSource = new Texture2D(2, 2);
        UnityEngine.Object.DestroyImmediate(goneSource);
        Expect.Throws<ArgumentException>(() => Graphics.Blit(goneSource, new RenderTexture(2, 2, 0)), "a destroyed source");
    }
}
