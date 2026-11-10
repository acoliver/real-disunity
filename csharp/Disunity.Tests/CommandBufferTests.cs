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
        Console.WriteLine("PASS: CommandBuffer name, Clear, Release, the members that cannot record yet and ComputeBuffer Release");
        Console.WriteLine("PASS: CommandBuffer.DrawMesh refuses a null or destroyed mesh or material, a sub-mesh outside the mesh, a pass below -1 and a property block with values");
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
        buffer.Release();
        Throws<ObjectDisposedException>(() => buffer.ClearRandomWriteTargets(), "a released buffer", "CommandBuffer");
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
        var block = new MaterialPropertyBlock();
        block.SetFloat("_Fade", 0.5f);
        Throws<NotSupportedException>(() => buffer.DrawMesh(mesh, matrix, material, 0, 0, block), "a property block with a value", "CommandBuffer.DrawMesh(properties)");
        Ensure(buffer.commandCount == 5, "refused draws add nothing to the buffer");
        buffer.Release();
        Throws<ObjectDisposedException>(() => buffer.DrawMesh(mesh, matrix, material), "a released buffer", "CommandBuffer");
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
