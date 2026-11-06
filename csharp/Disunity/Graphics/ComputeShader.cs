using Disunity.Hosting;

namespace UnityEngine;

/// <summary>
/// A compute shader asset. The player's plan names its kernels in index order and the file and object their code is
/// in; the world holds the values bound to each kernel and records every <see cref="Dispatch"/> in call order with the
/// other offscreen work. A host that executes the work reads the code from the player's data and runs the dispatches
/// when the engine asks it to flush (see <see cref="ComputeBuffer.GetData(Array)"/>); without a host executor the
/// dispatches do nothing and a buffer holds what the CPU wrote.
/// <para>
/// <see cref="SetInt"/>, <see cref="SetFloat"/>, <see cref="SetVector"/> and <see cref="SetMatrix"/> set values of the
/// whole shader; <see cref="SetBuffer"/>, <see cref="SetTexture"/> and <see cref="SetTextureFromGlobal"/> bind to one
/// kernel. A dispatch records the shader-wide values merged with its kernel's, as they are when it is called; later
/// calls leave it as it was. The values are sent to the world when they are set, so a world unloaded and loaded again
/// starts without them. <see cref="SetFloats"/> and the keyword and constant buffer members throw
/// <see cref="NotSupportedException"/> naming the member: the world holds no float arrays or compute keywords yet.
/// </para>
/// </summary>
public sealed class ComputeShader : Object
{
    private readonly string _file;
    private readonly long _pathId;
    private readonly (string Name, uint[]? Size)[] _kernels;
    private WorldGraphics.Link _world;

    private ComputeShader(string name, string file, long pathId, (string Name, uint[]? Size)[] kernels)
    {
        this.name = name;
        (_file, _pathId, _kernels) = (file, pathId, kernels);
    }

    /// <summary>A compute shader of the player's plan: its name, where its kernels are and the kernels in index order, each with the
    /// thread group size the plan states or null.</summary>
    internal static ComputeShader FromPlan(string name, string file, long pathId, IEnumerable<(string Name, uint[]? Size)> kernels) =>
        new(name, file, pathId, kernels.ToArray());

    /// <summary>The shader's resource in the loaded world, made on first use.</summary>
    internal WorldGraphics.Link WorldLink
    {
        get
        {
            if (!_world.IsCurrent) _world = WorldCompute.CreateShader(name, _file, _pathId, _kernels);
            return _world;
        }
    }

    private static NotSupportedException Unsupported(string member, string reason) =>
        new($"ComputeShader.{member} is not supported yet: {reason}");

    private int Kernel(int index, string member)
    {
        if (index < 0 || index >= _kernels.Length)
            throw new ArgumentOutOfRangeException(nameof(index), $"ComputeShader.{member}: kernel {index} does not exist in '{name}', which has {_kernels.Length}");
        return index;
    }

    /// <summary>The index of the kernel of this name; throws when the shader has none.</summary>
    public int FindKernel(string kernelName)
    {
        ArgumentNullException.ThrowIfNull(kernelName);
        var index = Array.FindIndex(_kernels, kernel => kernel.Name == kernelName);
        return index >= 0 ? index : throw new ArgumentException($"ComputeShader.FindKernel: '{name}' has no kernel '{kernelName}'", nameof(kernelName));
    }

    public bool HasKernel(string kernelName)
    {
        ArgumentNullException.ThrowIfNull(kernelName);
        return Array.Exists(_kernels, kernel => kernel.Name == kernelName);
    }

    /// <summary>The thread group size of a kernel: the size the plan states, or the one the host executor stated. A size nobody
    /// stated is asked of the host by flushing, and still unknown after that it throws <see cref="NotSupportedException"/>.</summary>
    public void GetKernelThreadGroupSizes(int kernelIndex, out uint x, out uint y, out uint z)
    {
        Kernel(kernelIndex, nameof(GetKernelThreadGroupSizes));
        if (_kernels[kernelIndex].Size is { } planned)
        {
            (x, y, z) = (planned[0], planned[1], planned[2]);
            return;
        }
        var link = WorldLink;
        var size = WorldCompute.KernelSizes(link)[kernelIndex];
        if (size.X == 0)
        {
            WorldCompute.Flush();
            size = WorldCompute.KernelSizes(link)[kernelIndex];
        }
        if (size.X == 0)
            throw Unsupported(nameof(GetKernelThreadGroupSizes), $"the thread group size of kernel '{_kernels[kernelIndex].Name}' of '{name}' is stated by neither the player's data nor the host");
        (x, y, z) = size;
    }

