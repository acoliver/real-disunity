namespace UnityEngine.Rendering;

/// <summary>
/// Global array bindings on a command buffer. A vector or matrix array is checked and copied when it is recorded: a null, empty or
/// oversized (over 1023 elements) array, a component that is not finite, a name that is not 1 to 256 UTF-8 bytes and a property id
/// that no name was registered for all throw before the buffer records anything. The copy is the record, so changing the array
/// or list afterwards, or clearing, re-recording or releasing the buffer after it was sent, never changes what was sent. The
/// values go to the world's offscreen list in order, at <see cref="Graphics.ExecuteCommandBuffer"/> or a context's
/// <see cref="ScriptableRenderContext.Submit"/>, and do not change the shaders' global state. A matrix is sent column by column. A
/// released buffer throws <see cref="ObjectDisposedException"/> before any other check. The float array forms have no host to
/// receive them and throw <see cref="NotSupportedException"/> naming the member, whatever the array holds.
/// </summary>
public sealed partial class CommandBuffer
{
    public void SetGlobalFloatArray(string name, float[] values) => Fail(nameof(SetGlobalFloatArray));
    public void SetGlobalFloatArray(string name, List<float> values) => Fail(nameof(SetGlobalFloatArray));
    public void SetGlobalFloatArray(int nameID, List<float> values) => Fail(nameof(SetGlobalFloatArray));
    public void SetGlobalFloatArray(int nameID, float[] values) => Fail(nameof(SetGlobalFloatArray));

    public void SetGlobalVectorArray(string name, Vector4[] values) => RecordVectors(name, values);
    public void SetGlobalVectorArray(string name, List<Vector4> values) => RecordVectors(name, values);
    public void SetGlobalVectorArray(int nameID, List<Vector4> values) => RecordVectors(NamedAfterRelease(nameID), values);
    public void SetGlobalVectorArray(int nameID, Vector4[] values) => RecordVectors(NamedAfterRelease(nameID), values);

    public void SetGlobalMatrixArray(string name, Matrix4x4[] values) => RecordMatrices(name, values);
    public void SetGlobalMatrixArray(string name, List<Matrix4x4> values) => RecordMatrices(name, values);
    public void SetGlobalMatrixArray(int nameID, List<Matrix4x4> values) => RecordMatrices(NamedAfterRelease(nameID), values);
    public void SetGlobalMatrixArray(int nameID, Matrix4x4[] values) => RecordMatrices(NamedAfterRelease(nameID), values);

    private void RecordVectors(string name, Vector4[]? values)
    {
        ThrowIfReleased();
        WorldGraphics.CheckArrayName(name, nameof(SetGlobalVectorArray));
        var copy = PropertyStore.CopyChecked(values, nameof(SetGlobalVectorArray));
        Record(() => WorldGraphics.RecordGlobalVectorArray(name, copy));
    }

    private void RecordVectors(string name, List<Vector4>? values)
    {
        ThrowIfReleased();
        WorldGraphics.CheckArrayName(name, nameof(SetGlobalVectorArray));
        var copy = PropertyStore.CopyChecked(values, nameof(SetGlobalVectorArray));
        Record(() => WorldGraphics.RecordGlobalVectorArray(name, copy));
    }

    private void RecordMatrices(string name, Matrix4x4[]? values)
    {
        ThrowIfReleased();
        WorldGraphics.CheckArrayName(name, nameof(SetGlobalMatrixArray));
        var copy = PropertyStore.CopyChecked(values, nameof(SetGlobalMatrixArray));
        Record(() => WorldGraphics.RecordGlobalMatrixArray(name, copy));
    }

    private void RecordMatrices(string name, List<Matrix4x4>? values)
    {
        ThrowIfReleased();
        WorldGraphics.CheckArrayName(name, nameof(SetGlobalMatrixArray));
        var copy = PropertyStore.CopyChecked(values, nameof(SetGlobalMatrixArray));
        Record(() => WorldGraphics.RecordGlobalMatrixArray(name, copy));
    }

    /// <summary>The name of a property id; a released buffer throws before an unknown id does.</summary>
    private string NamedAfterRelease(int nameID)
    {
        ThrowIfReleased();
        return NameOf(nameID);
    }
}
