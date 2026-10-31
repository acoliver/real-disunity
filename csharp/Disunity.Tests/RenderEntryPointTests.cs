using Unity.Collections;
using UnityEngine;
using UnityEngine.Profiling;
using UnityEngine.Rendering;

/// <summary>The rendering entry points a pipeline links against: declared, with their deferred behaviour stated.</summary>
internal static class RenderEntryPointTests
{
    public static void Run()
    {
        Context();
        Lists();
        Console.WriteLine("PASS: ScriptableRenderContext members throw NotSupportedException naming the member; RendererList is never valid");
        Manager();
        Console.WriteLine("PASS: RenderPipelineManager accepts and drops event handlers; no pipeline is current");
        Samplers();
        Console.WriteLine("PASS: Sampler, CustomSampler and Recorder identify by name and read zero");
    }

    private static void Unsupported(Action action, string member)
    {
        var error = Expect.Catch<NotSupportedException>(action, member);
        Expect.That(error.Message.Contains($"ScriptableRenderContext.{member} ", StringComparison.Ordinal), $"the message names {member}: {error.Message}");
    }

    private static void Context()
    {
        var context = default(ScriptableRenderContext);
        var culling = default(ScriptableCullingParameters);
        var drawing = default(DrawingSettings);
        var filtering = FilteringSettings.defaultValue;
        var block = new RenderStateBlock(RenderStateMask.Nothing);
        var list = new RendererListParams(default, drawing, filtering);
        Unsupported(() => context.Submit(), "Submit");
        Unsupported(() => context.SubmitForRenderPassValidation(), "SubmitForRenderPassValidation");
        Unsupported(() => context.ExecuteCommandBuffer(new CommandBuffer()), "ExecuteCommandBuffer");
        Unsupported(() => context.ExecuteCommandBufferAsync(new CommandBuffer(), ComputeQueueType.Default), "ExecuteCommandBufferAsync");
        Unsupported(() => context.Cull(ref culling), "Cull");
        Unsupported(() => context.SetupCameraProperties(null!), "SetupCameraProperties");
        Unsupported(() => context.DrawSkybox(null!), "DrawSkybox");
        Unsupported(() => context.DrawGizmos(null!, GizmoSubset.PreImageEffects), "DrawGizmos");
        Unsupported(() => context.DrawUIOverlay(null!), "DrawUIOverlay");
        Unsupported(() => context.DrawWireOverlay(null!), "DrawWireOverlay");
        Unsupported(() => context.InvokeOnRenderObjectCallback(), "InvokeOnRenderObjectCallback");
        Unsupported(() => context.DrawRenderers(default, ref drawing, ref filtering), "DrawRenderers");
        Unsupported(() => context.DrawRenderers(default, ref drawing, ref filtering, ref block), "DrawRenderers");
        Unsupported(() => context.DrawRenderers(default, ref drawing, ref filtering, default, default), "DrawRenderers");
        Unsupported(() => context.BeginRenderPass(1, 1, 1, default), "BeginRenderPass");
        Unsupported(() => context.BeginSubPass(default), "BeginSubPass");
        Unsupported(() => context.BeginSubPass(default, default), "BeginSubPass");
        Unsupported(() => context.EndSubPass(), "EndSubPass");
        Unsupported(() => context.EndRenderPass(), "EndRenderPass");
        Unsupported(() => context.CreateRendererList(ref list), "CreateRendererList");
        Unsupported(() => context.QueryRendererListStatus(default), "QueryRendererListStatus");
        Unsupported(() => context.PrepareRendererListsAsync(new List<RendererList>()), "PrepareRendererListsAsync");
        Expect.That(context == default && context.Equals((object)default(ScriptableRenderContext)) && !context.Equals("x"), "contexts are equal");
    }

    private static void Lists()
    {
        Expect.That(!default(RendererList).isValid && !RendererList.nullRendererList.isValid, "a renderer list is never valid");
        Expect.That(default(RendererList) == RendererList.nullRendererList, "renderer lists are equal");
        var filtering = new FilteringSettings(RenderQueueRange.opaque, 3, 1, 0);
        var parameters = new RendererListParams(default, default, filtering);
        Expect.Equal(filtering, parameters.filteringSettings, "params keep the filtering settings");
        Expect.Equal(ShaderTagId.none, parameters.tagName, "no tag name");
        Expect.That(!parameters.isPassTagName && !parameters.isValid && parameters.tagValues.Length == 0 && parameters.stateBlocks.Length == 0, "params start empty and invalid");
    }

