using System.Text.Json.Nodes;
using UnityEngine;

/// <summary>Animators a player's resource plan holds: an animator builds with its settings and its planned controller,
/// controllers list their clips, an override controller replaces some of its base's clips, and each planned object is shared.</summary>
internal static class PlayerAnimatorTests
{
    public static void Run()
    {
        PlanHarness.WithPlan(WritePlan, () =>
        {
            ControllersListTheirClips();
            AnOverrideControllerReplacesClipsOfItsBase();
            AnAnimatorBuildsWithItsSettingsAndItsController();
            AnimatorsThatNameOneControllerShareIt();
            AnAnimatorWithoutAControllerHasNone();
            ClonesKeepTheControllerAndTheEnabledState();
        });
    }

    private static JsonArray Floats(params float[] values) => new(values.Select(v => (JsonNode)JsonValue.Create(v)!).ToArray());

    private static JsonObject Obj(string name, JsonNode component) => new()
    {
        ["name"] = name, ["tag"] = "Untagged", ["layer"] = 0, ["active"] = true, ["parent"] = null, ["template"] = true,
        ["position"] = Floats(0, 0, 0), ["rotation"] = Floats(0, 0, 0, 1), ["scale"] = Floats(1, 1, 1), ["rect"] = null,
        ["components"] = new JsonArray(component),
    };

    private static JsonObject AnimatorPlan(JsonNode? controller, bool enabled) => new()
    {
        ["type"] = "Animator", ["enabled"] = enabled, ["controller"] = controller, ["culling_mode"] = 2, ["update_mode"] = 1,
        ["apply_root_motion"] = true, ["linear_velocity_blending"] = false, ["stabilize_feet"] = true,
        ["keep_state_on_disable"] = true, ["write_defaults_on_disable"] = false,
    };

    private static void SavePrefab(string directory, string file, string name, JsonNode animator)
    {
        var plan = new JsonObject
        {
            ["scene"] = name, ["objects"] = new JsonArray(Obj(name, animator)),
            ["assets"] = new JsonArray(), ["sprites"] = new JsonArray(), ["textures"] = new JsonArray(), ["fonts"] = new JsonArray(), ["axes"] = new JsonArray(),
        };
        File.WriteAllText(Path.Combine(directory, file), plan.ToJsonString());
    }

    private static JsonObject Pair(int original, JsonNode? replacement) => new() { ["original"] = original, ["replacement"] = replacement };

    /// <summary>Assets: 0 the override controller, 1 its base, 2 to 4 the clips, 5 to 7 the prefabs.</summary>
    private static string WritePlan(string directory)
    {
        SavePrefab(directory, "unit.json", "Unit", AnimatorPlan(0, enabled: false));
        SavePrefab(directory, "twin.json", "Twin", AnimatorPlan(0, enabled: true));
        SavePrefab(directory, "bare.json", "Bare", AnimatorPlan(null, enabled: true));
        var assets = new JsonArray
        {
            new JsonObject
            {
                ["type"] = "AnimatorOverrideController", ["name"] = "Nets Animator", ["base_controller"] = 1,
                ["overrides"] = new JsonArray(Pair(2, 3), Pair(4, null)),
            },
            new JsonObject { ["type"] = "AnimatorController", ["name"] = "Base", ["clips"] = new JsonArray(2, 4) },
            new JsonObject { ["type"] = "AnimationClip", ["name"] = "Walk" },
            new JsonObject { ["type"] = "AnimationClip", ["name"] = "WalkNets" },
            new JsonObject { ["type"] = "AnimationClip", ["name"] = "Idle" },
            new JsonObject { ["type"] = "Prefab", ["name"] = "Unit", ["plan"] = "unit.json" },
            new JsonObject { ["type"] = "Prefab", ["name"] = "Twin", ["plan"] = "twin.json" },
            new JsonObject { ["type"] = "Prefab", ["name"] = "Bare", ["plan"] = "bare.json" },
        };
        string[] paths = ["controllers/nets", "controllers/base", "clips/walk", "clips/walknets", "clips/idle", "prefabs/unit", "prefabs/twin", "prefabs/bare"];
        string[] classes = ["AnimatorOverrideController", "AnimatorController", "AnimationClip", "AnimationClip", "AnimationClip", "GameObject", "GameObject", "GameObject"];
        var entries = new JsonArray(paths.Select((path, i) => (JsonNode)new JsonObject { ["path"] = path, ["class"] = classes[i], ["asset"] = i }).ToArray());
        return PlanHarness.WriteJson(directory, entries, assets);
    }

