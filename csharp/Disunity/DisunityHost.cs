using System.Text;
using Disunity.Native;
using UnityEngine;

namespace Disunity.Hosting;

public enum NativeCommandKind : uint { SetPosition, SetRotationY, SetMaterialSlot }
internal readonly record struct NativeCommand(ulong Object, NativeCommandKind Kind, float X, float Y, float Z);

/// <summary>Managed callback execution. Eligibility, time and phase ordering belong to the native world.</summary>
public static partial class DisunityHost
{
    private static readonly List<NativeCommand> Pending = new();
    private static readonly List<GameObject> Tracked = new();
    private static readonly Dictionary<ulong, GameObject> TrackedByHandle = new();
    private static readonly Dictionary<ulong, MonoBehaviour> Scripts = new();
    private static ulong _nextScript = 1;
    private static bool _faulted;
    public static ulong WorldHandle { get; private set; }
    public static bool IsLoaded => WorldHandle != 0;

    public static void ConfigureNativeLibrary(string path) => NativeLibrary.Configure(path);

    /// <summary>The directory of the configured engine library, where the engine's substitute
    /// native libraries are, or null before <see cref="ConfigureNativeLibrary"/>.</summary>
    public static string? NativeLibraryDirectory => NativeLibrary.LibraryDirectory;
    public static unsafe void LoadWorld()
    {
        EnsureLoaded();
        ThrowIfError(NativeBindings.disunity_abi_check(NativeAbi.Version));
        if (IsLoaded) throw new InvalidOperationException("a world is already loaded");
        ulong world;
        ThrowIfError(NativeBindings.disunity_world_create(&world));
        WorldHandle = world;
        UnityEngine.Time.Start();
        _faulted = false;
        ResetScriptExceptions();
        ResetWorldSettings();
        _listenerDiagnostic = null;
        try { InstallMainThreadContext(); }
        catch
        {
            NativeBindings.disunity_world_destroy(world);
            WorldHandle = 0;
            throw;
        }
    }

    public static GameObject CreateGameObject() => Track(new GameObject(CreateNativeObject()));

    internal static unsafe ulong CreateNativeObject()
    {
        EnsureWorld();
        ulong handle;
        ThrowIfError(NativeBindings.disunity_gameobject_create(WorldHandle, &handle));
        return handle;
    }

    public static unsafe T AddComponent<T>(T behaviour) where T : MonoBehaviour
    {
        ArgumentNullException.ThrowIfNull(behaviour);
        return AttachComponent(CreateGameObject(), behaviour);
    }

    internal static IEnumerable<Component> Components(GameObject owner) =>
        Scripts.Values.Where(script => ReferenceEquals(script.gameObject, owner)).Cast<Component>().Concat(NativeComponents.Values.Where(c => ReferenceEquals(c.gameObject, owner))).Prepend(owner.transform);

    internal static unsafe T Attach<T>(GameObject owner, T behaviour) where T : MonoBehaviour
    {
        behaviour.gameObject = owner;
        var identity = checked(_nextScript++);
        ulong script;
        ThrowIfError(NativeBindings.disunity_script_register(WorldHandle, behaviour.gameObject.NativeHandle, identity, &script));
        Scripts.Add(identity, behaviour);
        DisunityAdvance work;
        ThrowIfError(NativeBindings.disunity_object_wake(WorldHandle, behaviour.gameObject.NativeHandle, &work));
        Drain(work);
        return behaviour;
    }

    public static void Step(float deltaTime)
    {
        Advance(deltaTime);
        ReportAudioListenerDiagnostic();
    }

    /// <summary>Advances without Unity's per-frame scene diagnostics, which do not apply while
    /// the world is being torn down.</summary>
    private static unsafe void Advance(float deltaTime)
    {
        EnsureWorld();
        SubmitPending();
        var denominator = float.IsFinite(deltaTime) && deltaTime >= 0 ? 1_000_000_000UL : 0;
        var nanos = denominator == 0 ? 0 : checked((ulong)Math.Round((double)deltaTime * 1_000_000_000));
        DisunityAdvance work;
        ThrowIfError(NativeBindings.disunity_world_advance(WorldHandle, nanos, denominator, &work));
        Drain(work);
    }

    public static unsafe void Dispatch(ulong identity, uint message)
    {
        Exception? failure = null;
        try { DrainCancelled(); }
        catch (Exception error) { failure = error; }
        DisunityCallback payload;
        ThrowIfError(NativeBindings.disunity_callback_read(WorldHandle, identity, &payload));
        var previousPhase = InputPhase;
        InputPhase = payload.input_phase;
        try
        {
            if (message >= 10)
            {
                if (!DispatchDelayedTasks(identity, message)) DispatchAsset(identity, message);
            }
            else
            {
                var script = Scripts[identity];
                if (message == 8) DispatchCoroutine(payload);
                else if (message == 9) DispatchContact(script, payload);
                else ScriptMessages.Invoke(script, message);
            }
            SubmitPending();
        }
        catch (Exception error) { failure ??= error; }
        finally
        {
            InputPhase = previousPhase;
            try { DrainCancelled(); }
            catch (Exception error) { failure ??= error; }
        }
        if (failure != null) System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }

    public static void ReportFault() => _faulted = true;

    public static Vector3 ObjectPosition(ulong handle) => ResolveObject(handle).Transform.position;