    private static void Manager()
    {
        Expect.That(RenderPipelineManager.currentPipeline == null, "no pipeline is current");
        Expect.That(!RenderPipelineManager.HasHandlers, "no handlers at first");
        Action<ScriptableRenderContext, List<Camera>> list = (_, _) => { };
        Action<ScriptableRenderContext, Camera> camera = (_, _) => { };
        Action<ScriptableRenderContext, Camera[]> frame = (_, _) => { };
        Action created = () => { };
        RenderPipelineManager.beginContextRendering += list;
        Expect.That(RenderPipelineManager.HasHandlers, "a handler is added");
        RenderPipelineManager.beginContextRendering -= list;
        Expect.That(!RenderPipelineManager.HasHandlers, "and removed");
        RenderPipelineManager.endContextRendering += list;
        RenderPipelineManager.beginCameraRendering += camera;
        RenderPipelineManager.endCameraRendering += camera;
        RenderPipelineManager.beginFrameRendering += frame;
        RenderPipelineManager.endFrameRendering += frame;
        RenderPipelineManager.activeRenderPipelineCreated += created;
        RenderPipelineManager.activeRenderPipelineDisposed += created;
        RenderPipelineManager.endContextRendering -= list;
        RenderPipelineManager.beginCameraRendering -= camera;
        RenderPipelineManager.endCameraRendering -= camera;
        RenderPipelineManager.beginFrameRendering -= frame;
        RenderPipelineManager.endFrameRendering -= frame;
        RenderPipelineManager.activeRenderPipelineCreated -= created;
        Expect.That(RenderPipelineManager.HasHandlers, "the disposed event still has its handler");
        RenderPipelineManager.activeRenderPipelineDisposed -= created;
        Expect.That(!RenderPipelineManager.HasHandlers, "every event can be removed");
    }

    private static void Samplers()
    {
        var sampler = Sampler.Get("R3.Sampler");
        Expect.That(ReferenceEquals(sampler, Sampler.Get("R3.Sampler")), "one sampler per name");
        Expect.That(!ReferenceEquals(sampler, Sampler.Get("R3.Other")), "different names, different samplers");
        Expect.That(sampler.name == "R3.Sampler" && sampler.isValid, "named and valid");
        Expect.Throws<ArgumentNullException>(() => Sampler.Get(null!), "Get(null)");
        var names = new List<string>();
        Expect.That(Sampler.GetNames(names) >= 2 && names.Contains("R3.Sampler") && names.Contains("R3.Other"), "GetNames lists the samplers made");

        var recorder = sampler.GetRecorder();
        Expect.That(ReferenceEquals(recorder, Sampler.Get("R3.Sampler").GetRecorder()) && ReferenceEquals(recorder, Recorder.Get("R3.Sampler")), "one recorder per sampler");
        Expect.That(recorder.isValid && recorder.enabled, "a recorder is valid and enabled");
        recorder.enabled = false;
        Expect.That(!recorder.enabled, "enabled is stored");
        recorder.FilterToCurrentThread();
        recorder.CollectFromAllThreads();
        Expect.That(recorder.elapsedNanoseconds == 0 && recorder.gpuElapsedNanoseconds == 0 && recorder.sampleBlockCount == 0 && recorder.gpuSampleBlockCount == 0, "recorded values are zero");

        var custom = CustomSampler.Create("R3.Custom");
        custom.Begin();
        custom.Begin(new Texture2D(1, 1));
        custom.End();
        Expect.That(ReferenceEquals(custom, CustomSampler.Create("R3.Custom", true)) && ReferenceEquals(custom, Sampler.Get("R3.Custom")), "a custom sampler is found by name");
        Expect.That(custom.isValid && custom.name == "R3.Custom", "a custom sampler is valid and named");
        var upgraded = CustomSampler.Create("R3.Sampler");
        Expect.That(ReferenceEquals(upgraded, Sampler.Get("R3.Sampler")) && !ReferenceEquals(upgraded, sampler), "creating a custom sampler of a plain sampler's name replaces it");
    }
}
