using Disunity.Hosting;
using UnityEngine;

/// <summary>KeyCode carries the standard numeric values, so the constants a compiled game holds read the keys they name.</summary>
internal static class KeyCodeTests
{
    public static void Run()
    {
        ValuesAreTheStandardOnes();
        MembersParseByName();
        AKeyPushedWithTheStandardValueReadsThroughItsMember();
        InputManagerNamesResolve();
        AnUnknownInputManagerNameHasNoKey();
        JoystickAndMouseButtonAxesRegisterAndReadNeverPressed();
        Console.WriteLine("PASS: KeyCode values, names and input manager button names");
    }

    private static void ValuesAreTheStandardOnes()
    {
        Expect.Equal(13, (int)KeyCode.Return, "Return");
        Expect.Equal(97, (int)KeyCode.A, "A");
        Expect.Equal(122, (int)KeyCode.Z, "Z");
        Expect.Equal(32, (int)KeyCode.Space, "Space");
        Expect.Equal(273, (int)KeyCode.UpArrow, "UpArrow");
        Expect.Equal(276, (int)KeyCode.LeftArrow, "LeftArrow");
        Expect.Equal(304, (int)KeyCode.LeftShift, "LeftShift");
        Expect.Equal(323, (int)KeyCode.Mouse0, "Mouse0");
        Expect.Equal(329, (int)KeyCode.Mouse6, "Mouse6");
        Expect.Equal(330, (int)KeyCode.JoystickButton0, "JoystickButton0");
        Expect.Equal(350, (int)KeyCode.Joystick1Button0, "Joystick1Button0");
        Expect.Equal(370, (int)KeyCode.Joystick2Button0, "Joystick2Button0");
        Expect.Equal(509, (int)KeyCode.Joystick8Button19, "Joystick8Button19");
        Expect.Equal(48, (int)KeyCode.Alpha0, "Alpha0");
        Expect.Equal(256, (int)KeyCode.Keypad0, "Keypad0");
        Expect.Equal(271, (int)KeyCode.KeypadEnter, "KeypadEnter");
        Expect.Equal(282, (int)KeyCode.F1, "F1");
        Expect.Equal(296, (int)KeyCode.F15, "F15");
        Expect.Equal(127, (int)KeyCode.Delete, "Delete");
        Expect.Equal(8, (int)KeyCode.Backspace, "Backspace");
        Expect.Equal(0, (int)KeyCode.None, "None");
        Expect.That(KeyCode.RightApple == KeyCode.RightCommand && KeyCode.LeftApple == KeyCode.LeftCommand, "the Apple names are aliases of the Command keys");
        Expect.Equal(309, (int)KeyCode.RightCommand, "RightCommand");
    }

    private static void MembersParseByName()
    {
        Expect.Equal(KeyCode.KeypadEnter, Enum.Parse<KeyCode>("KeypadEnter"), "KeypadEnter parses");
        Expect.Equal(KeyCode.RightShift, Enum.Parse<KeyCode>("RightShift"), "RightShift parses");
        Expect.Equal(KeyCode.LeftAlt, Enum.Parse<KeyCode>("LeftAlt"), "LeftAlt parses");
        Expect.Equal(KeyCode.Joystick3Button7, Enum.Parse<KeyCode>("Joystick3Button7"), "a numbered joystick button parses");
    }

    private static void AKeyPushedWithTheStandardValueReadsThroughItsMember()
    {
        DisunityHost.LoadWorld();
        DisunityHost.PushKey(0, (KeyCode)13, true);
        DisunityHost.PushKey(0, (KeyCode)304, true);
        DisunityHost.PushKey(0, (KeyCode)97, true);
        DisunityHost.Step(0.016f);
        Expect.That(Input.GetKey(KeyCode.Return) && Input.GetKeyDown(KeyCode.LeftShift) && Input.GetKey(KeyCode.A), "standard values read as their members");
        Expect.That(!Input.GetKey(KeyCode.Space) && !Input.GetKey(KeyCode.B) && !Input.GetKey(KeyCode.RightShift), "other keys read released");
        Expect.That(Input.anyKey && Input.anyKeyDown, "any key sees them");
        DisunityHost.UnloadWorld();
    }

