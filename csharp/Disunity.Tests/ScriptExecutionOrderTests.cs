using Disunity.Hosting;
using UnityEngine;

/// <summary>Scripts receive Start and the per-frame messages in execution order, lowest first, and in the order
/// they were attached among equal orders. The order comes from a script's <c>DefaultExecutionOrder</c> attribute
/// or from the order a scene plan carries for its class, as a built player stores a script's order with the script.
/// The fog case mirrors a game that clears a global in a script ordered before the script that sets it.</summary>
internal static class ScriptExecutionOrderTests
{
    private static readonly List<string> Log = new();
    private static readonly int Fog = Shader.PropertyToID("_ExecutionOrderFog");

    private abstract class Recording : MonoBehaviour
    {
        protected abstract string Label { get; }
        private void Start() => Log.Add($"{Label}.Start");
        private void Update() => Log.Add($"{Label}.Update");
        private void LateUpdate() => Log.Add($"{Label}.LateUpdate");
    }

    private sealed class Plain : Recording
    {
        protected override string Label => "plain";
    }

    private sealed class PlainToo : Recording
    {
        protected override string Label => "plain2";
    }

    [DefaultExecutionOrder(-200)]
    private sealed class Early : Recording
    {
        protected override string Label => "early";
    }

    [DefaultExecutionOrder(50)]
    private sealed class Late : Recording
    {
        protected override string Label => "late";
    }

    private sealed class SetsFog : MonoBehaviour
    {
        private void Update() => Shader.SetGlobalVector(Fog, new Vector4(1, 0, 200, 0.5f));
    }

    [DefaultExecutionOrder(-200)]
    private sealed class ClearsFog : MonoBehaviour
    {
        private void Update() => Shader.SetGlobalVector(Fog, Vector4.zero);
    }

    private sealed class PlanSetsFog : MonoBehaviour
    {
        private void Update() => Shader.SetGlobalVector(Fog, new Vector4(1, 0, 200, 0.5f));
    }

    private sealed class PlanClearsFog : MonoBehaviour
    {
        private void Update() => Shader.SetGlobalVector(Fog, Vector4.zero);
    }

    [DefaultExecutionOrder(-200)]
    private sealed class PlanZeroClearsFog : MonoBehaviour
    {
        private void Update() => Shader.SetGlobalVector(Fog, Vector4.zero);
    }

    public static void Run()
    {
        AttributeOrderDecidesTheSequence();
        EqualOrdersKeepAttachOrder();
        ALaterClearingScriptDoesNotOverrideAnEarlierSetter();
        APlanCarriesTheOrderOfItsClass();
        AStoredOrderOfZeroReplacesTheAttribute();
        Console.WriteLine("PASS: scripts run in execution order, from the attribute or from the plan");
    }

