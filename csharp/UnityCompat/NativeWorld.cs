using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace UnityEngine;

public interface INativeWorld
{
    JsonDocument Request(JsonDocument request);
    double TimeValue(string field);
}

public static class NativeWorld
{
    private static INativeWorld? _current;

    internal static ulong? AttachedWorld => _current is NativeJsonWorld native ? native.Handle : null;

    public static INativeWorld Current => _current ?? throw new InvalidOperationException(
        "Unity compatibility APIs require an attached native world.");

    public static void Attach(INativeWorld world) => _current = world ?? throw new ArgumentNullException(nameof(world));
    public static void Detach() { _current = null; NativePhaseScope.Reset(); NativeIdentity.Reset(); }

    public static JsonDocument Request(string operation, params (string key, object? value)[] fields)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(operation);
        using var trace = CompatibilityTrace.Begin("native", operation);
        var values = new Dictionary<string, object?> { ["version"] = 1, ["op"] = operation };
        foreach (var (key, value) in fields)
        {
            if (!values.TryAdd(key, value)) throw new ArgumentException($"duplicate request field '{key}'", nameof(fields));
        }
        using var request = _current is NativeJsonWorld { ByteProfile: true }
            ? NativeByteProfile.Document(values) : JsonDocument.Parse(JsonSerializer.Serialize(values));
        var response = Current.Request(request);
        if (!response.RootElement.TryGetProperty("ok", out var ok) || !ok.GetBoolean())
        {
            var error = response.RootElement.TryGetProperty("error", out var errorValue)
                ? errorValue : default;
            var code = error.ValueKind == JsonValueKind.Object && error.TryGetProperty("code", out var c)
                ? c.GetString() : "invalid_response";
            var message = error.ValueKind == JsonValueKind.Object && error.TryGetProperty("message", out var m)
                ? m.GetString() : "native response did not contain a successful result";
            CompatibilityTrace.Failure(values.ContainsKey("host") || values.ContainsKey("context") || operation.StartsWith("session.", StringComparison.Ordinal)
                ? operation : request.RootElement.GetRawText(), code ?? "native_error", message ?? "native request failed");
            response.Dispose();
            throw new NativeWorldException(code ?? "native_error", message ?? "native request failed");
        }
        return response;
    }

    public static JsonElement Data(JsonDocument response) => response.RootElement.GetProperty("data");

    public static void AttachNativeByteProfile(string path, ulong world)
    {
        NativeJsonWorld.Configure(path, world);
        var selected = new NativeJsonWorld(world, byteProfile: true);
        NativeByteProfile.Validate(selected);
        Attach(selected);
    }

    public static void AttachNativeLibrary(string path, ulong world)
    {
        NativeJsonWorld.Configure(path, world);
        Attach(new NativeJsonWorld(world));
    }
}

internal sealed partial class NativeJsonWorld(ulong world, bool byteProfile = false) : INativeWorld
{
    internal ulong Handle => world;
    internal bool ByteProfile => byteProfile;
    private const string LibraryName = "worldrunner_disunity_ffi";
    private static readonly object ResolverGate = new();
    private static string? _libraryPath;
    private static nint _nativeImage;
    private static bool _resolverInstalled;

