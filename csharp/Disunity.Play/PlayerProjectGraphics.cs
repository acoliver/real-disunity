using System.Text.Json;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

namespace Disunity.Play;

/// <summary>
/// The project graphics settings a built player starts with: the color space of its <c>PlayerSettings</c> and the
/// two light flags of its <c>GraphicsSettings</c>. The render pipeline reads them to convert light colors and to
/// pick the variant of its default render formats, so they are set once before any of the player's scripts run.
/// </summary>
internal static class PlayerProjectGraphics
{
    /// <summary>
    /// Applies the request's <c>project</c>: <c>{"color_space": "gamma" or "linear",
    /// "lights_use_linear_intensity": bool, "lights_use_color_temperature": bool}</c>. A request with no
    /// <c>project</c> leaves the engine's defaults, and a light flag the request omits stays at its default.
    /// </summary>
    /// <returns>A line that says what was applied, or null when the request holds no project settings.</returns>
    internal static string? Apply(JsonElement request)
    {
        if (!request.TryGetProperty("project", out var project)) return null;
        var space = ParseColorSpace(project.GetProperty("color_space").GetString());
        DisunityHost.SetColorSpace(space);
        var linear = Flag(project, "lights_use_linear_intensity");
        var temperature = Flag(project, "lights_use_color_temperature");
        GraphicsSettings.lightsUseLinearIntensity = linear;
        GraphicsSettings.lightsUseColorTemperature = temperature;
        return $"project graphics: {space} color space, lights use linear intensity {linear}, lights use color temperature {temperature}";
    }

    private static ColorSpace ParseColorSpace(string? name) => name switch
    {
        "gamma" => ColorSpace.Gamma,
        "linear" => ColorSpace.Linear,
        _ => throw new InvalidOperationException($"the player's color space {name ?? "(missing)"} is neither gamma nor linear"),
    };

    private static bool Flag(JsonElement project, string name) =>
        project.TryGetProperty(name, out var value) && value.GetBoolean();
}
