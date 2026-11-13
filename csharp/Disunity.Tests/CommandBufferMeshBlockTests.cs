using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>CommandBuffer.DrawMesh with a property block that holds values records the draw with a copy of the block taken when
/// DrawMesh is called, through a real world and the public execution paths. Later edits to the block, and edits to the same
/// names on the material or the globals, change no recorded block; a null or empty block records none, a block holding an
/// array or a buffer is refused, and a block the world refuses leaves nothing staged for a later draw. The copied block and the
/// world's refusal are also checked through ScriptableRenderContext.Submit.</summary>
internal static class CommandBufferMeshBlockTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            ABlockIsRecordedWithItsDraw();
            TheBlockIsCopiedWhenDrawMeshIsCalled();
            ANullOrEmptyBlockRecordsNone();
            AStagedBlockAttachesToItsOwnDrawOnly();
            EveryValueKindTravels();
            ABlockWithAnArrayOrABufferIsRefused();
            ABlockTheWorldRefusesLeavesNothingForTheNextDraw();
            SubmitCarriesTheBlockAndClearsARefusedOne();
            AReleasedBufferIsRefusedFirst();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private const string Identity = "(1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1)";

    private static Mesh Triangle()
    {
        var mesh = new Mesh();
        mesh.vertices = [new Vector3(0, 0, 0), new Vector3(1, 0, 0), new Vector3(0, 1, 0)];
        mesh.triangles = [0, 1, 2];
        return mesh;
    }

    private static string Line(Mesh mesh, Material material, string block) =>
        $"DrawMesh mesh={mesh.RendererIdentity} matrix={Identity} material={material.WorldLink.Id} submesh=0 pass=0 block=[{block}]";

    private static string[] Execute(CommandBuffer buffer)
    {
        CommandWireReader.Take();
        Graphics.ExecuteCommandBuffer(buffer);
        return CommandWireReader.Take();
    }

    private static void ABlockIsRecordedWithItsDraw()
    {
        var (mesh, material) = (Triangle(), new Material(Material.SpriteLit));
        var block = new MaterialPropertyBlock();
        block.SetVector("_Tint", new Vector4(0.25f, 0.5f, 0.75f, 1));
        var buffer = new CommandBuffer();
        buffer.DrawMesh(mesh, Matrix4x4.identity, material, 0, 0, block);
        Expect.Sequence(new[] { Line(mesh, material, "_Tint=vector (0.25 0.5 0.75 1)") }, Execute(buffer),
            "the draw carries the block's value");
        Console.WriteLine("PASS: CommandBuffer.DrawMesh records a property block that holds values with its draw");
    }

    private static void TheBlockIsCopiedWhenDrawMeshIsCalled()
    {
        var (mesh, material) = (Triangle(), new Material(Material.SpriteLit));
        var block = new MaterialPropertyBlock();
        block.SetFloat("_Fade", 0.5f);
        var buffer = new CommandBuffer();
        buffer.DrawMesh(mesh, Matrix4x4.identity, material, 0, 0, block);
        block.SetFloat("_Fade", 0.75f);
        block.SetFloat("_Late", 1f);
        material.SetFloat("_Fade", 0.125f);
        Shader.SetGlobalFloat("_Fade", 0.0625f);
        Expect.Sequence(new[] { Line(mesh, material, "_Fade=float (0.5)") }, Execute(buffer),
            "edits after DrawMesh to the block, the material and the globals do not reach the recorded block");
        Console.WriteLine("PASS: a DrawMesh property block is copied when DrawMesh is called");
    }

    private static void ANullOrEmptyBlockRecordsNone()
    {
        var (mesh, material) = (Triangle(), new Material(Material.SpriteLit));
        var buffer = new CommandBuffer();
        buffer.DrawMesh(mesh, Matrix4x4.identity, material, 0, 0, null);
        buffer.DrawMesh(mesh, Matrix4x4.identity, material, 0, 0, new MaterialPropertyBlock());
        Expect.Sequence(new[] { Line(mesh, material, "-"), Line(mesh, material, "-") }, Execute(buffer), "no block values");
        Console.WriteLine("PASS: a null or empty DrawMesh property block records no block");
    }

    private static void AStagedBlockAttachesToItsOwnDrawOnly()
    {
        var (mesh, material) = (Triangle(), new Material(Material.SpriteLit));
        var block = new MaterialPropertyBlock();
        block.SetInteger("_Mode", 3);
        var buffer = new CommandBuffer();
        buffer.DrawMesh(mesh, Matrix4x4.identity, material, 0, 0, block);
        buffer.DrawMesh(mesh, Matrix4x4.identity, material, 0, 0);
        buffer.SetGlobalFloat("_After", 1);
        buffer.DrawMesh(mesh, Matrix4x4.identity, material, 0, 0, null);
        var records = Execute(buffer);
        Expect.Sequence(new[]
        {
            Line(mesh, material, "_Mode=int 3"), Line(mesh, material, "-"), "SetGlobal _After=float (1)", Line(mesh, material, "-"),
        }, records, "only the draw that named the block carries it");
        Console.WriteLine("PASS: a DrawMesh property block attaches to its own draw and to no later one");
    }

    private static void EveryValueKindTravels()
    {
        var (mesh, material) = (Triangle(), new Material(Material.SpriteLit));
        var block = new MaterialPropertyBlock();
        block.SetFloat("_A", 1.5f);
        block.SetInteger("_B", -2);
        block.SetInt("_G", 7);
        block.SetVector("_C", new Vector4(1, 2, 3, 4));
        block.SetMatrix("_D", Matrix4x4.identity);
        var buffer = new CommandBuffer();
        buffer.DrawMesh(mesh, Matrix4x4.identity, material, 0, 0, block);
        var line = Execute(buffer).Single();
        foreach (var part in new[] { "_A=float (1.5)", "_B=int -2", "_C=vector (1 2 3 4)", $"_D=matrix {Identity}", "_G=float (7)" })
            Ensure(line.Contains(part, StringComparison.Ordinal), $"the block carries {part}: {line}");
        Console.WriteLine("PASS: a DrawMesh property block carries floats, integers, vectors and matrices; SetInt is a float as on a material");
    }

    private static void ABlockTheWorldRefusesLeavesNothingForTheNextDraw()
    {
        var (mesh, material) = (Triangle(), new Material(Material.SpriteLit));
        var block = new MaterialPropertyBlock();
        block.SetFloat("_Good", 1);
        block.SetVector(new string('x', 300), Vector4.one);
        var refused = new CommandBuffer();
        refused.DrawMesh(mesh, Matrix4x4.identity, material, 0, 0, block);
        CommandWireReader.Take();
        var threw = false;
        try { Graphics.ExecuteCommandBuffer(refused); }
        catch (DisunityException) { threw = true; }
        Ensure(threw, "a block value with a name the world refuses fails the draw when the buffer runs");
        var leftover = CommandWireReader.Take();
        Ensure(leftover.Length == 0, $"the refused draw records nothing: {string.Join(" | ", leftover)}");
        var next = new CommandBuffer();
        next.DrawMesh(mesh, Matrix4x4.identity, material, 0, 0);
        Expect.Sequence(new[] { Line(mesh, material, "-") }, Execute(next),
            "the refused draw is not recorded and the value staged before the refusal reaches no later draw");
        Console.WriteLine("PASS: a DrawMesh block the world refuses part way leaves nothing staged for the next draw");
    }

    private static void SubmitCarriesTheBlockAndClearsARefusedOne()
    {
        var (mesh, material) = (Triangle(), new Material(Material.SpriteLit));
        var block = new MaterialPropertyBlock();
        block.SetFloat("_Fade", 0.5f);
        var buffer = new CommandBuffer();
        buffer.DrawMesh(mesh, Matrix4x4.identity, material, 0, 0, block);
        new ScriptableRenderContext().Submit();
        CommandWireReader.Take();
        var context = new ScriptableRenderContext();
        context.ExecuteCommandBuffer(buffer);
        block.SetFloat("_Fade", 0.75f);
        Ensure(CommandWireReader.Take().Length == 0, "the context holds the draw until Submit");
        context.Submit();
        Expect.Sequence(new[] { Line(mesh, material, "_Fade=float (0.5)") }, CommandWireReader.Take(),
            "Submit records the block copied when DrawMesh was called");
        var refusedBlock = new MaterialPropertyBlock();
        refusedBlock.SetFloat("_Good", 1);
        refusedBlock.SetVector(new string('x', 300), Vector4.one);
        var refused = new CommandBuffer();
        refused.DrawMesh(mesh, Matrix4x4.identity, material, 0, 0, refusedBlock);
        context.ExecuteCommandBuffer(refused);
        var threw = false;
        try { context.Submit(); }
        catch (DisunityException) { threw = true; }
        Ensure(threw, "a block value with a name the world refuses fails the draw when Submit runs it");
        Ensure(CommandWireReader.Take().Length == 0, "the draw Submit refused records nothing");
        var next = new CommandBuffer();
        next.DrawMesh(mesh, Matrix4x4.identity, material, 0, 0);
        Expect.Sequence(new[] { Line(mesh, material, "-") }, Execute(next), "nothing staged before the refusal reaches a later draw");
        Console.WriteLine("PASS: Submit records a DrawMesh property block copied at call time, and a refused one leaves nothing staged");
    }

    private static void ABlockWithAnArrayOrABufferIsRefused()
    {
        var (mesh, material) = (Triangle(), new Material(Material.SpriteLit));
        var buffer = new CommandBuffer();
        var data = new ComputeBuffer(4, 4);
        var cases = new (string Kind, string Holds, Action<MaterialPropertyBlock> Fill)[]
        {
            ("a float array", "an array", block => block.SetFloatArray("_Weights", new[] { 0.25f, 0.75f })),
            ("a vector array", "an array", block => block.SetVectorArray("_Points", new[] { new Vector4(5, 6, 7, 8) })),
            ("a matrix array", "an array", block => block.SetMatrixArray("_Bones", new[] { Matrix4x4.identity })),
            ("a buffer", "a buffer", block => block.SetBuffer("_Data", data)),
            ("a constant buffer", "a buffer", block => block.SetConstantBuffer("_Constants", data, 0, 16)),
        };
        foreach (var (kind, holds, fill) in cases)
        {
            var block = new MaterialPropertyBlock();
            block.SetFloat("_Fade", 1);
            fill(block);
            Throws<NotSupportedException>(() => buffer.DrawMesh(mesh, Matrix4x4.identity, material, 0, 0, block),
                $"a block with {kind} beside a float", $"CommandBuffer.DrawMesh(properties): a property block that holds {holds}");
        }
        data.Release();
        Ensure(buffer.commandCount == 0, "a refused block records nothing");
        Console.WriteLine("PASS: a DrawMesh property block that holds an array or a buffer throws NotSupportedException and records nothing");
    }

    private static void AReleasedBufferIsRefusedFirst()
    {
        var buffer = new CommandBuffer();
        buffer.Release();
        var block = new MaterialPropertyBlock();
        block.SetFloat("_Fade", 1);
        Throws<ObjectDisposedException>(() => buffer.DrawMesh(Triangle(), Matrix4x4.identity, new Material(Material.SpriteLit), 0, 0, block),
            "a released buffer with a block", "CommandBuffer");
        Console.WriteLine("PASS: a released buffer throws ObjectDisposedException before it reads a DrawMesh property block");
    }
}
