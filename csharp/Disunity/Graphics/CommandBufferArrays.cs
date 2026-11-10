namespace UnityEngine.Rendering;

/// <summary>
/// Global array bindings on a command buffer, declared because a render pipeline binds to the overload it was built against. Every
/// form throws <see cref="NotSupportedException"/> naming the member and records nothing, whatever the array or list holds: the
/// world's global properties and its offscreen records carry no arrays, so no host could receive the values. A released buffer
/// throws <see cref="ObjectDisposedException"/> first. Delivering the arrays in order is tracked in the Hubnot issue "Deliver
/// CommandBuffer global arrays to the offscreen host"; the static <c>Shader.SetGlobal*Array</c> members are managed state and unchanged.
/// </summary>
public sealed partial class CommandBuffer
{
    public void SetGlobalFloatArray(string name, float[] values) => Fail(nameof(SetGlobalFloatArray));
    public void SetGlobalFloatArray(string name, List<float> values) => Fail(nameof(SetGlobalFloatArray));
    public void SetGlobalFloatArray(int nameID, List<float> values) => Fail(nameof(SetGlobalFloatArray));
    public void SetGlobalFloatArray(int nameID, float[] values) => Fail(nameof(SetGlobalFloatArray));

    public void SetGlobalVectorArray(string name, Vector4[] values) => Fail(nameof(SetGlobalVectorArray));
    public void SetGlobalVectorArray(string name, List<Vector4> values) => Fail(nameof(SetGlobalVectorArray));
    public void SetGlobalVectorArray(int nameID, List<Vector4> values) => Fail(nameof(SetGlobalVectorArray));
    public void SetGlobalVectorArray(int nameID, Vector4[] values) => Fail(nameof(SetGlobalVectorArray));

    public void SetGlobalMatrixArray(string name, Matrix4x4[] values) => Fail(nameof(SetGlobalMatrixArray));
    public void SetGlobalMatrixArray(string name, List<Matrix4x4> values) => Fail(nameof(SetGlobalMatrixArray));
    public void SetGlobalMatrixArray(int nameID, List<Matrix4x4> values) => Fail(nameof(SetGlobalMatrixArray));
    public void SetGlobalMatrixArray(int nameID, Matrix4x4[] values) => Fail(nameof(SetGlobalMatrixArray));
}