    private static void InputManagerNamesResolve()
    {
        (string Name, KeyCode Key)[] names =
        [
            ("backspace", KeyCode.Backspace), ("tab", KeyCode.Tab), ("return", KeyCode.Return), ("enter", KeyCode.KeypadEnter),
            ("escape", KeyCode.Escape), ("space", KeyCode.Space), ("delete", KeyCode.Delete), ("insert", KeyCode.Insert),
            ("home", KeyCode.Home), ("end", KeyCode.End), ("page up", KeyCode.PageUp), ("page down", KeyCode.PageDown),
            ("up", KeyCode.UpArrow), ("down", KeyCode.DownArrow), ("left", KeyCode.LeftArrow), ("right", KeyCode.RightArrow),
            ("left shift", KeyCode.LeftShift), ("right shift", KeyCode.RightShift), ("left ctrl", KeyCode.LeftControl),
            ("right ctrl", KeyCode.RightControl), ("left alt", KeyCode.LeftAlt), ("right alt", KeyCode.RightAlt),
            ("left cmd", KeyCode.LeftCommand), ("right cmd", KeyCode.RightCommand), ("f1", KeyCode.F1), ("f15", KeyCode.F15),
            ("0", KeyCode.Alpha0), ("9", KeyCode.Alpha9), ("a", KeyCode.A), ("z", KeyCode.Z),
            ("[0]", KeyCode.Keypad0), ("[9]", KeyCode.Keypad9), ("[+]", KeyCode.KeypadPlus), ("[-]", KeyCode.KeypadMinus),
            ("[*]", KeyCode.KeypadMultiply), ("[/]", KeyCode.KeypadDivide), ("[.]", KeyCode.KeypadPeriod), ("equals", KeyCode.Equals),
            ("-", KeyCode.Minus), (",", KeyCode.Comma), (";", KeyCode.Semicolon), ("[", KeyCode.LeftBracket),
            ("mouse 0", KeyCode.Mouse0), ("mouse 6", KeyCode.Mouse6), ("joystick button 0", KeyCode.JoystickButton0),
            ("joystick button 19", KeyCode.JoystickButton19), ("joystick 1 button 0", KeyCode.Joystick1Button0),
            ("joystick 8 button 19", KeyCode.Joystick8Button19), ("", KeyCode.None),
        ];
        foreach (var (name, key) in names) Expect.Equal<KeyCode?>(key, InputAxes.Key(name), $"input manager name '{name}'");
    }

    private static void AnUnknownInputManagerNameHasNoKey()
    {
        foreach (var name in new[] { "banana", "joystick button 20", "joystick 9 button 0", "mouse 7", "f16", "Left Shift" })
            Expect.That(InputAxes.Key(name) is null, $"'{name}' names no key");
    }

    private static void JoystickAndMouseButtonAxesRegisterAndReadNeverPressed()
    {
        DisunityHost.LoadWorld();
        var log = new List<string>();
        var axes = System.Text.Json.JsonDocument.Parse("""
            [{"name":"Fire","kind":"KeyOrMouseButton","negative":"","positive":"joystick button 0","alt_negative":"","alt_positive":"mouse 1",
              "gravity":1000,"dead":0.001,"sensitivity":0.1,"snap":false,"invert":false},
             {"name":"Odd","kind":"KeyOrMouseButton","negative":"","positive":"banana","alt_negative":"","alt_positive":"",
              "gravity":1000,"dead":0.001,"sensitivity":0.1,"snap":false,"invert":false}]
            """).RootElement;
        log.AddRange(InputAxes.Register(axes));
        DisunityHost.Step(0.016f);
        Expect.Sequence(["Odd: no key for `banana`"], log, "only the unknown name is left unregistered");
        Expect.That(!Input.GetButton("Fire") && !Input.GetButtonDown("Fire") && Input.GetAxisRaw("Fire") == 0, "a joystick button axis reads never pressed");
        Expect.Throws<ArgumentException>(() => Input.GetAxis("Odd"), "the unknown axis stays unregistered");
        DisunityHost.UnloadWorld();
    }
}
