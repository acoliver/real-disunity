using System.Runtime.InteropServices;
using Disunity;

internal static class LifecycleDestructionTests
{
    [StructLayout(LayoutKind.Sequential)]
    private struct Result
    {
        public int Code;
        public ulong Detail;
    }

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate Result Statistics(out ulong liveWorlds, out ulong allocatedSlots);

    private class Probe(List<string> calls) : MonoBehaviour
    {
        public Action? OnStart { get; init; }
        public Action? OnUpdate { get; init; }
        public Action? OnDestruction { get; init; }

        protected override void Awake() => calls.Add("Awake");

        protected override void Start()
        {
            calls.Add("Start");
            OnStart?.Invoke();
        }

        protected override void Update()
        {
            calls.Add("Update");
            OnUpdate?.Invoke();
        }

        protected override void LateUpdate() => calls.Add("LateUpdate");

        protected override void OnDestroy()
        {
            calls.Add("OnDestroy");
            OnDestruction?.Invoke();
        }
    }

    private sealed class EqualProbe(List<string> calls) : Probe(calls)
    {
        public override bool Equals(object? other) => other is EqualProbe;
        public override int GetHashCode() => 0;
    }

    private static void RequireCalls(List<string> calls, params string[] expected)
    {
        if (!calls.SequenceEqual(expected))
        {
            throw new InvalidOperationException(
                $"expected lifecycle [{string.Join(", ", expected)}], got [{string.Join(", ", calls)}]");
        }
    }

    private static void EqualInstances()
    {
        WorldRunner.LoadWorld();
        var firstCalls = new List<string>();
        var secondCalls = new List<string>();
        var first = WorldRunner.AddComponent(new EqualProbe(firstCalls));
        var second = WorldRunner.AddComponent(new EqualProbe(secondCalls));
        if (ReferenceEquals(first, second) || !first.Equals(second))
        {
            throw new InvalidOperationException("regression requires distinct behaviours with equal values");
        }
        WorldRunner.Step(0.125f);
        second.gameObject.Destroy();
        WorldRunner.Step(0.125f);
        RequireCalls(firstCalls, "Awake", "Start", "Update", "LateUpdate", "Update", "LateUpdate");
        RequireCalls(secondCalls, "Awake", "Start", "Update", "LateUpdate", "OnDestroy");
        WorldRunner.UnloadWorld();
        RequireCalls(firstCalls, "Awake", "Start", "Update", "LateUpdate", "Update", "LateUpdate", "OnDestroy");
        RequireCalls(secondCalls, "Awake", "Start", "Update", "LateUpdate", "OnDestroy");
        Console.WriteLine("PASS: equal-valued behaviours unregister by instance and receive destruction once");
    }

    private static void UnloadAdditions(nint image)
    {
        WorldRunner.LoadWorld();
        var parentCalls = new List<string>();
        var victimCalls = new List<string>();
        var childCalls = new List<string>();
        var grandchildCalls = new List<string>();
        Probe? victim = null;
        Probe? child = null;
        Probe? grandchild = null;
        WorldRunner.AddComponent(new Probe(parentCalls)
        {
            OnDestruction = () =>
            {
                victim!.gameObject.Destroy();
                child = WorldRunner.AddComponent(new Probe(childCalls)
                {
                    OnDestruction = () => grandchild = WorldRunner.AddComponent(new Probe(grandchildCalls)),
                });
            },
        });
        victim = WorldRunner.AddComponent(new Probe(victimCalls));
        WorldRunner.UnloadWorld();
        RequireCalls(parentCalls, "Awake", "OnDestroy");
        RequireCalls(victimCalls, "Awake", "OnDestroy");
        RequireCalls(childCalls, "Awake", "OnDestroy");
        RequireCalls(grandchildCalls, "Awake", "OnDestroy");
        foreach (var behaviour in new[] { child!, grandchild! })
        {
            try
            {
                _ = behaviour.gameObject.Transform.position;
            }
            catch (ObjectDisposedException)
            {
                continue;
            }
            throw new InvalidOperationException("unload must invalidate callback-created objects");
        }
        var statistics = Marshal.GetDelegateForFunctionPointer<Statistics>(
            System.Runtime.InteropServices.NativeLibrary.GetExport(image, "disunity_store_statistics"));
        var result = statistics(out var liveWorlds, out _);
        if (result.Code != 0 || liveWorlds != 0 || WorldRunner.IsLoaded || WorldRunner.WorldHandle != 0)
        {
            throw new InvalidOperationException("unload must release the native world and its objects");
        }
        Console.WriteLine("PASS: unload drains callback additions and cross-destruction without duplicate notifications");
    }

    private static void SelfDestruction()
    {
        foreach (var phase in new[] { "Start", "Update" })
        {
            WorldRunner.LoadWorld();
            var calls = new List<string>();
            Probe? behaviour = null;
            void DestroySelf() => behaviour!.gameObject.Destroy();
            behaviour = WorldRunner.AddComponent(new Probe(calls)
            {
                OnStart = phase == "Start" ? DestroySelf : null,
                OnUpdate = phase == "Update" ? DestroySelf : null,
            });
            WorldRunner.Step(0.125f);
            WorldRunner.Step(0.125f);
            WorldRunner.UnloadWorld();
            if (phase == "Start")
            {
                RequireCalls(calls, "Awake", "Start", "OnDestroy");
            }
            else
            {
                RequireCalls(calls, "Awake", "Start", "Update", "OnDestroy");
            }
        }
        Console.WriteLine("PASS: self-destruction during Start or Update skips remaining tick callbacks");
    }

    public static void Run(nint image)
    {
        WorldRunner.UnloadWorld();
        EqualInstances();
        UnloadAdditions(image);
        SelfDestruction();
    }
}
