using Disunity.Hosting;
using UnityEngine;

internal static class AssetSleepTests
{
    private sealed class Asset : ScriptableObject
    {
        public static readonly List<string> Calls = new();
        private void Awake() => Calls.Add("awake");
        private void OnEnable() => Calls.Add("enable");
        private void OnDisable() => Calls.Add("disable");
        private void OnDestroy() => Calls.Add("destroy");
        private void Update() => throw new Exception("assets do not receive frame messages");
    }
    public static void Run()
    {
        DisunityHost.LoadWorld();
        Asset.Calls.Clear();
        var before = UnityEngine.Object.FindObjectsByType<GameObject>(FindObjectsSortMode.None).Length;
        var asset = ScriptableObject.CreateInstance<Asset>();
        Require(Asset.Calls.SequenceEqual(new[] { "awake", "enable" }), "asset activation messages");
        Require(UnityEngine.Object.FindObjectsByType<GameObject>(FindObjectsSortMode.None).Length == before, "asset has no GameObject");
        UnityEngine.Object.Destroy(asset);
        UnityEngine.Object.Destroy(asset);
        DisunityHost.Step(0);
        Require(Asset.Calls.SequenceEqual(new[] { "awake", "enable", "disable", "destroy" }), "asset teardown exactly once");
        Require(!asset && asset == null, "destroyed asset null semantics");
        ScriptableObject.CreateInstance(typeof(Asset));
        DisunityHost.UnloadWorld();
        Require(Asset.Calls.Count == 8, "unload tears down live assets");
        TestSleep();
        Console.WriteLine("PASS: native standalone asset lifetime and body sleep/wake");
    }
    private static void TestSleep()
    {
        DisunityHost.LoadWorld();
        var owner = DisunityHost.CreateGameObject();
        var body = owner.AddComponent<Rigidbody2D>();
        body.gravityScale = 1;
        body.velocity = new(3, 0);
        body.angularVelocity = 90;
        body.Sleep();
        Require(body.IsSleeping(), "explicit native sleep");
        DisunityHost.Step(0.041f);
        Require(body.position == Vector2.zero && owner.transform.rotation == Quaternion.identity, "sleep skips gravity and motion");
        body.WakeUp();
        Require(!body.IsSleeping(), "explicit native wake");
        DisunityHost.Step(0.02f);
        Require(body.position.y < 0, "awake body integrates gravity");
        DisunityHost.UnloadWorld();
    }
    private static void Require(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }
}
