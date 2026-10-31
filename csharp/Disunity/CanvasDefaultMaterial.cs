using Disunity.Hosting;

namespace UnityEngine;

public sealed partial class Canvas
{
    private const int TransparentQueue = 3000;
    private static readonly object DefaultMaterialGate = new();
    private static Material? _defaultMaterial;

    /// <summary>
    /// The material a UI graphic uses when it has none of its own: one shared material on the shader
    /// <c>UI/Default</c>, which the engine registers with the transparent queue (3000) and one pass. It is
    /// created on first use and the same object is returned each time, until a game destroys it, when the next
    /// call makes a new one (the engine's choice). Nothing draws with it yet.
    /// </summary>
    public static Material GetDefaultCanvasMaterial()
    {
        lock (DefaultMaterialGate)
        {
            if (_defaultMaterial) return _defaultMaterial!;
            var shader = DisunityHost.RegisterShader("UI/Default").Declare(TransparentQueue, 1, 1);
            return _defaultMaterial = new Material(shader) { name = "Default UI Material" };
        }
    }

    /// <summary>The material for a graphic whose texture is split into colour and alpha. The engine has no split
    /// textures, so this is <see cref="GetDefaultCanvasMaterial"/>.</summary>
    public static Material GetETC1SupportedCanvasMaterial() => GetDefaultCanvasMaterial();
}
