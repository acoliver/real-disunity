using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering.Universal;
using EngineObject = UnityEngine.Object;

internal static class PendingCameraComponentTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        var owner = DisunityHost.CreateGameObject();
        var listener = owner.AddComponent<AudioListener>();
        var light = owner.AddComponent<Light2D>();
        var gui = owner.AddComponent<GUILayer>();
        var flare = owner.AddComponent<FlareLayer>();
        var data = owner.AddComponent<UniversalAdditionalCameraData>();
        EngineObject.Destroy(owner);
        if (!owner || !listener || !light || !listener.enabled || !gui.enabled || !flare.enabled || !data.enabled)
            throw new Exception("pending native components must remain readable");
        if (DisunityHost.AudioListenerDiagnostic is null)
            throw new Exception("pending listener must not count as active");
        listener.enabled = false;
        listener.enabled = true;
        light.intensity = 3;
        if (light.intensity != 3 || DisunityHost.AudioListenerDiagnostic is null)
            throw new Exception("pending writes must not reactivate listener participation");
        var clone = EngineObject.Instantiate(owner);
        if (clone.GetComponent<Light2D>()!.intensity != 3 || !clone.GetComponent<AudioListener>()!.enabled
            || clone.GetComponent<GUILayer>() is null || clone.GetComponent<FlareLayer>() is null
            || clone.GetComponent<UniversalAdditionalCameraData>() is null)
            throw new Exception("pending native components must clone completely");
        if (DisunityHost.AudioListenerDiagnostic is not null)
            throw new Exception("independent clone must be the single active listener");
        var pendingChild = DisunityHost.CreateGameObject();
        pendingChild.transform.SetParent(owner.transform, false);
        var addedListener = pendingChild.AddComponent<AudioListener>();
        var addedLight = pendingChild.AddComponent<Light2D>();
        if (!addedListener.enabled || !addedLight.enabled || DisunityHost.AudioListenerDiagnostic is not null)
            throw new Exception("components added under pending parent must inherit retirement");
        DisunityHost.Step(0);
        if (owner || listener || light || gui || flare || data || pendingChild || addedListener || addedLight || !clone)
            throw new Exception("frame cleanup must retire only the pending subtree");
        if (DisunityHost.AudioListenerDiagnostic is not null)
            throw new Exception("listener diagnostic after pending subtree retirement");
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: pending native camera component access, cloning, registration and listener diagnostics");
    }
}
