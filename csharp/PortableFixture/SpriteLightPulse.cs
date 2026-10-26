using UnityEngine;
using UnityEngine.Rendering.Universal;
using Disunity.Hosting;
namespace PortableFixture;

public sealed class SpriteLightPulse : MonoBehaviour
{
    private Light2D _light = null!;
    public static void Create()
    {
        var light = DisunityHost.AddComponent(new Light2D());
        light.affectedSortingLayers = [1];
        var pulse = light.gameObject.AddComponent<SpriteLightPulse>();
        pulse._light = light;
    }
    private void Update() => _light.color = Time.Frame == 1
        ? new Color(0.25f, 0.5f, 0.75f) : new Color(0.5f, 0.25f, 1);
}
