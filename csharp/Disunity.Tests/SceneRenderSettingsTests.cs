using System.Globalization;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.SceneManagement;

/// <summary>The managed values of a plan's <c>environment</c> (fog, the subtractive shadow colour, reflection bounces and
/// resolution, flares, the sun and the skybox material) reach <see cref="RenderSettings"/> with the ambient light and default
/// reflection of the same plan, before any Awake runs; a plan without an environment leaves all of them as they are.</summary>
internal static class SceneRenderSettingsTests
{
    private const string LightJson = """
        {"type":"Light","enabled":true,"kind":"Directional","color":[1,1,1,1],"intensity":1,"range":10,"spot_angle":30,"inner_spot_angle":20,
        "shadows":"None","shadow_strength":1,"shadow_bias":0.05,"shadow_normal_bias":0.4,"shadow_near_plane":0.2,"culling_mask":4294967295,
        "rendering_layer_mask":1,"bounce_intensity":1,"color_temperature":6570,"use_color_temperature":false,"cookie_size":10,"lightmap_bake_type":4}
        """;

    private const string SkyMaterial = """
        {"type":"Material","name":"Sky","shader":null,"render_queue":-1,"keywords":[],"tags":[],"disabled_passes":[],"floats":[],"ints":[],"colors":[],"textures":[]}
        """;

    /// <summary>Records what RenderSettings holds when it wakes.</summary>
    public sealed class Reader : MonoBehaviour
    {
        public static Color FogColor;
        public static bool Fog;
        public static Light? Sun;
        public static string? Skybox;
        public static float Probe;
        public static int Wakes;

        private void Awake()
        {
            Wakes++;
            (Fog, FogColor, Sun, Skybox, Probe) = (RenderSettings.fog, RenderSettings.fogColor, RenderSettings.sun, RenderSettings.skybox?.name, RenderSettings.ambientProbe[2, 8]);
        }
    }

    private static readonly SceneLoadOptions Options = new(typeof(SceneRenderSettingsTests).Assembly, _ => [])
    {
        Assemblies = _ => typeof(SceneRenderSettingsTests).Assembly,
    };

    private static string F(float value) => value.ToString("R", CultureInfo.InvariantCulture);

    private static string Floats(IEnumerable<float> values) => "[" + string.Join(",", values.Select(F)) + "]";

    /// <summary>Probe slot <c>i</c> holds <c>0.5 * i</c>, so a coefficient read back names the slot it came from.</summary>
    private static float Slot(int rgb, int coefficient) => 0.5f * (rgb * 9 + coefficient);

    private static string Settings(string sun, string skybox, float ambientIntensity = 1.25f, float reflectionIntensity = 0.875f, int bounces = 3,
        float fogDensity = 0.0625f) => $$"""
        "environment":{
        "ambient_mode":1,"ambient_sky":[1,0.875,0.75,0.625],"ambient_equator":[0.5,0.4375,0.375,0.3125],"ambient_ground":[0.25,0.125,0.0625,0.03125],
        "ambient_intensity":{{F(ambientIntensity)}},"ambient_probe":{{Floats(Enumerable.Range(0, 27).Select(i => 0.5f * i))}},
        "reflection_mode":0,"reflection_intensity":{{F(reflectionIntensity)}},"custom_reflection":null,"skybox_reflection":null,
        "reflection_bounces":{{bounces}},"reflection_resolution":256,
        "fog_enabled":true,"fog_color":[0.125,0.25,0.375,0.5],"fog_mode":3,"fog_density":{{F(fogDensity)}},"fog_start":10,"fog_end":300,
        "subtractive_shadow_color":[0.5,0.75,0.875,1],
        "halo_strength":0.25,"flare_strength":1.5,"flare_fade_speed":0.75,"sun":{{sun}},"skybox_material":{{skybox}}}
        """;

    private static string Object(string components) =>
        """{"name":"Lamp","tag":"Untagged","layer":0,"active":true,"parent":null,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[""" + components + "]}";

    private static string ReaderJson =>
        "{\"type\":\"Script\",\"class\":\"SceneRenderSettingsTests/Reader\",\"assembly\":\"" + typeof(SceneRenderSettingsTests).Assembly.GetName().Name + "\",\"enabled\":true,\"fields\":{}}";

    /// <summary>A scene with one lamp (component 0 a light, component 1 the reader) and the material <c>Sky</c> as asset 0.</summary>
    private static string Plan(string scene, string? settings) =>
        "{\"scene\":\"" + scene + "\",\"assets\":[" + SkyMaterial + "],\"sprites\":[],\"textures\":[],\"fonts\":[],\"axes\":[],\"differences\":[],\"objects\":["
        + Object(LightJson + "," + ReaderJson) + "]" + (settings is null ? "" : "," + settings) + "}";

    public static void Run()
    {
        ASceneLoadSetsEveryValue();
        Console.WriteLine("PASS: a scene load writes the plan's fog, ambient, probe, reflection, flare, sun and skybox values");
        AwakeSeesTheValues();
        Console.WriteLine("PASS: a script's Awake already reads the plan's RenderSettings");
        ValuesKeepTheirClamps();
        Console.WriteLine("PASS: planned values go through RenderSettings' clamps");
        NullSunAndSkyboxResetThem();
        Console.WriteLine("PASS: a plan without a sun or skybox clears the earlier ones; a sun that is not a light fails the load and writes nothing");
        APlanWithoutAnEnvironmentLeavesTheValues();
        Console.WriteLine("PASS: a plan without an environment leaves RenderSettings as it is");
    }

    private static void InWorld(Action body)
    {
        DisunityHost.LoadWorld();
        try { body(); }
        finally
        {
            DisunityHost.UnloadWorld();
            SceneManager.ClearHandlers();
        }
    }