    private static void AttributeOrderDecidesTheSequence()
    {
        Log.Clear();
        DisunityHost.LoadWorld();
        try
        {
            DisunityHost.AddComponent(new Late());
            DisunityHost.AddComponent(new Plain());
            DisunityHost.AddComponent(new Early());
            DisunityHost.Step(0.05f);
            Expect.Sequence(new[] { "early.Start", "plain.Start", "late.Start" }, Log.Where(l => l.EndsWith(".Start")), "Start follows execution order");
            Expect.Sequence(new[] { "early.Update", "plain.Update", "late.Update" }, Log.Where(l => l.EndsWith(".Update")), "Update follows execution order");
            Expect.Sequence(new[] { "early.LateUpdate", "plain.LateUpdate", "late.LateUpdate" }, Log.Where(l => l.EndsWith(".LateUpdate")), "LateUpdate follows execution order");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void EqualOrdersKeepAttachOrder()
    {
        Log.Clear();
        DisunityHost.LoadWorld();
        try
        {
            DisunityHost.AddComponent(new PlainToo());
            DisunityHost.AddComponent(new Plain());
            DisunityHost.Step(0.05f);
            DisunityHost.Step(0.05f);
            Expect.Sequence(new[] { "plain2.Update", "plain.Update", "plain2.Update", "plain.Update" }, Log.Where(l => l.EndsWith(".Update")), "equal orders run in attach order");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void ALaterClearingScriptDoesNotOverrideAnEarlierSetter()
    {
        DisunityHost.LoadWorld();
        try
        {
            DisunityHost.AddComponent(new SetsFog());
            DisunityHost.AddComponent(new ClearsFog());
            DisunityHost.Step(0.05f);
            Expect.Equal(new Vector4(1, 0, 200, 0.5f), Shader.GetGlobalVector(Fog), "the setter runs after the clearing script, so its value stays");
            DisunityHost.Step(0.05f);
            Expect.Equal(new Vector4(1, 0, 200, 0.5f), Shader.GetGlobalVector(Fog), "and stays on the next frame");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private const string PlanText = """
        {"scene":"order","assets":[],"sprites":[],"textures":[],"fonts":[],"axes":[],"differences":[],"objects":[
        {"name":"Setter","tag":"Untagged","layer":0,"active":true,"parent":null,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[
        {"type":"Script","class":"ScriptExecutionOrderTests/PlanSetsFog","assembly":"ASSEMBLY","enabled":true,"fields":{}}]},
        {"name":"Clearer","tag":"Untagged","layer":0,"active":true,"parent":null,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[
        {"type":"Script","class":"ScriptExecutionOrderTests/PlanClearsFog","assembly":"ASSEMBLY","execution_order":-200,"enabled":true,"fields":{}}]}]}
        """;

    private static void APlanCarriesTheOrderOfItsClass()
    {
        var assembly = typeof(ScriptExecutionOrderTests).Assembly;
        DisunityHost.LoadWorld();
        try
        {
            DisunityHost.LoadScene(PlanText.Replace("ASSEMBLY", assembly.GetName().Name),
                new SceneLoadOptions(assembly, _ => []) { Assemblies = _ => assembly });
            DisunityHost.Step(0.05f);
            Expect.Equal(new Vector4(1, 0, 200, 0.5f), Shader.GetGlobalVector(Fog), "the plan's order puts the clearing script first");
            Shader.SetGlobalVector(Fog, Vector4.one);
            DisunityHost.AddComponent(new PlanClearsFog());
            DisunityHost.Step(0.05f);
            Expect.Equal(new Vector4(1, 0, 200, 0.5f), Shader.GetGlobalVector(Fog), "a script of the same class added later takes the class's order");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private const string ZeroPlanText = """
        {"scene":"zero","assets":[],"sprites":[],"textures":[],"fonts":[],"axes":[],"differences":[],"objects":[
        {"name":"Setter","tag":"Untagged","layer":0,"active":true,"parent":null,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[
        {"type":"Script","class":"ScriptExecutionOrderTests/PlanSetsFog","assembly":"ASSEMBLY","execution_order":0,"enabled":true,"fields":{}}]},
        {"name":"Clearer","tag":"Untagged","layer":0,"active":true,"parent":null,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[
        {"type":"Script","class":"ScriptExecutionOrderTests/PlanZeroClearsFog","assembly":"ASSEMBLY","execution_order":0,"enabled":true,"fields":{}}]}]}
        """;

    /// <summary>The clearing class carries <c>DefaultExecutionOrder(-200)</c> in its assembly, but the player stored 0
    /// for it, so it runs after the setter and clears the fog.</summary>
    private static void AStoredOrderOfZeroReplacesTheAttribute()
    {
        var assembly = typeof(ScriptExecutionOrderTests).Assembly;
        DisunityHost.LoadWorld();
        try
        {
            DisunityHost.LoadScene(ZeroPlanText.Replace("ASSEMBLY", assembly.GetName().Name),
                new SceneLoadOptions(assembly, _ => []) { Assemblies = _ => assembly });
            Shader.SetGlobalVector(Fog, Vector4.one);
            DisunityHost.Step(0.05f);
            Expect.Equal(Vector4.zero, Shader.GetGlobalVector(Fog), "the stored order 0 puts the clearing script after the setter");
        }
        finally { DisunityHost.UnloadWorld(); }
    }
}