    private static string[] Names(AnimationClip[] clips) => clips.Select(c => c.name).ToArray();

    private static void ControllersListTheirClips()
    {
        var controller = Resources.Load<RuntimeAnimatorController>("controllers/base")!;
        Expect.Sequence(new[] { "Walk", "Idle" }, Names(controller.animationClips), "a controller lists its planned clips in order");
        Expect.That(ReferenceEquals(controller.animationClips[0], Resources.Load<AnimationClip>("clips/walk")), "the listed clip is the planned clip");
        Console.WriteLine("PASS: a planned animator controller lists its clips");
    }

    private static void AnOverrideControllerReplacesClipsOfItsBase()
    {
        var overrides = Resources.Load<AnimatorOverrideController>("controllers/nets")!;
        Expect.Equal("Nets Animator", overrides.name, "the override controller has its planned name");
        Expect.That(ReferenceEquals(overrides.runtimeAnimatorController, Resources.Load<RuntimeAnimatorController>("controllers/base")), "its base is the planned base controller");
        Expect.Sequence(new[] { "WalkNets", "Idle" }, Names(overrides.animationClips), "an override replaces its original and a null override keeps the original");
        Expect.Equal(2, overrides.overridesCount, "one slot per clip of the base");
        Console.WriteLine("PASS: a planned override controller replaces the clips of its base and keeps the rest");
    }

    private static void AnAnimatorBuildsWithItsSettingsAndItsController()
    {
        var animator = Resources.Load<GameObject>("prefabs/unit")!.GetComponent<Animator>()!;
        Expect.That(!animator.enabled, "the planned enabled flag");
        Expect.That(ReferenceEquals(animator.runtimeAnimatorController, Resources.Load<AnimatorOverrideController>("controllers/nets")), "the planned controller");
        Expect.Equal(AnimatorCullingMode.CullCompletely, animator.cullingMode, "culling mode");
        Expect.Equal(AnimatorUpdateMode.Fixed, animator.updateMode, "update mode");
        Expect.That(animator.applyRootMotion && !animator.linearVelocityBlending && animator.stabilizeFeet, "root motion flags");
        Expect.That(animator.keepAnimatorStateOnDisable && !animator.writeDefaultValuesOnDisable, "disable flags");
        Expect.Equal(0, animator.layerCount, "no playback state: the engine plays no animation");
        Console.WriteLine("PASS: a planned animator has its settings and its planned controller");
    }

    private static void AnimatorsThatNameOneControllerShareIt()
    {
        var unit = Resources.Load<GameObject>("prefabs/unit")!.GetComponent<Animator>()!;
        var twin = Resources.Load<GameObject>("prefabs/twin")!.GetComponent<Animator>()!;
        Expect.That(unit.runtimeAnimatorController is not null && ReferenceEquals(unit.runtimeAnimatorController, twin.runtimeAnimatorController), "two animators that name one controller get one object");
        Console.WriteLine("PASS: animators that name one planned controller share it");
    }

    private static void AnAnimatorWithoutAControllerHasNone()
    {
        var bare = Resources.Load<GameObject>("prefabs/bare")!.GetComponent<Animator>()!;
        Expect.That(bare.runtimeAnimatorController is null && bare.enabled, "an animator without a controller has none");
        Console.WriteLine("PASS: an animator without a planned controller has none");
    }

    private static void ClonesKeepTheControllerAndTheEnabledState()
    {
        var clone = UnityEngine.Object.Instantiate(Resources.Load<GameObject>("prefabs/unit")!);
        var animator = clone.GetComponent<Animator>()!;
        Expect.That(animator is not null && !animator.enabled, "a clone keeps the enabled flag");
        Expect.That(ReferenceEquals(animator!.runtimeAnimatorController, Resources.Load<AnimatorOverrideController>("controllers/nets")), "a clone shares the planned controller");
        Expect.Equal(AnimatorCullingMode.CullCompletely, animator.cullingMode, "a clone keeps the culling mode");
        Console.WriteLine("PASS: clones keep the planned controller and settings");
    }
}
