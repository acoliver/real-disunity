using System.Diagnostics.CodeAnalysis;
using UnityEngine.Experimental.Rendering;

namespace UnityEngine.Rendering;

/// <summary>
/// A list of rendering commands. Recording a command appends it to the buffer's own list and touches no world state: nothing
/// reaches the world until <see cref="Graphics.ExecuteCommandBuffer"/> or a <see cref="ScriptableRenderContext"/> sends the
/// commands, in order. Sending does not empty the buffer, so a buffer sent twice delivers its commands twice; <see cref="Clear"/>
/// empties it. The members that record are in CommandBufferTargets.cs, CommandBufferGlobals.cs, CommandBufferCamera.cs,
/// CommandBufferTemporaries.cs and CommandBufferRendererLists.cs.
/// The members that would draw indexed procedural geometry, set a global float array, set the viewport or open profiler
/// samples have no record yet and throw <see cref="NotSupportedException"/> naming themselves. <see cref="DrawMesh(Mesh, Matrix4x4, Material)"/>
/// is recorded for the host to draw (see CommandBufferMesh.cs), and so are the non-indexed procedural draws (see
/// CommandBufferProcedural.cs). Global vector and matrix arrays are recorded in order (see
/// CommandBufferArrays.cs).
/// After <see cref="Release"/> every member except <see cref="name"/> and Release throws <see cref="ObjectDisposedException"/>;
/// releasing twice is allowed. The name starts empty, which is the engine's choice, and a null name becomes empty.
/// </summary>
public sealed partial class CommandBuffer : IDisposable
{
    private readonly List<Action> _commands = new();
    private string _name = "";
    private bool _released;

    public string name
    {
        get => _name;
        set => _name = value ?? "";
    }

    /// <summary>How many commands the buffer holds.</summary>
    public int commandCount
    {
        get
        {
            ThrowIfReleased();
            return _commands.Count;
        }
    }

    public void Clear()
    {
        ThrowIfReleased();
        _commands.Clear();
        _pass = null;
    }

    public void Release() => _released = true;

    public void Dispose() => Release();

    /// <summary>The commands in the order they were recorded; later records do not change the copy.</summary>
    internal Action[] Commands()
    {
        ThrowIfReleased();
        return _commands.ToArray();
    }

    /// <summary>Appends a command that sends one record to the world when the buffer is executed.</summary>
    private void Record(Action send)
    {
        ThrowIfReleased();
        _commands.Add(send);
    }

    public void BeginSample(string name) => Fail(nameof(BeginSample));
    public void EndSample(string name) => Fail(nameof(EndSample));

    public void DrawMeshInstancedIndirect(Mesh mesh, int submeshIndex, Material material, int shaderPass, ComputeBuffer bufferWithArgs,
        int argsOffset, MaterialPropertyBlock properties) => Fail(nameof(DrawMeshInstancedIndirect));

    public void SetViewport(Rect pixelRect) => Fail(nameof(SetViewport));

    /// <summary>Records nothing: the engine binds no random-write targets, so there are none to clear. The render graph calls
    /// it after every pass.</summary>
    public void ClearRandomWriteTargets() => ThrowIfReleased();
    /// <summary>Records the view and projection that the draws after it use, until a camera setup or another change. It
    /// names no camera and leaves the bound targets as they are.</summary>
    public void SetViewProjectionMatrices(Matrix4x4 view, Matrix4x4 proj) =>
        Record(() => WorldGraphics.RecordSetViewProjection(view, proj));

    private void ThrowIfReleased() => ObjectDisposedException.ThrowIf(_released, this);

    [DoesNotReturn]
    private void Fail(string member)
    {
        ThrowIfReleased();
        throw new NotSupportedException($"CommandBuffer.{member}: the engine cannot record this member yet");
    }
}