    internal static GameObject ResolveObject(ulong handle) =>
        TrackedByHandle.TryGetValue(handle, out var value) && !value.IsDestroyed
            ? value
            : throw new InvalidOperationException("no live object has that native handle");

    internal static unsafe void Drain(DisunityAdvance work)
    {
        while (work.kind == 1)
        {
            Exception? failure = null;
            try
            {
                Dispatch(work.script, work.message);
                SubmitPending();
            }
            catch (Exception error) { failure = error; Pending.Clear(); _faulted = true; }
            var bytes = failure == null ? Array.Empty<byte>() : Encoding.UTF8.GetBytes(failure.ToString());
            var ticket = work.ticket;
            ThrowIfError(NativeBindings.disunity_callback_check(WorldHandle, ticket));
            fixed (byte* message = bytes)
                ThrowIfError(NativeBindings.disunity_world_resume(WorldHandle, ticket, failure == null ? 0u : 1u, message, (uint)bytes.Length, &work));
        }
        if (work.kind == 0) PruneRetired();
    }

    internal static unsafe void PruneInvalidated()
    {
        foreach (var pair in Scripts.Where(pair => pair.Value.gameObject.IsDestroyed).ToArray())
        {
            uint retained;
            ThrowIfError(NativeBindings.disunity_script_retained(WorldHandle, pair.Key, &retained));
            if (retained == 0) Scripts.Remove(pair.Key);
        }
        foreach (var handle in NativeComponents.Where(pair => pair.Value.gameObject.IsDestroyed).Select(pair => pair.Key).ToArray()) NativeComponents.Remove(handle);
        RemoveDestroyedObjects();
    }

    private static void PruneRetired()
    {
        foreach (var identity in Scripts.Where(pair => pair.Value.gameObject.IsDestroyed).Select(pair => pair.Key).ToArray()) Scripts.Remove(identity);
        foreach (var handle in NativeComponents.Where(pair => pair.Value.gameObject.IsDestroyed).Select(pair => pair.Key).ToArray()) NativeComponents.Remove(handle);
        RemoveDestroyedObjects();
    }

    internal static unsafe void SubmitPending()
    {
        if (Pending.Count == 0) return;
        var buffer = Pending.Select(c => new DisunityCommand { @object = c.Object, kind = (uint)c.Kind, x = c.X, y = c.Y, z = c.Z }).ToArray();
        fixed (DisunityCommand* commands = buffer)
            ThrowIfError(NativeBindings.disunity_command_buffer_submit(WorldHandle, commands, (uint)buffer.Length));
        Pending.Clear();
    }
    internal static void Enqueue(NativeCommand command) => Pending.Add(command);
    internal static void RemovePendingCommands(ulong handle) => Pending.RemoveAll(c => c.Object == handle);

    public static unsafe void UnloadWorld()
    {
        if (!IsLoaded) return;
        UnloadScene();
        try
        {
            // Objects created by final callbacks are retired by another native cleanup advance.
            while (!_faulted && (Tracked.Any(o => !o.IsDestroyed) || Assets.Count != 0))
            {
                foreach (var asset in Assets.Values.ToArray()) UnityEngine.Object.Destroy(asset);
                foreach (var value in Tracked.ToArray()) value.Destroy();
                Advance(0);
            }
        }
        finally
        {
            try { DisposeAllRoutines(); }
            finally
            {
                foreach (var value in Tracked) value.MarkDestroyed();
                CurrentScene = null; LoadedScenes.Clear(); UnloadingScenes.Clear(); AxisDefinitions.Clear(); Tracked.Clear(); TrackedByHandle.Clear(); Scripts.Clear(); Pending.Clear(); NativeComponents.Clear(); Fonts.Clear(); Assets.Clear(); PhysicsMaterial2D.Clear();
                var world = WorldHandle;
                WorldHandle = 0;
                _listenerDiagnostic = null;
                RemoveMainThreadContext();
                Resources?.Clear();
                ThrowIfError(NativeBindings.disunity_world_destroy(world));
            }
        }
    }
    internal static void EnsureLoaded() => NativeLibrary.EnsureLoaded();
    private static void EnsureWorld()
    {
        EnsureLoaded();
        if (!IsLoaded) throw new InvalidOperationException("no world is loaded");
    }
    internal static void ThrowIfError(DisunityResult result)
    {
        if (result.code == 0) return;
        throw new DisunityException(result.code, (DisunityErrorReason)result.detail, FetchErrorMessage());
    }
    private static unsafe string FetchErrorMessage()
    {
        DisunityBuffer owned;
        var result = NativeBindings.disunity_error_take(WorldHandle, &owned);
        if (result.code != 0) return "native operation failed";
        try
        {
            var bytes = new byte[owned.length];
            uint written;
            fixed (byte* target = bytes)
            {
                var copied = NativeBindings.disunity_buffer_copy(owned.handle, target, owned.length, &written);
                if (copied.code != 0 || written != owned.length) throw new InvalidOperationException("diagnostic buffer copy failed");
            }
            return Encoding.UTF8.GetString(bytes);
        }
        finally
        {
            var freed = NativeBindings.disunity_buffer_free(owned.handle);
            if (freed.code != 0) throw new InvalidOperationException("diagnostic buffer ownership lost");
        }
    }
}

public sealed class DisunityException(int code, DisunityErrorReason reason, string message) : Exception($"[{code}/{reason}] {message}")
{
    public int Code { get; } = code;
    public DisunityErrorReason Reason { get; } = reason;
}
