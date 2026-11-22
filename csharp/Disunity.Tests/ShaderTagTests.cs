using System.Text.Json.Nodes;
using Disunity.Hosting;
using UnityEngine;

/// <summary>Material.GetTag reads the tags the material's shader declares (the first sub-shader's), after the material's own
/// override tags and, when asked, along the shader's fallback chain. What the engine cannot know throws by name.</summary>
internal static class ShaderTagTests
{
    private static readonly string[] OnePass = ["Forward"];

    public static void Run()
    {
        AMaterialReadsTheTagsItsShaderDeclares();
        Console.WriteLine("PASS: Material.GetTag returns a tag its shader declares, the default for one it does not, and the material's override over the shader's value");
        SearchingFallbacksFollowsTheDeclaredChain();
        Console.WriteLine("PASS: Material.GetTag with searchFallbacks reads the fallback shader's tags, ends at a shader with no fallback and survives a cycle");
        TheEnginesOwnShadersDeclareNoTags();
        Console.WriteLine("PASS: Material.GetTag gives the default on the sprite and UI shaders the engine defines, with and without searchFallbacks");
        WhatTheEngineCannotKnowThrowsByName();
        Console.WriteLine("PASS: Material.GetTag throws NotSupportedException naming the shader for undeclared tags, an undeclared fallback and a fallback shader the engine does not have");
        ALaterDeclarationCompletesOrDisagrees();
        Console.WriteLine("PASS: a later declaration completes unknown tags and fallback, and one that disagrees is noted and not applied");
        PlanHarness.WithPlan(WritePlan, APlannedShaderCarriesItsTags);
        Console.WriteLine("PASS: a planned shader's tags reach Material.GetTag, through a copied material as Old World's batching reads them");
        PlanHarness.WithPlan(WriteBuiltInPlan, APlanDeclaringABuiltInShaderWins);
        Console.WriteLine("PASS: a plan that declares UI/Default replaces the engine's declaration, with no difference noted, and the engine does not declare over it again");
    }

    private static Shader Declare(string name, string? fallback, params (string Key, string Value)[] tags) =>
        DisunityHost.DeclareShader(name, new ShaderDeclaration(2000, 1, 1, OnePass, tags.Select(tag => KeyValuePair.Create(tag.Key, tag.Value)).ToArray(), fallback), _ => { });

    private static void AMaterialReadsTheTagsItsShaderDeclares()
    {
        var material = new Material(Declare("Test/ShaderTags/Own", "", ("RenderType", "Opaque"), ("MohawkStaticBatching", "True")));
        Expect.Equal("True", material.GetTag("MohawkStaticBatching", false, ""), "a tag the shader declares");
        Expect.Equal("Opaque", material.GetTag("RenderType", false), "the two-argument form reads the shader's tag");
        Expect.Equal("fallback", material.GetTag("MRTBlending", false, "fallback"), "a tag nothing declares gives the default");
        Expect.Equal("", material.GetTag("MRTBlending", false), "the two-argument form's default is empty");
        Expect.Equal("True", material.GetTag("mohawkstaticbatching", false, "fallback"), "a shader tag name matches in any case");
        material.SetOverrideTag("RenderType", "Transparent");
        Expect.Equal("Transparent", material.GetTag("RenderType", false, "fallback"), "the material's override wins over the shader's value");
        material.SetOverrideTag("RenderType", "");
        Expect.Equal("Opaque", material.GetTag("RenderType", false, "fallback"), "removing the override shows the shader's value again");
        material.SetOverrideTag("renderTYPE", "Overlay");
        Expect.Equal("Overlay", material.GetTag("RenderType", false, "fallback"), "an override name matches in any case, as a shader's tag name does");
        material.SetOverrideTag("RENDERTYPE", "");
        Expect.Equal("Opaque", material.GetTag("RenderType", false, "fallback"), "an empty value in another case removes the override");
        material.SetOverrideTag("queue", "Overlay");
        Expect.Equal("Overlay", material.GetTag("QUEUE", false, "fallback"), "an override set in lower case is read in upper case");
        material.SetOverrideTag("QUEUE", "");
        var copy = new Material(material);
        Expect.Equal("True", copy.GetTag("MohawkStaticBatching", false, ""), "a copied material reads the same shader's tag");
    }

