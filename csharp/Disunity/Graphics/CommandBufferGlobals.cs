namespace UnityEngine.Rendering;

/// <summary>
/// The recording members that set global shader values and keywords. Executing the buffer (<see cref="Graphics.ExecuteCommandBuffer"/>
/// or a context's <see cref="ScriptableRenderContext.Submit"/>) makes each write the way the matching <c>Shader.SetGlobal*</c> or
/// <c>Shader.EnableKeyword</c> call does: it sets the managed global and the world's global shader state, so the value outlives
/// the buffer and the take of work that ran it, and <c>Shader.GetGlobal*</c> reads it back. The world orders every direct write
/// of a take among its records, a write after the last one included, and the take replays from the state before its first write,
/// so the host still applies the value at its place among the buffer's other commands.
/// A temporary render texture has no identity that outlives its take; a global set to one stays an ordered record of that take and
/// changes neither the managed global nor the world's state. A property id that no name was ever registered for throws
/// <see cref="ArgumentException"/> when it is recorded, and a name the world refuses (empty, or over 256 UTF-8 bytes) throws it when the
/// buffer runs, before any global changes. A texture is a render texture, a texture asset (a <see cref="Texture2D"/>,
/// <see cref="Texture2DArray"/>, <see cref="Cubemap"/> or <see cref="Texture3D"/>, which <c>Shader.SetGlobalTexture</c> takes as well, so a
/// pipeline's reflection cube is set as any texture is), a temporary render texture or nothing; the camera target as a global texture throws
/// <see cref="NotSupportedException"/>.
/// </summary>
public sealed partial class CommandBuffer
{
    public void SetGlobalFloat(string name, float value) => Global(name, () => Shader.SetGlobalFloat(name, value));
    public void SetGlobalFloat(int nameID, float value) => SetGlobalFloat(NameOf(nameID), value);
    public void SetGlobalInt(string name, int value) => Global(name, () => Shader.SetGlobalInt(name, value));
    public void SetGlobalInt(int nameID, int value) => SetGlobalInt(NameOf(nameID), value);
    public void SetGlobalInteger(string name, int value) => Global(name, () => Shader.SetGlobalInteger(name, value));
    public void SetGlobalInteger(int nameID, int value) => SetGlobalInteger(NameOf(nameID), value);
    public void SetGlobalVector(string name, Vector4 value) => Global(name, () => Shader.SetGlobalVector(name, value));
    public void SetGlobalVector(int nameID, Vector4 value) => SetGlobalVector(NameOf(nameID), value);
    public void SetGlobalColor(string name, Color value) => Global(name, () => Shader.SetGlobalColor(name, value));
    public void SetGlobalColor(int nameID, Color value) => SetGlobalColor(NameOf(nameID), value);
    public void SetGlobalMatrix(string name, Matrix4x4 value) => Global(name, () => Shader.SetGlobalMatrix(name, value));
    public void SetGlobalMatrix(int nameID, Matrix4x4 value) => SetGlobalMatrix(NameOf(nameID), value);

    public void SetGlobalTexture(string name, RenderTargetIdentifier value)
    {
        ThrowIfReleased();
        var target = CommandTarget.Of(value, "CommandBuffer.SetGlobalTexture").AsTexture("CommandBuffer.SetGlobalTexture");
        Global(name, () => ApplyGlobalTexture(name, target));
    }

    public void SetGlobalTexture(int nameID, RenderTargetIdentifier value) => SetGlobalTexture(NameOf(nameID), value);

    public void EnableShaderKeyword(string keyword) => Keyword(keyword, true);
    public void DisableShaderKeyword(string keyword) => Keyword(keyword, false);
    public void EnableKeyword(in GlobalKeyword keyword) => Keyword(keyword.name, true);
    public void DisableKeyword(in GlobalKeyword keyword) => Keyword(keyword.name, false);
    public void SetKeyword(in GlobalKeyword keyword, bool value) => Keyword(keyword.name, value);

    private void Keyword(string keyword, bool enabled)
    {
        ArgumentNullException.ThrowIfNull(keyword);
        Record(() =>
        {
            WorldGraphics.CheckName(keyword, "CommandBuffer keyword");
            if (enabled) Shader.EnableKeyword(keyword);
            else Shader.DisableKeyword(keyword);
        });
    }

    private void Global(string name, Action apply)
    {
        ArgumentNullException.ThrowIfNull(name);
        Record(() =>
        {
            WorldGraphics.CheckName(name, "CommandBuffer.SetGlobal");
            apply();
        });
    }

    /// <summary>Sets a global texture when the buffer runs. A temporary render texture is the only target without a managed
    /// <see cref="Texture"/> to hold, so it alone stays an ordered record of the take. A texture destroyed since the command was
    /// recorded throws <see cref="InvalidOperationException"/> before anything is set.</summary>
    private static void ApplyGlobalTexture(string name, CommandTarget target)
    {
        var texture = target.ToTexture();
        if (texture.Kind == WorldGraphics.TextureTemporary) WorldGraphics.RecordGlobalTexture(name, texture);
        else Shader.SetGlobalTexture(name, target.Texture);
    }

    private static string NameOf(int nameID) =>
        Shader.PropertyName(nameID) ?? throw new ArgumentException($"no shader property has the id {nameID}", nameof(nameID));
}
