using UnityEngine;
using Disunity.Hosting;
namespace PortableFixture;

/// <summary>Records one camera render into a render texture and one blit with a material from it into another, in the first
/// frame's Update, so the host finds the offscreen work pending when the step returns. A mesh object that the camera draws is
/// active only around the render, as a script that bakes into a render texture switches its objects.</summary>
public sealed class OffscreenPass : MonoBehaviour
{
    private Camera _camera = null!;
    private GameObject _baked = null!;
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
        pass._baked = DisunityHost.CreateGameObject();
        pass._baked.layer = 3;
        pass._baked.AddComponent<MeshFilter>().sharedMesh = new Mesh
        {
            vertices = [new Vector3(0, 0, 0), new Vector3(1, 0, 0), new Vector3(1, 1, 0)],
            triangles = [0, 2, 1],
        };
        pass._baked.AddComponent<MeshRenderer>().sharedMaterial = pass._material;
        pass._baked.SetActive(false);
    }

    private void Update()
    {
        _baked.SetActive(true);
        _camera.Render();
        _baked.SetActive(false);
        Graphics.Blit(_source, _target, _material, -1);
    }
}
