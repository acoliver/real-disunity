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
    }
}
