using Disunity.Hosting;
using UnityEngine;

/// <summary>Per-frame messages reach only scripts whose type defines them. A lookup for a message
/// happens only when the native world delivers it, so a missing lookup shows it was never delivered.</summary>
internal static class ScriptMessageSetTests
{
    private sealed class AllThree : MonoBehaviour
    {
        public int Fixed, Updates, Late;
        private void FixedUpdate() => Fixed++;
        private void Update() => Updates++;
        private void LateUpdate() => Late++;
    }
    private sealed class WithoutFixed : MonoBehaviour
    {
        public int Updates, Late;
        private void Update() => Updates++;
        public void LateUpdate() => Late++;
    }
    private class UpdatingBase : MonoBehaviour
    {
        public int Updates;
        protected void Update() => Updates++;
    }
    private sealed class InheritsUpdate : UpdatingBase { }
    private sealed class OnlyPrivateLate : MonoBehaviour
    {
        public int Late;
        private void LateUpdate() => Late++;
    }
    private sealed class OnlyStart : MonoBehaviour
    {
        public int Starts;
        private void Start() => Starts++;
    }
    private sealed class WrongSignature : MonoBehaviour
    {
        public int Started;
        private void Start() => Started++;
        private void Update(int unused) { }
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        var all = DisunityHost.AddComponent(new AllThree());
        var without = DisunityHost.AddComponent(new WithoutFixed());
        var inherits = DisunityHost.AddComponent(new InheritsUpdate());
        var late = DisunityHost.AddComponent(new OnlyPrivateLate());
        var start = DisunityHost.AddComponent(new OnlyStart());
        var wrong = DisunityHost.AddComponent(new WrongSignature());
        var copy = UnityEngine.Object.Instantiate(without);
        for (var frame = 0; frame < 3; frame++) DisunityHost.Step(0.05f);
        Require(all.Fixed > 0 && all.Updates == 3 && all.Late == 3, "a script with all three receives all three");
        Require(without.Updates == 3 && without.Late == 3 && copy.Updates == 3, "a script without FixedUpdate still receives Update and LateUpdate");
        Require(!ScriptMessages.HasLooked(typeof(WithoutFixed), "FixedUpdate"), "FixedUpdate is not delivered to a type without it");
        Require(ScriptMessages.HasLooked(typeof(AllThree), "FixedUpdate"), "FixedUpdate is delivered to a type with it");
        Require(inherits.Updates == 3, "an inherited Update counts");
        Require(!ScriptMessages.HasLooked(typeof(InheritsUpdate), "LateUpdate"), "an inheriting type without LateUpdate gets none");
        Require(late.Late == 3 && !ScriptMessages.HasLooked(typeof(OnlyPrivateLate), "Update"), "a private LateUpdate counts and nothing else is delivered");
        Require(start.Starts == 1, "Start runs once for a script with no per-frame messages");
        Require(wrong.Started == 1 && !ScriptMessages.HasLooked(typeof(WrongSignature), "Update"), "an Update that takes parameters is not a per-frame message");
        Require(!ScriptMessages.HasLooked(typeof(OnlyStart), "FixedUpdate") && !ScriptMessages.HasLooked(typeof(OnlyStart), "Update")
            && !ScriptMessages.HasLooked(typeof(OnlyStart), "LateUpdate"), "no per-frame message reaches a type that defines none");
        Require(ScriptMessages.PerFrameMessages(typeof(AllThree)) == 7 && ScriptMessages.PerFrameMessages(typeof(WithoutFixed)) == 6
            && ScriptMessages.PerFrameMessages(typeof(OnlyStart)) == 0 && ScriptMessages.PerFrameMessages(typeof(InheritsUpdate)) == 2
            && ScriptMessages.PerFrameMessages(typeof(OnlyPrivateLate)) == 4, "the defined sets");
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: per-frame messages reach only the scripts whose type defines them");
    }

    private static void Require(bool condition, string what)
    {
        if (!condition) throw new Exception(what);
    }
}
