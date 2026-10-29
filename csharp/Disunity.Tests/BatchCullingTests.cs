using Unity.Collections;
using Unity.Collections.LowLevel.Unsafe;
using Unity.Jobs;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>
/// Culling callbacks written the way the game writes its own, run through the engine's seam for them, and
/// a class shaped like the game's quad tree that owns a BatchRendererGroup.
/// </summary>
internal static unsafe class BatchCullingTests
{
    /// <summary>A culling callback with the same body as the game's: it allocates the output arrays and fills one command.</summary>
    private sealed class GameStyleCulling
    {
        public Plane[] SeenPlanes = [];
        public int SeenPlaneCount;
        public uint SeenLayerMask;
        public IntPtr SeenUserContext;
        public BatchRendererGroup? SeenGroup;
        public BatchID Batch;
        public BatchMaterialID Material;
        public BatchMeshID Mesh;

        public JobHandle OnPerformCulling(BatchRendererGroup rendererGroup, BatchCullingContext cullingContext, BatchCullingOutput cullingOutput, IntPtr userContext)
        {
            SeenGroup = rendererGroup;
            SeenUserContext = userContext;
            SeenLayerMask = cullingContext.cullingLayerMask;
            SeenPlaneCount = cullingContext.cullingPlanes.Length;
            SeenPlanes = new Plane[SeenPlaneCount];
            cullingContext.cullingPlanes.CopyTo(SeenPlanes);

            const int commandCount = 1;
            const int instanceCount = 3;
            var output = (BatchCullingOutputDrawCommands*)cullingOutput.drawCommands.GetUnsafePtr();
            output->drawCommands = (BatchDrawCommand*)UnsafeUtility.Malloc(UnsafeUtility.SizeOf<BatchDrawCommand>() * commandCount, UnsafeUtility.AlignOf<long>(), Allocator.TempJob);
            output->drawRanges = (BatchDrawRange*)UnsafeUtility.Malloc(UnsafeUtility.SizeOf<BatchDrawRange>(), UnsafeUtility.AlignOf<long>(), Allocator.TempJob);
            output->visibleInstances = (int*)UnsafeUtility.Malloc(instanceCount * 4, UnsafeUtility.AlignOf<long>(), Allocator.TempJob);
            output->drawCommandCount = commandCount;
            output->drawRangeCount = 1;
            output->visibleInstanceCount = instanceCount;
            output->instanceSortingPositions = null;
            output->instanceSortingPositionFloatCount = 0;
            for (var i = 0; i < instanceCount; i++) output->visibleInstances[i] = 10 + i;
            output->drawCommands[0] = new BatchDrawCommand
            {
                visibleOffset = 0,
                visibleCount = instanceCount,
                batchID = Batch,
                materialID = Material,
                meshID = Mesh,
                submeshIndex = 0,
                splitVisibilityMask = 255,
                flags = BatchDrawCommandFlags.None,
                sortingPosition = 0,
            };
            output->drawRanges[0] = new BatchDrawRange
            {
                drawCommandsType = BatchDrawCommandType.Direct,
                drawCommandsBegin = 0,
                drawCommandsCount = commandCount,
                filterSettings = new BatchFilterSettings
                {
                    renderingLayerMask = uint.MaxValue,
                    shadowCastingMode = ShadowCastingMode.On,
                    receiveShadows = true,
                },
            };
            return default;
        }
    }

    /// <summary>The shape of the game's BRGQuadTree around its BatchRendererGroup.</summary>
    private sealed class QuadTreeShape : IDisposable
    {
        public BatchRendererGroup rendererGroup;
        public List<(BatchMeshID, BatchMaterialID)> meshMaterialPairs;
        public int Culls;

        public QuadTreeShape()
        {
            meshMaterialPairs = new List<(BatchMeshID, BatchMaterialID)>();
            rendererGroup = new BatchRendererGroup(OnPerformCulling, IntPtr.Zero);
        }

        public JobHandle OnPerformCulling(BatchRendererGroup group, BatchCullingContext cullingContext, BatchCullingOutput cullingOutput, IntPtr userContext)
        {
            Culls++;
            return default;
        }

        public void Dispose() => rendererGroup.Dispose();
    }

    public static void Run()
    {
        RunsAGameStyleCullingCallback();
        PassesTheCullingContextToTheCallback();
        RunsACallbackThatWritesNothing();
        LoadsAndRunsAClassShapedLikeTheQuadTree();
        BatchFilterSettingsDefaultToOffAndNoShadows();
        Console.WriteLine("PASS: game-style culling callbacks, the culling seam, a quad-tree-shaped class and filter settings");
    }

    private static NativeArray<Plane> SixPlanes()
    {
        var planes = new NativeArray<Plane>(6, Allocator.Temp);
        for (var i = 0; i < planes.Length; i++) planes[i] = new Plane(new Vector3(i + 1, 0, 1), i * 2.5f);
        return planes;
    }

