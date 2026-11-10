namespace UnityEngine.Rendering;

/// <summary>
/// Global buffer bindings on a command buffer. The engine keeps buffer contents in managed memory and the world holds no
/// GPU buffers yet, so a binding is recorded as a command that sets the shaders' global binding when the buffer executes,
/// in order with its other commands, as <see cref="Shader.SetGlobalBuffer(int, GraphicsBuffer)"/> and
/// <see cref="Shader.SetGlobalConstantBuffer(int, GraphicsBuffer, int, int)"/> do at once. A render pipeline binds its
/// per-frame constant buffers this way. A null buffer, or an offset or size below zero, throws when the command is recorded.
/// </summary>
public sealed partial class CommandBuffer
{
    public void SetGlobalBuffer(string name, ComputeBuffer value) => SetGlobalBuffer(Shader.PropertyToID(name), value);

    public void SetGlobalBuffer(int nameID, ComputeBuffer value)
    {
        ArgumentNullException.ThrowIfNull(value);
        Record(() => Shader.SetGlobalBuffer(nameID, value));
    }

    public void SetGlobalBuffer(string name, GraphicsBuffer value) => SetGlobalBuffer(Shader.PropertyToID(name), value);

    public void SetGlobalBuffer(int nameID, GraphicsBuffer value)
    {
        ArgumentNullException.ThrowIfNull(value);
        Record(() => Shader.SetGlobalBuffer(nameID, value));
    }

    public void SetGlobalConstantBuffer(ComputeBuffer buffer, string name, int offset, int size) =>
        SetGlobalConstantBuffer(buffer, Shader.PropertyToID(name), offset, size);

    public void SetGlobalConstantBuffer(ComputeBuffer buffer, int nameID, int offset, int size)
    {
        ArgumentNullException.ThrowIfNull(buffer);
        CheckRange(offset, size);
        Record(() => Shader.SetGlobalConstantBuffer(nameID, buffer, offset, size));
    }

    public void SetGlobalConstantBuffer(GraphicsBuffer buffer, string name, int offset, int size) =>
        SetGlobalConstantBuffer(buffer, Shader.PropertyToID(name), offset, size);

    public void SetGlobalConstantBuffer(GraphicsBuffer buffer, int nameID, int offset, int size)
    {
        ArgumentNullException.ThrowIfNull(buffer);
        CheckRange(offset, size);
        Record(() => Shader.SetGlobalConstantBuffer(nameID, buffer, offset, size));
    }

    private static void CheckRange(int offset, int size)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(offset);
        ArgumentOutOfRangeException.ThrowIfNegative(size);
    }
}
