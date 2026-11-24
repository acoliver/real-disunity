using System.Text.Json.Nodes;
using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

/// <summary>The environment lighting of <see cref="RenderSettings"/> lives in the world's one environment record: a script's
/// writes and a scene plan's environment reach it through the ABI, a read of the managed values reads it back, and a section
/// nobody wrote stays unset, so the host keeps its own defaults for it.</summary>
internal static unsafe class RenderSettingsEnvironmentTests
{
    public static void Run()
    {
        var directory = Directory.CreateTempSubdirectory("disunity-environment-");
        try
        {
            DisunityHost.LoadWorld();
            try
            {
                ANewWorldHoldsNoEnvironmentAndReadsTheDefaultsOfANewScene();
                WritesRoundTripThroughTheWorldRecord();
                TheRecordAndTheManagedValuesAreOneStore();
                ACustomReflectionTextureIsStoredByIdentity();
            }
            finally { DisunityHost.UnloadWorld(); }
            DisunityHost.LoadWorld();
            try { ASceneLoadFillsTheRecordFromItsPlan(directory.FullName); }
            finally { DisunityHost.UnloadWorld(); }
            DisunityHost.LoadWorld();
            try { ASceneWithoutAnEnvironmentLeavesTheRecordUnset(directory.FullName); }
            finally { DisunityHost.UnloadWorld(); }
            DisunityHost.LoadWorld();
            try { TheDefaultReflectionTextureIsTheCubeTheSceneReflects(directory.FullName); }
            finally { DisunityHost.UnloadWorld(); }
        }
        finally { directory.Delete(recursive: true); }
    }

    private static DisunityEnvironment Read()
    {
        DisunityEnvironment record;
        DisunityHost.ThrowIfError(NativeBindings.disunity_environment_read(DisunityHost.WorldHandle, &record));
        return record;
    }

    private static void Write(DisunityEnvironment record) =>
        DisunityHost.ThrowIfError(NativeBindings.disunity_environment_write(DisunityHost.WorldHandle, &record));

    private static SphericalHarmonicsL2 Probe()
    {
        var probe = new SphericalHarmonicsL2();
        for (var rgb = 0; rgb < 3; rgb++)
            for (var coefficient = 0; coefficient < 9; coefficient++) probe[rgb, coefficient] = 0.5f * rgb + 0.125f * coefficient - 0.25f;
        return probe;
    }

    private static Cubemap NewCube() => new(2, GraphicsFormat.R8G8B8A8_UNorm, TextureCreationFlags.None);

    private static void ANewWorldHoldsNoEnvironmentAndReadsTheDefaultsOfANewScene()
    {
        var record = Read();
        Expect.That(record.ambient_set == 0 && record.reflection_set == 0, "a new world holds no environment");
        Expect.That(RenderSettings.ambientMode == AmbientMode.Skybox && RenderSettings.ambientIntensity == 1f, "the ambient reads as a new scene's");
        Expect.That(RenderSettings.defaultReflectionMode == DefaultReflectionMode.Skybox && RenderSettings.reflectionIntensity == 1f
            && RenderSettings.customReflectionTexture == null, "the reflection reads as a new scene's");
        Expect.That(RenderSettings.ambientProbe == default, "the probe reads as empty");
        Expect.That(Read().ambient_set == 0 && Read().reflection_set == 0, "reading writes nothing");
        Console.WriteLine("PASS: a new world holds no environment record and RenderSettings reads a new scene's values from it");
    }

    private static void WritesRoundTripThroughTheWorldRecord()
    {
        RenderSettings.reflectionIntensity = 0.5f;
        var record = Read();
        Expect.That(record.reflection_set == 1 && record.ambient_set == 0, "a reflection write sets only the reflection section");
        Expect.Near(0.5f, record.reflection_intensity, "the record holds the reflection intensity");
        RenderSettings.defaultReflectionMode = DefaultReflectionMode.Custom;
        Expect.Equal(1u, Read().reflection_mode, "the record holds the reflection mode");
        RenderSettings.ambientMode = AmbientMode.Trilight;
        RenderSettings.ambientSkyColor = new Color(0.1f, 0.2f, 0.3f, 1f);
        RenderSettings.ambientEquatorColor = new Color(0.4f, 0.5f, 0.6f, 1f);
        RenderSettings.ambientGroundColor = new Color(0.7f, 0.8f, 0.9f, 1f);
        RenderSettings.ambientLight = new Color(0.9f, 0.8f, 0.7f, 1f);
        RenderSettings.ambientIntensity = 2f;
        RenderSettings.ambientProbe = Probe();
        record = Read();
        Expect.That(record.ambient_set == 1 && record.ambient_mode == 1 && record.ambient_intensity == 2f, "the record holds the ambient mode and intensity");
        Expect.That(record.ambient_sky[0] == 0.1f && record.ambient_equator[1] == 0.5f && record.ambient_ground[2] == 0.9f && record.ambient_flat[0] == 0.9f,
            "the record holds the ambient colours");
        var probe = Probe();
        for (var i = 0; i < 27; i++) Expect.That(record.ambient_probe[i] == probe[i / 9, i % 9], $"the record holds probe coefficient {i}");
        Expect.That(RenderSettings.ambientMode == AmbientMode.Trilight && RenderSettings.ambientSkyColor == new Color(0.1f, 0.2f, 0.3f, 1f)
            && RenderSettings.ambientLight == new Color(0.9f, 0.8f, 0.7f, 1f) && RenderSettings.ambientProbe == probe, "the managed values read back");
        RenderSettings.ambientIntensity = 20f;
        Expect.Near(8f, Read().ambient_intensity, "the clamp still applies before the record");
        Console.WriteLine("PASS: RenderSettings writes of the ambient and reflection values reach the world record and read back");
    }

