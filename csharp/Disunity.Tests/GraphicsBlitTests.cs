using System.Reflection;
using System.Runtime.ExceptionServices;
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
            ThreeArgumentBlitRecordsTheAllPassBlit();
            Console.WriteLine("PASS: Graphics.Blit(Texture, RenderTexture, Material) records the same blit as the explicit pass -1, in call order");
            ThreeArgumentBlitValidatesBeforeItRecords();
            Console.WriteLine("PASS: Graphics.Blit(Texture, RenderTexture, Material) throws for a null or destroyed material or texture, names itself and records nothing");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private sealed record BlitCall(string Member, bool TakesMaterial, Action<Material?, int> Run);

    private static Material TwoPassMaterial() => new(DisunityHost.DeclareShader("Test/Blit", 2000, Unnamed(2), 1, _ => { }));

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

    /// <summary>A compiled game binds to the three-argument signature itself, not to the four-argument one with a default
    /// pass, so the tests resolve it by its exact parameter list as the runtime does.</summary>
    private static MethodInfo ThreeArgumentBlit()
    {
        var method = typeof(Graphics).GetMethod("Blit", BindingFlags.Public | BindingFlags.Static, null,
            new[] { typeof(Texture), typeof(RenderTexture), typeof(Material) }, null);
        Expect.That(method is not null, "Graphics.Blit(Texture, RenderTexture, Material) is declared");
        return method!;
    }

    private static void BlitWithoutPass(Texture? source, RenderTexture? dest, Material? material)
    {
        try { ThreeArgumentBlit().Invoke(null, new object?[] { source, dest, material }); }
        catch (TargetInvocationException error) { ExceptionDispatchInfo.Capture(error.InnerException!).Throw(); }
    }

    private static void ThreeArgumentBlitRecordsTheAllPassBlit()
    {
        var first = TwoPassMaterial();
        var second = TwoPassMaterial();
        var plain = new Texture2D(4, 4);
        var a = new RenderTexture(8, 8, 0);
        var b = new RenderTexture(8, 8, 0);
        var active = RenderTexture.active;
        CommandWireReader.Take();
        Graphics.Blit(plain, a, first);
        Graphics.Blit(plain, a, first, -1);
        BlitWithoutPass(a, b, second);
        Graphics.CopyTexture(a, b);
        Graphics.Blit(null!, b, first);
        BlitWithoutPass(plain, b, second);
        var taken = CommandWireReader.Take();

        Expect.That(first.WorldLink.Id != 0 && first.WorldLink.Id != second.WorldLink.Id, "the two materials have distinct world ids");
        var expected = new[]
        {
            $"Blit source=1:{plain.RendererIdentity} target={a.WorldId} material={first.WorldLink.Id} pass=-1",
            $"Blit source=1:{plain.RendererIdentity} target={a.WorldId} material={first.WorldLink.Id} pass=-1",
            $"Blit source=2:{a.WorldId} target={b.WorldId} material={second.WorldLink.Id} pass=-1",
            $"Copy source=2:{a.WorldId}[0,0] target={b.WorldId}[0,0]",
            $"Blit source=0:0 target={b.WorldId} material={first.WorldLink.Id} pass=-1",
            $"Blit source=1:{plain.RendererIdentity} target={b.WorldId} material={second.WorldLink.Id} pass=-1",
        };
        Expect.Sequence(expected, taken, "the records of three-argument and explicit blits");
        Ensure(RenderTexture.active == active, "RenderTexture.active is unchanged");
    }

    private static void ThreeArgumentBlitValidatesBeforeItRecords()
    {
        var material = TwoPassMaterial();
        var destroyedMaterial = TwoPassMaterial();
        UnityEngine.Object.DestroyImmediate(destroyedMaterial);
        var source = new Texture2D(2, 2);
        var goneSource = new Texture2D(2, 2);
        UnityEngine.Object.DestroyImmediate(goneSource);
        var dest = new RenderTexture(2, 2, 0);
        var goneDest = new RenderTexture(2, 2, 0);
        UnityEngine.Object.DestroyImmediate(goneDest);
        CommandWireReader.Take();

        Expect.Throws<ArgumentNullException>(() => BlitWithoutPass(source, dest, null), "a null material");
        Expect.Throws<ArgumentException>(() => BlitWithoutPass(source, dest, destroyedMaterial), "a destroyed material");
        Expect.Throws<ArgumentException>(() => BlitWithoutPass(source, goneDest, material), "a destroyed destination");
        Expect.Throws<ArgumentException>(() => BlitWithoutPass(goneSource, dest, material), "a destroyed source");
        var screen = Expect.Catch<NotSupportedException>(() => BlitWithoutPass(source, null, material), "a null destination draws to the screen");
        Expect.That(screen.Message.StartsWith("Graphics.Blit(Texture, RenderTexture, Material) is not supported yet", StringComparison.Ordinal),
            $"the message names the three-argument overload: {screen.Message}");
        Expect.That(CommandWireReader.Take().Length == 0, "a blit that throws records nothing");
    }
}
