namespace UnityEngine;

/// <summary>
/// A probe that captures reflections. The engine has no probe lighting: the type exists so a pipeline can read the defaults
/// below, and a probe cannot be added to a GameObject. <see cref="defaultTexture"/> is null, the engine has no default
/// reflection cubemap, and <see cref="defaultTextureHDRDecodeValues"/> is (1, 1, 0, 0), the decode of a cubemap that holds
/// linear colour as stored.
/// </summary>
public sealed class ReflectionProbe : Behaviour
{
    private protected override bool EnabledState { get; set; } = true;

    public static Texture? defaultTexture => null;

    public static Vector4 defaultTextureHDRDecodeValues => new(1f, 1f, 0f, 0f);
}
