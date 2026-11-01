using System.Text.Json;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>Building the animation assets of a resource plan: <c>{"type": "AnimationClip", "name"}</c>,
/// <c>{"type": "AnimatorController", "name", "clips" [asset index or null]}</c> and <c>{"type": "AnimatorOverrideController",
/// "name", "base_controller" (asset index or null), "overrides" [{"original", "replacement"}]}</c>. The engine plays no
/// animation, so a clip holds its name, a controller lists its clips and has no states, and an override controller replaces
/// the clips of its base. A pair with no original, or with no replacement, overrides nothing, as in the original data.</summary>
internal static class PlayerAnimations
{
    public static AnimationClip Clip(JsonElement plan) => new() { name = plan.Str("name") };

    /// <summary>Builds the controller. <paramref name="resolve"/> gives the built object of a planned asset index.</summary>
    public static RuntimeAnimatorController Controller(JsonElement plan, Func<int, UnityEngine.Object> resolve)
    {
        var clips = plan.GetProperty("clips").EnumerateArray()
            .Where(clip => clip.ValueKind == JsonValueKind.Number)
            .Select(clip => (AnimationClip)resolve(clip.GetInt32()))
            .ToArray();
        return new PlannedAnimatorController(clips) { name = plan.Str("name") };
    }

    public static AnimatorOverrideController Overrides(JsonElement plan, Func<int, UnityEngine.Object> resolve)
    {
        var controller = new AnimatorOverrideController { name = plan.Str("name") };
        if (plan.Index("base_controller") is { } baseIndex) controller.runtimeAnimatorController = (RuntimeAnimatorController)resolve(baseIndex);
        foreach (var pair in plan.GetProperty("overrides").EnumerateArray())
            if (pair.Index("original") is { } original && pair.Index("replacement") is { } replacement)
                controller[(AnimationClip)resolve(original)] = (AnimationClip)resolve(replacement);
        return controller;
    }
}
