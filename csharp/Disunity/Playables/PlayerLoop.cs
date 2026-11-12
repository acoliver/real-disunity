using Disunity.Hosting;

namespace UnityEngine.LowLevel;

/// <summary>One system of the player loop: a type that names it, its child systems and the function it runs.</summary>
public struct PlayerLoopSystem
{
    public delegate void UpdateFunction();

    public Type? type;
    public PlayerLoopSystem[]? subSystemList;
    public UpdateFunction? updateDelegate;
    public IntPtr updateFunction;
    public IntPtr loopConditionFunction;

    public override readonly string ToString() => type?.Name ?? "";
}

/// <summary>
/// The player loop: the engine's seven frame phases, in order, with the systems a game or package adds under them. The engine
/// runs its own steps natively; an added system's <see cref="PlayerLoopSystem.updateDelegate"/> runs once a frame at its phase:
/// <c>Initialization</c>, <c>EarlyUpdate</c> and <c>PreUpdate</c> before the frame's scripts, <c>Update</c> after the scripts'
/// <c>Update</c> and the main thread's posted work, <c>PreLateUpdate</c> after the scripts' <c>LateUpdate</c>, and
/// <c>PostLateUpdate</c> before the frame is drawn. Within a phase, systems run in tree order, a system before its children.
/// </summary>
public static class PlayerLoop
{
    private static PlayerLoopSystem? _current;
    private static Dictionary<Type, PlayerLoopSystem.UpdateFunction[]> _added = [];

    public static PlayerLoopSystem GetDefaultPlayerLoop() => PlayerLoopPhases.Default();

    /// <summary>The loop last set, or the default loop; a copy, so editing it changes nothing until it is set.</summary>
    public static PlayerLoopSystem GetCurrentPlayerLoop() => PlayerLoopPhases.Copy(_current ?? PlayerLoopPhases.Default());

    /// <summary>
    /// Replaces the loop with a copy of <paramref name="loop"/>. Its top level must be the seven phases in their order, because
    /// the engine cannot drop or move its own steps. A system with a delegate under <c>FixedUpdate</c> throws
    /// <see cref="NotSupportedException"/>: fixed steps run natively, with no managed point per step to run it at.
    /// </summary>
    public static void SetPlayerLoop(PlayerLoopSystem loop)
    {
        var added = PlayerLoopPhases.Added(loop);
        _current = PlayerLoopPhases.Copy(loop);
        _added = added;
    }

    /// <summary>Runs the delegates added under <paramref name="phase"/>. A delegate that throws is reported as a script exception,
    /// and the rest still run.</summary>
    internal static void Run(Type phase)
    {
        if (!_added.TryGetValue(phase, out var functions)) return;
        foreach (var function in functions)
        {
            try { function(); }
            catch (Exception error) { DisunityHost.ReportScriptException(error, null); }
        }
    }

    /// <summary>Back to the default loop, as a new player starts with.</summary>
    internal static void Reset()
    {
        _current = null;
        _added = [];
    }
}
