using Disunity.Hosting;
using Unity.Collections;
using Unity.Collections.LowLevel.Unsafe;
using Unity.Jobs;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>Culling invokes the game's BatchRendererGroup callback with the camera's (or the shadow split's) planes, and drawing a
/// renderer list records each draw command the callback wrote as one batch draw, with the list's pass tags, into every list whose
/// queue range and rendering layer filter accept the command's material. The tests read the offscreen work the way a host takes it.</summary>
internal static unsafe class BatchRendererGroupDrawTests
{
    private const string Metadata = "unity_ObjectToWorld=2147483744,_Color=2147484160";

    /// <summary>A culling callback written the way the game writes its own: it allocates the output arrays and writes a command for each
    /// run of visible instances it is told to emit, all in one draw range.</summary>
    private sealed class Emitter
    {
        public (uint Offset, uint Count)[] Runs = [];
        public int[] Visible = [];
        public BatchID Batch;
        public BatchMaterialID[] Materials = [];
        public BatchMeshID Mesh;
        public ShadowCastingMode Shadows = ShadowCastingMode.On;
        public uint RangeLayers = uint.MaxValue;
        public int Calls;
        public Plane[] SeenPlanes = [];
        public BatchCullingViewType SeenView;
        public uint SeenLayerMask;
        public Action<BatchRendererGroup>? BeforeEmit;

        public JobHandle OnPerformCulling(BatchRendererGroup group, BatchCullingContext context, BatchCullingOutput output, IntPtr userContext)
        {
            Calls++;
            BeforeEmit?.Invoke(group);
            SeenPlanes = context.cullingPlanes.ToArray();
            (SeenView, SeenLayerMask) = (context.viewType, context.cullingLayerMask);
            if (Runs.Length == 0) return default;
            var result = (BatchCullingOutputDrawCommands*)output.drawCommands.GetUnsafePtr();
            result->drawCommands = (BatchDrawCommand*)UnsafeUtility.Malloc(UnsafeUtility.SizeOf<BatchDrawCommand>() * Runs.Length, UnsafeUtility.AlignOf<long>(), Allocator.TempJob);
            result->drawRanges = (BatchDrawRange*)UnsafeUtility.Malloc(UnsafeUtility.SizeOf<BatchDrawRange>(), UnsafeUtility.AlignOf<long>(), Allocator.TempJob);
            result->visibleInstances = (int*)UnsafeUtility.Malloc(Visible.Length * 4, UnsafeUtility.AlignOf<long>(), Allocator.TempJob);
            (result->drawCommandCount, result->drawRangeCount, result->visibleInstanceCount) = (Runs.Length, 1, Visible.Length);
            for (var i = 0; i < Visible.Length; i++) result->visibleInstances[i] = Visible[i];
            for (var i = 0; i < Runs.Length; i++)
            {
                result->drawCommands[i] = new BatchDrawCommand
                {
                    visibleOffset = Runs[i].Offset, visibleCount = Runs[i].Count, batchID = Batch, materialID = Materials[i % Materials.Length],
                    meshID = Mesh, submeshIndex = 0, splitVisibilityMask = 255,
                };
            }
            result->drawRanges[0] = new BatchDrawRange
            {
                drawCommandsType = BatchDrawCommandType.Direct, drawCommandsBegin = 0, drawCommandsCount = (uint)Runs.Length,
                filterSettings = new BatchFilterSettings { renderingLayerMask = RangeLayers, shadowCastingMode = Shadows },
            };
            return default;
        }
    }

    /// <summary>A group with one mesh, the materials of the queues asked for and a raw buffer, as the game's instance renderer builds them.</summary>
    private sealed class Group : IDisposable
    {
        public readonly Emitter Emitter = new();
        public readonly BatchRendererGroup Brg;
        public readonly Mesh Mesh = Quad();
        public readonly Material[] Materials;
        public readonly GraphicsBuffer Buffer = new(GraphicsBuffer.Target.Raw, 64, 4);

        public Group(params int[] queues)
        {
            Brg = new BatchRendererGroup(Emitter.OnPerformCulling, IntPtr.Zero);
            Materials = queues.Select(queue => new Material(Material.SpriteLit) { renderQueue = queue }).ToArray();
            Buffer.SetData(Enumerable.Range(0, 64).ToArray());
            var metadata = new NativeArray<MetadataValue>(2, Allocator.Temp);
            metadata[0] = new MetadataValue { NameID = Shader.PropertyToID("unity_ObjectToWorld"), Value = 0x80000060 };
            metadata[1] = new MetadataValue { NameID = Shader.PropertyToID("_Color"), Value = 0x80000200 };
            Emitter.Batch = Brg.AddBatch(metadata, Buffer.bufferHandle);
            metadata.Dispose();
            Emitter.Mesh = Brg.RegisterMesh(Mesh);
            Emitter.Materials = Materials.Select(Brg.RegisterMaterial).ToArray();
        }

        public string Line(int material, string visible, string tags) =>
            $"DrawMeshBatch mesh={Mesh.RendererIdentity} material={Materials[material].WorldLink.Id} submesh=0 pass=-1 buffer={Buffer.WorldLink.Id} window=0+256 "
            + $"metadata=[{Metadata}] visible=[{visible}] tags=[{tags}]";

        public void Dispose()
        {
            Brg.Dispose();
            Buffer.Dispose();
        }
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            TwoCommandsGiveTwoBatchDrawsWithTheirVisibleIndexesAndTags();
            Console.WriteLine("PASS: a callback that emits two draw commands gives two batch draws with the visible indexes of each and the list's pass tags");
            ACallbackThatEmitsNothingGivesNoDraw();
            Console.WriteLine("PASS: a callback that emits nothing, and a disposed group, give no batch draw");
            TheCallbackSeesTheCamerasCullingPlanes();
            Console.WriteLine("PASS: the culling callback sees the camera's culling planes, a camera view and the culling mask");
            AShadowListGetsTheShadowCasterTags();
            Console.WriteLine("PASS: a shadow list invokes the callback with the split's planes as a light view and draws with the ShadowCaster tag, for casters only");
            EachListTakesTheDrawsItsFilterAccepts();
            Console.WriteLine("PASS: each renderer list of a camera takes the batch draws whose material is in its queue range and whose range shares a rendering layer");
            ACallbackMayRemoveTheEmptyBatchBeforeEmitting();
            Console.WriteLine("PASS: a callback that removes the empty batch ID and then emits draw commands has its draws recorded");
            AThrowingCallbackLeavesOutOnlyThatCull();
            Console.WriteLine("PASS: a callback that throws gives no draws for that cull and is asked again at the next one");
        }
        finally
        {
            ScriptableRenderContext.DiscardPending();
            DisunityHost.UnloadWorld();
        }
    }

    private static Mesh Quad()
    {
        var mesh = new Mesh { vertices = [new Vector3(0, 0, 0), new Vector3(1, 0, 0), new Vector3(0, 1, 0), new Vector3(1, 1, 0)] };
        mesh.SetTriangles(new[] { 0, 1, 2, 1, 3, 2 }, 0);
        return mesh;
    }

    private static ScriptableCullingParameters CameraParameters()
    {
        var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
        camera.farClipPlane = 100f;
        Expect.That(camera.TryGetCullingParameters(out var parameters), "the camera can be culled");
        return parameters;
    }

    private static DrawingSettings Drawing(params string[] names)
    {
        var settings = new DrawingSettings(new ShaderTagId(names[0]), new SortingSettings { criteria = SortingCriteria.CommonOpaque });
        for (var slot = 1; slot < names.Length; slot++) settings.SetShaderPassName(slot, new ShaderTagId(names[slot]));
        return settings;
    }

    /// <summary>Executes a buffer that draws the lists and returns the records after the culling and the list creations.</summary>
    private static string[] DrawAll(ScriptableRenderContext context, params RendererList[] lists)
    {
        var buffer = new CommandBuffer();
        foreach (var list in lists) buffer.DrawRendererList(list);
        context.ExecuteCommandBuffer(buffer);
        context.Submit();
        return CommandWireReader.TakeOrdered().Where(line => line.StartsWith("DrawMeshBatch", StringComparison.Ordinal)).ToArray();
    }

    private static RendererList DrawList(ScriptableRenderContext context, CullingResults culling, FilteringSettings filtering, params string[] tags)
    {
        var parameters = new RendererListParams(culling, Drawing(tags), filtering);
        return context.CreateRendererList(ref parameters);
    }

    private static void TwoCommandsGiveTwoBatchDrawsWithTheirVisibleIndexesAndTags()
    {
        using var group = new Group(2000);
        group.Emitter.Runs = [(0, 3), (3, 1)];
        group.Emitter.Visible = [5, 1, 2, 7];
        CommandWireReader.Take();
        var context = default(ScriptableRenderContext);
        var parameters = CameraParameters();
        var culling = context.Cull(ref parameters);
        var list = DrawList(context, culling, FilteringSettings.defaultValue, "UniversalForward", "SRPDefaultUnlit");
        var drawn = DrawAll(context, list);
        Expect.Sequence(
            new[] { group.Line(0, "5,1,2", "UniversalForward,SRPDefaultUnlit"), group.Line(0, "7", "UniversalForward,SRPDefaultUnlit") }, drawn,
            "one batch draw for each command, in the callback's order");
        Expect.Equal(1, group.Emitter.Calls, "the callback ran once, for the cull");
    }

    private static void ACallbackThatEmitsNothingGivesNoDraw()
    {
        var group = new Group(2000);
        CommandWireReader.Take();
        var context = default(ScriptableRenderContext);
        var parameters = CameraParameters();
        var culling = context.Cull(ref parameters);
        Expect.Sequence(Array.Empty<string>(), DrawAll(context, DrawList(context, culling, FilteringSettings.defaultValue, "A")), "an empty output draws nothing");
        Expect.Equal(1, group.Emitter.Calls, "the callback ran");
        group.Emitter.Runs = [(0, 1)];
        group.Emitter.Visible = [1];
        group.Dispose();
        var second = context.Cull(ref parameters);
        Expect.Sequence(Array.Empty<string>(), DrawAll(context, DrawList(context, second, FilteringSettings.defaultValue, "A")), "a disposed group is not asked");
        Expect.Equal(1, group.Emitter.Calls, "the callback of a disposed group did not run again");
    }

    private static void TheCallbackSeesTheCamerasCullingPlanes()
    {
        using var group = new Group(2000);
        var context = default(ScriptableRenderContext);
        var parameters = CameraParameters();
        parameters.cullingMask = 0x35;
        _ = context.Cull(ref parameters);
        Expect.That(parameters.cullingPlaneCount >= 6, "the camera has culling planes");
        Expect.Equal(parameters.cullingPlaneCount, group.Emitter.SeenPlanes.Length, "the callback sees every plane");
        for (var index = 0; index < parameters.cullingPlaneCount; index++)
            Expect.That(group.Emitter.SeenPlanes[index] == parameters.GetCullingPlane(index), $"culling plane {index} is the camera's");
        Expect.That(group.Emitter.SeenView == BatchCullingViewType.Camera && group.Emitter.SeenLayerMask == 0x35, "a camera view with the culling mask");
        ScriptableRenderContext.DiscardPending();
    }

    private static void AShadowListGetsTheShadowCasterTags()
    {
        using var group = new Group(2000);
        group.Emitter.Runs = [(0, 2)];
        group.Emitter.Visible = [4, 6];
        CommandWireReader.Take();
        var context = default(ScriptableRenderContext);
        var parameters = CameraParameters();
        var culling = context.Cull(ref parameters);
        var split = new ShadowSplitData { cullingSphere = new Vector4(1f, 2f, 3f, 40f), cullingPlaneCount = 2 };
        split.SetCullingPlane(0, new Plane(new Vector3(0f, 1f, 0f), 2.5f));
        split.SetCullingPlane(1, new Plane(new Vector3(1f, 0f, 0f), -4f));
        var settings = new ShadowDrawingSettings(culling, 0) { splitData = split };
        var shadow = context.CreateShadowRendererList(ref settings);
        Expect.Equal(2, group.Emitter.SeenPlanes.Length, "the callback sees the split's planes");
        Expect.That(group.Emitter.SeenPlanes[0] == split.GetCullingPlane(0) && group.Emitter.SeenPlanes[1] == split.GetCullingPlane(1), "they are the split's planes");
        Expect.Equal(BatchCullingViewType.Light, group.Emitter.SeenView, "a light view");
        Expect.Sequence(new[] { group.Line(0, "4,6", "ShadowCaster") }, DrawAll(context, shadow), "the shadow list draws with the shadow caster tag");

        group.Emitter.Shadows = ShadowCastingMode.Off;
        var second = context.Cull(ref parameters);
        settings.cullingResults = second;
        var noCasters = context.CreateShadowRendererList(ref settings);
        var main = DrawList(context, second, FilteringSettings.defaultValue, "UniversalForward");
        Expect.Sequence(new[] { group.Line(0, "4,6", "UniversalForward") }, DrawAll(context, noCasters, main), "a range that casts no shadow is left out of the shadow list only");
    }

    private static void EachListTakesTheDrawsItsFilterAccepts()
    {
        using var group = new Group(2000, 3000);
        group.Emitter.Runs = [(0, 1), (1, 2)];
        group.Emitter.Visible = [8, 9, 10];
        group.Emitter.RangeLayers = 0x3;
        CommandWireReader.Take();
        var context = default(ScriptableRenderContext);
        var parameters = CameraParameters();
        var culling = context.Cull(ref parameters);
        var opaque = DrawList(context, culling, new FilteringSettings(new RenderQueueRange { lowerBound = 1000, upperBound = 2500 }), "Opaque");
        var transparent = DrawList(context, culling, new FilteringSettings(new RenderQueueRange { lowerBound = 2501, upperBound = 5000 }), "Transparent");
        var otherLayer = DrawList(context, culling, new FilteringSettings(RenderQueueRange.all, -1, 0x4), "Layered");
        var sharedLayer = DrawList(context, culling, new FilteringSettings(RenderQueueRange.all, -1, 0x2), "Shared");
        Expect.Sequence(new[] { group.Line(0, "8", "Opaque"), group.Line(1, "9,10", "Transparent"), group.Line(0, "8", "Shared"), group.Line(1, "9,10", "Shared") },
            DrawAll(context, opaque, transparent, otherLayer, sharedLayer), "the queue range and the rendering layer choose the draws of each list");
        Expect.Equal(1, group.Emitter.Calls, "the camera's lists share the one culling callback run");
    }

    private static void ACallbackMayRemoveTheEmptyBatchBeforeEmitting()
    {
        using var group = new Group(2000);
        group.Emitter.Runs = [(0, 2)];
        group.Emitter.Visible = [4, 6];
        group.Emitter.BeforeEmit = brg => brg.RemoveBatch(default);
        CommandWireReader.Take();
        var context = default(ScriptableRenderContext);
        var parameters = CameraParameters();
        var culling = context.Cull(ref parameters);
        var drawn = DrawAll(context, DrawList(context, culling, FilteringSettings.defaultValue, "UniversalForward"));
        Expect.Sequence(new[] { group.Line(0, "4,6", "UniversalForward") }, drawn, "the draws written after RemoveBatch of the empty batch ID are recorded");
    }

    private static void AThrowingCallbackLeavesOutOnlyThatCull()
    {
        using var group = new Group(2000);
        group.Emitter.Runs = [(0, 1)];
        group.Emitter.Visible = [3];
        group.Emitter.BeforeEmit = _ =>
        {
            if (group.Emitter.Calls == 1) throw new InvalidOperationException("first call fails");
        };
        CommandWireReader.Take();
        var context = default(ScriptableRenderContext);
        var parameters = CameraParameters();
        var first = context.Cull(ref parameters);
        Expect.Sequence(Array.Empty<string>(), DrawAll(context, DrawList(context, first, FilteringSettings.defaultValue, "A")), "the cull whose callback threw draws nothing");
        var second = context.Cull(ref parameters);
        Expect.Sequence(new[] { group.Line(0, "3", "A") }, DrawAll(context, DrawList(context, second, FilteringSettings.defaultValue, "A")), "the next cull draws what the callback wrote");
        Expect.Equal(2, group.Emitter.Calls, "the group was asked again");
    }
}