    public void SetInt(string name, int val) => WorldCompute.SetInt(WorldLink, WorldCompute.ShaderWide, name, val);
    public void SetInt(int nameID, int val) => SetInt(NameOf(nameID), val);
    public void SetBool(string name, bool val) => SetInt(name, val ? 1 : 0);
    public void SetFloat(string name, float val) => WorldCompute.SetFloat(WorldLink, WorldCompute.ShaderWide, name, val);
    public void SetFloat(int nameID, float val) => SetFloat(NameOf(nameID), val);
    public void SetVector(string name, Vector4 val) => WorldCompute.SetVector(WorldLink, WorldCompute.ShaderWide, name, val);
    public void SetVector(int nameID, Vector4 val) => SetVector(NameOf(nameID), val);
    public void SetMatrix(string name, Matrix4x4 val) => WorldCompute.SetMatrix(WorldLink, WorldCompute.ShaderWide, name, val);
    public void SetMatrix(int nameID, Matrix4x4 val) => SetMatrix(NameOf(nameID), val);

    public void SetFloats(string name, params float[] values) => throw Unsupported(nameof(SetFloats), "the world holds no float arrays");
    public void SetFloats(int nameID, params float[] values) => throw Unsupported(nameof(SetFloats), "the world holds no float arrays");

    public void SetBuffer(int kernelIndex, string name, ComputeBuffer buffer)
    {
        Kernel(kernelIndex, nameof(SetBuffer));
        ArgumentNullException.ThrowIfNull(buffer);
        WorldCompute.BindBuffer(WorldLink, (uint)kernelIndex, name, buffer.WorldLink);
    }

    public void SetBuffer(int kernelIndex, int nameID, ComputeBuffer buffer) => SetBuffer(kernelIndex, NameOf(nameID), buffer);

    /// <summary>Binds a texture to a kernel: a <see cref="Texture2D"/> with pixels or a <see cref="RenderTexture"/>. Other textures bind as none.</summary>
    public void SetTexture(int kernelIndex, string name, Texture texture)
    {
        Kernel(kernelIndex, nameof(SetTexture));
        WorldCompute.SetTexture(WorldLink, kernelIndex, name, WorldGraphics.TextureOf(texture));
    }

    public void SetTexture(int kernelIndex, int nameID, Texture texture) => SetTexture(kernelIndex, NameOf(nameID), texture);

    /// <summary>Binds to a kernel the texture the global shader state holds under <paramref name="globalTextureName"/> when the dispatch
    /// runs, so a later <see cref="Shader.SetGlobalTexture(string, Texture)"/> is what the dispatch reads.</summary>
    public void SetTextureFromGlobal(int kernelIndex, string name, string globalTextureName)
    {
        Kernel(kernelIndex, nameof(SetTextureFromGlobal));
        ArgumentNullException.ThrowIfNull(globalTextureName);
        WorldCompute.SetTextureFromGlobal(WorldLink, kernelIndex, name, globalTextureName);
    }

    /// <summary>Records a dispatch of a kernel with the values it has now. The world checks the group counts (1 to 65535 each).</summary>
    public void Dispatch(int kernelIndex, int threadGroupsX, int threadGroupsY, int threadGroupsZ)
    {
        Kernel(kernelIndex, nameof(Dispatch));
        WorldCompute.Dispatch(WorldLink, (uint)kernelIndex, threadGroupsX, threadGroupsY, threadGroupsZ);
    }

    private static string NameOf(int nameID) =>
        Shader.PropertyName(nameID) ?? throw new ArgumentException($"ComputeShader: {nameID} is not a property ID this process handed out", nameof(nameID));

    public void EnableKeyword(string keyword) => throw Unsupported(nameof(EnableKeyword), "the engine has no compute shader keywords");
    public void DisableKeyword(string keyword) => throw Unsupported(nameof(DisableKeyword), "the engine has no compute shader keywords");
    public void SetConstantBuffer(string name, ComputeBuffer buffer, int offset, int size) => throw Unsupported(nameof(SetConstantBuffer), "the engine binds no constant buffers");
    public void DispatchIndirect(int kernelIndex, ComputeBuffer argsBuffer, uint argsOffset = 0) => throw Unsupported(nameof(DispatchIndirect), "the engine records only direct dispatches");
}
