using System.Runtime.InteropServices;
using UnityEngine;
using Disunity.Hosting;

internal static class CommandDestructionTests
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

    [StructLayout(LayoutKind.Sequential)]
    private struct Command
    {
        public ulong Object;
        public uint Kind;
        public float X, Y, Z;
    }

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate Result Export(ulong world, nint states, uint capacity, out uint count);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate Result Submit(ulong world, ref Command command, uint count);

    private sealed class Probe : MonoBehaviour
    {
        public Action? OnStart { get; init; }
        public Action? OnUpdate { get; init; }
        public Action? OnLateUpdate { get; init; }
        public Action? OnDestruction { get; init; }

        private void Start() => OnStart?.Invoke();
        private void Update() => OnUpdate?.Invoke();
        private void LateUpdate() => OnLateUpdate?.Invoke();
        private void OnDestroy() => OnDestruction?.Invoke();
    }

    private static void Destruction(Export export, nint buffer, string phase, bool self)
    {
        DisunityHost.LoadWorld();
        Probe? survivor = null;
        survivor = DisunityHost.AddComponent(new Probe
        {
            OnStart = () =>
            {
                survivor!.gameObject.Transform.position = new Vector3(1, 2, 3);
                survivor.gameObject.Transform.rotationY = 0.25f;
            },
            OnLateUpdate = () => survivor!.gameObject.Transform.materialSlot = 4,
        });
        Probe? victim = null;
        void WriteVictim()
        {
            victim!.gameObject.Transform.position = new Vector3(9, 8, 7);
            victim.gameObject.Transform.rotationY = 1.5f;
            victim.gameObject.Transform.materialSlot = 2;
            if (self)
            {
                victim.gameObject.Destroy();
            }
        }
        victim = DisunityHost.AddComponent(new Probe
        {
            OnStart = phase == "Start" ? WriteVictim : null,
            OnUpdate = phase == "Update" ? WriteVictim : null,
            OnDestruction = () => survivor!.gameObject.Transform.position = new Vector3(5, 6, 7),
        });
        if (!self)
        {
            DisunityHost.AddComponent(new Probe
            {
                OnStart = phase == "Start" ? () => victim.gameObject.Destroy() : null,
                OnUpdate = phase == "Update" ? () => victim.gameObject.Destroy() : null,
            });
        }
        DisunityHost.Step(0.125f);
        var result = export(DisunityHost.WorldHandle, buffer, 2, out var count);
        var state = Marshal.PtrToStructure<ObjectState>(buffer);
        if (result.Code != 0 || count != (self ? 1u : 2u) || state.X != 5 || state.Y != 6
            || state.Z != 7 || state.Rotation != 0.25f || state.Material != 4)
        {
            throw new InvalidOperationException(
                $"{phase} destruction must preserve ordered survivor commands in the same step");
        }
        DisunityHost.UnloadWorld();
        Console.WriteLine($"PASS: {(self ? "self" : "earlier writer")} destruction after {phase} writes preserves survivor commands");
    }

    private static void InvalidNativeCommand(Export export, Submit submit, nint buffer, bool stale)
    {
        DisunityHost.LoadWorld();
        var handle = ulong.MaxValue;
        if (stale)
        {
            var victim = DisunityHost.CreateGameObject();
            var exported = export(DisunityHost.WorldHandle, buffer, 1, out var count);
            if (exported.Code != 0 || count != 1)
            {
                throw new InvalidOperationException("expected one live object before destruction");
            }
            handle = Marshal.PtrToStructure<ObjectState>(buffer).Object;
            victim.Destroy();
        }
        var command = new Command { Object = handle, Kind = 0, X = 1, Y = 2, Z = 3 };
        var result = submit(DisunityHost.WorldHandle, ref command, 1);
        if (result.Code != 0)
        {
            throw new InvalidOperationException("native command submission must accept a well-formed batch");
        }
        try
        {
            DisunityHost.Step(0.125f);
        }
        catch (DisunityException exception) when (exception.Message.Contains("object handle", StringComparison.Ordinal))
        {
            DisunityHost.UnloadWorld();
            Console.WriteLine($"PASS: native step rejects a {(stale ? "destroyed" : "never-valid")} object handle");
            return;
        }
        throw new InvalidOperationException("native step must reject an invalid object handle");
    }

    public static void Run(nint image)
    {
        DisunityHost.UnloadWorld();
        var export = Marshal.GetDelegateForFunctionPointer<Export>(
            System.Runtime.InteropServices.NativeLibrary.GetExport(image, "disunity_world_export_objects"));
        var submit = Marshal.GetDelegateForFunctionPointer<Submit>(
            System.Runtime.InteropServices.NativeLibrary.GetExport(image, "disunity_command_buffer_submit"));
        var buffer = Marshal.AllocHGlobal(64);
        try
        {
            foreach (var phase in new[] { "Start", "Update" })
            {
                Destruction(export, buffer, phase, self: true);
                Destruction(export, buffer, phase, self: false);
            }
            InvalidNativeCommand(export, submit, buffer, stale: false);
            InvalidNativeCommand(export, submit, buffer, stale: true);
        }
        finally
        {
            DisunityHost.UnloadWorld();
            Marshal.FreeHGlobal(buffer);
        }
    }
}
