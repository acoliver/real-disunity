using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;
using EngineObject = UnityEngine.Object;

/// <summary>The native 3D Light component: engine defaults, validation, clamping, cloning, its place in
/// the component hierarchy, the loader and the command buffers the engine cannot run.</summary>
internal static class LightTests
{
    private const string LightJson = """
        {"type":"Light","enabled":false,"kind":"Directional","color":[1,0.5,0.25,1],"intensity":2,"range":12,"spot_angle":40,"inner_spot_angle":20,
        "shadows":"Soft","shadow_strength":0.75,"shadow_bias":0.0625,"shadow_normal_bias":0.5,"shadow_near_plane":0.25,"culling_mask":4294967291,
        "rendering_layer_mask":3,"bounce_intensity":1.5,"color_temperature":5000,"use_color_temperature":true,"cookie_size":4,"lightmap_bake_type":2}
        """;

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Defaults();
            Console.WriteLine("PASS: Light defaults, attachment and clone state");
            Validation();
            Console.WriteLine("PASS: Light validation rejects negative and non-finite values and clamps angles and shadow strength");
            CommandBuffers();
            Console.WriteLine("PASS: Light command buffers throw NotSupportedException naming the member");
            Hierarchy();
            Console.WriteLine("PASS: a Light is a Behaviour that GetComponents<MonoBehaviour> and FindObjectsByType<MonoBehaviour> do not return");
            CloneIsolation();
            Console.WriteLine("PASS: a cloned Light has its own native settings and a disabled Light keeps them");
            Loaded();
            Console.WriteLine("PASS: a player plan builds a Light with every serialized setting");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static Light NewLight() => DisunityHost.CreateGameObject().AddComponent<Light>();

    private static void Defaults()
    {
        var light = NewLight();
        Ensure(light.type == LightType.Point && light.color == Color.white && light.intensity == 1f && light.range == 10f, "type, color, intensity and range");
        Ensure(light.spotAngle == 30f && light.innerSpotAngle == 21.8f && light.shadows == LightShadows.None && light.shadowStrength == 1f, "angles and shadows");
        Ensure(light.shadowBias == 0.05f && light.shadowNormalBias == 0.4f && light.shadowNearPlane == 0.2f, "shadow biases");
        Ensure(light.cullingMask == -1 && light.renderingLayerMask == 1 && light.bounceIntensity == 1f, "masks and bounce");
        Ensure(light.colorTemperature == 6570f && !light.useColorTemperature && light.cookieSize == 10f && light.cookie is null, "temperature and cookie");
        Ensure(light.lightmapBakeType == LightmapBakeType.Realtime && light.enabled, "bake type and enabled");
        Ensure(light.gameObject.GetComponent<Light>() == light, "GetComponent finds the light");
        light.type = LightType.Spot;
        light.intensity = 3f;
        light.enabled = false;
        light.renderingLayerMask = 6;
        var clone = EngineObject.Instantiate(light);
        Ensure(clone != light && clone.type == LightType.Spot && clone.intensity == 3f && !clone.enabled && clone.renderingLayerMask == 6, "a clone keeps the light's settings");
        clone.intensity = 5f;
        Ensure(light.intensity == 3f, "a clone's settings are its own");
        var owner = light.gameObject;
        EngineObject.Destroy(owner);
        DisunityHost.Step(0.01f);
        Ensure(!light, "destroying the object destroys the light");
    }

    private static void Validation()
    {
        var light = NewLight();
        foreach (var bad in new[] { -1f, float.NaN, float.PositiveInfinity, float.NegativeInfinity })
        {
            Throws<DisunityException>(() => light.range = bad, $"range {bad}");
            Throws<DisunityException>(() => light.intensity = bad, $"intensity {bad}");
            Throws<DisunityException>(() => light.cookieSize = bad, $"cookieSize {bad}");
            Throws<DisunityException>(() => light.bounceIntensity = bad, $"bounceIntensity {bad}");
        }
        foreach (var bad in new[] { 0f, -5f, float.NaN, float.PositiveInfinity })
            Throws<DisunityException>(() => light.colorTemperature = bad, $"colorTemperature {bad}");
        Ensure(light.range == 10f && light.intensity == 1f && light.cookieSize == 10f && light.bounceIntensity == 1f && light.colorTemperature == 6570f, "a rejected value leaves the setting unchanged");
        light.range = 0f;
        light.intensity = 0f;
        Ensure(light.range == 0f && light.intensity == 0f, "zero is allowed");
        light.spotAngle = 0f;
        Ensure(light.spotAngle == 1f, "spotAngle clamps up to 1");
        light.spotAngle = 500f;
        Ensure(light.spotAngle == 179f, "spotAngle clamps down to 179");
        light.innerSpotAngle = -3f;
        Ensure(light.innerSpotAngle == 0f, "innerSpotAngle clamps up to 0");
        light.spotAngle = 60f;
        light.innerSpotAngle = 90f;
        Ensure(light.innerSpotAngle == 60f, "innerSpotAngle clamps to spotAngle");
        light.spotAngle = 30f;
        Ensure(light.innerSpotAngle == 30f, "lowering spotAngle lowers innerSpotAngle with it");
        light.shadowStrength = 2f;
        Ensure(light.shadowStrength == 1f, "shadowStrength clamps down to 1");
        light.shadowStrength = -1f;
        Ensure(light.shadowStrength == 0f, "shadowStrength clamps up to 0");
        light.colorTemperature = 3200f;
        Ensure(light.colorTemperature == 3200f, "a positive temperature is stored");
    }

