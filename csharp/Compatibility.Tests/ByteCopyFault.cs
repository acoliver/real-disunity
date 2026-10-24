using System.Runtime.InteropServices;
using System.Text.Json;
using UnityEngine;

internal static unsafe class ByteCopyFault
{
    internal static void Run(string path)
    {
        var image = NativeLibrary.Load(path);
        var create = Marshal.GetDelegateForFunctionPointer<WorldCreate>(NativeLibrary.GetExport(image, "disunity_world_create"));
        var destroy = Marshal.GetDelegateForFunctionPointer<WorldDestroy>(NativeLibrary.GetExport(image, "disunity_world_destroy"));
        var arm = (delegate* unmanaged[Cdecl]<int, void>)NativeLibrary.GetExport(image, "byte_fault_arm");
        var counts = (delegate* unmanaged[Cdecl]<int*, int*, int*, void>)NativeLibrary.GetExport(image, "byte_fault_counts");
        BinderBehavior.Check(create(out var world).code == 0, "copy-fault world");
        try
        {
            NativeWorld.AttachNativeLibrary(path, world);
            NativeWorld.Request("transport.select", ("transport_profile", "transport_byte_limits_v1")).Dispose();
            NativeWorld.AttachNativeByteProfile(path, world);
            NativeWorld.Request("time.get").Dispose();
            OversizeRequest(arm, counts);
            for (var mode = 1; mode <= 2; mode++)
            {
                arm(mode);
                var allocated = GC.GetAllocatedBytesForCurrentThread();
                Exception? failure = null;
                try { NativeWorld.Request("time.get").Dispose(); }
                catch (Exception error) { failure = error; }
                allocated = GC.GetAllocatedBytesForCurrentThread() - allocated;
                var copied = 0; var released = 0; var requested = 0;
                counts(&copied, &released, &requested);
                BinderBehavior.Check(failure is not null && failure.ToString().Contains("eight MiB"), "claimed oversize rejected by managed copy bound");
                BinderBehavior.Check(copied == 0 && released == 1 && requested == 1, "no actual copy and exactly one real retained response release");
                BinderBehavior.Check(allocated < 1 << 20, "8MiB+1 claimed response was not allocated");
                if (mode == 2) BinderBehavior.Check(failure is AggregateException && failure.ToString().Contains("release failed"), "primary copy budget plus release failure both preserved");
                arm(0);
            }
        }
        finally { arm(0); NativeWorld.Detach(); BinderBehavior.Check(destroy(world).code == 0, "copy-fault teardown"); }
        Console.WriteLine("PASS injected Cdecl copy shim: claimed8MiB+1 rejected before allocation/copy; actual retained handle freed once; primary+cleanup failures retained");
    }

    private static void OversizeRequest(delegate* unmanaged[Cdecl]<int, void> arm, delegate* unmanaged[Cdecl]<int*, int*, int*, void> counts)
    {
        arm(0);
        using var request = JsonDocument.Parse("{\"version\":1,\"op\":\"object.create\",\"name\":\"" + new string('x', 1 << 20) + "\"}");
        var copied = 0; var released = 0; var requested = 0;
        Exception? failure = null;
        try { NativeWorld.Current.Request(request).Dispose(); }
        catch (Exception error) { failure = error; }
        counts(&copied, &released, &requested);
        BinderBehavior.Check(failure is InvalidDataException && requested == 0, "oversized serialized request rejects before native dispatch");
    }
}
