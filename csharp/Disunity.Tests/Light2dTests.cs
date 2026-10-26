using UnityEngine;
using UnityEngine.Rendering.Universal;
using Disunity.Hosting;
internal static class Light2dTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        var light = DisunityHost.CreateGameObject().AddComponent<Light2D>();
        light.color = new Color(0.25f, 0.5f, 0.75f, 1);
        light.intensity = 2;
        light.enabled = false;
        light.affectedSortingLayers = [7, -2];
        if (light.color.g != 0.5f || light.intensity != 2 || light.enabled || light.lightType != Light2D.LightType.Global)
            throw new InvalidOperationException("native global light state");
        if (!light.affectedSortingLayers.SequenceEqual(new[] { 7, -2 })) throw new Exception("native target sorting layers");
        var copy = light.affectedSortingLayers;
        copy[0] = 99;
        if (light.affectedSortingLayers[0] != 7) throw new Exception("layer readback must be owned");
        foreach (var type in new[] { Light2D.LightType.Parametric, Light2D.LightType.Freeform, Light2D.LightType.Sprite, Light2D.LightType.Point, Light2D.LightType.Spot })
        {
            try { light.lightType = type; throw new Exception("unsupported light type admitted"); }
            catch (UnsupportedLight2DException e) { if (e.Field != "lightType") throw; }
        }
        try { light.blendStyleIndex = 1; throw new Exception("unsupported blend style admitted"); }
        catch (UnsupportedLight2DException e) { if (e.Field != "blendStyleIndex") throw; }
        try { _ = new ShadowCaster2D(); throw new Exception("unsupported shadow caster admitted"); }
        catch (UnsupportedLight2DException e) { if (e.Field != "ShadowCaster2D") throw; }
        var before = light.color;
        try { light.color = new Color(0.75f, 0.25f, float.NaN, 1); throw new Exception("invalid color admitted"); }
        catch (DisunityException e) { if (e.Reason != DisunityErrorReason.InvalidComponent) throw; }
        if (light.color != before) throw new Exception("invalid light color partially published");
        var clone = UnityEngine.Object.Instantiate(light);
        if (clone.intensity != 2 || clone.enabled || !clone.affectedSortingLayers.SequenceEqual(new[] { 7, -2 })) throw new Exception("native light clone state");
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: native global Light2D, target layers, typed errors, atomic color and clone state");
    }
}