    private static void TheRecordAndTheManagedValuesAreOneStore()
    {
        var record = Read();
        record.ambient_mode = (uint)AmbientMode.Flat;
        record.ambient_intensity = 3f;
        record.ambient_probe[4] = 7.5f;
        record.reflection_mode = (uint)DefaultReflectionMode.Skybox;
        record.reflection_intensity = 0.25f;
        Write(record);
        Expect.That(RenderSettings.ambientMode == AmbientMode.Flat && RenderSettings.ambientIntensity == 3f && RenderSettings.ambientProbe[0, 4] == 7.5f,
            "a write through the ABI reads back through RenderSettings");
        Expect.That(RenderSettings.defaultReflectionMode == DefaultReflectionMode.Skybox && RenderSettings.reflectionIntensity == 0.25f,
            "so does the reflection");
        Console.WriteLine("PASS: RenderSettings and the ABI read and write one world record");
    }

    private static void ACustomReflectionTextureIsStoredByIdentity()
    {
        var cube = NewCube();
        RenderSettings.customReflectionTexture = cube;
        var record = Read();
        Expect.That(record.custom_kind == 1 && record.custom_texture == cube.StoredIdentity && cube.StoredIdentity >= DisunityHost.FirstPlayerTextureIdentity,
            "the record names the cubemap by the identity the host stores it under");
        Expect.That(ReferenceEquals(RenderSettings.customReflectionTexture, cube), "the managed value reads the texture back");
        RenderSettings.customReflectionTexture = null;
        record = Read();
        Expect.That(record.custom_kind == 0 && record.custom_texture == 0 && RenderSettings.customReflectionTexture == null, "no texture clears the record's cube");
        var render = new RenderTexture(4, 4, 0);
        RenderSettings.customReflectionTexture = render;
        Expect.That(Read().custom_kind == 2 && RenderSettings.customReflectionTexture == render, "a render texture is named by its world id");
        RenderSettings.customReflectionTexture = null;
        Console.WriteLine("PASS: customReflectionTexture is stored in the record by the identity of the texture");
    }

    private static string ScenePlan(string directory, bool environment)
    {
        var bytes = Enumerable.Range(0, 96).Select(i => (byte)i).ToArray();
        File.WriteAllBytes(Path.Combine(directory, "cubes.bin"), bytes);
        JsonObject Cube(string name) => new()
        {
            ["type"] = "Cubemap", ["name"] = name, ["width"] = 2, ["height"] = 2, ["format"] = (int)TextureFormat.RGBA32, ["mip_count"] = 1,
            ["readable"] = false, ["color_space"] = 0, ["filter_mode"] = 1, ["wrap_u"] = 1, ["wrap_v"] = 1, ["wrap_w"] = 1, ["aniso_level"] = 1,
            ["mip_bias"] = 0.0, ["data"] = new JsonObject { ["file"] = Path.Combine(directory, "cubes.bin"), ["offset"] = 0, ["size"] = 96 },
        };
        var plan = new JsonObject
        {
            ["scene"] = "level0", ["sprites"] = new JsonArray(), ["textures"] = new JsonArray(), ["fonts"] = new JsonArray(), ["axes"] = new JsonArray(),
            ["differences"] = new JsonArray(), ["objects"] = new JsonArray(),
            ["assets"] = environment ? new JsonArray { Cube("Custom"), Cube("Baked") } : new JsonArray(),
        };
        if (environment)
            plan["environment"] = new JsonObject
            {
                ["ambient_mode"] = 1, ["ambient_sky"] = Floats(1f, 0.9f, 0.8f, 0.7f), ["ambient_equator"] = Floats(0.6f, 0.5f, 0.4f, 0.3f),
                ["ambient_ground"] = Floats(0.2f, 0.1f, 0.05f, 0.02f), ["ambient_intensity"] = 1.5, ["ambient_probe"] = Floats(Enumerable.Range(0, 27).Select(i => i * 0.5f).ToArray()),
                ["reflection_mode"] = 1, ["reflection_intensity"] = 0.75, ["custom_reflection"] = 0, ["skybox_reflection"] = 1,
                ["reflection_bounces"] = 1, ["reflection_resolution"] = 128, ["fog_enabled"] = false, ["fog_color"] = Floats(0.5f, 0.5f, 0.5f, 1f),
                ["fog_mode"] = 3, ["fog_density"] = 0.01, ["fog_start"] = 0, ["fog_end"] = 300, ["subtractive_shadow_color"] = Floats(0.42f, 0.478f, 0.627f, 1f),
                ["halo_strength"] = 0.5, ["flare_strength"] = 1, ["flare_fade_speed"] = 3, ["sun"] = null, ["skybox_material"] = null,
            };
        return plan.ToJsonString();
    }

