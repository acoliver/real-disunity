using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering.Universal;
internal static class CameraDataTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        var owner = DisunityHost.CreateGameObject();
        var data = owner.AddComponent<UniversalAdditionalCameraData>();
        data.renderPostProcessing = false;
        data.requiresDepthOption = CameraOverrideOption.Off;
        data.requiresColorOption = CameraOverrideOption.UsePipelineSettings;
        LayerMask mask = 7;
        data.volumeLayerMask = mask;
        if (data.volumeLayerMask.value != 7 || data.renderPostProcessing) throw new Exception("native camera state");
        foreach (var (field, write) in new (string, Action)[] {
            ("renderPostProcessing", () => data.renderPostProcessing = true),
            ("antialiasing", () => data.antialiasing = AntialiasingMode.FastApproximateAntialiasing),
            ("requiresDepthTexture", () => data.requiresDepthTexture = true),
            ("requiresColorTexture", () => data.requiresColorTexture = true),
            ("renderType", () => data.renderType = CameraRenderType.Overlay) })
        {
            try { write(); throw new Exception($"admitted {field}"); }
            catch (NotSupportedException e) { if (!e.Message.Contains(field)) throw; }
        }
        data.enabled = false;
        if (data.enabled || data.isActiveAndEnabled) throw new Exception("native camera enabled");
        DisunityHost.UnloadWorld();
    }
}
