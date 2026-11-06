using UnityEngine;
using Disunity.Hosting;
namespace PortableFixture;

/// <summary>Records one camera render into a render texture and one blit with a material from it into another, in the first
/// frame's Update, so the host finds the offscreen work pending when the step returns.</summary>
public sealed class OffscreenPass : MonoBehaviour
{
    private Camera _camera = null!;
    private Material _material = null!;
    private RenderTexture _source = null!;
    private RenderTexture _target = null!;

    public static void Create()
    {
        var holder = DisunityHost.CreateGameObject();
        holder.transform.position = new Vector3(1, 2, 3);
        var pass = holder.AddComponent<OffscreenPass>();
        pass._material = new Material(DisunityHost.RegisterShader("Fixture/Blit"));
        pass._source = new RenderTexture(8, 8, 0);
        pass._target = new RenderTexture(16, 4, 0);
        pass._camera = holder.AddComponent<Camera>();
        pass._camera.targetTexture = pass._source;
        pass._camera.cullingMask = 1 << 3;
    }

    private void Update()
    {
        _camera.Render();
        Graphics.Blit(_source, _target, _material, -1);
    }
}
