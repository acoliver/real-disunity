using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering.Universal;

internal static class ParticipantRegressionTests
{
    public static void Run()
    {
        var failures = new List<Exception>();
        foreach (var test in new Action[] { CameraNumbers, MaterialSurface, ListenerReporting, HostAttachment })
        {
            try { test(); Console.WriteLine($"PASS: {test.Method.Name}"); }
            catch (Exception error) { Console.WriteLine($"FAIL: {test.Method.Name}: {error}"); failures.Add(error); }
            finally { DisunityHost.UnloadWorld(); }
        }
        if (failures.Count != 0) throw new AggregateException(failures);
    }
    private static void CameraNumbers()
    {
        if ((int)CameraOverrideOption.Off != 0 || (int)CameraOverrideOption.On != 1 || (int)CameraOverrideOption.UsePipelineSettings != 2)
            throw new Exception("Unity camera override numeric contract");
        DisunityHost.LoadWorld();
        var data = DisunityHost.CreateGameObject().AddComponent<UniversalAdditionalCameraData>();
        data.requiresDepthTexture = false; data.requiresColorTexture = false;
        if ((int)data.requiresDepthOption != 0 || (int)data.requiresColorOption != 0) throw new Exception("false must select Off=0");
    }
    private static void MaterialSurface()
    {
        DisunityHost.LoadWorld();
        var renderer = DisunityHost.CreateGameObject().AddComponent<SpriteRenderer>();
        if (renderer.sharedMaterial != Material.SpriteUnlit) throw new Exception("default sprite material");
        renderer.sharedMaterial = Material.SpriteLit;
        renderer.color = new Color(0.5f, 0.75f, 1);
        renderer.sprite = DisunityHost.CreateSprite(42, 1, 1, new Rect(0, 0, 1, 1), new Vector2(0.5f, 0.5f));
        var clone = UnityEngine.Object.Instantiate(renderer);
        if (renderer.sharedMaterial != Material.SpriteLit || clone.sharedMaterial != Material.SpriteLit)
            throw new Exception("native material selection or clone lost");
        renderer.sharedMaterial = Material.SpriteUnlit;
        if (renderer.sharedMaterial != Material.SpriteUnlit || clone.sharedMaterial != Material.SpriteLit)
            throw new Exception("per-renderer material selection");
    }
    private static void ListenerReporting()
    {
        var previous = Console.Error;
        using var output = new StringWriter();
        Console.SetError(output);
        try { ListenerTransitions(output); }
        finally { Console.SetError(previous); }
    }
    private static void ListenerTransitions(StringWriter output)
    {
        DisunityHost.LoadWorld();
        DisunityHost.Step(0);
        RequireDiagnostic(output, "no audio listeners");
        DisunityHost.Step(0);
        RequireDiagnostic(output, null);
        var one = DisunityHost.CreateGameObject().AddComponent<AudioListener>();
        DisunityHost.Step(0); RequireDiagnostic(output, null);
        var two = DisunityHost.CreateGameObject().AddComponent<AudioListener>();
        DisunityHost.Step(0); RequireDiagnostic(output, "2 audio listeners");
        two.enabled = false; DisunityHost.Step(0); RequireDiagnostic(output, null);
        one.gameObject.SetActive(false); DisunityHost.Step(0); RequireDiagnostic(output, "no audio listeners");
        DisunityHost.UnloadWorld(); output.GetStringBuilder().Clear();
        DisunityHost.LoadWorld(); DisunityHost.Step(0); RequireDiagnostic(output, "no audio listeners");
    }
    private static void RequireDiagnostic(StringWriter output, string? expected)
    {
        var text = output.ToString(); output.GetStringBuilder().Clear();
        if (expected is null ? text.Length != 0 : !text.Contains(expected)) throw new Exception($"diagnostic expected {expected}, got {text}");
    }
    private static void HostAttachment()
    {
        DisunityHost.LoadWorld();
        var light = DisunityHost.AddComponent(new Light2D());
        light.intensity = 2; light.enabled = false;
        var data = DisunityHost.AddComponent(new UniversalAdditionalCameraData());
        data.volumeLayerMask = 7; data.enabled = false;
        var lightClone = UnityEngine.Object.Instantiate(light);
        var cameraClone = UnityEngine.Object.Instantiate(data);
        if (light.intensity != 2 || light.enabled || lightClone.intensity != 2 || lightClone.enabled)
            throw new Exception("host native light registration and clone");
        if (data.enabled || cameraClone.enabled || cameraClone.volumeLayerMask.value != 7)
            throw new Exception("host native camera registration and clone");
        if (light.gameObject.GetComponent<Light2D>() != light || data.gameObject.GetComponent<UniversalAdditionalCameraData>() != data)
            throw new Exception("host native component lookup");
    }
}
