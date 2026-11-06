using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

namespace PortableFixture;

/// <summary>A render pipeline that moves a probe object each time its Render runs, so a host can count the renders it caused.</summary>
internal sealed class ProbePipeline : RenderPipeline
{
    private readonly GameObject _probe = DisunityHost.CreateGameObject();
    private int _renders;

    internal static void Install()
    {
        DisunityHost.SetRenderPipeline(ScriptableObject.CreateInstance<ProbePipelineAsset>(), null);
        DisunityHost.ApplyRenderPipelines(_ => { });
    }

    protected override void Render(ScriptableRenderContext context, List<Camera> cameras)
    {
        _renders++;
        _probe.Transform.position = new Vector3(100f + 10f * _renders, 7f, 7f);
    }
}

internal sealed class ProbePipelineAsset : RenderPipelineAsset
{
    protected override RenderPipeline CreatePipeline() => new ProbePipeline();
}
