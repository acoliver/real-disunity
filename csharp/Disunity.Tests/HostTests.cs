using Disunity.Hosting;
using UnityEngine;

internal static class HostTests
{
    private sealed class Probe : MonoBehaviour
    {
        public Action? OnAwake { get; init; }
        public Action? OnUpdate { get; init; }
        public Action? OnDestruction { get; init; }
        private void Awake() => OnAwake?.Invoke();
        private void Update() => OnUpdate?.Invoke();
        private void OnDestroy() => OnDestruction?.Invoke();
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        var calls = new List<string>();
        DisunityHost.AddComponent(new Probe
        {
            OnUpdate = () =>
            {
                calls.Add("parent");
                DisunityHost.AddComponent(new Probe
                {
                    OnAwake = () =>
                    {
                        calls.Add("child");
                        DisunityHost.AddComponent(new Probe { OnAwake = () => calls.Add("grandchild") });
                        calls.Add("child-return");
                    },
                });
                calls.Add("parent-return");
            },
        });
        DisunityHost.Step(0.01f);
        if (!calls.SequenceEqual(new[] { "parent", "child", "grandchild", "child-return", "parent-return" }))
            throw new InvalidOperationException("nested native lifecycle must finish before control returns");
        DisunityHost.UnloadWorld();
        DisunityHost.UnloadWorld();

        DisunityHost.LoadWorld();
        var cleaned = false;
        DisunityHost.AddComponent(new Probe
        {
            OnUpdate = () => throw EngineFault.Make("managed failure λ"),
            OnDestruction = () => cleaned = true,
        });
        try { DisunityHost.Step(0.01f); throw new Exception("callback failure was lost"); }
        catch (DisunityException error) when (error.Message.Contains("managed failure λ")) { }
        if (!cleaned) throw new Exception("native failure teardown omitted managed cleanup");
        DisunityHost.UnloadWorld();
        DisunityHost.LoadWorld();
        try { DisunityHost.Step(float.NaN); throw new Exception("invalid native delta was accepted"); }
        catch (DisunityException) { }
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: nested native lifecycle, callback reentry, exception translation, repeated unload");
    }
}
