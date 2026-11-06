using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>Material.FindPass and Material.GetPassName read the pass names a player's shader declaration carries.</summary>
internal static class MaterialPassNameTests
{
    private static readonly string[] Names = ["ForwardLit", "", "ShadowCaster", "ForwardLit"];

    public static void Run()
    {
        FindPassFindsAPassByNameAndIgnoresCase();
        Console.WriteLine("PASS: Material.FindPass returns the first pass of a name, matching exactly or in another case, and -1 for a name no pass has");
        FindPassRefusesNullAndUndeclaredShaders();
        Console.WriteLine("PASS: Material.FindPass throws ArgumentNullException for null and NotSupportedException naming the member for a shader without pass names");
        GetPassNameReturnsTheNameOfAPassInRange();
        Console.WriteLine("PASS: Material.GetPassName returns the name of each pass, empty for an unnamed one");
        GetPassNameOutOfRangeLogsAndReturnsEmpty();
        Console.WriteLine("PASS: Material.GetPassName out of range logs an error naming the material, pass and count and returns the empty string");
        GetPassNameRefusesUndeclaredShaders();
        Console.WriteLine("PASS: Material.GetPassName throws NotSupportedException naming the member for a shader without pass names");
        ADeclarationWithoutNamesIsCompletedByALaterOne();
        Console.WriteLine("PASS: a shader declared by count alone takes the pass names of a later declaration that agrees");
    }

    private static Material Declared(string shader) =>
        new(DisunityHost.DeclareShader(shader, 2000, Names, 1, _ => { })) { name = "Lit" };

    private static void FindPassFindsAPassByNameAndIgnoresCase()
    {
        var material = Declared("Test/PassNames/Find");
        Expect.Equal(0, material.FindPass("ForwardLit"), "an exact name finds its first pass");
        Expect.Equal(2, material.FindPass("ShadowCaster"), "an exact name finds a later pass");
        Expect.Equal(2, material.FindPass("shadowcaster"), "a name in lower case finds the pass");
        Expect.Equal(0, material.FindPass("FORWARDLIT"), "a name in upper case finds the first of two passes with that name");
        Expect.Equal(1, material.FindPass(""), "the empty name finds an unnamed pass");
        Expect.Equal(-1, material.FindPass("Meta"), "a name no pass has gives -1");
        Expect.Equal(-1, material.FindPass("ForwardLit "), "a name that only starts like a pass gives -1");
    }

    private static void FindPassRefusesNullAndUndeclaredShaders()
    {
        var declared = Declared("Test/PassNames/Null");
        Expect.Catch<ArgumentNullException>(() => declared.FindPass(null!), "FindPass(null)");
        var undeclared = new Material(DisunityHost.RegisterShader("Test/PassNames/Undeclared"));
        var error = Expect.Catch<NotSupportedException>(() => undeclared.FindPass("Forward"), "FindPass on an undeclared shader");
        Expect.That(error.Message.Contains("Material.FindPass") && error.Message.Contains("Test/PassNames/Undeclared"), $"the exception names the member and the shader: {error.Message}");
        Expect.Catch<ArgumentNullException>(() => undeclared.FindPass(null!), "FindPass(null) on an undeclared shader");
    }

    private static void GetPassNameReturnsTheNameOfAPassInRange()
    {
        var material = Declared("Test/PassNames/Get");
        for (var pass = 0; pass < Names.Length; pass++)
        {
            using var log = new LogRecorder();
            Expect.Equal(Names[pass], material.GetPassName(pass), $"the name of pass {pass}");
            Expect.Equal(0, log.Messages.Count, $"pass {pass} in range logs nothing");
        }
    }

    private static void GetPassNameOutOfRangeLogsAndReturnsEmpty()
    {
        var material = Declared("Test/PassNames/Range");
        foreach (var bad in new[] { 4, 9, -1 })
        {
            using var log = new LogRecorder();
            Expect.Equal("", material.GetPassName(bad), $"pass {bad} of 4 has no name");
            Expect.Equal(1, log.Messages.Count, $"pass {bad} logs once");
            var message = log.Messages[0];
            Expect.That(message.StartsWith("Error:") && message.Contains("'Lit'") && message.Contains($"pass {bad} ") && message.Contains("4 passes"),
                $"the error names the material, the pass and the count: {message}");
        }
    }

    private static void GetPassNameRefusesUndeclaredShaders()
    {
        var undeclared = new Material(DisunityHost.RegisterShader("Test/PassNames/UndeclaredGet"));
        var error = Expect.Catch<NotSupportedException>(() => undeclared.GetPassName(0), "GetPassName on an undeclared shader");
        Expect.That(error.Message.Contains("Material.GetPassName") && error.Message.Contains("Test/PassNames/UndeclaredGet"), $"the exception names the member and the shader: {error.Message}");
    }

    private static void ADeclarationWithoutNamesIsCompletedByALaterOne()
    {
        var shader = DisunityHost.RegisterShader("Test/PassNames/Completed").Declare(2000, 4, 1);
        Expect.Catch<NotSupportedException>(() => new Material(shader).FindPass("ForwardLit"), "FindPass before the names are declared");
        var differences = new List<string>();
        DisunityHost.DeclareShader("Test/PassNames/Completed", 2000, Names, 1, differences.Add);
        Expect.That(differences.Count == 0 && new Material(shader).FindPass("ShadowCaster") == 2, "the later declaration supplies the names without a difference");
        DisunityHost.DeclareShader("Test/PassNames/Completed", 2000, ["ForwardLit", "", "Meta", "ForwardLit"], 1, differences.Add);
        Expect.That(differences.Count == 1 && new Material(shader).FindPass("Meta") == -1, "a third declaration with another name is noted and not applied");
    }
}
