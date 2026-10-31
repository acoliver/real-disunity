using System.Text.Json.Nodes;
using Disunity.Hosting;
using UnityEngine;

/// <summary>A player's resource plan carries the axes of its input manager. Applying them before the first scene
/// registers the ones the engine can read, so a game's reads of Submit and Mouse X succeed instead of throwing.</summary>
internal static class PlayerInputTests
{
    public static void Run()
    {
        RegisteredAxesReadWithoutThrowing();
        AnAxisTheEngineCannotReadIsLoggedAndStaysUnregistered();
        APlanWithoutAxesRegistersNone();
        Console.WriteLine("PASS: a player's planned input axes are registered before its first scene, the unreadable ones are logged");
    }

    private static JsonObject Axis(string name, string kind, string positive = "", string altPositive = "") => new()
    {
        ["name"] = name, ["kind"] = kind, ["negative"] = "", ["positive"] = positive, ["alt_negative"] = "", ["alt_positive"] = altPositive,
        ["gravity"] = 1000, ["dead"] = 0.001, ["sensitivity"] = 0.1, ["snap"] = false, ["invert"] = false,
    };

    private static void With(JsonArray? axes, Action<List<string>> run)
    {
        var directory = Directory.CreateTempSubdirectory("disunity-input-");
        try
        {
            var plan = new JsonObject { ["entries"] = new JsonArray(), ["assets"] = new JsonArray(), ["differences"] = new JsonArray() };
            if (axes is not null) plan["axes"] = axes;
            var path = Path.Combine(directory.FullName, "resources.json");
            File.WriteAllText(path, plan.ToJsonString());
            DisunityHost.LoadResources(path, _ => typeof(PlayerInputTests).Assembly);
            DisunityHost.LoadWorld();
            try
            {
                var log = new List<string>();
                DisunityHost.ApplyInputAxes(log.Add);
                run(log);
            }
            finally { DisunityHost.UnloadWorld(); }
        }
        finally
        {
            DisunityHost.UnloadResources();
            directory.Delete(recursive: true);
        }
    }

    private static void RegisteredAxesReadWithoutThrowing()
    {
        var axes = new JsonArray(Axis("Submit", "KeyOrMouseButton", "return"), Axis("Mouse X", "MouseMovement"), Axis("Mouse Y", "MouseMovement", "left ctrl"));
        With(axes, log =>
        {
            Expect.Equal(0, log.Count, "every axis registers");
            Expect.That(!Input.GetButtonDown("Submit"), "a key axis reads released and does not throw");
            Expect.Equal(0f, Input.GetAxis("Mouse X"), "the idle mouse reads zero on Mouse X");
            Expect.Equal(0f, Input.GetAxisRaw("Mouse Y"), "and on Mouse Y");
            DisunityHost.PushKey(0, KeyCode.Return, true);
            DisunityHost.Step(0.016f);
            Expect.That(Input.GetButton("Submit"), "a key axis reads the key it names");
        });
    }

    private static void AnAxisTheEngineCannotReadIsLoggedAndStaysUnregistered()
    {
        var axes = new JsonArray(Axis("Submit", "KeyOrMouseButton", "return"), Axis("Submit", "KeyOrMouseButton", "return", "banana"),
            Axis("Pad", "JoystickAxis"));
        With(axes, log =>
        {
            Expect.Sequence(["input axis not registered: Submit: no key for `banana`", "input axis not registered: Pad: JoystickAxis axes read a device the engine has no input for"], log, "the log names each axis and why");
            Expect.That(!Input.GetButton("Submit"), "the first Submit entry still registers");
            Expect.Throws<ArgumentException>(() => Input.GetAxis("Pad"), "a joystick axis stays unregistered");
        });
    }

    private static void APlanWithoutAxesRegistersNone()
    {
        With(null, log =>
        {
            Expect.Equal(0, log.Count, "nothing to log");
            Expect.Throws<ArgumentException>(() => Input.GetAxis("Mouse X"), "no axes are registered");
        });
        DisunityHost.ApplyInputAxes(_ => throw new InvalidOperationException("no resources are loaded"));
    }
}