    private static void ASceneLoadSetsEveryValue() => InWorld(() =>
    {
        using var scene = DisunityHost.LoadScene(Plan("level0", Settings("[0,0]", "0")), Options);
        Expect.That(RenderSettings.fog && RenderSettings.fogMode == FogMode.ExponentialSquared, "fog and its mode");
        Expect.Equal(new Color(0.125f, 0.25f, 0.375f, 0.5f), RenderSettings.fogColor, "fog color");
        Expect.Near(0.0625f, RenderSettings.fogDensity, "fog density");
        Expect.Near(10f, RenderSettings.fogStartDistance, "fog start");
        Expect.Near(300f, RenderSettings.fogEndDistance, "fog end");
        Expect.Equal(AmbientMode.Trilight, RenderSettings.ambientMode, "ambient mode");
        Expect.Equal(new Color(1f, 0.875f, 0.75f, 0.625f), RenderSettings.ambientSkyColor, "ambient sky");
        Expect.Equal(new Color(0.5f, 0.4375f, 0.375f, 0.3125f), RenderSettings.ambientEquatorColor, "ambient equator");
        Expect.Equal(new Color(0.25f, 0.125f, 0.0625f, 0.03125f), RenderSettings.ambientGroundColor, "ambient ground");
        Expect.Equal(RenderSettings.ambientSkyColor, RenderSettings.ambientLight, "ambientLight is the sky color");
        Expect.Near(1.25f, RenderSettings.ambientIntensity, "ambient intensity");
        for (var rgb = 0; rgb < 3; rgb++)
            for (var coefficient = 0; coefficient < 9; coefficient++)
                Expect.Near(Slot(rgb, coefficient), RenderSettings.ambientProbe[rgb, coefficient], $"probe[{rgb}, {coefficient}]");
        Expect.Equal(new Color(0.5f, 0.75f, 0.875f, 1f), RenderSettings.subtractiveShadowColor, "subtractive shadow color");
        Expect.Equal(DefaultReflectionMode.Skybox, RenderSettings.defaultReflectionMode, "reflection mode");
        Expect.Near(0.875f, RenderSettings.reflectionIntensity, "reflection intensity");
        Expect.Equal(3, RenderSettings.reflectionBounces, "reflection bounces");
        Expect.Equal(256, RenderSettings.defaultReflectionResolution, "reflection resolution");
        Expect.Near(0.25f, RenderSettings.haloStrength, "halo strength");
        Expect.Near(1.5f, RenderSettings.flareStrength, "flare strength");
        Expect.Near(0.75f, RenderSettings.flareFadeSpeed, "flare fade speed");
        Expect.That(RenderSettings.sun == scene.Objects[0].GetComponent<Light>(), "the sun is the lamp's light");
        Expect.That(RenderSettings.skybox is { name: "Sky" }, "the skybox is the planned material");
        Expect.That(RenderSettings.customReflectionTexture == null, "no custom reflection texture");
    });

    private static void AwakeSeesTheValues() => InWorld(() =>
    {
        Reader.Wakes = 0;
        using var scene = DisunityHost.LoadScene(Plan("level0", Settings("[0,0]", "0")), Options);
        Expect.Equal(1, Reader.Wakes, "the reader woke with the scene");
        Expect.That(Reader.Fog && Reader.FogColor == new Color(0.125f, 0.25f, 0.375f, 0.5f), "Awake read the fog");
        Expect.That(Reader.Sun == scene.Objects[0].GetComponent<Light>() && Reader.Skybox == "Sky", "Awake read the sun and skybox");
        Expect.Near(Slot(2, 8), Reader.Probe, "Awake read the probe");
    });

    private static void ValuesKeepTheirClamps() => InWorld(() =>
    {
        using var scene = DisunityHost.LoadScene(Plan("level0", Settings("null", "null", ambientIntensity: 12f, reflectionIntensity: 3f, bounces: 9)), Options);
        Expect.Near(8f, RenderSettings.ambientIntensity, "ambient intensity stops at 8");
        Expect.Near(1f, RenderSettings.reflectionIntensity, "reflection intensity stops at 1");
        Expect.Equal(5, RenderSettings.reflectionBounces, "reflection bounces stop at 5");
    });

    private static void NullSunAndSkyboxResetThem() => InWorld(() =>
    {
        using var first = DisunityHost.LoadScene(Plan("level0", Settings("[0,0]", "0")), Options);
        Expect.That(RenderSettings.sun != null && RenderSettings.skybox != null, "the first scene sets a sun and a skybox");
        using var second = DisunityHost.LoadScene(Plan("level1", Settings("null", "null")), Options);
        Expect.That(RenderSettings.sun == null && RenderSettings.skybox == null, "the second scene has neither");
        Expect.Throws<SceneLoadException>(() => DisunityHost.LoadScene(Plan("level2", Settings("[0,1]", "null", fogDensity: 0.5f)), Options), "a sun that is not a light");
        Expect.Near(0.0625f, RenderSettings.fogDensity, "a plan that fails writes none of the managed values");
    });

    private static void APlanWithoutAnEnvironmentLeavesTheValues() => InWorld(() =>
    {
        using var bare = DisunityHost.LoadScene(Plan("level0", null), Options);
        Expect.That(!RenderSettings.fog && RenderSettings.ambientMode == AmbientMode.Skybox && RenderSettings.sun == null && RenderSettings.skybox == null, "the defaults stay");
        Expect.That(RenderSettings.ambientProbe == default, "an empty probe stays");
        RenderSettings.fogDensity = 0.5f;
        using var again = DisunityHost.LoadScene(Plan("level1", null), Options);
        Expect.Near(0.5f, RenderSettings.fogDensity, "a value a script set stays");
    });
}
