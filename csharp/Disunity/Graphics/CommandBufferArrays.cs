namespace UnityEngine.Rendering;

/// <summary>
/// Global array bindings on a command buffer. Each is recorded as a command that sets the shaders' global array when the buffer
/// executes, in order with its other commands, as <see cref="Shader.SetGlobalVectorArray(int, Vector4[])"/> and its siblings do
/// at once. The values are copied when the member is called, so a later change to the caller's array or list does not reach the
/// bound array. A null array throws <see cref="ArgumentNullException"/> and an empty one, or one above 1023 elements,
/// <see cref="ArgumentException"/>, when the member is called.
/// </summary>
public sealed partial class CommandBuffer
{
    public void SetGlobalFloatArray(string name, float[] values) => SetGlobalFloatArray(Shader.PropertyToID(name), values);
    public void SetGlobalFloatArray(string name, List<float> values) => SetGlobalFloatArray(Shader.PropertyToID(name), values);
    public void SetGlobalFloatArray(int nameID, List<float> values) => SetGlobalFloatArray(nameID, ListToArray(values));
    public void SetGlobalFloatArray(int nameID, float[] values) => GlobalArray(values, nameof(SetGlobalFloatArray), copy => Shader.SetGlobalFloatArray(nameID, copy));

    public void SetGlobalVectorArray(string name, Vector4[] values) => SetGlobalVectorArray(Shader.PropertyToID(name), values);
    public void SetGlobalVectorArray(string name, List<Vector4> values) => SetGlobalVectorArray(Shader.PropertyToID(name), values);
    public void SetGlobalVectorArray(int nameID, List<Vector4> values) => SetGlobalVectorArray(nameID, ListToArray(values));
    public void SetGlobalVectorArray(int nameID, Vector4[] values) => GlobalArray(values, nameof(SetGlobalVectorArray), copy => Shader.SetGlobalVectorArray(nameID, copy));

    public void SetGlobalMatrixArray(string name, Matrix4x4[] values) => SetGlobalMatrixArray(Shader.PropertyToID(name), values);
    public void SetGlobalMatrixArray(string name, List<Matrix4x4> values) => SetGlobalMatrixArray(Shader.PropertyToID(name), values);
    public void SetGlobalMatrixArray(int nameID, List<Matrix4x4> values) => SetGlobalMatrixArray(nameID, ListToArray(values));
    public void SetGlobalMatrixArray(int nameID, Matrix4x4[] values) => GlobalArray(values, nameof(SetGlobalMatrixArray), copy => Shader.SetGlobalMatrixArray(nameID, copy));

    private void GlobalArray<T>(T[] values, string member, Action<T[]> bind)
    {
        var copy = PropertyStore.Copy(values, member);
        Record(() => bind(copy));
    }

    private static T[] ListToArray<T>(List<T> values)
    {
        ArgumentNullException.ThrowIfNull(values);
        return values.ToArray();
    }
}
