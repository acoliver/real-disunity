using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>Renderer lists: a culling result becomes a drawing list or a shadow caster list in the world, and a command buffer draws
/// the list. The tests read the offscreen work back the way a host takes it.</summary>
internal static class RendererListTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            DrawListCarriesItsSettings();
            Console.WriteLine("PASS: CreateRendererList sends the culling result first, then a list with the pass names, queue range, masks, sorting, per-object data and override material");
            ShadowListCarriesItsSplit();
            Console.WriteLine("PASS: CreateShadowRendererList sends the light index, split planes and sphere, objects filter and projection");
            ListsAreDistinctAndEqualById();
            Console.WriteLine("PASS: each list has its own id, a valid list is not the null list, and lists compare by id");
            DrawRecordsInOrder();
            Console.WriteLine("PASS: CommandBuffer.DrawRendererList records the draw in order with the other commands, sent when the context submits");
            InvalidListsAreRefused();
            Console.WriteLine("PASS: DrawRendererList of an invalid list throws ArgumentException and records nothing");
            BadSettingsAreRefused();
            Console.WriteLine("PASS: a list without a culling result, pass name or valid queue range, or with a destroyed override material, throws ArgumentException");
            StatusFollowsValidity();
            Console.WriteLine("PASS: QueryRendererListStatus reports invalid for the null list and populated for a list; PrepareRendererListsAsync does nothing");
            ListsEndWithTheirFrame();
            Console.WriteLine("PASS: a list cannot be drawn after the frame that made it, and a stale list is refused when the draw is sent");
            ParamsValidityFollowsTheCullingResult();
            Console.WriteLine("PASS: RendererListParams.isValid is true once its culling results hold a culling result");
        }
        finally
        {
            ScriptableRenderContext.DiscardPending();
            DisunityHost.UnloadWorld();
        }
    }

    private static void Same(string[] expected, string[] actual, string what) =>
        Expect.That(expected.SequenceEqual(actual), $"{what}: expected\n  {string.Join("\n  ", expected)}\ngot\n  {string.Join("\n  ", actual)}");

    private static Camera NewCamera()
    {
        var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
        camera.fieldOfView = 60f;
        camera.nearClipPlane = 0.3f;
        camera.farClipPlane = 100f;
        return camera;
    }

    /// <summary>A camera's culling result, queued and not yet sent.</summary>
    private static CullingResults Cull(ScriptableRenderContext context)
    {
        CommandWireReader.Take();
        Expect.That(NewCamera().TryGetCullingParameters(out var parameters), "the camera can be culled");
        return context.Cull(ref parameters);
    }

    private static DrawingSettings Drawing(params string[] names)
    {
        var settings = new DrawingSettings(new ShaderTagId(names[0]), new SortingSettings { criteria = SortingCriteria.CommonOpaque });
        for (var slot = 1; slot < names.Length; slot++) settings.SetShaderPassName(slot, new ShaderTagId(names[slot]));
        return settings;
    }

    private static string CullLine(string record) => record.Split(' ')[1].Substring("id=".Length);

    private static void DrawListCarriesItsSettings()
    {
        var context = default(ScriptableRenderContext);
        var culling = Cull(context);
        var material = new Material(Material.SpriteLit);
        var drawing = Drawing("UniversalForward", "SRPDefaultUnlit");
        drawing.perObjectData = PerObjectData.LightProbe | PerObjectData.Lightmaps;
        drawing.overrideMaterial = material;
        drawing.overrideMaterialPassIndex = 2;
        var filtering = new FilteringSettings(new RenderQueueRange { lowerBound = 1000, upperBound = 2450 }, 0x55, 7);
        var parameters = new RendererListParams(culling, drawing, filtering);
        var list = context.CreateRendererList(ref parameters);
        Expect.That(list.isValid && list != RendererList.nullRendererList, "the list is valid");
        var taken = CommandWireReader.Take();
        Expect.Equal(2, taken.Length, "the culling result, then the list");
        Expect.That(taken[0].StartsWith("Cull id=", StringComparison.Ordinal), $"the culling result comes first: {taken[0]}");
        var sorting = (uint)SortingCriteria.CommonOpaque;
        Expect.Equal(
            $"CreateList id={list.Id} draw cull={CullLine(taken[0])} tags=[UniversalForward,SRPDefaultUnlit] queue=1000..2450 layers=85 renderingLayers=7 sorting={sorting} perObject=9 material={material.WorldLink.Id} pass=2",
            taken[1], "the list record");
    }

    private static void ShadowListCarriesItsSplit()
    {
        var context = default(ScriptableRenderContext);
        var culling = Cull(context);
        var split = new ShadowSplitData { cullingSphere = new Vector4(1f, 2f, 3f, 40f), cullingPlaneCount = 2 };
        split.SetCullingPlane(0, new Plane(new Vector3(0f, 1f, 0f), 2.5f));
        split.SetCullingPlane(1, new Plane(new Vector3(1f, 0f, 0f), -4f));
        var settings = new ShadowDrawingSettings(culling, 3, BatchCullingProjectionType.Orthographic)
        {
            splitData = split,
            objectsFilter = ShadowObjectsFilter.StaticOnly,
        };
        var list = context.CreateShadowRendererList(ref settings);
        var taken = CommandWireReader.Take();
        Expect.Equal(2, taken.Length, "the culling result, then the list");
        Expect.Equal(
            $"CreateList id={list.Id} shadow cull={CullLine(taken[0])} light=3 planes=[(0 1 0 2.5),(1 0 0 -4)] sphere=(1 2 3 40) filter=2 projection=2",
            taken[1], "the shadow list record");
        var second = Cull(context);
        var none = new ShadowDrawingSettings(second, 0);
        var bare = context.CreateShadowRendererList(ref none);
        var again = CommandWireReader.Take();
        Expect.Equal(
            $"CreateList id={bare.Id} shadow cull={CullLine(again[0])} light=0 planes=[] sphere=(0 0 0 0) filter=0 projection=1",
            again[1], "an empty split and the defaults");
    }

    private static void ListsAreDistinctAndEqualById()
    {
        var context = default(ScriptableRenderContext);
        var culling = Cull(context);
        var parameters = new RendererListParams(culling, Drawing("A"), FilteringSettings.defaultValue);
        var shadow = new ShadowDrawingSettings(culling, 0);
        var first = context.CreateRendererList(ref parameters);
        var second = context.CreateRendererList(ref parameters);
        var third = context.CreateShadowRendererList(ref shadow);
        Expect.That(first != second && second != third && first != third, "each list has its own id");
        Expect.That(first.Equals(first) && first.Equals((object)first) && !first.Equals("x") && first.GetHashCode() == first.GetHashCode(), "equality by id");
        Expect.That(!default(RendererList).isValid && !RendererList.nullRendererList.isValid && default(RendererList) == RendererList.nullRendererList, "the null list");
        Expect.That(first.Id != culling.Handle && second.Id != culling.Handle && third.Id != culling.Handle, "list ids differ from the culling id");
    }

    private static void DrawRecordsInOrder()
    {
        var context = default(ScriptableRenderContext);
        var culling = Cull(context);
        var parameters = new RendererListParams(culling, Drawing("A"), FilteringSettings.defaultValue);
        var list = context.CreateRendererList(ref parameters);
        var buffer = new CommandBuffer();
        buffer.EnableShaderKeyword("BEFORE");
        buffer.DrawRendererList(list);
        buffer.DisableShaderKeyword("BEFORE");
        Expect.Equal(3, buffer.commandCount, "the draw is one command");
        context.ExecuteCommandBuffer(buffer);
        context.Submit();
        var taken = CommandWireReader.Take();
        Expect.Equal(5, taken.Length, "the culling result, the list and the three commands");
        Same(["Keyword BEFORE=True", $"DrawList list={list.Id}", "Keyword BEFORE=False"], taken[2..], "the draw sits between the keywords");
    }

    private static void InvalidListsAreRefused()
    {
        var buffer = new CommandBuffer();
        var error = Expect.Catch<ArgumentException>(() => buffer.DrawRendererList(default), "the null list");
        Expect.That(error.Message.Contains("CommandBuffer.DrawRendererList", StringComparison.Ordinal), $"the message names the member: {error.Message}");
        Expect.Equal(0, buffer.commandCount, "nothing was recorded");
        buffer.Release();
        Expect.Throws<ObjectDisposedException>(() => buffer.DrawRendererList(default), "a released buffer");
    }

    private static void BadSettingsAreRefused()
    {
        var context = default(ScriptableRenderContext);
        var culling = Cull(context);
        var valid = new RendererListParams(culling, Drawing("A"), FilteringSettings.defaultValue);
        var noCulling = valid;
        noCulling.cullingResults = default;
        var error = Expect.Catch<ArgumentException>(() => context.CreateRendererList(ref noCulling), "no culling result");
        Expect.That(error.Message.Contains("RendererListParams.cullingResults", StringComparison.Ordinal), $"the message names the member: {error.Message}");
        var noName = valid;
        noName.drawSettings = new DrawingSettings(ShaderTagId.none, default);
        Expect.Throws<ArgumentException>(() => context.CreateRendererList(ref noName), "no pass name");
        var unset = valid;
        unset.drawSettings = default;
        Expect.Throws<ArgumentException>(() => context.CreateRendererList(ref unset), "default drawing settings");
        var backwards = valid;
        backwards.filteringSettings = new FilteringSettings(new RenderQueueRange { lowerBound = 5, upperBound = 4 });
        Expect.Throws<ArgumentException>(() => context.CreateRendererList(ref backwards), "an empty queue range");
        var single = valid;
        single.filteringSettings = new FilteringSettings(new RenderQueueRange { lowerBound = 4, upperBound = 4 });
        Expect.That(context.CreateRendererList(ref single).isValid, "a one-queue range is accepted");
        var gone = new Material(Material.SpriteLit);
        var destroyed = valid;
        destroyed.drawSettings.overrideMaterial = gone;
        UnityEngine.Object.DestroyImmediate(gone);
        var wrongMaterial = Expect.Catch<ArgumentException>(() => context.CreateRendererList(ref destroyed), "a destroyed override material");
        Expect.That(wrongMaterial.Message.Contains("override material", StringComparison.Ordinal), $"the message says what is wrong: {wrongMaterial.Message}");
        var shadowNone = new ShadowDrawingSettings(default, 0);
        var shadowError = Expect.Catch<ArgumentException>(() => context.CreateShadowRendererList(ref shadowNone), "a shadow list without a culling result");
        Expect.That(shadowError.Message.Contains("ShadowDrawingSettings.cullingResults", StringComparison.Ordinal), $"the message names the member: {shadowError.Message}");
        var negative = new ShadowDrawingSettings(culling, -1);
        Expect.Throws<ArgumentOutOfRangeException>(() => context.CreateShadowRendererList(ref negative), "a negative light index");
    }

    private static void StatusFollowsValidity()
    {
        var context = default(ScriptableRenderContext);
        var culling = Cull(context);
        var parameters = new RendererListParams(culling, Drawing("A"), FilteringSettings.defaultValue);
        var list = context.CreateRendererList(ref parameters);
        Expect.Equal(RendererListStatus.kRendererListPopulated, context.QueryRendererListStatus(list), "a list is populated");
        Expect.Equal(RendererListStatus.kRendererListInvalid, context.QueryRendererListStatus(default), "the null list is invalid");
        Expect.Equal(RendererListStatus.kRendererListInvalid, context.QueryRendererListStatus(RendererList.nullRendererList), "the null list again");
        CommandWireReader.Take();
        context.PrepareRendererListsAsync([list]);
        Expect.Equal(0, CommandWireReader.Take().Length, "preparing records nothing");
    }

    private static void ListsEndWithTheirFrame()
    {
        var context = default(ScriptableRenderContext);
        var culling = Cull(context);
        var parameters = new RendererListParams(culling, Drawing("A"), FilteringSettings.defaultValue);
        var list = context.CreateRendererList(ref parameters);
        CommandWireReader.Take();
        var buffer = new CommandBuffer();
        buffer.DrawRendererList(list);
        context.ExecuteCommandBuffer(buffer);
        Expect.Throws<DisunityException>(context.Submit, "the world no longer holds the list");
        Expect.Equal(0, CommandWireReader.Take().Length, "the refused draw left nothing");
        var old = new RendererListParams(culling, Drawing("A"), FilteringSettings.defaultValue);
        Expect.Throws<DisunityException>(() => context.CreateRendererList(ref old), "the culling result ended with its frame");
    }

    private static void ParamsValidityFollowsTheCullingResult()
    {
        var context = default(ScriptableRenderContext);
        var culling = Cull(context);
        Expect.That(!new RendererListParams(default, Drawing("A"), FilteringSettings.defaultValue).isValid, "no culling result");
        Expect.That(new RendererListParams(culling, Drawing("A"), FilteringSettings.defaultValue).isValid, "with a culling result");
        Expect.That(!default(RendererListParams).isValid, "the default value");
        var carried = new RendererListParams(culling, Drawing("A"), FilteringSettings.defaultValue);
        Expect.That(carried.tagName == ShaderTagId.none && !carried.isPassTagName && carried.tagValues.Length == 0 && carried.stateBlocks.Length == 0, "the tag fields start empty");
    }
}
