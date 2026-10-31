using UnityEngine;

/// <summary>Font.textureRebuilt exists as a static event of Action&lt;Font&gt; that accepts and drops handlers, which UI text
/// does as it tracks fonts. The engine never raises it.</summary>
internal static class FontEventTests
{
    public static void Run()
    {
        FontTextureRebuiltAcceptsHandlers();
        Console.WriteLine("PASS: Font.textureRebuilt takes and drops handlers");
    }

    private static void FontTextureRebuiltAcceptsHandlers()
    {
        var type = typeof(Font);
        var rebuilt = type.GetEvent("textureRebuilt") ?? throw new InvalidOperationException("Font has no textureRebuilt event");
        Expect.That(rebuilt.EventHandlerType == typeof(Action<Font>), "the event takes an Action<Font>");
        Expect.That(rebuilt.AddMethod!.IsStatic && rebuilt.RemoveMethod!.IsStatic, "the event is static");
        Action<Font> handler = _ => throw new InvalidOperationException("the engine never rebuilds a font atlas");
        rebuilt.AddMethod!.Invoke(null, [handler]);
        rebuilt.RemoveMethod!.Invoke(null, [handler]);
        rebuilt.RemoveMethod!.Invoke(null, [handler]);
    }
}
