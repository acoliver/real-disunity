namespace UnityEngine.Rendering;

public sealed partial class CommandBuffer
{
    /// <summary>Wireframe drawing on or off for the draws recorded after this point. The engine draws solid triangles only, so
    /// turning wireframe off records nothing (it is already off), and turning it on throws <see cref="NotSupportedException"/>
    /// rather than draw solid where lines were asked for. A released buffer throws either way.</summary>
    public void SetWireframe(bool enable)
    {
        ThrowIfReleased();
        if (enable) throw new NotSupportedException("CommandBuffer.SetWireframe(true): wireframe drawing is not supported yet");
    }
}