    private static void SearchingFallbacksFollowsTheDeclaredChain()
    {
        Declare("Test/ShaderTags/Base", "", ("FromBase", "yes"), ("Shared", "base"));
        Declare("Test/ShaderTags/Middle", "Test/ShaderTags/Base", ("Shared", "middle"));
        var top = new Material(Declare("Test/ShaderTags/Top", "Test/ShaderTags/Middle", ("Own", "top")));
        Expect.Equal("d", top.GetTag("FromBase", false, "d"), "without searchFallbacks a fallback's tag is not seen");
        Expect.Equal("yes", top.GetTag("FromBase", true, "d"), "with searchFallbacks the chain's second fallback is read");
        Expect.Equal("middle", top.GetTag("Shared", true, "d"), "the nearest shader that declares the tag wins");
        Expect.Equal("top", top.GetTag("Own", true, "d"), "a tag of the shader itself is found without leaving it");
        Expect.Equal("d", top.GetTag("Nobody", true, "d"), "a tag nothing in a fully known chain declares gives the default");
        top.SetOverrideTag("FromBase", "set");
        Expect.Equal("set", top.GetTag("FromBase", true, "d"), "the material's override wins over a fallback's value");
        Declare("Test/ShaderTags/Loop1", "Test/ShaderTags/Loop2", ("A", "1"));
        Declare("Test/ShaderTags/Loop2", "Test/ShaderTags/Loop1", ("B", "2"));
        var loop = new Material(Shader.Find("Test/ShaderTags/Loop1")!);
        Expect.Equal("2", loop.GetTag("B", true, "d"), "a fallback cycle still finds a tag on it");
        Expect.Equal("d", loop.GetTag("C", true, "d"), "a fallback cycle ends the search with the default");
    }

    private static void TheEnginesOwnShadersDeclareNoTags()
    {
        // The registered shaders of these names carry whatever an earlier plan declared, so the engine's declarations are applied to fresh shaders.
        var engineDefined = new (string What, ShaderDeclaration Declaration)[]
        {
            ("Sprites/Default", BuiltInShaders.SpriteUnlit), ("the lit sprite shader", BuiltInShaders.SpriteLit), ("UI/Default", BuiltInShaders.CanvasDefault),
        };
        foreach (var (what, declaration) in engineDefined)
        {
            var material = new Material(new Shader($"Test/ShaderTags/Engine/{what}").Declare(declaration));
            Expect.Equal("d", material.GetTag("MohawkStaticBatching", false, "d"), $"{what} declares no tags");
            Expect.Equal("d", material.GetTag("MRTBlending", true, "d"), $"{what} has no fallback to search");
            Expect.Equal("", material.GetTag("RenderType", false), $"{what}: the two-argument form gives the empty default");
        }
        Expect.Equal("d", Material.SpriteUnlit.GetTag("MohawkStaticBatching", false, "d"), "the engine's unlit sprite material reads a tag nobody declares as the default");
    }

    private static void WhatTheEngineCannotKnowThrowsByName()
    {
        var undeclared = new Material(DisunityHost.RegisterShader("Test/ShaderTags/Undeclared"));
        var error = Expect.Catch<NotSupportedException>(() => undeclared.GetTag("RenderType", false, "d"), "GetTag on a shader without declared tags");
        Expect.That(error.Message.Contains("Material.GetTag") && error.Message.Contains("Test/ShaderTags/Undeclared"), $"the exception names the member and the shader: {error.Message}");
        undeclared.SetOverrideTag("RenderType", "Opaque");
        Expect.Equal("Opaque", undeclared.GetTag("RenderType", false, "d"), "an override is read although the shader's tags are unknown");

        var unknownFallback = new Material(Declare("Test/ShaderTags/NoChain", null, ("Own", "x")));
        Expect.Equal("x", unknownFallback.GetTag("Own", true, "d"), "a tag found on the shader needs no fallback");
        Expect.Equal("d", unknownFallback.GetTag("Other", false, "d"), "without searchFallbacks an unknown fallback does not matter");
        error = Expect.Catch<NotSupportedException>(() => unknownFallback.GetTag("Other", true, "d"), "searching an undeclared fallback");
        Expect.That(error.Message.Contains("Material.GetTag") && error.Message.Contains("Test/ShaderTags/NoChain"), $"the exception names the member and the shader: {error.Message}");

        var missing = new Material(Declare("Test/ShaderTags/Missing", "Test/ShaderTags/NotThere", ("Own", "x")));
        error = Expect.Catch<NotSupportedException>(() => missing.GetTag("Other", true, "d"), "searching a fallback shader the engine does not have");
        Expect.That(error.Message.Contains("Test/ShaderTags/NotThere") && error.Message.Contains("Test/ShaderTags/Missing"), $"the exception names both shaders: {error.Message}");

        var chain = new Material(Declare("Test/ShaderTags/ToUndeclared", "Test/ShaderTags/Undeclared"));
        error = Expect.Catch<NotSupportedException>(() => chain.GetTag("Other", true, "d"), "a fallback whose tags are unknown");
        Expect.That(error.Message.Contains("Test/ShaderTags/Undeclared"), $"the exception names the fallback shader: {error.Message}");
        Expect.Throws<ArgumentNullException>(() => chain.GetTag(null!, false, "d"), "GetTag(null)");
    }

