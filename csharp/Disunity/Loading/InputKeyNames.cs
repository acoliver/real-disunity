using UnityEngine;

namespace Disunity.Hosting;

/// <summary>The input manager's names for keys and buttons. The engine accepts every name it knows, including the
/// joystick and mouse buttons the machine may have but nothing here presses; such a key registers and reads never pressed.</summary>
internal static class InputKeyNames
{
    private static readonly Dictionary<string, KeyCode> Names = Build();

    public static KeyCode? Find(string name) => Names.TryGetValue(name, out var key) ? key : null;

    private static Dictionary<string, KeyCode> Build()
    {
        var names = new Dictionary<string, KeyCode>
        {
            ["backspace"] = KeyCode.Backspace, ["tab"] = KeyCode.Tab, ["clear"] = KeyCode.Clear, ["return"] = KeyCode.Return,
            ["enter"] = KeyCode.KeypadEnter, ["pause"] = KeyCode.Pause, ["escape"] = KeyCode.Escape, ["space"] = KeyCode.Space,
            ["delete"] = KeyCode.Delete, ["insert"] = KeyCode.Insert, ["home"] = KeyCode.Home, ["end"] = KeyCode.End,
            ["page up"] = KeyCode.PageUp, ["page down"] = KeyCode.PageDown,
            ["up"] = KeyCode.UpArrow, ["down"] = KeyCode.DownArrow, ["left"] = KeyCode.LeftArrow, ["right"] = KeyCode.RightArrow,
            ["left shift"] = KeyCode.LeftShift, ["right shift"] = KeyCode.RightShift,
            ["left ctrl"] = KeyCode.LeftControl, ["right ctrl"] = KeyCode.RightControl,
            ["left alt"] = KeyCode.LeftAlt, ["right alt"] = KeyCode.RightAlt,
            ["left cmd"] = KeyCode.LeftCommand, ["right cmd"] = KeyCode.RightCommand,
            ["caps lock"] = KeyCode.CapsLock, ["numlock"] = KeyCode.Numlock, ["scroll lock"] = KeyCode.ScrollLock,
            ["print screen"] = KeyCode.Print,
            ["[+]"] = KeyCode.KeypadPlus, ["[-]"] = KeyCode.KeypadMinus, ["[*]"] = KeyCode.KeypadMultiply,
            ["[/]"] = KeyCode.KeypadDivide, ["[.]"] = KeyCode.KeypadPeriod, ["[=]"] = KeyCode.KeypadEquals,
            ["equals"] = KeyCode.Equals,
        };
        AddPunctuation(names);
        for (var i = 0; i < 10; i++)
        {
            names[i.ToString()] = KeyCode.Alpha0 + i;
            names[$"[{i}]"] = KeyCode.Keypad0 + i;
        }
        for (var i = 0; i < 26; i++) names[((char)('a' + i)).ToString()] = KeyCode.A + i;
        for (var i = 1; i <= 15; i++) names[$"f{i}"] = KeyCode.F1 + i - 1;
        for (var i = 0; i <= 6; i++) names[$"mouse {i}"] = KeyCode.Mouse0 + i;
        AddJoysticks(names);
        return names;
    }

    private static void AddPunctuation(Dictionary<string, KeyCode> names)
    {
        const string Marks = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`";
        foreach (var mark in Marks) names[mark.ToString()] = (KeyCode)mark;
    }

    private static void AddJoysticks(Dictionary<string, KeyCode> names)
    {
        for (var button = 0; button < 20; button++)
        {
            names[$"joystick button {button}"] = KeyCode.JoystickButton0 + button;
            for (var stick = 1; stick <= 8; stick++)
                names[$"joystick {stick} button {button}"] = KeyCode.Joystick1Button0 + (stick - 1) * 20 + button;
        }
    }
}
