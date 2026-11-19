using System.Buffers.Binary;

namespace UnityEngine.Rendering;

public sealed partial class CommandBuffer
{
    private const string IndirectMember = "CommandBuffer.DrawMeshInstancedIndirect";
    private const int ArgsInts = 5;
    private const int ArgsBytes = ArgsInts * sizeof(int);

    /// <summary>Records a draw of one sub-mesh as many times as the args buffer says when the command buffer runs. The five integers
    /// at <paramref name="argsOffset"/> are the index count per instance, the instance count, the start index, the base vertex and
    /// the start instance. The call refuses what <see cref="DrawMesh(Mesh, Matrix4x4, Material, int, int, MaterialPropertyBlock?)"/>
    /// refuses, and a null args buffer or an offset that is negative, not a multiple of 4 or leaves fewer than five integers throws
    /// <see cref="ArgumentException"/>. The draw uses the identity matrix and the property block as it is copied now.
    /// <para>The args are read each time the buffer runs, from the contents the CPU wrote to the buffer, and the draw is recorded as a
    /// <c>DrawMesh</c> of that many instances; zero instances is a draw of nothing that keeps its place. The engine draws whole
    /// sub-meshes from instance 0, so an index count or start index that is not the sub-mesh's, a base vertex that is not its base
    /// vertex, a start instance other than 0 or a negative instance count throws <see cref="NotSupportedException"/> naming the
    /// field and records nothing. A buffer that is bound to a compute kernel with <see cref="ComputeShader.SetBuffer(int, string, ComputeBuffer)"/>
    /// also throws <see cref="NotSupportedException"/> then: the kernel may write it on the GPU, the engine does not know whether it did,
    /// and the CPU copy may not be current. A buffer a kernel wrote to without being bound that way is not detected. A released
    /// args buffer throws <see cref="ObjectDisposedException"/> when the buffer runs.</para></summary>
    public void DrawMeshInstancedIndirect(Mesh mesh, int submeshIndex, Material material, int shaderPass, ComputeBuffer bufferWithArgs,
        int argsOffset, MaterialPropertyBlock? properties)
    {
        ThrowIfReleased();
        ArgumentNullException.ThrowIfNull(mesh);
        ArgumentNullException.ThrowIfNull(material);
        var block = CheckedBlock(mesh, material, (submeshIndex, shaderPass), properties, IndirectMember);
        ThrowIfArgsCannotBeHeld(bufferWithArgs, argsOffset);
        Record(() => DrawNow(mesh, Matrix4x4.identity, material, (submeshIndex, shaderPass), block, IndirectMember,
            () => InstanceCount(bufferWithArgs, argsOffset, mesh, submeshIndex)));
    }

    private static void ThrowIfArgsCannotBeHeld(ComputeBuffer? args, int argsOffset)
    {
        if (args is null) throw new ArgumentException($"{IndirectMember}: the args buffer is null", "bufferWithArgs");
        if (argsOffset < 0 || argsOffset % sizeof(int) != 0)
            throw new ArgumentException($"{IndirectMember}: the args offset {argsOffset} is not a non-negative multiple of 4", nameof(argsOffset));
        var size = (long)args.count * args.stride;
        if (argsOffset + (long)ArgsBytes > size)
            throw new ArgumentException($"{IndirectMember}: {ArgsInts} integers at byte {argsOffset} do not fit an args buffer of {size} bytes", nameof(argsOffset));
    }

    /// <summary>The instance count of the args at <paramref name="argsOffset"/>, after checking that they draw the whole sub-mesh
    /// from instance 0 and that the CPU copy of the buffer is the one the draw will use.</summary>
    private static uint InstanceCount(ComputeBuffer args, int argsOffset, Mesh mesh, int submeshIndex)
    {
        if (args.BoundToKernel)
            throw new NotSupportedException($"{IndirectMember}: the args buffer is bound to a compute kernel, which may write it on the GPU, so the engine cannot know its contents");
        var bytes = new byte[ArgsBytes];
        args.ReadWritten(argsOffset, bytes);
        int Argument(int index) => BinaryPrimitives.ReadInt32LittleEndian(bytes.AsSpan(index * sizeof(int)));
        RequireEqual("index count", Argument(0), (int)mesh.GetIndexCount(submeshIndex), submeshIndex);
        RequireEqual("start index", Argument(2), (int)mesh.GetIndexStart(submeshIndex), submeshIndex);
        RequireEqual("base vertex", Argument(3), unchecked((int)mesh.GetBaseVertex(submeshIndex)), submeshIndex);
        RequireEqual("start instance", Argument(4), 0, submeshIndex);
        var instances = Argument(1);
        if (instances < 0) throw new NotSupportedException($"{IndirectMember}: the instance count {instances} is negative");
        return (uint)instances;
    }

    private static void RequireEqual(string field, int given, int expected, int submeshIndex)
    {
        if (given != expected)
            throw new NotSupportedException($"{IndirectMember}: the {field} {given} is not {expected}, the {field} of sub-mesh {submeshIndex}; only whole sub-meshes from instance 0 are drawn");
    }
}