    private static JsonArray Floats(params float[] values) => new(values.Select(value => (JsonNode)JsonValue.Create(value)).ToArray());

    private static void ASceneLoadFillsTheRecordFromItsPlan(string directory)
    {
        var assembly = typeof(RenderSettingsEnvironmentTests).Assembly;
        using var scene = DisunityHost.LoadScene(ScenePlan(directory, environment: true), new SceneLoadOptions(assembly, _ => []));
        var record = Read();
        Expect.That(record.ambient_set == 1 && record.ambient_mode == 1 && record.ambient_intensity == 1.5f, "the scene's ambient mode and intensity are in the record");
        Expect.That(record.ambient_sky[0] == 1f && record.ambient_equator[2] == 0.4f && record.ambient_ground[3] == 0.02f, "the scene's ambient colours are in the record");
        Expect.That(record.ambient_flat[0] == 1f && record.ambient_flat[1] == 0.9f, "the flat colour is the sky colour");
        Expect.That(record.ambient_probe[0] == 0f && record.ambient_probe[13] == 6.5f && record.ambient_probe[26] == 13f, "the scene's probe is in the record");
        Expect.That(record.reflection_set == 1 && record.reflection_mode == 1 && record.reflection_intensity == 0.75f, "the scene's reflection mode and intensity are in the record");
        var custom = RenderSettings.customReflectionTexture as Cubemap;
        Expect.That(custom is { name: "Custom" } && record.custom_kind == 1 && record.custom_texture == custom.StoredIdentity, "the record names the planned custom cube");
        Expect.That(record.skybox_kind == 1 && record.skybox_texture >= DisunityHost.FirstPlayerTextureIdentity && record.skybox_texture != record.custom_texture,
            "the record names the planned skybox cube by another identity");
        Expect.That(RenderSettings.defaultReflectionMode == DefaultReflectionMode.Custom && RenderSettings.ambientMode == AmbientMode.Trilight
            && RenderSettings.ambientLight == new Color(1f, 0.9f, 0.8f, 0.7f), "RenderSettings reads the scene's values");
        Console.WriteLine("PASS: a scene load fills the world's environment record from its plan, with its cubemaps stored by identity");
    }

    private static void TheDefaultReflectionTextureIsTheCubeTheSceneReflects(string directory)
    {
        Expect.That(ReflectionProbe.defaultTexture == null, "a world whose scene wrote no reflection has no default reflection texture");
        var assembly = typeof(RenderSettingsEnvironmentTests).Assembly;
        using var scene = DisunityHost.LoadScene(ScenePlan(directory, environment: true), new SceneLoadOptions(assembly, _ => []));
        Expect.That(ReflectionProbe.defaultTexture is Cubemap { name: "Custom" }, "the custom mode reads the custom cube");
        RenderSettings.defaultReflectionMode = DefaultReflectionMode.Skybox;
        Expect.That(ReflectionProbe.defaultTexture is Cubemap { name: "Baked" }, "the skybox mode reads the cube baked from the skybox");
        RenderSettings.defaultReflectionMode = DefaultReflectionMode.Custom;
        RenderSettings.customReflectionTexture = null;
        Expect.That(ReflectionProbe.defaultTexture is Cubemap { name: "Baked" }, "the custom mode without a custom cube reads the cube baked from the skybox");
        var cube = (Cubemap)ReflectionProbe.defaultTexture!;
        var buffer = new CommandBuffer();
        buffer.SetGlobalTexture("_DefaultProbe", cube);
        Graphics.ExecuteCommandBuffer(buffer);
        Expect.That(ReferenceEquals(Shader.GetGlobalTexture("_DefaultProbe"), cube), "a CommandBuffer sets ReflectionProbe.defaultTexture as a global cube");
        Shader.SetGlobalTexture("_DefaultProbe", (Texture?)null);
        Console.WriteLine("PASS: ReflectionProbe.defaultTexture is the cube the scene's default reflection reads, settable as a global cube");
    }

    private static void ASceneWithoutAnEnvironmentLeavesTheRecordUnset(string directory)
    {
        var assembly = typeof(RenderSettingsEnvironmentTests).Assembly;
        using var scene = DisunityHost.LoadScene(ScenePlan(directory, environment: false), new SceneLoadOptions(assembly, _ => []));
        var record = Read();
        Expect.That(record.ambient_set == 0 && record.reflection_set == 0, "a plan without an environment leaves both sections unset");
        Console.WriteLine("PASS: a scene plan without an environment leaves the record unset");
    }
}
