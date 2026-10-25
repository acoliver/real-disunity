using System.Text.Json;
using UnityEngine;

internal static class ByteCopyFault
{
    internal static void Run(string path)
    {
        using var world = new ReadinessWorld(path, openSession: false);
        var fault = new FaultTransport(((NativeJsonWorld)NativeWorld.Current).Transport);
        NativeWorld.Attach(new NativeJsonWorld(world.World, transport: fault));
        OversizeRequest(fault);
        for (var mode = 1; mode <= 2; mode++) VerifyCopyFault(fault, mode);
        Console.WriteLine("PASS injected copy transport: oversize rejected before allocation/copy; real retained handle freed once; primary+cleanup failures retained");
    }

    private static void VerifyCopyFault(FaultTransport fault, int mode)
    {
        fault.Reset(mode);
        var allocated = GC.GetAllocatedBytesForCurrentThread();
        Exception? failure = null;
        try { NativeWorld.Request("time.get").Dispose(); }
        catch (Exception error) { failure = error; }
        allocated = GC.GetAllocatedBytesForCurrentThread() - allocated;
        BinderBehavior.Check(failure is not null && failure.ToString().Contains("eight MiB"), "claimed oversize rejected by managed copy bound");
        BinderBehavior.Check(fault.Copied == 0 && fault.Released == 1 && fault.Requested == 1, "no actual copy and exactly one real retained response release");
        BinderBehavior.Check(allocated < 1 << 20, "8MiB+1 claimed response was not allocated");
        if (mode == 2) BinderBehavior.Check(failure is AggregateException && failure.ToString().Contains("release failed"), "primary copy budget plus release failure both preserved");
        fault.Reset(0);
    }

    private static void OversizeRequest(FaultTransport fault)
    {
        fault.Reset(0);
        using var request = JsonDocument.Parse("{\"version\":1,\"op\":\"object.create\",\"name\":\"" + new string('x', 1 << 20) + "\"}");
        Exception? failure = null;
        try { NativeWorld.Current.Request(request).Dispose(); }
        catch (Exception error) { failure = error; }
        BinderBehavior.Check(failure is InvalidDataException && fault.Requested == 0, "oversized serialized request rejects before native dispatch");
    }

    private sealed unsafe class FaultTransport(INativeTransport native) : INativeTransport
    {
        private int _mode;
        internal int Copied { get; private set; }
        internal int Released { get; private set; }
        internal int Requested { get; private set; }
        internal void Reset(int mode) { _mode = mode; Copied = Released = Requested = 0; }
        public DisunityResult Request(ulong world, byte* bytes, nuint length, ulong* response)
        {
            Requested++;
            return native.Request(world, bytes, length, response);
        }
        public DisunityResult Copy(ulong response, byte* bytes, nuint capacity, nuint* length)
        {
            if (bytes != null) Copied++;
            if (_mode == 0) return native.Copy(response, bytes, capacity, length);
            *length = (8 << 20) + 1;
            return new DisunityResult { code = 2 };
        }
        public DisunityResult Free(ulong response)
        {
            Released++;
            var result = native.Free(response);
            BinderBehavior.Check(result.code == 0, "real retained response release");
            return _mode == 2 ? new DisunityResult { code = -77 } : result;
        }
    }
}