    internal static void Configure(string path, ulong world)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(path);
        var fullPath = Path.GetFullPath(path);
        if (!File.Exists(fullPath)) throw new FileNotFoundException("native FFI image not found", fullPath);
        lock (ResolverGate)
        {
            if (_libraryPath is not null && !string.Equals(_libraryPath, fullPath, StringComparison.Ordinal))
                throw new InvalidOperationException("the native FFI image cannot change in a live process");
            _libraryPath = fullPath;
            if (!_resolverInstalled)
            {
                NativeLibrary.SetDllImportResolver(typeof(NativeJsonWorld).Assembly, Resolve);
                _resolverInstalled = true;
            }
        }
    }

    public JsonDocument Request(JsonDocument request)
    {
        var bytes = byteProfile ? NativeByteProfile.Encode(request.RootElement) : Encoding.UTF8.GetBytes(request.RootElement.GetRawText());
        return RequestRaw(bytes);
    }

    internal JsonDocument RequestRaw(ReadOnlySpan<byte> bytes)
    {
        if (bytes.Length == 0 || bytes.Length > (1 << 20)) throw new InvalidDataException("native raw request length budget");
        ulong response;
        DisunityResult result;
        using (CompatibilityTrace.Begin("ffi", "world_request"))
        unsafe
        {
            fixed (byte* requestPointer = bytes)
                result = NativeBindings.disunity_world_request(world, requestPointer, (nuint)bytes.Length, &response);
        }
        Check(result, "native request failed");
        Exception? primary = null;
        try
        {
            nuint length = 0;
            unsafe
            {
                var probe = NativeBindings.disunity_response_copy(response, null, 0, &length);
                if (probe.code != 0 && probe.code != 2)
                    throw new NativeWorldException(probe.code.ToString(), "native response length probe failed");
            }
            if (byteProfile && (length == 0 || length > NativeByteProfile.ReplyLimit))
                throw new InvalidDataException("byte transport native response exceeds eight MiB or is empty");
            if (length > int.MaxValue) throw new InvalidDataException("native response exceeds the managed JSON limit");
            var output = new byte[(int)length];
            unsafe
            {
                fixed (byte* outputPointer = output)
                    Check(NativeBindings.disunity_response_copy(response, outputPointer, (nuint)output.Length, &length), "native response copy failed");
            }
            if (length != (nuint)output.Length) throw new InvalidDataException("native response length changed during copy");
            return JsonDocument.Parse(output);
        }
        catch (Exception error) { primary = error; throw; }
        finally
        {
            var released = NativeBindings.disunity_response_free(response);
            if (released.code != 0 && primary is not null)
                throw new AggregateException(primary, new NativeWorldException(released.code.ToString(), "native response release failed"));
            Check(released, "native response release failed");
        }
    }

    public double TimeValue(string field)
    {
        using var response = NativeWorld.Request("time.get");
        var time = NativeWorld.Data(response);
        if (!time.TryGetProperty(field, out var value) || !value.TryGetDouble(out var number))
            throw new NativeWorldException("invalid_response", $"time.get did not return numeric field '{field}'");
        return number;
    }

    private static void Check(DisunityResult result, string operation)
    {
        if (result.code != 0) throw new NativeWorldException(result.code.ToString(), operation);
    }

    private static nint Resolve(string name, Assembly assembly, DllImportSearchPath? searchPath)
    {
        if (name != LibraryName) return 0;
        lock (ResolverGate)
        {
            if (_nativeImage == 0)
                _nativeImage = System.Runtime.InteropServices.NativeLibrary.Load(_libraryPath
                    ?? throw new InvalidOperationException("configure the native FFI image before calling it"));
            return _nativeImage;
        }
    }

    private static class NativeBindings
    {
        [DllImport(LibraryName, EntryPoint = "disunity_world_request", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
        internal static extern unsafe DisunityResult disunity_world_request(ulong world, byte* request, nuint length, ulong* response);
        [DllImport(LibraryName, EntryPoint = "disunity_response_copy", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
        internal static extern unsafe DisunityResult disunity_response_copy(ulong response, byte* buffer, nuint capacity, nuint* length);
        [DllImport(LibraryName, EntryPoint = "disunity_response_free", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
        internal static extern DisunityResult disunity_response_free(ulong response);
    }
}

[StructLayout(LayoutKind.Sequential)]
internal struct DisunityResult
{
    public int code;
    public ulong detail;
}

public sealed class NativeWorldException(string code, string message) : Exception($"[{code}] {message}")
{
    public string Code { get; } = code;
}
