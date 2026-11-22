using System.Diagnostics.CodeAnalysis;
using UnityEngine.Experimental.Rendering;

namespace UnityEngine.Rendering;

/// <summary>
/// A list of rendering commands. Recording a command appends it to the buffer's own list and touches no world state: nothing
/// reaches the world until <see cref="Graphics.ExecuteCommandBuffer"/> or a <see cref="ScriptableRenderContext"/> sends the
/// commands, in order. Sending does not empty the buffer, so a buffer sent twice delivers its commands twice; <see cref="Clear"/>
/// empties it. The members that record are in CommandBufferTargets.cs, CommandBufferGlobals.cs, CommandBufferCamera.cs,
/// CommandBufferTemporaries.cs and CommandBufferRendererLists.cs.
/// The members that would draw indexed procedural geometry or set a global float array have no record yet and throw
/// <see cref="NotSupportedException"/> naming themselves. <see cref="DrawMesh(Mesh, Matrix4x4, Material)"/>
/// is recorded for the host to draw (see CommandBufferMesh.cs), and so are <see cref="DrawMeshInstancedIndirect"/> (see
/// CommandBufferMeshIndirect.cs), the renderer draws (see CommandBufferRenderer.cs), <see cref="SetViewport"/> and the non-indexed
/// procedural draws (see CommandBufferProcedural.cs). The globals a buffer sets, arrays included, are written when
/// the buffer runs, as the matching <c>Shader.SetGlobal*</c> calls write them (see CommandBufferGlobals.cs and
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

    /// <summary>Records nothing: the engine has no profiler capture, so a sample marks no span. A render graph pass and
    /// third-party passes open and close samples around their work every frame.</summary>
    public void BeginSample(string name) => ThrowIfReleased();

    /// <summary>Records nothing, as <see cref="BeginSample(string)"/>; an end without a begin is not checked.</summary>
    public void EndSample(string name) => ThrowIfReleased();

    /// <summary>Records the rectangle, in pixels of the bound targets from the bottom left, that the draws after it cover.
    /// <see cref="SetRenderTarget(RenderTargetIdentifier)"/> and a camera setup reset it to the whole target. A component that
    /// is not finite or a negative width or height throws <see cref="ArgumentException"/> and records nothing.</summary>
    public void SetViewport(Rect pixelRect)
    {
        ThrowIfReleased();
        if (!float.IsFinite(pixelRect.x) || !float.IsFinite(pixelRect.y) || !float.IsFinite(pixelRect.width) || !float.IsFinite(pixelRect.height)
            || pixelRect.width < 0f || pixelRect.height < 0f)
            throw new ArgumentException($"CommandBuffer.SetViewport: the rectangle {pixelRect} needs finite components and a width and height of 0 or more", nameof(pixelRect));
        Record(() => WorldGraphics.RecordSetViewport(pixelRect));
    }

    /// <summary>Records nothing: the engine binds no random-write targets, so there are none to clear. The render graph calls
    /// it after every pass.</summary>
    public void ClearRandomWriteTargets() => ThrowIfReleased();

    /// <summary>A multiplier of 1 records nothing: every draw already runs the instances it names, which is what a multiplier
    /// of 1 means. A larger multiplier repeats each draw's instances for the views of single-pass stereo rendering, which the
    /// engine does not do, so it throws <see cref="NotSupportedException"/> naming the value and records nothing. A released
    /// buffer throws <see cref="ObjectDisposedException"/> first.</summary>
    public void SetInstanceMultiplier(uint multiplier)
    {
        ThrowIfReleased();
        if (multiplier != 1)
            throw new NotSupportedException($"CommandBuffer.SetInstanceMultiplier({multiplier}): only 1 is supported; the engine does not render several views per draw");
    }

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
