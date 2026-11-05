using Disunity.Hosting;
using UnityEngine;
using static ComputeTestSupport;

/// <summary>ComputeShader: kernels from the plan, bindings and recorded dispatches, and kernel group sizes.</summary>
internal static class ComputeDispatchTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            KernelsComeFromThePlan();
            Console.WriteLine("PASS: a planned ComputeShader finds its kernels by name and index and refuses a kernel it lacks");
            DispatchRecordsGroupsAndBindings();
            Console.WriteLine("PASS: ComputeShader.Dispatch records the kernel, the groups and the shader-wide and kernel values it had at the call");
            GlobalTexturesAreResolvedAtTheDispatch();
            Console.WriteLine("PASS: a kernel's global texture is the texture the global state held at the dispatch, and a later change does not reach the record");
            DispatchOrderAndSnapshot();
            Console.WriteLine("PASS: dispatches keep call order beside blits and later changes do not reach an earlier dispatch");
            GroupSizesComeFromThePlanThenTheHost();
            Console.WriteLine("PASS: GetKernelThreadGroupSizes uses the planned size, asks the host for an unknown one and throws naming the member when nobody states it");
            UnsupportedMembersThrowNamingTheMember();
            Console.WriteLine("PASS: ComputeShader members the engine cannot honour throw NotSupportedException naming the member");
            TheTerrainReadBackSequenceRuns();
            Console.WriteLine("PASS: the terrain read-back call sequence works end to end with a host executor");
        }
        finally
        {
            Unregister(DisunityHost.WorldHandle);
            DisunityHost.UnloadWorld();
        }
    }

    private static void KernelsComeFromThePlan()
    {
        var shader = Planned(TerrainPlan);
        Expect.Equal("TerrainReadBackShader", shader.name, "the planned name");
        Expect.Equal(0, shader.FindKernel("TerrainReadBackCS"), "the first kernel");
        Expect.Equal(1, shader.FindKernel("Unsized"), "the second kernel");
        Expect.That(shader.HasKernel("Unsized") && !shader.HasKernel("Missing") && !shader.HasKernel("terrainreadbackcs"), "HasKernel matches the exact name");
        Expect.Throws<ArgumentException>(() => shader.FindKernel("Missing"), "FindKernel of a kernel it lacks");
        Expect.Throws<ArgumentOutOfRangeException>(() => shader.Dispatch(2, 1, 1, 1), "a kernel index past the end");
        Expect.Throws<ArgumentOutOfRangeException>(() => shader.SetBuffer(-1, "_Output", new ComputeBuffer(1, 4)), "a negative kernel index");
    }

    private static void DispatchRecordsGroupsAndBindings()
    {
        var world = DisunityHost.WorldHandle;
        TakeWork(world);
        var shader = Planned(TerrainPlan);
        using var output = new ComputeBuffer(8, 4);
        shader.SetInt("_OutputWidth", 2048);
        shader.SetFloat("_Scale", 0.5f);
        shader.SetVector("_Tint", new Vector4(1, 2, 3, 4));
        shader.SetMatrix("_View", new Matrix4x4(new Vector4(1, 2, 3, 4), new Vector4(5, 6, 7, 8), new Vector4(9, 10, 11, 12), new Vector4(13, 14, 15, 16)));
        shader.SetBuffer(0, "_Output", output);
        shader.SetTextureFromGlobal(0, "_Heights", "_GlobalTerrainHeightmap");
        shader.SetBuffer(1, "_OtherKernelOnly", output);
        shader.Dispatch(0, 256, 128, 1);
        var work = TakeWork(world);
        Expect.Equal(1, work.Length, "one dispatch recorded");
        Expect.That(work[0].kind == 3 && work[0].kernel == 0 && work[0].groups_x == 256 && work[0].groups_y == 128 && work[0].groups_z == 1, "kernel and groups");
        var entries = Bindings(world, work[0].bindings).ToDictionary(entry => entry.Name);
        Expect.Equal(2048, entries["_OutputWidth"].Int, "the shader-wide integer");
        Expect.Near(0.5f, entries["_Scale"].Floats[0], "the shader-wide float");
        Expect.Sequence(new[] { 1f, 2f, 3f, 4f }, entries["_Tint"].Floats, "the shader-wide vector");
        Expect.Sequence(Enumerable.Range(1, 16).Select(i => (float)i), entries["_View"].Floats, "the shader-wide matrix, column by column");
        Expect.That(entries["_Output"].Kind == 5, "the buffer binding");
        Expect.That(entries["_Heights"].TextureKind == 0, "a global texture the global state does not hold reads as no texture in the record");
        Expect.That(!entries.ContainsKey("_OtherKernelOnly"), "a value of another kernel is not in this dispatch");
    }

    private static void GlobalTexturesAreResolvedAtTheDispatch()
    {
        var world = DisunityHost.WorldHandle;
        TakeWork(world);
        var shader = Planned(TerrainPlan);
        var first = new RenderTexture(4, 4, 0);
        first.Create();
        var second = new RenderTexture(8, 8, 0);
        second.Create();
        shader.SetTextureFromGlobal(0, "_Heights", "_GlobalTerrainHeightmap");
        Shader.SetGlobalTexture("_GlobalTerrainHeightmap", first);
        shader.Dispatch(0, 1, 1, 1);
        Shader.SetGlobalTexture("_GlobalTerrainHeightmap", second);
        shader.Dispatch(0, 1, 1, 1);
        Shader.SetGlobalTexture("_GlobalTerrainHeightmap", (Texture)null!);
        var work = TakeWork(world);
        var heights = work.Select(record => Bindings(world, record.bindings).Single(entry => entry.Name == "_Heights")).ToArray();
        Expect.That(heights[0].TextureKind == 2 && heights[0].Resource == first.WorldId, "the first dispatch holds the render texture the global held then");
        Expect.That(heights[1].TextureKind == 2 && heights[1].Resource == second.WorldId, "the second dispatch holds the render texture set after the first");
    }

    private static void DispatchOrderAndSnapshot()
    {
        var world = DisunityHost.WorldHandle;
        TakeWork(world);
        var shader = Planned(TerrainPlan);
        var target = new RenderTexture(4, 4, 0);
        target.Create();
        shader.SetInt("_Mode", 1);
        shader.Dispatch(0, 1, 1, 1);
        Graphics.Blit(Texture2D.whiteTexture, target);
        shader.SetInt("_Mode", 2);
        shader.Dispatch(0, 2, 1, 1);
        var work = TakeWork(world);
        Expect.Sequence(new uint[] { 3, 1, 3 }, work.Select(record => record.kind), "dispatch, blit, dispatch in call order");
        Expect.Equal(1, Bindings(world, work[0].bindings).Single(entry => entry.Name == "_Mode").Int, "the first dispatch kept the value it had");
        Expect.Equal(2, Bindings(world, work[2].bindings).Single(entry => entry.Name == "_Mode").Int, "the second dispatch has the later value");
        Expect.Throws<DisunityException>(() => shader.Dispatch(0, 0, 1, 1), "zero groups");
        Expect.Throws<DisunityException>(() => shader.Dispatch(0, 65536, 1, 1), "too many groups");
        Expect.Equal(0, TakeWork(world).Length, "a refused dispatch records nothing");
    }

    private static void GroupSizesComeFromThePlanThenTheHost()
    {
        var world = DisunityHost.WorldHandle;
        var shader = Planned(TerrainPlan);
        Reset(Behavior.Nothing);
        Register(world);
        shader.GetKernelThreadGroupSizes(0, out var x, out var y, out var z);
        Expect.That((x, y, z) == (8u, 8u, 1u), "the planned size");
        Expect.Equal(0, Flushes, "a planned size does not ask the host");
        var missing = Expect.Catch<NotSupportedException>(() => shader.GetKernelThreadGroupSizes(1, out _, out _, out _), "an unknown size nobody states");
        Expect.That(missing.Message.Contains("ComputeShader.GetKernelThreadGroupSizes"), "the message names the member: " + missing.Message);
        Expect.Equal(1, Flushes, "the engine asked the host once");
        Reset(Behavior.StateKernelSizes);
        shader.Dispatch(1, 1, 1, 1);
        shader.GetKernelThreadGroupSizes(1, out x, out y, out z);
        Expect.That((x, y, z) == (8u, 4u, 2u), "the size the host stated while it flushed");
        Expect.Equal(1, Flushes, "one flush answered it");
        Reset(Behavior.Nothing);
        shader.GetKernelThreadGroupSizes(1, out x, out _, out _);
        Expect.Equal(8u, x, "a stated size stays");
        Expect.Equal(0, Flushes, "and is not asked again");
        Unregister(world);
    }

    private static void UnsupportedMembersThrowNamingTheMember()
    {
        var shader = Planned(TerrainPlan);
        var buffer = new ComputeBuffer(1, 4);
        var calls = new (string Member, Action Call)[]
        {
            ("SetFloats", () => shader.SetFloats("_Weights", 1f, 2f)),
            ("SetFloats", () => shader.SetFloats(Shader.PropertyToID("_Weights"), 1f)),
            ("EnableKeyword", () => shader.EnableKeyword("FAST")),
            ("DisableKeyword", () => shader.DisableKeyword("FAST")),
            ("SetConstantBuffer", () => shader.SetConstantBuffer("cb", buffer, 0, 4)),
            ("DispatchIndirect", () => shader.DispatchIndirect(0, buffer)),
        };
        foreach (var (member, call) in calls)
        {
            var error = Expect.Catch<NotSupportedException>(call, member);
            Expect.That(error.Message.Contains($"ComputeShader.{member}"), $"{member}: the message names the member: {error.Message}");
        }
    }

    private static void TheTerrainReadBackSequenceRuns()
    {
        var world = DisunityHost.WorldHandle;
        var shader = Planned(TerrainPlan);
        using var buffer = new ComputeBuffer(4194304, 4, ComputeBufferType.Default);
        Reset(Behavior.ExecuteDispatches);
        Register(world);
        shader.GetKernelThreadGroupSizes(0, out var x, out var y, out var z);
        shader.SetBuffer(0, "_Output", buffer);
        shader.SetInt("_OutputWidth", 2048);
        shader.SetInt("_OutputHeight", 2048);
        shader.SetTextureFromGlobal(0, "_GlobalTerrainHeightmap", "_GlobalTerrainHeightmap");
        shader.SetTextureFromGlobal(0, "_GlobalTerrainCluttermap", "_GlobalTerrainCluttermap");
        shader.Dispatch(0, 2048 / (int)x, 2048 / (int)y, 1);
        var packed = new uint[buffer.count];
        buffer.GetData(packed);
        Expect.Equal(256u * 256u, packed[0], "the executor's result reached the managed array (0 + 256 x 256 groups)");
        Expect.Equal(0u, packed[1], "the rest stays zero");
        Expect.That(Executed.Count == 1 && Executed[0].X == 256 && Executed[0].Y == 256 && z == 1, "the executor saw the dispatch");
        Unregister(world);
    }
}