    private static void ALaterDeclarationCompletesOrDisagrees()
    {
        var differences = new List<string>();
        var first = DisunityHost.DeclareShader("Test/ShaderTags/Later", new ShaderDeclaration(2000, 1, 1, OnePass), differences.Add);
        var tags = new[] { KeyValuePair.Create("Own", "x") };
        DisunityHost.DeclareShader("Test/ShaderTags/Later", new ShaderDeclaration(2000, 1, 1, OnePass, tags, ""), differences.Add);
        Expect.Equal("x", new Material(first).GetTag("Own", true, "d"), "a later declaration supplies the tags and fallback the first left unknown");
        DisunityHost.DeclareShader("Test/ShaderTags/Later", new ShaderDeclaration(2000, 1, 1, OnePass), differences.Add);
        Expect.Equal("x", new Material(first).GetTag("Own", false, "d"), "a declaration without tags keeps the known ones");
        Expect.Equal(0, differences.Count, "agreeing declarations are not noted");
        DisunityHost.DeclareShader("Test/ShaderTags/Later", new ShaderDeclaration(2000, 1, 1, OnePass, [KeyValuePair.Create("Own", "y")], ""), differences.Add);
        Expect.That(differences.Count == 1 && differences[0].Contains("first stays"), $"a different tag value is noted, got {string.Join("; ", differences)}");
        Expect.Equal("x", new Material(first).GetTag("Own", false, "d"), "the first tags stay");
        DisunityHost.DeclareShader("Test/ShaderTags/Later", new ShaderDeclaration(2000, 1, 1, OnePass, tags, "Other"), differences.Add);
        Expect.Equal(2, differences.Count, "a different fallback is noted");
    }

    private static string WritePlan(string directory)
    {
        var shader = new JsonObject
        {
            ["type"] = "Shader", ["name"] = "Test/ShaderTags/Planned", ["render_queue"] = 2000, ["pass_count"] = 1,
            ["pass_names"] = new JsonArray("Forward"), ["subshader_count"] = 1, ["fallback"] = "",
            ["tags"] = new JsonArray(new JsonArray("RenderType", "Opaque"), new JsonArray("MohawkStaticBatching", "True"), new JsonArray("QUEUE", "Geometry")),
        };
        var entries = new JsonArray(new JsonObject { ["path"] = "shaders/planned", ["class"] = "Shader", ["asset"] = 0 });
        return PlanHarness.WriteJson(directory, entries, new JsonArray(shader));
    }

    private static void APlannedShaderCarriesItsTags()
    {
        var shader = Resources.Load<Shader>("shaders/planned")!;
        var source = new Material(shader);
        var batched = new Material(source);
        Expect.That("True".Equals(batched.GetTag("MohawkStaticBatching", searchFallbacks: false, ""), StringComparison.OrdinalIgnoreCase), "the batching tag reads True");
        Expect.Equal("Opaque", source.GetTag("RenderType", true), "a planned shader's own tag is found with searchFallbacks, which ends at its empty fallback");
        Expect.Equal("Geometry", source.GetTag("Queue", searchFallbacks: false), "a tag the player stores in upper case is read by its usual spelling");
        Expect.Equal("", source.GetTag("MRTBlending", searchFallbacks: false), "an undeclared tag reads empty");
    }

    private static string WriteBuiltInPlan(string directory)
    {
        var shader = new JsonObject
        {
            ["type"] = "Shader", ["name"] = "UI/Default", ["render_queue"] = 3000, ["pass_count"] = 1,
            ["pass_names"] = new JsonArray(""), ["subshader_count"] = 1, ["fallback"] = "",
            ["tags"] = new JsonArray(new JsonArray("PlannedUiTag", "Ui")),
        };
        var entries = new JsonArray(new JsonObject { ["path"] = "shaders/ui", ["class"] = "Shader", ["asset"] = 0 });
        return PlanHarness.WriteJson(directory, entries, new JsonArray(shader));
    }

    private static void APlanDeclaringABuiltInShaderWins()
    {
        var engineCanvas = Canvas.GetDefaultCanvasMaterial();
        Expect.Equal("d", engineCanvas.GetTag("PlannedUiTag", false, "d"), "before the plan, the engine's declaration of UI/Default has no tags");
        var ui = Resources.Load<Shader>("shaders/ui")!;
        Expect.That(ReferenceEquals(ui, engineCanvas.shader), "the planned shader is the built-in one");
        Expect.Equal(0, DisunityHost.Resources!.Differences.Count, $"the plan's declaration of a built-in shader is no difference, got {string.Join("; ", DisunityHost.Resources.Differences)}");
        Expect.Equal("Ui", engineCanvas.GetTag("PlannedUiTag", false, "d"), "the plan's tag on UI/Default is read through the engine's material");
        UnityEngine.Object.DestroyImmediate(engineCanvas);
        Expect.Equal("Ui", Canvas.GetDefaultCanvasMaterial().GetTag("PlannedUiTag", false, "d"), "the engine declaring UI/Default again leaves the plan's tags");
    }
}
