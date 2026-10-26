using System.Text.Json;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>Registers a plan's input axes. An axis is registered only when the engine can
/// deliver every input it names, so reading any other fails as an unregistered axis does
/// instead of reading zero forever.</summary>
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
            if (kind != "KeyOrMouseButton")
                unregistered.Add($"{name}: {kind} axes read a device the engine has no input for");
            else if (missing.Count > 0)
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

    /// <summary>The key a Unity input manager button name names, for the keys the engine
    /// delivers. "enter" is the keypad key in Unity, which the engine does not deliver.</summary>
    public static KeyCode? Key(string name) => name switch
    {
        "" => KeyCode.None,
        "space" => KeyCode.Space,
        "up" => KeyCode.UpArrow,
        "down" => KeyCode.DownArrow,
        "left" => KeyCode.LeftArrow,
        "right" => KeyCode.RightArrow,
        "return" => KeyCode.Return,
        "escape" => KeyCode.Escape,
        [var letter and >= 'a' and <= 'z'] => Enum.Parse<KeyCode>(letter.ToString().ToUpperInvariant()),
        _ => null,
    };
}
