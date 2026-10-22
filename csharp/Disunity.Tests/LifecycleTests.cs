using System.Runtime.InteropServices;
using Disunity;
using PortableFixture;

internal static class LifecycleTests
{
    [StructLayout(LayoutKind.Sequential)]
    private struct Result
    {
        public int Code;
        public ulong Detail;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct ObjectState
    {
        public ulong Object;
        public float X, Y, Z, Rotation;
        public uint Material;
    }

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate Result Export(ulong world, nint states, uint capacity, out uint count);

    private sealed class ProbeBehaviour(List<string> calls) : MonoBehaviour
    {
        public Action? OnStart { get; init; }
        public Action? OnUpdate { get; init; }
        public Action? OnLateUpdate { get; init; }
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

        protected override void LateUpdate()
        {
            calls.Add("LateUpdate");
            OnLateUpdate?.Invoke();
        }

        protected override void OnDestroy()
        {
            calls.Add("OnDestroy");
            OnDestruction?.Invoke();
        }
    }

    private static void RequireCalls(List<string> calls, params string[] expected)
    {
        if (!calls.SequenceEqual(expected))
        {
            throw new InvalidOperationException(
                $"expected lifecycle [{string.Join(", ", expected)}], got [{string.Join(", ", calls)}]");
        }
    }

    private static void ReloadWorld()
    {
        WorldRunner.UnloadWorld();
        WorldRunner.LoadWorld();
    }

    private static void LateAttachment(Export export, nint buffer)
    {
        ReloadWorld();
        WorldRunner.Step(0.125f);
        var calls = new List<string>();
        ProbeBehaviour? behaviour = null;
        behaviour = WorldRunner.AddComponent(new ProbeBehaviour(calls)
        {
            OnStart = () => behaviour!.gameObject.Transform.position = new Vector3(4.0f, 1.25f, -6.0f),
            OnUpdate = () => behaviour!.gameObject.Transform.rotationY = 0.5f,
            OnLateUpdate = () => behaviour!.gameObject.Transform.materialSlot = 2,
        });
        RequireCalls(calls, "Awake");
        for (var tick = 0; tick < 2; tick++)
        {
            WorldRunner.Step(0.125f);
            var result = export(WorldRunner.WorldHandle, buffer, 1, out var count);
            var state = Marshal.PtrToStructure<ObjectState>(buffer);
            if (result.Code != 0 || count != 1 || state.X != 4.0f || state.Y != 1.25f
                || state.Z != -6.0f || state.Rotation != 0.5f || state.Material != 2)
            {
                throw new InvalidOperationException("late lifecycle commands must apply in the same step");
            }
            if (tick == 0)
            {
                RequireCalls(calls, "Awake", "Start", "Update", "LateUpdate");
            }
        }
        RequireCalls(calls, "Awake", "Start", "Update", "LateUpdate", "Update", "LateUpdate");
        Console.WriteLine("PASS: late attachment starts once before update phases and applies commands in the same step");
    }

    private static void CallbackAttachment()
    {
        foreach (var phase in new[] { "Start", "Update", "LateUpdate" })
        {
            ReloadWorld();
            var parentCalls = new List<string>();
            var childCalls = new List<string>();
            var added = false;
            void AddChild()
            {
                if (!added)
                {
                    added = true;
                    WorldRunner.AddComponent(new ProbeBehaviour(childCalls));
                }
            }
            WorldRunner.AddComponent(new ProbeBehaviour(parentCalls)
            {
                OnStart = phase == "Start" ? AddChild : null,
                OnUpdate = phase == "Update" ? AddChild : null,
                OnLateUpdate = phase == "LateUpdate" ? AddChild : null,
            });
            WorldRunner.Step(0.125f);
            RequireCalls(parentCalls, "Awake", "Start", "Update", "LateUpdate");
            RequireCalls(childCalls, "Awake");
            WorldRunner.Step(0.125f);
            RequireCalls(childCalls, "Awake", "Start", "Update", "LateUpdate");
            WorldRunner.Step(0.125f);
            RequireCalls(childCalls, "Awake", "Start", "Update", "LateUpdate", "Update", "LateUpdate");
        }
        Console.WriteLine("PASS: attachment during every step callback waits until the next step");
    }

    private static void RemovalBeforeStart()
    {
        ReloadWorld();
        var calls = new List<string>();
        var behaviour = WorldRunner.AddComponent(new ProbeBehaviour(calls));
        behaviour.gameObject.Destroy();
        WorldRunner.Step(0.125f);
        WorldRunner.Step(0.125f);
        WorldRunner.UnloadWorld();
        RequireCalls(calls, "Awake", "OnDestroy");

        WorldRunner.LoadWorld();
        var removedCalls = new List<string>();
        ProbeBehaviour? removed = null;
        WorldRunner.AddComponent(new ProbeBehaviour(new List<string>())
        {
            OnStart = () => removed!.gameObject.Destroy(),
        });
        removed = WorldRunner.AddComponent(new ProbeBehaviour(removedCalls));
        WorldRunner.Step(0.125f);
        WorldRunner.Step(0.125f);
        WorldRunner.UnloadWorld();
        RequireCalls(removedCalls, "Awake", "OnDestroy");

        WorldRunner.LoadWorld();
        var destroyCalls = new List<string>();
        ProbeBehaviour? selfDestroying = null;
        selfDestroying = WorldRunner.AddComponent(new ProbeBehaviour(destroyCalls)
        {
            OnDestruction = () => selfDestroying!.gameObject.Destroy(),
        });
        WorldRunner.UnloadWorld();
        RequireCalls(destroyCalls, "Awake", "OnDestroy");
        Console.WriteLine("PASS: removal before startup never runs tick callbacks; destruction notifications run once");
    }

    public static void Run(string nativePath)
    {
        WorldRunner.ConfigureNativeLibrary(nativePath);
        var image = System.Runtime.InteropServices.NativeLibrary.Load(nativePath);
        var buffer = Marshal.AllocHGlobal(32);
        try
        {
            var export = Marshal.GetDelegateForFunctionPointer<Export>(
                System.Runtime.InteropServices.NativeLibrary.GetExport(image, "disunity_world_export_objects"));
            WorldRunner.LoadWorld();
            var behaviour = WorldRunner.AddComponent(new TriangleBehaviour(2));
            if (behaviour.GetType().Assembly == typeof(MonoBehaviour).Assembly)
            {
                throw new InvalidOperationException("fixture must derive from a separate assembly");
            }
            for (var tick = 1; tick <= 2; tick++)
            {
                WorldRunner.Step(0.125f);
                var result = export(WorldRunner.WorldHandle, buffer, 1, out var count);
                var state = Marshal.PtrToStructure<ObjectState>(buffer);
                if (result.Code != 0 || count != 1 || state.X != 4.0f || state.Y != 1.25f
                    || state.Z != -6.0f || state.Rotation != 0.5f || state.Material != 2)
                {
                    throw new InvalidOperationException("all callback commands must reach native state in the same step");
                }
                if (behaviour.Starts != 1 || behaviour.Updates != tick || behaviour.LateUpdates != tick)
                {
                    throw new InvalidOperationException("Start must run once and update phases once per step");
                }
            }
            LateAttachment(export, buffer);
            CallbackAttachment();
            RemovalBeforeStart();
            LifecycleDestructionTests.Run(image);
        }
        finally
        {
            WorldRunner.UnloadWorld();
            Marshal.FreeHGlobal(buffer);
            System.Runtime.InteropServices.NativeLibrary.Free(image);
        }
        Console.WriteLine("PASS: external behaviour commands apply in the same step; Start runs once");
    }
}
