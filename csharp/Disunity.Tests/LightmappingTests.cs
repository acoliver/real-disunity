using Unity.Collections;
using UnityEngine;
using UnityEngine.Experimental.GlobalIllumination;

/// <summary>A render pipeline hands the global illumination system a delegate that converts lights; the engine holds it.</summary>
internal static class LightmappingTests
{
    public static void Run()
    {
        Lightmapping.ResetDelegate();
        Expect.That(Lightmapping.GetDelegate() is null, "no delegate is set at first");
        var calls = 0;
        Lightmapping.RequestLightsDelegate convert = (Light[] requests, NativeArray<LightDataGI> output) => calls++;
        Lightmapping.SetDelegate(convert);
        Expect.That(ReferenceEquals(Lightmapping.GetDelegate(), convert), "the delegate set is the delegate read");
        Expect.Equal(0, calls, "the engine does not call it");
        var replacement = new Lightmapping.RequestLightsDelegate((requests, output) => { });
        Lightmapping.SetDelegate(replacement);
        Expect.That(ReferenceEquals(Lightmapping.GetDelegate(), replacement), "a second delegate replaces the first");
        Lightmapping.ResetDelegate();
        Expect.That(Lightmapping.GetDelegate() is null, "resetting clears it");
        var light = new LightDataGI { instanceID = 7, mode = LightMode.Baked, color = new LinearColor { red = 1, intensity = 2 } };
        Expect.That(light.instanceID == 7 && light.mode == LightMode.Baked && light.color.intensity == 2, "light data holds its fields");
        Console.WriteLine("PASS: Lightmapping holds the light conversion delegate a pipeline sets and never calls it");
    }
}
