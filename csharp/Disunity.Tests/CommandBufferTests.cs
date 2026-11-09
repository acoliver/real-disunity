using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>CommandBuffer name and lifetime, its recording members, and ComputeBuffer release.</summary>
internal static class CommandBufferTests
{
    public static void Run()
    {
        Naming();
        Lifetime();
        Recording();
        Compute();
        Console.WriteLine("PASS: CommandBuffer name, Clear, Release, the members that cannot record yet and ComputeBuffer Release");
    }

    private static void Naming()
    {
        var buffer = new CommandBuffer();
        Ensure(buffer.name == "", "the name defaults to empty");
        buffer.name = "Outline";
        Ensure(buffer.name == "Outline", "the name is settable");
        buffer.name = null!;
        Ensure(buffer.name == "", "a null name becomes empty");
        Ensure(ModuleFacades.Forwards(typeof(DisunityHost).Assembly, "UnityEngine.Rendering.CommandBuffer"), "the module facades forward CommandBuffer");
    }

    private static void Lifetime()
    {
        var buffer = new CommandBuffer { name = "kept" };
        buffer.Clear();
        buffer.Release();
        buffer.Release();
        buffer.name = "after";
        Ensure(buffer.name == "after", "the name works after Release");
        Throws<ObjectDisposedException>(buffer.Clear, "Clear after Release");
        Throws<ObjectDisposedException>(() => buffer.BeginSample("x"), "BeginSample after Release");
        Throws<ObjectDisposedException>(() => buffer.SetViewport(default), "SetViewport after Release");
    }

    private static void Recording()
    {
        var buffer = new CommandBuffer();
        var material = new Material(Material.SpriteLit);
        var probes = new (string Member, Action Call)[]
        {
            ("BeginSample", () => buffer.BeginSample("s")),
            ("EndSample", () => buffer.EndSample("s")),
            ("DrawMesh", () => buffer.DrawMesh(new Mesh(), Matrix4x4.identity, material, 0, 0)),
            ("DrawMeshInstancedIndirect", () => buffer.DrawMeshInstancedIndirect(new Mesh(), 0, material, 0, new ComputeBuffer(1, 4), 0, new MaterialPropertyBlock())),
            ("SetViewport", () => buffer.SetViewport(new Rect(0, 0, 1, 1))),
        };
        foreach (var (member, call) in probes)
            Throws<NotSupportedException>(call, member, $"CommandBuffer.{member}", "cannot record this member yet");
        Ensure(buffer.commandCount == 0, "members that cannot record add nothing to the buffer");
        buffer.ClearRandomWriteTargets();
        Ensure(buffer.commandCount == 0, "clearing random-write targets records nothing, since none can be bound");
        buffer.Release();
        Throws<ObjectDisposedException>(() => buffer.ClearRandomWriteTargets(), "a released buffer", "CommandBuffer");
        ExecutionFlagsAreSetOnAnEmptyBuffer();
        FencesHavePassedAndRecordNothing();
    }

    private static void FencesHavePassedAndRecordNothing()
    {
        var buffer = new CommandBuffer();
        GraphicsFence[] fences = [buffer.CreateAsyncGraphicsFence(), buffer.CreateAsyncGraphicsFence(SynchronisationStage.VertexProcessing),
            buffer.CreateGraphicsFence(GraphicsFenceType.AsyncQueueSynchronisation, SynchronisationStageFlags.ComputeProcessing)];
        Ensure(fences.All(fence => fence.passed), "a command buffer's fence has passed");
        buffer.WaitOnAsyncGraphicsFence(fences[0]);
        buffer.WaitOnAsyncGraphicsFence(fences[1], SynchronisationStage.PixelProcessing);
        buffer.WaitOnAsyncGraphicsFence(fences[2], SynchronisationStageFlags.AllGPUOperations);
        Ensure(buffer.commandCount == 0, "fences and waits record nothing");
        buffer.Release();
        Throws<ObjectDisposedException>(() => buffer.CreateAsyncGraphicsFence(), "a fence on a released buffer", "CommandBuffer");
        Throws<ObjectDisposedException>(() => buffer.CreateGraphicsFence(GraphicsFenceType.CPUSynchronisation, SynchronisationStageFlags.PixelProcessing),
            "a fence on a released buffer", "CommandBuffer");
        Throws<ObjectDisposedException>(() => buffer.WaitOnAsyncGraphicsFence(default, SynchronisationStageFlags.PixelProcessing), "a wait on a released buffer", "CommandBuffer");
    }

    private static void ExecutionFlagsAreSetOnAnEmptyBuffer()
    {
        var buffer = new CommandBuffer();
        Ensure(buffer.ExecutionFlags == CommandBufferExecutionFlags.None, "a new buffer has no execution flags");
        buffer.SetExecutionFlags(CommandBufferExecutionFlags.AsyncCompute);
        Ensure(buffer.ExecutionFlags == CommandBufferExecutionFlags.AsyncCompute && buffer.commandCount == 0, "the flag is kept and records nothing");
        Throws<ArgumentException>(() => buffer.SetExecutionFlags((CommandBufferExecutionFlags)1), "an unknown flag", "SetExecutionFlags");
        buffer.ClearRandomWriteTargets();
        buffer.SetGlobalFloat("_Probe", 1);
        Throws<InvalidOperationException>(() => buffer.SetExecutionFlags(CommandBufferExecutionFlags.None), "flags on a buffer with commands", "SetExecutionFlags");
        buffer.Clear();
        buffer.SetExecutionFlags(CommandBufferExecutionFlags.None);
        Ensure(buffer.ExecutionFlags == CommandBufferExecutionFlags.None, "a cleared buffer takes new flags");
        buffer.Release();
        Throws<ObjectDisposedException>(() => buffer.SetExecutionFlags(CommandBufferExecutionFlags.None), "flags on a released buffer", "CommandBuffer");
    }

    private static void Compute()
    {
        var buffer = new ComputeBuffer(4, 16, ComputeBufferType.Structured, ComputeBufferMode.Immutable);
        Ensure(buffer.count == 4, "count");
        Ensure(new ComputeBuffer(2, 4).count == 2 && new ComputeBuffer(3, 4, ComputeBufferType.Raw).count == 3, "the other constructors");
        Throws<ArgumentException>(() => new ComputeBuffer(0, 4), "a zero count");
        Throws<ArgumentException>(() => new ComputeBuffer(4, 0), "a zero stride");
        buffer.Release();
        buffer.Release();
        buffer.Dispose();
        Ensure(buffer.count == 4, "count survives Release");
        Throws<ObjectDisposedException>(() => buffer.SetData(new int[4]), "SetData after Release");
    }
}
