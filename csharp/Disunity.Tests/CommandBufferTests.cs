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
        DrawMeshArguments();
        Compute();
        GlobalBuffersBindWhenTheBufferRuns();
        Console.WriteLine("PASS: CommandBuffer name, Clear, Release, the members that cannot record yet and ComputeBuffer Release");
        Console.WriteLine("PASS: CommandBuffer.DrawMesh refuses a null or destroyed mesh or material, a sub-mesh outside the mesh, and a pass below -1, and records a property block with values");
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
            ("DrawMeshInstancedIndirect", () => buffer.DrawMeshInstancedIndirect(new Mesh(), 0, material, 0, new ComputeBuffer(1, 4), 0, new MaterialPropertyBlock())),
            ("SetViewport", () => buffer.SetViewport(new Rect(0, 0, 1, 1))),
        };
        foreach (var (member, call) in probes)
            Throws<NotSupportedException>(call, member, $"CommandBuffer.{member}", "cannot record this member yet");
        Ensure(buffer.commandCount == 0, "members that cannot record add nothing to the buffer");
        buffer.ClearRandomWriteTargets();
        Ensure(buffer.commandCount == 0, "clearing random-write targets records nothing, since none can be bound");
        buffer.SetInstanceMultiplier(1);
        Ensure(buffer.commandCount == 0, "an instance multiplier of 1 records nothing");
        foreach (var multiplier in new uint[] { 0, 2, uint.MaxValue })
            Throws<NotSupportedException>(() => buffer.SetInstanceMultiplier(multiplier), $"instance multiplier {multiplier}",
                $"SetInstanceMultiplier({multiplier})", "only 1 is supported");
        Ensure(buffer.commandCount == 0, "a refused instance multiplier records nothing");
        buffer.Release();
        Throws<ObjectDisposedException>(() => buffer.ClearRandomWriteTargets(), "a released buffer", "CommandBuffer");
        Throws<ObjectDisposedException>(() => buffer.SetInstanceMultiplier(1), "an instance multiplier on a released buffer", "CommandBuffer");
        ExecutionFlagsAreSetOnAnEmptyBuffer();
        ShadingRatesRecordNothing();
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

    private static Mesh Triangle(int subMeshes = 1)
    {
        var mesh = new Mesh();
        mesh.vertices = [new Vector3(0, 0, 0), new Vector3(1, 0, 0), new Vector3(0, 1, 0)];
        mesh.triangles = [0, 1, 2];
        mesh.subMeshCount = subMeshes;
        return mesh;
    }

    private static void DrawMeshArguments()
    {
        var buffer = new CommandBuffer();
        var material = new Material(Material.SpriteLit);
        var mesh = Triangle(2);
        var matrix = Matrix4x4.identity;
        buffer.DrawMesh(mesh, matrix, material);
        buffer.DrawMesh(mesh, matrix, material, 1);
        buffer.DrawMesh(mesh, matrix, material, 1, 2);
        buffer.DrawMesh(mesh, matrix, material, 0, -1, null);
        buffer.DrawMesh(mesh, matrix, material, 0, 0, new MaterialPropertyBlock());
        Ensure(buffer.commandCount == 5, "every overload records one command, and an empty or null property block is accepted");
        Throws<ArgumentNullException>(() => buffer.DrawMesh(null!, matrix, material, 0, 0), "a null mesh");
        Throws<ArgumentNullException>(() => buffer.DrawMesh(mesh, matrix, null!, 0, 0), "a null material");
        var gone = Triangle();
        UnityEngine.Object.DestroyImmediate(gone);
        Throws<ArgumentException>(() => buffer.DrawMesh(gone, matrix, material, 0, 0), "a destroyed mesh", "destroyed");
        var goneMaterial = new Material(Material.SpriteLit);
        UnityEngine.Object.DestroyImmediate(goneMaterial);
        Throws<ArgumentException>(() => buffer.DrawMesh(mesh, matrix, goneMaterial, 0, 0), "a destroyed material", "destroyed");
        Throws<ArgumentException>(() => buffer.DrawMesh(mesh, matrix, material, 2, 0), "a sub-mesh past the last", "sub-mesh 2");
        Throws<ArgumentException>(() => buffer.DrawMesh(mesh, matrix, material, -1, 0), "a negative sub-mesh", "sub-mesh -1");
        Throws<ArgumentException>(() => buffer.DrawMesh(new Mesh(), matrix, material, 1, 0), "the second sub-mesh of a new mesh", "sub-mesh 1");
        Throws<ArgumentException>(() => buffer.DrawMesh(mesh, matrix, material, 0, -2), "a pass below -1", "pass -2");
        Ensure(buffer.commandCount == 5, "refused draws add nothing to the buffer");
        var block = new MaterialPropertyBlock();
        block.SetFloat("_Fade", 0.5f);
        buffer.DrawMesh(mesh, matrix, material, 0, 0, block);
        Ensure(buffer.commandCount == 6, "a property block with a value is recorded with its draw");
        buffer.Release();
        Throws<ObjectDisposedException>(() => buffer.DrawMesh(mesh, matrix, material), "a released buffer", "CommandBuffer");
    }

    private static void ShadingRatesRecordNothing()
    {
        var buffer = new CommandBuffer();
        buffer.SetShadingRateFragmentSize(ShadingRateFragmentSize.FragmentSize2x2);
        buffer.SetShadingRateCombiner(ShadingRateCombinerStage.Fragment, ShadingRateCombiner.Max);
        buffer.SetShadingRateImage(new RenderTargetIdentifier(BuiltinRenderTextureType.CameraTarget));
        buffer.ResetShadingRate();
        buffer.SetFoveatedRenderingMode(FoveatedRenderingMode.Enabled);
        buffer.SetFoveatedRenderingMode(FoveatedRenderingMode.Disabled);
        Ensure(buffer.commandCount == 0, "shading-rate and foveated rendering members record nothing");
        buffer.Release();
        Throws<ObjectDisposedException>(() => buffer.ResetShadingRate(), "a shading-rate reset on a released buffer", "CommandBuffer");
        Throws<ObjectDisposedException>(() => buffer.SetFoveatedRenderingMode(FoveatedRenderingMode.Disabled), "a foveated mode on a released buffer", "CommandBuffer");
        Throws<ObjectDisposedException>(() => buffer.SetShadingRateFragmentSize(ShadingRateFragmentSize.FragmentSize1x1),
            "a fragment size on a released buffer", "CommandBuffer");
        Throws<ObjectDisposedException>(() => buffer.SetShadingRateCombiner(ShadingRateCombinerStage.Primitive, ShadingRateCombiner.Keep),
            "a combiner on a released buffer", "CommandBuffer");
        Throws<ObjectDisposedException>(() => buffer.SetShadingRateImage(new RenderTargetIdentifier(BuiltinRenderTextureType.CameraTarget)),
            "a shading-rate image on a released buffer", "CommandBuffer");
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

    private static void GlobalBuffersBindWhenTheBufferRuns()
    {
        int constants = Shader.PropertyToID("_CommandBufferTestConstants"), structured = Shader.PropertyToID("_CommandBufferTestStructured");
        using var data = new GraphicsBuffer(GraphicsBuffer.Target.Constant, 16, 16);
        using var items = new ComputeBuffer(4, 4);
        var buffer = new CommandBuffer();
        buffer.SetGlobalConstantBuffer(data, constants, 64, 128);
        buffer.SetGlobalBuffer(structured, items);
        Ensure(buffer.commandCount == 2, "each binding is one command");
        Ensure(!Shader.Globals.ConstantBuffers.ContainsKey(constants) && !Shader.Globals.Buffers.ContainsKey(structured), "recording binds nothing yet");
        foreach (var send in buffer.Commands()) send();
        Ensure(Shader.Globals.ConstantBuffers[constants] == new ConstantBufferBinding(data, 64, 128), "the constant buffer is bound with its range when the buffer runs");
        Ensure(ReferenceEquals(Shader.Globals.Buffers[structured], items), "the structured buffer is bound when the buffer runs");
        buffer.SetGlobalConstantBuffer(items, "_CommandBufferTestConstants", 0, 16);
        buffer.SetGlobalBuffer("_CommandBufferTestStructured", data);
        foreach (var send in buffer.Commands()[2..]) send();
        Ensure(Shader.Globals.ConstantBuffers[constants] == new ConstantBufferBinding(items, 0, 16), "a later binding by name replaces the earlier one");
        Ensure(ReferenceEquals(Shader.Globals.Buffers[structured], data), "a later buffer by name replaces the earlier one");
        Throws<ArgumentNullException>(() => buffer.SetGlobalConstantBuffer((GraphicsBuffer)null!, constants, 0, 16), "a null constant buffer", "buffer");
        Throws<ArgumentNullException>(() => buffer.SetGlobalBuffer(structured, (ComputeBuffer)null!), "a null buffer", "value");
        Throws<ArgumentOutOfRangeException>(() => buffer.SetGlobalConstantBuffer(data, constants, -1, 16), "a negative offset", "offset");
        Throws<ArgumentOutOfRangeException>(() => buffer.SetGlobalConstantBuffer(data, constants, 0, -16), "a negative size", "size");
        Ensure(buffer.commandCount == 4, "a refused binding records nothing");
        Shader.Globals.ConstantBuffers.Remove(constants);
        Shader.Globals.Buffers.Remove(structured);
        buffer.Release();
        Throws<ObjectDisposedException>(() => buffer.SetGlobalConstantBuffer(data, constants, 0, 16), "a binding on a released buffer", "CommandBuffer");
    }
}
