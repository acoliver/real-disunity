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
