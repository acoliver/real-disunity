using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using UnityEngine;

namespace CompatibilityBridge;

public static unsafe partial class ManagedBridge
{
    private static string? _libraryPath;
    private static string _lastError = string.Empty;
    private static bool _loaded;
    private static bool _cleanupPending;
    private static ScriptSession? _session;

    [UnmanagedCallersOnly(EntryPoint = "ConfigureNativeLibrary", CallConvs = [typeof(CallConvCdecl)])]
    public static int ConfigureNativeLibrary(byte* path, nuint length) => Guard(() =>
    {
        var text = ReadUtf8(path, length);
        if (string.IsNullOrWhiteSpace(text)) throw new ArgumentException("native library path is empty");
        if (_loaded || _cleanupPending) throw new InvalidOperationException("cannot configure a loaded or cleanup-pending fixture");
        using var description = NativeDescription.Read(text, 0);
        _libraryPath = text;
        return 0;
    });

    [UnmanagedCallersOnly(EntryPoint = "LoadFixture", CallConvs = [typeof(CallConvCdecl)])]
    public static int LoadFixture(byte* fixture, nuint length) => Guard(() =>
    {
        if (_loaded) throw new InvalidOperationException("a compatibility fixture is already loaded");
        if (_cleanupPending) throw new InvalidOperationException("a compatibility fixture is cleanup-pending");
        if (_session is not null && (_session.ScriptCount != 0 || _session.CoroutineCount != 0 || _session.BindingCount != 0))
            throw new InvalidOperationException("previous fixture still retains measured managed roots; refusing to replace its statistics");
        if (_libraryPath is null) throw new InvalidOperationException("ConfigureNativeLibrary must run before LoadFixture");
        using var json = JsonDocument.Parse(ReadUtf8(fixture, length));
        var root = json.RootElement;
        using var description = NativeDescription.Read(_libraryPath, 0);
        var world = BridgeProtocol.ValidateManifest(root, description.RootElement.GetProperty("data"));
        using var liveDescription = NativeDescription.Read(_libraryPath, world);
        if (!System.Text.Json.Nodes.JsonNode.DeepEquals(
            System.Text.Json.Nodes.JsonNode.Parse(description.RootElement.GetProperty("data").GetRawText()),
            System.Text.Json.Nodes.JsonNode.Parse(liveDescription.RootElement.GetProperty("data").GetRawText())))
            throw BridgeProtocol.Invalid("world descriptor differs from preactivation negotiation");
        var strict = PhaseContexts.Selected(_libraryPath, world, description.RootElement.GetProperty("data"), root);
        var terminal = TerminalCleanupProfile.Selected(root, description.RootElement.GetProperty("data"));
        NativeWorld.AttachNativeLibrary(_libraryPath, world);
        NativePhaseScope.Strict = strict;
        IDisposable? scope = null;
        try
        {
            scope = strict ? PhaseContexts.Claim("Initialize", root, "initial_context") : null;
            _session = terminal ? new ScriptSession(root, pending => { _session = pending; _cleanupPending = true; }) : new ScriptSession(root);
            if (strict) _session.InitializeClaimed();
            else _session.Initialize();
            _loaded = true;
            _cleanupPending = false;
            return 0;
        }
        catch (Exception primary)
        {
            if (!terminal)
            {
                try { _session?.Dispose(); }
                catch (Exception cleanup) { throw new AggregateException(primary, cleanup); }
                finally { NativeWorld.Detach(); }
            }
            throw;
        }
        finally { scope?.Dispose(); }
    });

    [UnmanagedCallersOnly(EntryPoint = "DispatchPhase", CallConvs = [typeof(CallConvCdecl)])]
    public static int DispatchPhase(byte* phase, nuint length) => Guard(() =>
    {
        using var json = JsonDocument.Parse(ReadUtf8(phase, length));
        var root = json.RootElement;
        if (!root.TryGetProperty("phase", out var phaseNode) || phaseNode.ValueKind != JsonValueKind.String)
            throw new InvalidDataException("phase request requires a string 'phase'");
        var name = phaseNode.GetString()!;
        if (name != "Cleanup" || !_cleanupPending) EnsureLoaded();
        if (_session!.Strict)
        {
            if (name is not ("FixedUpdate" or "FixedCoroutines" or "Update" or "UpdateCoroutines" or "LateUpdate" or "EndOfFrame" or "Cleanup"))
                throw new NotSupportedException($"strict DispatchPhase('{name}')");
            if (root.TryGetProperty("cleanup", out var cleanup) && cleanup.GetBoolean() && name != "Cleanup")
                throw new InvalidDataException("strict disposal requires the native Cleanup phase");
            using var scope = PhaseContexts.Claim(name, root);
            if (name == "Cleanup")
            {
                try { _session.CleanupClaimed(); }
                finally
                {
                    _cleanupPending = _session.ScriptCount != 0 || _session.CoroutineCount != 0 || _session.BindingCount != 0;
                    if (!_cleanupPending) { NativeWorld.Detach(); _loaded = false; }
                }
            }
            else _session.DispatchClaimed(name);
            return 0;
        }
        if (name is not ("Awake" or "OnEnable" or "Start" or "FixedUpdate" or "Update" or "LateUpdate" or "OnDisable" or "OnDestroy"))
            throw new NotSupportedException($"DispatchPhase('{name}')");
        _session!.Dispatch(name);
        return 0;
    });