    private static void CommandBuffers()
    {
        var light = NewLight();
        var buffer = new CommandBuffer();
        Throws<NotSupportedException>(() => light.AddCommandBuffer(LightEvent.AfterShadowMap, buffer), "AddCommandBuffer",
            "Light.AddCommandBuffer", "does not render 3D lights yet");
        Throws<NotSupportedException>(() => light.RemoveCommandBuffer(LightEvent.AfterShadowMap, buffer), "RemoveCommandBuffer",
            "Light.RemoveCommandBuffer", "does not render 3D lights yet");
    }

    private static void Loaded()
    {
        using var loaded = PlayerCameraPlanTests.Load(LightJson);
        var light = loaded.Objects[0].GetComponent<Light>()!;
        Ensure(light.type == LightType.Directional && light.color == new Color(1f, 0.5f, 0.25f, 1f), "type and color");
        Ensure(light.intensity == 2f && light.range == 12f && light.spotAngle == 40f && light.innerSpotAngle == 20f, "intensity, range and angles");
        Ensure(light.shadows == LightShadows.Soft && light.shadowStrength == 0.75f && light.shadowBias == 0.0625f, "shadows");
        Ensure(light.shadowNormalBias == 0.5f && light.shadowNearPlane == 0.25f, "shadow normal bias and near plane");
        Ensure(light.cullingMask == unchecked((int)0xffff_fffb) && light.renderingLayerMask == 3, "masks");
        Ensure(light.bounceIntensity == 1.5f && light.colorTemperature == 5000f && light.useColorTemperature, "bounce and temperature");
        Ensure(light.cookieSize == 4f && light.cookie is null && light.lightmapBakeType == LightmapBakeType.Baked, "cookie and bake type");
        Ensure(!light.enabled, "the plan's enabled flag");
    }

    private static void Hierarchy()
    {
        var light = NewLight();
        Ensure(light is Behaviour && !typeof(MonoBehaviour).IsAssignableFrom(typeof(Light)), "a Light is a Behaviour and not a MonoBehaviour");
        var owner = light.gameObject;
        var script = owner.AddComponent<LightScriptProbe>();
        Ensure(owner.GetComponent<Light>() == light, "GetComponent<Light> finds the light");
        Ensure(owner.GetComponents<Behaviour>().Contains(light), "GetComponents<Behaviour> includes the light");
        var scripts = owner.GetComponents<MonoBehaviour>();
        Ensure(scripts.Length == 1 && scripts[0] == script, "GetComponents<MonoBehaviour> returns only the script");
        var found = EngineObject.FindObjectsByType<MonoBehaviour>(FindObjectsSortMode.None);
        Ensure(found.Contains(script) && found.Cast<EngineObject>().All(value => value is not Light), "FindObjectsByType<MonoBehaviour> returns the script and no light");
        Ensure(EngineObject.FindObjectsByType<Light>(FindObjectsSortMode.None).Contains(light), "FindObjectsByType<Light> finds the light");
        Ensure(owner.GetComponents<Component>().Contains(light), "GetComponents<Component> includes the light");
        EngineObject.Destroy(owner);
        DisunityHost.Step(0.01f);
    }

    private static void CloneIsolation()
    {
        var original = NewLight();
        original.intensity = 3f;
        original.type = LightType.Spot;
        original.color = new Color(0.25f, 0.5f, 0.75f, 1f);
        original.enabled = false;
        var cookie = new Texture2D(2, 2);
        original.cookie = cookie;
        var clone = EngineObject.Instantiate(original);
        Ensure(original.cookie == cookie && clone.cookie == cookie, "the cookie is stored in the native light and cloned with it");
        Ensure(clone != original && clone.gameObject != original.gameObject, "the clone is a separate light on a separate object");
        Ensure(clone.intensity == 3f && clone.type == LightType.Spot && clone.color == original.color && !clone.enabled, "the clone starts with the original's settings");
        Ensure(original.gameObject.GetComponent<Light>() == original && clone.gameObject.GetComponent<Light>() == clone, "each object finds its own light");
        clone.intensity = 7f;
        clone.enabled = true;
        Ensure(original.intensity == 3f && !original.enabled, "changing the clone leaves the original's settings");
        original.range = 99f;
        Ensure(clone.range == 10f, "changing the original leaves the clone's settings");
        EngineObject.Destroy(clone.gameObject);
        DisunityHost.Step(0.01f);
        Ensure(!clone && original && original.intensity == 3f, "destroying the clone leaves the original");
    }

    private sealed class LightScriptProbe : MonoBehaviour { }
}
