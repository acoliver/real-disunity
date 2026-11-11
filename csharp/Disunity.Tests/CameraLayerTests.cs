using Disunity.Hosting;
using UnityEngine;

internal static class CameraLayerTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        var owner = DisunityHost.CreateGameObject();
        var listener = owner.AddComponent<AudioListener>();
        var gui = owner.AddComponent<GUILayer>();
        var flare = owner.AddComponent<FlareLayer>();
        if (!listener.enabled || !gui.enabled || !flare.enabled || !listener.isActiveAndEnabled) throw new Exception("native camera component defaults");
        if (DisunityHost.AudioListenerDiagnostic is not null) throw new Exception("one listener");
        AudioListener.volume = 0.4f; AudioListener.pause = true;
        if (AudioListener.volume != 0.4f || !AudioListener.pause) throw new Exception("listener controls must be static");
        listener.enabled = false;
        if (listener.enabled || listener.isActiveAndEnabled || DisunityHost.AudioListenerDiagnostic is null) throw new Exception("disabled listener diagnostic");
        listener.enabled = true;
        owner.SetActive(false);
        if (listener.isActiveAndEnabled || DisunityHost.AudioListenerDiagnostic is null) throw new Exception("inactive listener diagnostic");
        owner.SetActive(true);
        var second = DisunityHost.CreateGameObject(); second.AddComponent<AudioListener>();
        if (DisunityHost.AudioListenerDiagnostic is null) throw new Exception("multiple listener diagnostic");
        second.Destroy();
        if (DisunityHost.AudioListenerDiagnostic is not null) throw new Exception("destroyed listener diagnostic");
        AudioListener.volume = 1; AudioListener.pause = false;
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: native camera-side components, static listener controls and active listener diagnostics");
        Skyboxes();
        Console.WriteLine("PASS: a Skybox is a native camera-side component that holds its material");
    }

    private static void Skyboxes()
    {
        DisunityHost.LoadWorld();
        var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
        if (camera.TryGetComponent<Skybox>(out _)) throw new Exception("a camera has no Skybox of its own");
        var sky = camera.gameObject.AddComponent<Skybox>();
        if (!sky.enabled || sky.material != null) throw new Exception("a new Skybox is enabled and has no material");
        if (!camera.TryGetComponent<Skybox>(out var found) || found != sky) throw new Exception("the camera finds its Skybox");
        sky.enabled = false;
        if (sky.enabled || sky.isActiveAndEnabled) throw new Exception("a disabled Skybox reads as disabled");
        var copy = UnityEngine.Object.Instantiate(camera.gameObject).GetComponent<Skybox>()!;
        if (copy.enabled) throw new Exception("the world holds the enabled flag, so a clone copies it");
        sky.enabled = true;
        var material = new Material(Material.SpriteUnlit);
        sky.material = material;
        if (sky.material != material) throw new Exception("the Skybox keeps its material");
        DisunityHost.UnloadWorld();
    }
}