    [UnmanagedCallersOnly(EntryPoint = "InvokeContact", CallConvs = [typeof(CallConvCdecl)])]
    public static int InvokeContact(byte* contact, nuint length) => Guard(() =>
    {
        EnsureLoaded();
        using var json = JsonDocument.Parse(ReadUtf8(contact, length));
        if (_session!.Strict)
        {
            using var scope = PhaseContexts.Claim("Contacts", json.RootElement);
            _session.ContactsClaimed(json.RootElement);
        }
        else _session.InvokeContact(json.RootElement);
        return 0;
    });

    [UnmanagedCallersOnly(EntryPoint = "Observe", CallConvs = [typeof(CallConvCdecl)])]
    public static int Observe(byte* query, nuint queryLength, byte* output, nuint capacity, nuint* outLength) => Guard(() =>
    {
        if (outLength is null) throw new ArgumentNullException(nameof(outLength));
        using var request = JsonDocument.Parse(ReadUtf8(query, queryLength));
        object observation;
        if (request.RootElement.TryGetProperty("op", out var operation) && operation.GetString() == "managed.statistics")
        {
            observation = new { version = 1, ok = true, data = new {
                scripts = (ulong)(_session?.ScriptCount ?? 0),
                coroutines = (ulong)(_session?.CoroutineCount ?? 0),
                bindings = (ulong)(_session?.BindingCount ?? 0) + (ulong)NativeIdentity.BindingCount + (NativeIdentity.Runtime is null ? 0UL : 1UL) + (SpriteRenderer.ResolveSprite is null ? 0UL : 1UL),
                loaded = _loaded, cleanup_pending = _cleanupPending, world = NativeWorld.AttachedWorld, callback_failures = _session?.CallbackFailureCount ?? 0
            } };
        }
        else
        {
            if (operation.ValueKind != JsonValueKind.String || operation.GetString() != "managed.failures") EnsureLoaded();
            observation = _session is not null ? _session.Observe(request.RootElement) : new CallbackFailures().Observe(request.RootElement);
        }
        var result = BoundedReply.Encode(observation);
        if (result.Length > BridgeProtocol.ResponseLimit) throw new InvalidDataException("managed observation response budget exceeded");
        *outLength = (nuint)result.Length;
        if (capacity == 0 && output is null)
        {
            return -2;
        }
        if (output is null) throw new ArgumentNullException(nameof(output));
        if (capacity < (nuint)result.Length)
        {
            return -2;
        }
        result.CopyTo(new Span<byte>(output, result.Length));
        return 0;
    }, preserveError: true);

    [UnmanagedCallersOnly(EntryPoint = "DescribeBridge", CallConvs = [typeof(CallConvCdecl)])]
    public static int DescribeBridge(byte* output, nuint capacity, nuint* outLength)
    {
        if (outLength is null) return -1;
        var bytes = BridgeProtocol.Describe();
        *outLength = (nuint)bytes.Length;
        if (output is null && capacity == 0) return -2;
        if (output is null) return -1;
        if (capacity < (nuint)bytes.Length) return -2;
        bytes.CopyTo(new Span<byte>(output, bytes.Length));
        return 0;
    }

    [UnmanagedCallersOnly(EntryPoint = "GetLastError", CallConvs = [typeof(CallConvCdecl)])]
    public static int GetLastError(byte* output, nuint capacity, nuint* outLength)
    {
        if (outLength is null) return -1;
        var bytes = Encoding.UTF8.GetBytes(_lastError);
        *outLength = (nuint)bytes.Length;
        if (output is null && capacity == 0) return bytes.Length == 0 ? 0 : -2;
        if (output is null || capacity < (nuint)bytes.Length) return -2;
        bytes.CopyTo(new Span<byte>(output, bytes.Length));
        return 0;
    }

    [UnmanagedCallersOnly(EntryPoint = "Unload", CallConvs = [typeof(CallConvCdecl)])]
    public static int Unload() => Guard(() =>
    {
        if (_session is { Strict: true, ScriptCount: > 0 } && NativePhaseScope.Phase is null)
            throw new InvalidOperationException("strict unload requires a native claimed cleanup phase");
        try { _session?.Dispose(); }
        finally { NativeWorld.Detach(); _loaded = false; }
        return 0;
    });

    private static int Guard(Func<int> action, bool preserveError = false)
    {
        try
        {
            var result = action();
            if (result == 0 && !preserveError) _lastError = string.Empty;
            return result;
        }
        catch (Exception error)
        {
            var text = error.ToString();
            _lastError = text.Length > BridgeProtocol.ResponseLimit / 3
                ? text[..(BridgeProtocol.ResponseLimit / 3)] : text;
            return -1;
        }
    }

    private static string ReadUtf8(byte* value, nuint length)
    {
        if (value is null && length != 0) throw new ArgumentNullException(nameof(value));
        if (length > BridgeProtocol.RequestLimit) throw new ArgumentOutOfRangeException(nameof(length));
        return Encoding.UTF8.GetString(new ReadOnlySpan<byte>(value, (int)length));
    }

    private static void EnsureLoaded()
    {
        if (!_loaded) throw new InvalidOperationException("no compatibility fixture is loaded");
    }
}
