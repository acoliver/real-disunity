using Disunity.Hosting;
using UnityEngine;

internal static class NamedAxisTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        DisunityHost.RegisterInputAxis(10, new("First", KeyCode.S, KeyCode.W) { AltPositive = KeyCode.UpArrow });
        DisunityHost.RegisterInputAxis(11, new("Second", KeyCode.DownArrow, KeyCode.UpArrow) { Invert = true });
        DisunityHost.PushKey(0, KeyCode.UpArrow, true);
        DisunityHost.Step(0.01f);
        Require(Input.GetAxisRaw("First") == 1 && Input.GetAxisRaw("Second") == -1, "separate named bindings and invert");
        Require(MathF.Abs(Input.GetAxis("First") - 0.03f) < 0.00001f, "raw bypasses smoothing");
        try { Input.GetAxis("Absent"); throw new Exception("unknown name accepted"); }
        catch (ArgumentException) { }
        DisunityHost.PushFocus(0.011, false);
        DisunityHost.Step(0.01f);
        Require(Input.GetAxis("First") == 0 && Input.GetAxisRaw("Second") == 0, "focus releases named axes");
        DisunityHost.PushFocus(0.021, true);
        DisunityHost.PushAxis(0.021, "First", 0.5f);
        DisunityHost.Step(0.01f);
        Require(Input.GetAxisRaw("First") == 0.5f, "host axis lookup uses native registry");
        DisunityHost.UnloadWorld();
        DisunityHost.LoadWorld();
        try { Input.GetAxis("First"); throw new Exception("registry survived unload"); }
        catch (ArgumentException) { }
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: named axes, smoothing, focus and registry lifetime");
    }
    private static void Require(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }
}
