using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;
using Disunity.Hosting;
namespace PortableFixture;

/// <summary>A script whose first two Updates (the first runs while the scene loads and the second in the first frame the host
/// advances) clear the screen to yellow with a command buffer, and whose third, in the second frame, clears a render texture no
/// storage budget holds: a 256-layer array of 8192 by 8192 float texels, 256 GiB. The GPU refuses that frame whole, and a run
/// that goes on after it would write the yellow of the first frame again.</summary>
public sealed class ScreenRefusedFixture : MonoBehaviour
{
    private const int YellowUpdates = 2;
    private static readonly Color Cleared = new(1f, 1f, 0f, 1f);

    private CommandBuffer _yellow = null!;
    private CommandBuffer _huge = null!;
    private int _updates;

    public static void Create()
    {
        var pass = DisunityHost.CreateGameObject().AddComponent<ScreenRefusedFixture>();
        pass._yellow = new CommandBuffer();
        pass._yellow.SetRenderTarget(BuiltinRenderTextureType.CameraTarget);
        pass._yellow.ClearRenderTarget(true, true, Cleared);
        var description = new RenderTextureDescriptor(8192, 8192, GraphicsFormat.R32G32B32A32_SFloat, 0)
        {
            dimension = TextureDimension.Tex2DArray,
            volumeDepth = 256,
        };
        pass._huge = new CommandBuffer();
        pass._huge.SetRenderTarget(new RenderTexture(description));
        pass._huge.ClearRenderTarget(true, true, Cleared);
    }

    private void Update()
    {
        _updates++;
        if (_updates <= YellowUpdates) Graphics.ExecuteCommandBuffer(_yellow);
        else if (_updates == YellowUpdates + 1) Graphics.ExecuteCommandBuffer(_huge);
    }
}
