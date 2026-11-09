namespace UnityEngine.Rendering;

/// <summary>
/// How a command buffer is meant to run. A render graph marks the buffers it may send to an async compute queue. The engine
/// runs every buffer in order on one queue, so the flags are kept and change nothing about how the buffer runs. They can
/// only be set while the buffer is empty, and only <see cref="CommandBufferExecutionFlags.AsyncCompute"/> is a known flag.
/// </summary>
public sealed partial class CommandBuffer
{
    private CommandBufferExecutionFlags _executionFlags;

    /// <summary>The flags <see cref="SetExecutionFlags"/> set, <see cref="CommandBufferExecutionFlags.None"/> at first.</summary>
    internal CommandBufferExecutionFlags ExecutionFlags => _executionFlags;

    public void SetExecutionFlags(CommandBufferExecutionFlags flags)
    {
        ThrowIfReleased();
        if ((flags & ~CommandBufferExecutionFlags.AsyncCompute) != 0)
            throw new ArgumentException($"CommandBuffer.SetExecutionFlags: {(int)flags} holds a flag other than AsyncCompute", nameof(flags));
        if (_commands.Count != 0)
            throw new InvalidOperationException("CommandBuffer.SetExecutionFlags: the buffer already holds commands; set the flags while it is empty");
        _executionFlags = flags;
    }
}
