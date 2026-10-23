using System.Runtime.InteropServices;

namespace Disunity;

/// <summary>Command kinds as defined by the native ABI.</summary>
public enum NativeCommandKind : uint
{
    SetPosition = 0,
    SetRotationY = 1,
    SetMaterialSlot = 2,
}

/// <summary>One native command; slot values ride in X for small integers.</summary>
internal readonly record struct NativeCommand(ulong Object, NativeCommandKind Kind, float X, float Y, float Z);

/// <summary>
/// Managed engine entry point. One native world at a time;
/// the host loads, ticks, extracts, and unloads.
/// </summary>
public static class WorldRunner
{
    private static readonly List<NativeCommand> Pending = new();
    private static readonly List<GameObject> Tracked = new();
    private static readonly List<MonoBehaviour> Behaviours = new();
    private static bool _loaded;

    /// <summary>Selects the shared native engine image before any engine APIs are used.</summary>
    public static void ConfigureNativeLibrary(string path) => NativeLibrary.Configure(path);

    /// <summary>The native world handle; hosts pass it to bulk export calls.</summary>
    public static ulong WorldHandle { get; private set; }

    /// <summary>True between <see cref="LoadWorld"/> and <see cref="UnloadWorld"/>.</summary>
    public static bool IsLoaded => _loaded;

    /// <summary>Creates the native world and runs Awake on all behaviours.</summary>
    public static void LoadWorld()
    {
        EnsureLoaded();
        if (_loaded)
        {
            throw new InvalidOperationException("a world is already loaded");
        }
        ulong world;
        DisunityResult result;
        unsafe
        {
            result = NativeBindings.disunity_world_create(&world);
        }
        ThrowIfError(result);
        WorldHandle = world;
        _loaded = true;
        Time.DeltaTime = 0.0f;
        Time.Frame = 0;
    }

    /// <summary>Creates a tracked object in the native store.</summary>
    public static GameObject CreateGameObject()
    {
        EnsureLoaded();
        ulong handle;
        DisunityResult result;
        unsafe
        {
            result = NativeBindings.disunity_gameobject_create(WorldHandle, &handle);
        }
        ThrowIfError(result);
        var gameObject = new GameObject(handle);
        Tracked.Add(gameObject);
        return gameObject;
    }

    /// <summary>Attaches a behaviour whose lifecycle the host drives.</summary>
    public static T AddComponent<T>(T behaviour) where T : MonoBehaviour
    {
        ArgumentNullException.ThrowIfNull(behaviour);
        if (!_loaded)
        {
            throw new InvalidOperationException("components require a loaded world");
        }
        behaviour.gameObject = CreateGameObject();
        behaviour.HasStarted = false;
        Behaviours.Add(behaviour);
        behaviour.Awake();
        return behaviour;
    }

    /// <summary>
    /// One simulation tick: run script phases, then flush their commands and
    /// step the native world before state extraction. Start runs once before
    /// each behaviour's first Update; LateUpdate runs after Update in the same tick.
    /// Behaviours added during callbacks join the next tick.
    /// </summary>
    public static void Step(float deltaTime)
    {
        EnsureLoaded();
        if (!_loaded)
        {
            throw new InvalidOperationException("no world is loaded");
        }
        Time.DeltaTime = deltaTime;
        Time.Frame += 1;
        var participants = Behaviours.ToArray();
        foreach (var behaviour in participants)
        {
            if (!behaviour.gameObject.IsDestroyed && !behaviour.HasStarted)
            {
                behaviour.HasStarted = true;
                behaviour.Start();
            }
        }
        foreach (var behaviour in participants)
        {
            if (!behaviour.gameObject.IsDestroyed)
            {
                behaviour.Update();
            }
        }
        foreach (var behaviour in participants)
        {
            if (!behaviour.gameObject.IsDestroyed)
            {
                behaviour.LateUpdate();
            }
        }
        SubmitPending();
        var stepResult = NativeBindings.disunity_world_step(WorldHandle, deltaTime, 1);
        ThrowIfError(stepResult);
    }

    /// <summary>Drains pending commands into the native buffer.</summary>
    internal static void SubmitPending()
    {
        if (Pending.Count == 0)
        {
            return;
        }
        unsafe
        {
            var buffer = new DisunityCommand[Pending.Count];
            for (var i = 0; i < Pending.Count; i++)
            {
                buffer[i] = new DisunityCommand
                {
                    @object = Pending[i].Object,
                    kind = (uint)Pending[i].Kind,
                    x = Pending[i].X,
                    y = Pending[i].Y,
                    z = Pending[i].Z,
                };
            }
            fixed (DisunityCommand* commands = buffer)
            {
                var result = NativeBindings.disunity_command_buffer_submit(
                    WorldHandle, commands, (uint)buffer.Length);
                ThrowIfError(result);
            }
        }
        Pending.Clear();
    }

    internal static void Enqueue(NativeCommand command) => Pending.Add(command);

    internal static void RemovePendingCommands(ulong handle) =>
        Pending.RemoveAll(command => command.Object == handle);

    internal static void Forget(GameObject gameObject)
    {
        Tracked.Remove(gameObject);
        var index = Behaviours.FindIndex(candidate => ReferenceEquals(candidate.gameObject, gameObject));
        if (index >= 0)
        {
            var behaviour = Behaviours[index];
            Behaviours.RemoveAt(index);
            behaviour.OnDestroy();
        }
    }

    /// <summary>Destroys the native world and runs OnDestroy on all behaviours.</summary>
    public static void UnloadWorld()
    {
        if (!_loaded)
        {
            return;
        }
        while (Behaviours.Count > 0)
        {
            var behaviour = Behaviours[0];
            Behaviours.RemoveAt(0);
            behaviour.OnDestroy();
        }
        foreach (var gameObject in Tracked)
        {
            gameObject.MarkDestroyed();
        }
        Tracked.Clear();
        Pending.Clear();
        var destroyResult = NativeBindings.disunity_world_destroy(WorldHandle);
        ThrowIfError(destroyResult);
        WorldHandle = 0;
        _loaded = false;
    }

    internal static void EnsureLoaded() => NativeLibrary.EnsureLoaded();

    internal static void ThrowIfError(DisunityResult result)
    {
        if (result.code == 0)
        {
            return;
        }
        var message = FetchErrorMessage();
        throw new DisunityException(result.code, string.IsNullOrEmpty(message) ? $"native error {result.code}" : message);
    }

    private static string FetchErrorMessage()
    {
        var buffer = new byte[512];
        int decoded;
        unsafe
        {
            uint length = 0;
            fixed (byte* target = buffer)
            {
                var errorResult = NativeBindings.disunity_error_message(
                    WorldHandle, target, (uint)buffer.Length, &length);
                decoded = errorResult.code == 0 ? (int)length : 0;
            }
        }
        return decoded > 0 ? System.Text.Encoding.UTF8.GetString(buffer.AsSpan(0, decoded)) : string.Empty;
    }
}

/// <summary>Native failure surfaced with the engine's own message.</summary>
public sealed class DisunityException(int code, string message) : Exception($"[{code}] {message}");
