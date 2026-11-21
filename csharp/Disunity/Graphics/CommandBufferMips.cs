namespace UnityEngine.Rendering;

public sealed partial class CommandBuffer
{
    /// <summary>Records a request, run in order with the other commands, for the host to fill mip levels 1 and up of the render
    /// texture <paramref name="rt"/> names from its level 0. Only a <see cref="RenderTexture"/> has a world render texture to
    /// fill: a temporary render texture, the camera target, no target and a plain texture throw
    /// <see cref="NotSupportedException"/>. The world refuses the record when the buffer is executed if the texture has a single
    /// mip level.</summary>
    public void GenerateMips(RenderTargetIdentifier rt)
    {
        ThrowIfReleased();
        const string member = "CommandBuffer.GenerateMips";
        var target = CommandTarget.Of(rt, member);
        if (target.Texture is not RenderTexture)
            throw new NotSupportedException($"{member}: only a render texture object has mips the engine generates; a temporary render texture, the camera target, no target and a plain texture have no render texture of their own");
        Record(() => WorldGraphics.RecordGenerateMips(target.ToNative().render));
    }
}
