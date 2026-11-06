using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>Material.SetPass checks the pass against the shader's declared pass count and records the active pass in the
/// native world. Nothing draws with it yet.</summary>
internal static unsafe class MaterialSetPassTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            APassInRangeIsRecordedInTheWorld();
            Console.WriteLine("PASS: Material.SetPass with a pass in range returns true and the world holds that material and pass");
            APassOutOfRangeLogsAndKeepsTheEarlierPass();
            Console.WriteLine("PASS: Material.SetPass with a pass out of range logs an error naming the material, pass and count, returns false and keeps the earlier pass");
            AnUndeclaredShaderThrows();
            Console.WriteLine("PASS: Material.SetPass on a shader without a declared pass count throws NotSupportedException and records nothing");
            AMaterialWithoutAWorldMaterialStillValidates();
            Console.WriteLine("PASS: Material.SetPass on a material with no world material validates the pass and records nothing");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static DisunityActivePass Active()
    {
        DisunityActivePass held;
        DisunityHost.ThrowIfError(NativeBindings.disunity_active_pass_read(DisunityHost.WorldHandle, &held));
        return held;
    }

    private static Material Declared(string shader, int passes, string name)
    {
        var material = new Material(DisunityHost.RegisterShader(shader).Declare(2000, passes, 1)) { name = name };
        _ = material.WorldLink;
        return material;
    }

    private static void APassInRangeIsRecordedInTheWorld()
    {
        var material = Declared("Test/SetPass/Range", 2, "Icons");
        Expect.That(Active().active == 0, "the world starts without an active pass");
        Expect.That(material.SetPass(1), "pass 1 of 2 is accepted");
        var held = Active();
        Expect.That(held.active == 1 && held.material == material.WorldLink.Id && held.pass == 1, "the world holds the material and pass 1");
        Expect.That(material.SetPass(0), "pass 0 of 2 is accepted");
        Expect.Equal(0u, Active().pass, "a second SetPass replaces the first");
        var other = Declared("Test/SetPass/Other", 1, "Other");
        Expect.That(other.SetPass(0) && Active().material == other.WorldLink.Id, "another material takes over the active pass");
    }

    private static void APassOutOfRangeLogsAndKeepsTheEarlierPass()
    {
        var material = Declared("Test/SetPass/Bounds", 2, "Bounded");
        Expect.That(material.SetPass(1), "an earlier pass is active");
        foreach (var bad in new[] { 2, 7, -1 })
        {
            using var log = new LogRecorder();
            Expect.That(!material.SetPass(bad), $"pass {bad} of 2 is refused");
            Expect.Equal(1, log.Messages.Count, $"pass {bad} logs once");
            var message = log.Messages[0];
            Expect.That(message.StartsWith("Error:") && message.Contains("Bounded") && message.Contains($"{bad}") && message.Contains("2 passes"),
                $"the error names the material, the pass and the count: {message}");
            var held = Active();
            Expect.That(held.active == 1 && held.material == material.WorldLink.Id && held.pass == 1, $"pass {bad} leaves the earlier active pass");
        }
    }

    private static void AnUndeclaredShaderThrows()
    {
        var material = new Material(DisunityHost.RegisterShader("Test/SetPass/Undeclared"));
        _ = material.WorldLink;
        var before = Active();
        var error = Expect.Catch<NotSupportedException>(() => material.SetPass(0), "SetPass on an undeclared shader");
        Expect.That(error.Message.Contains("passCount"), $"the exception names Shader.passCount: {error.Message}");
        Expect.Equal(before, Active(), "nothing was recorded");
    }

    private static void AMaterialWithoutAWorldMaterialStillValidates()
    {
        var kept = Declared("Test/SetPass/Kept", 1, "Kept");
        Expect.That(kept.SetPass(0), "an earlier pass is active");
        var loose = new Material(DisunityHost.RegisterShader("Test/SetPass/Loose").Declare(2000, 3, 1));
        Expect.That(loose.SetPass(2), "pass 2 of 3 is accepted");
        using (var log = new LogRecorder())
            Expect.That(!loose.SetPass(3) && log.Messages.Count == 1, "pass 3 of 3 is refused and logged");
        var held = Active();
        Expect.That(held.active == 1 && held.material == kept.WorldLink.Id, "a material without a world material records nothing");
    }
}