    private static void RunsAGameStyleCullingCallback()
    {
        var culling = new GameStyleCulling
        {
            Batch = new BatchID { value = 4 },
            Material = new BatchMaterialID { value = 5 },
            Mesh = new BatchMeshID { value = 6 },
        };
        using var group = new BatchRendererGroup(culling.OnPerformCulling, (IntPtr)77);
        var planes = SixPlanes();
        var commands = group.InvokeCullingCallback(new BatchCullingContext { cullingPlanes = planes, cullingLayerMask = 0x5 });
        try
        {
            Expect.That(commands.drawCommandCount == 1 && commands.drawRangeCount == 1 && commands.visibleInstanceCount == 3, "the counts the callback wrote");
            Expect.That(commands.instanceSortingPositions == null && commands.instanceSortingPositionFloatCount == 0, "no sorting positions");
            var command = commands.drawCommands[0];
            Expect.That(command.visibleOffset == 0 && command.visibleCount == 3 && command.submeshIndex == 0 && command.splitVisibilityMask == 255, "the command's counts and visibility mask");
            Expect.That(command.batchID.value == 4 && command.materialID.value == 5 && command.meshID.value == 6, "the command's batch, material and mesh IDs");
            Expect.That(command.flags == BatchDrawCommandFlags.None && command.sortingPosition == 0, "the command's flags and sorting position");
            var range = commands.drawRanges[0];
            Expect.That(range.drawCommandsType == BatchDrawCommandType.Direct && range.drawCommandsBegin == 0 && range.drawCommandsCount == 1, "the range covers the command");
            Expect.That(range.filterSettings.renderingLayerMask == uint.MaxValue && range.filterSettings.shadowCastingMode == ShadowCastingMode.On && range.filterSettings.receiveShadows, "the range's filter settings");
            Expect.Sequence(new[] { 10, 11, 12 }, new[] { commands.visibleInstances[0], commands.visibleInstances[1], commands.visibleInstances[2] }, "the visible instances");
        }
        finally
        {
            BatchRendererGroup.FreeDrawCommands(commands);
            planes.Dispose();
        }
    }

    private static void PassesTheCullingContextToTheCallback()
    {
        var culling = new GameStyleCulling();
        using var group = new BatchRendererGroup(culling.OnPerformCulling, (IntPtr)77);
        var planes = SixPlanes();
        BatchRendererGroup.FreeDrawCommands(group.InvokeCullingCallback(new BatchCullingContext { cullingPlanes = planes, cullingLayerMask = 0x5 }));
        Expect.That(ReferenceEquals(culling.SeenGroup, group) && culling.SeenUserContext == (IntPtr)77 && culling.SeenLayerMask == 0x5, "the group, user context and layer mask");
        Expect.Equal(6, culling.SeenPlaneCount, "the callback sees as many culling planes as the context holds");
        for (var i = 0; i < 6; i++)
            Expect.That(culling.SeenPlanes[i] == planes[i], $"culling plane {i} is the one passed in");
        planes.Dispose();

        var empty = new NativeArray<Plane>(0, Allocator.Temp);
        BatchRendererGroup.FreeDrawCommands(group.InvokeCullingCallback(new BatchCullingContext { cullingPlanes = empty, cullingLayerMask = 0 }));
        Expect.That(culling.SeenPlaneCount == 0 && culling.SeenLayerMask == 0, "a context without planes");
        empty.Dispose();
    }

    private static void RunsACallbackThatWritesNothing()
    {
        var shape = new QuadTreeShape();
        var commands = shape.rendererGroup.InvokeCullingCallback(default);
        Expect.That(shape.Culls == 1 && commands.drawCommandCount == 0 && commands.drawCommands == null && commands.visibleInstances == null, "an untouched output is empty");
        BatchRendererGroup.FreeDrawCommands(commands);
        shape.Dispose();
    }

    private static void LoadsAndRunsAClassShapedLikeTheQuadTree()
    {
        var tree = new QuadTreeShape();
        var mesh = new Mesh();
        var material = new Material(Material.SpriteUnlit);
        var pair = (tree.rendererGroup.RegisterMesh(mesh), tree.rendererGroup.RegisterMaterial(material));
        tree.meshMaterialPairs.Add(pair);
        var lookup = new Dictionary<(BatchMeshID, BatchMaterialID), int> { [pair] = 7 };
        Expect.That(tree.meshMaterialPairs.IndexOf(pair) == 0 && lookup[(pair.Item1, pair.Item2)] == 7, "pairs of IDs work in lists and dictionaries");
        Expect.That(ReferenceEquals(tree.rendererGroup.GetRegisteredMesh(pair.Item1), mesh) && ReferenceEquals(tree.rendererGroup.GetRegisteredMaterial(pair.Item2), material), "the pair resolves to the objects");
        tree.rendererGroup.UnregisterMesh(pair.Item1);
        tree.rendererGroup.UnregisterMaterial(pair.Item2);
        tree.Dispose();
    }

    private static void BatchFilterSettingsDefaultToOffAndNoShadows()
    {
        var defaults = default(BatchFilterSettings);
        Expect.That(defaults.shadowCastingMode == ShadowCastingMode.Off && !defaults.receiveShadows && defaults.renderingLayerMask == 0, "the default filter settings");
        var settings = new BatchFilterSettings { shadowCastingMode = ShadowCastingMode.TwoSided, receiveShadows = true, renderingLayerMask = 3 };
        Expect.That(settings.shadowCastingMode == ShadowCastingMode.TwoSided && settings.receiveShadows && settings.renderingLayerMask == 3, "the properties store their values");
        settings.receiveShadows = false;
        Expect.That(!settings.receiveShadows && settings.shadowCastingMode == ShadowCastingMode.TwoSided, "setting one property leaves the other");
    }
}
