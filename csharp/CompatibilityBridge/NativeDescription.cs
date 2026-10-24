using System.Runtime.InteropServices;
using System.Text.Json;

namespace CompatibilityBridge;

internal static unsafe class NativeDescription
{
    [StructLayout(LayoutKind.Sequential)]
    private struct Result
    {
        internal int Code;
        internal ulong Detail;
    }
    internal static JsonDocument Read(string path, ulong world, bool status = false)
    {
        var image = NativeLibrary.Load(path);
        try { return ReadImage(image, world, status ? "session.status" : "describe", BridgeProtocol.ResponseLimit); }
        finally { NativeLibrary.Free(image); }
    }
    internal static JsonDocument ReadTransport(string path, ulong world)
    {
        var image = NativeLibrary.Load(path);
        try { return ReadImage(image, world, "transport.get", ByteTransportProfile.ReplyLimit); }
        finally { NativeLibrary.Free(image); }
    }
    private static JsonDocument ReadImage(nint image, ulong world, string operation, int limit)
    {
        var version = (delegate* unmanaged[Cdecl]<uint*, Result>)NativeLibrary.GetExport(image, "disunity_abi_version");
        uint abi = 0;
        Check(version(&abi));
        if (abi != 2) throw BridgeProtocol.Invalid("native ABI must be 2");
        var request = (delegate* unmanaged[Cdecl]<ulong, byte*, nuint, ulong*, Result>)NativeLibrary.GetExport(image, "disunity_world_request");
        var free = (delegate* unmanaged[Cdecl]<ulong, Result>)NativeLibrary.GetExport(image, "disunity_response_free");
        var bytes = JsonSerializer.SerializeToUtf8Bytes(new { version = 1, op = operation });
        ulong response = 0;
        fixed (byte* input = bytes) Check(request(world, input, (nuint)bytes.Length, &response));
        Exception? primary = null;
        try { return Copy(image, response, operation, limit); }
        catch (Exception error) { primary = error; throw; }
        finally
        {
            var released = free(response);
            if (released.Code != 0 && primary is not null)
                throw new AggregateException(primary, BridgeProtocol.Invalid($"native response release code {released.Code}"));
            Check(released);
        }
    }
    private static JsonDocument Copy(nint image, ulong response, string operation, int limit)
    {
        var copy = (delegate* unmanaged[Cdecl]<ulong, byte*, nuint, nuint*, Result>)NativeLibrary.GetExport(image, "disunity_response_copy");
        nuint length = 0;
        var probe = copy(response, null, 0, &length);
        if (probe.Code != 2) Check(probe);
        if (length == 0 || length > (nuint)limit) throw BridgeProtocol.Invalid("native descriptor response budget");
        var bytes = new byte[checked((int)length)];
        fixed (byte* output = bytes) Check(copy(response, output, (nuint)bytes.Length, &length));
        if (length != (nuint)bytes.Length) throw BridgeProtocol.Invalid("native descriptor copy length changed");
        var json = JsonDocument.Parse(bytes);
        try
        {
            var root = json.RootElement;
            if (root.GetProperty("version").GetInt32() != 1 || !root.GetProperty("ok").GetBoolean())
                throw BridgeProtocol.Invalid("native describe rejected");
            if (operation == "describe") BridgeProtocol.ValidateNative(root.GetProperty("data"));
            return json;
        }
        catch { json.Dispose(); throw; }
    }
    private static void Check(Result result)
    {
        if (result.Code != 0) throw BridgeProtocol.Invalid($"native describe transport code {result.Code}");
    }
}
