using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>Graphics.Blit and BlitMultiTap validate their material and pass, then complete without drawing until the native draw surface exists.</summary>
internal static class GraphicsBlitTests
{
    private const string Note = "draws nothing yet: the engine has no draw surface";

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var calls = Calls();
            EveryOverloadReturns(calls);
            Console.WriteLine("PASS: every Graphics.Blit and BlitMultiTap overload returns and leaves the render textures alone");
            MaterialAndPassAreValidated(calls);
            Console.WriteLine("PASS: Graphics.Blit and BlitMultiTap throw for a null material and for a pass outside the material's passes");
            NotesOncePerMember();
            Console.WriteLine("PASS: Graphics.Blit and BlitMultiTap log one note per member per world");
            UndeclaredShaderAcceptsAnyPass();
            Console.WriteLine("PASS: a material whose shader declares no pass count accepts any pass from -1");
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
        var taps = new[] { Vector2.zero, Vector2.one };
        return new List<BlitCall>
        {
            new("Graphics.Blit", false, (_, _) => Graphics.Blit(source, dest)),
            new("Graphics.Blit", false, (_, _) => Graphics.Blit(source, dest, Vector2.one, Vector2.zero)),
            new("Graphics.Blit", false, (_, _) => Graphics.Blit(source, dest, Vector2.one, Vector2.zero, 0, 0)),
            new("Graphics.Blit", true, (m, p) => Graphics.Blit(source, dest, m!, p)),
            new("Graphics.Blit", true, (m, p) => Graphics.Blit(source, dest, m!, p, 0)),
            new("Graphics.Blit", true, (m, p) => Graphics.Blit(source, m!, p)),
            new("Graphics.Blit", true, (m, p) => Graphics.Blit(source, m!, p, 0)),
            new("Graphics.BlitMultiTap", true, (m, _) => Graphics.BlitMultiTap(source, dest, m!, taps)),
            new("Graphics.BlitMultiTap", true, (m, _) => Graphics.BlitMultiTap(source, dest, m!, 0, taps)),
        };
    }

    private static void EveryOverloadReturns(List<BlitCall> calls)
    {
        var material = TwoPassMaterial();
        var active = new RenderTexture(2, 2, 0);
        RenderTexture.active = active;
        foreach (var call in calls) call.Run(material, 1);
        Ensure(RenderTexture.active == active, "RenderTexture.active is unchanged");
        Graphics.Blit(null!, new RenderTexture(2, 2, 0));
        Graphics.Blit(new Texture2D(2, 2), (RenderTexture)null!);
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
            if (call.Member == "Graphics.BlitMultiTap") continue;
            Expect.Throws<ArgumentException>(() => call.Run(material, -2), $"{call.Member} with pass -2");
            Expect.Throws<ArgumentException>(() => call.Run(material, 2), $"{call.Member} with pass 2 of 2");
            foreach (var pass in new[] { -1, 0, 1 }) call.Run(material, pass);
        }
        var source = new Texture2D(2, 2);
        Expect.Throws<ArgumentNullException>(() => Graphics.BlitMultiTap(source, null!, material, (Vector2[])null!), "BlitMultiTap with null offsets");
    }

    private static void NotesOncePerMember()
    {
        for (var world = 0; world < 2; world++)
        {
            DisunityHost.UnloadWorld();
            DisunityHost.LoadWorld();
            var material = TwoPassMaterial();
            var calls = Calls();
            using (var log = new LogRecorder())
            {
                foreach (var call in calls) call.Run(material, 0);
                foreach (var call in calls) call.Run(material, 0);
                Ensure(log.Messages.Count(m => m == $"Warning: Graphics.Blit {Note}") == 1, $"Blit notes once in world {world}");
                Ensure(log.Messages.Count(m => m == $"Warning: Graphics.BlitMultiTap {Note}") == 1, $"BlitMultiTap notes once in world {world}");
            }
        }
    }

    private static void UndeclaredShaderAcceptsAnyPass()
    {
        var material = new Material(DisunityHost.RegisterShader("Test/BlitUndeclared"));
        var source = new Texture2D(2, 2);
        Graphics.Blit(source, material, 7);
        Graphics.Blit(source, material, -1);
        Expect.Throws<ArgumentException>(() => Graphics.Blit(source, material, -2), "pass -2 on an undeclared shader");
    }
}
