using System.Text.Json;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>Registers a plan's input axes. A key axis is registered when the engine knows every key name it
/// uses, so a misspelt or unknown name fails as an unregistered axis does instead of reading zero forever. A
/// joystick or mouse button registers and reads never pressed, and a mouse movement axis registers with no keys
/// and reads zero, because nothing here moves a mouse or presses a joystick; a joystick axis is not registered,
/// because the engine has no joystick that could be at rest.</summary>
internal static class InputAxes
{
    private static readonly string[] Buttons = ["negative", "positive", "alt_negative", "alt_positive"];

    /// <summary>Registers the axes with their position in the input manager as their id, and
    /// returns why each other axis was not registered.</summary>
    public static List<string> Register(JsonElement axes)
    {
        var unregistered = new List<string>();
        ushort id = 0;
        foreach (var axis in axes.EnumerateArray())
        {
            var name = axis.Str("name");
            var kind = axis.Str("kind");
            var missing = Buttons.Select(button => axis.Str(button)).Where(button => button != "" && Key(button) is null).ToList();
            if (kind is not ("KeyOrMouseButton" or "MouseMovement"))
                unregistered.Add($"{name}: {kind} axes read a device the engine has no input for");
            else if (kind == "KeyOrMouseButton" && missing.Count > 0)
                unregistered.Add($"{name}: no key for {string.Join(", ", missing.Select(button => $"`{button}`"))}");
            else
                DisunityHost.RegisterInputAxis(id, Definition(axis));
            id++;
        }
        return unregistered;
    }

    private static InputAxisDefinition Definition(JsonElement axis) =>
        new(axis.Str("name"), Bound(axis, "negative"), Bound(axis, "positive"))
        {
            AltNegative = Bound(axis, "alt_negative"),
            AltPositive = Bound(axis, "alt_positive"),
            Gravity = axis.Float("gravity"),
            Sensitivity = axis.Float("sensitivity"),
            Dead = axis.Float("dead"),
            Snap = axis.Bool("snap"),
            Invert = axis.Bool("invert"),
        };

    private static KeyCode Bound(JsonElement axis, string button) => Key(axis.Str(button)) ?? KeyCode.None;

    /// <summary>The key an input manager button name names, or null when the engine does not know the name.
    /// The empty name is no key.</summary>
    public static KeyCode? Key(string name) => name == "" ? KeyCode.None : InputKeyNames.Find(name);
}
