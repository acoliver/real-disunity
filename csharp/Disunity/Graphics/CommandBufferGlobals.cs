namespace UnityEngine.Rendering;

/// <summary>
/// The recording members that set global shader values and keywords. The values go to the world's offscreen list at the place
/// the buffer is executed and are not folded into the global shader state, so the host applies them in order with the other work. A
/// property id that no name was ever registered for throws <see cref="ArgumentException"/> when it is recorded. A texture is a
/// render texture or nothing; a temporary render texture or the camera target as a global texture throws
/// <see cref="NotSupportedException"/>.
/// </summary>
public sealed partial class CommandBuffer
{
    public void SetGlobalFloat(string name, float value) => Global(name, () => WorldGraphics.RecordGlobalFloat(name, value));
    public void SetGlobalFloat(int nameID, float value) => SetGlobalFloat(NameOf(nameID), value);
    public void SetGlobalInt(string name, int value) => Global(name, () => WorldGraphics.RecordGlobalInt(name, value));
    public void SetGlobalInt(int nameID, int value) => SetGlobalInt(NameOf(nameID), value);
    public void SetGlobalInteger(string name, int value) => SetGlobalInt(name, value);
    public void SetGlobalInteger(int nameID, int value) => SetGlobalInt(NameOf(nameID), value);
    public void SetGlobalVector(string name, Vector4 value) => Global(name, () => WorldGraphics.RecordGlobalVector(name, value));
    public void SetGlobalVector(int nameID, Vector4 value) => SetGlobalVector(NameOf(nameID), value);
    public void SetGlobalColor(string name, Color value) => SetGlobalVector(name, value);
    public void SetGlobalColor(int nameID, Color value) => SetGlobalVector(NameOf(nameID), value);
    public void SetGlobalMatrix(string name, Matrix4x4 value) => Global(name, () => WorldGraphics.RecordGlobalMatrix(name, value));
    public void SetGlobalMatrix(int nameID, Matrix4x4 value) => SetGlobalMatrix(NameOf(nameID), value);

    public void SetGlobalTexture(string name, RenderTargetIdentifier value)
    {
        ThrowIfReleased();
        var target = CommandTarget.Of(value, "CommandBuffer.SetGlobalTexture").AsTexture("CommandBuffer.SetGlobalTexture");
        Global(name, () => WorldGraphics.RecordGlobalTexture(name, target.ToTexture()));
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
        Record(() => WorldGraphics.RecordGlobalKeyword(keyword, enabled));
    }

    private void Global(string name, Action send)
    {
        ArgumentNullException.ThrowIfNull(name);
        Record(send);
    }

    private static string NameOf(int nameID) =>
        Shader.PropertyName(nameID) ?? throw new ArgumentException($"no shader property has the id {nameID}", nameof(nameID));
}
