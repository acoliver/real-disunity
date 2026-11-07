namespace UnityEngine.LowLevel;

/// <summary>One system of the player loop: a type that names it, its child systems and the function it runs. The
/// engine runs its own frame steps and no loop a game or package edits, so a system is data only.</summary>
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

/// <summary>The player loop. The engine has no loop to read or replace, so each member throws
/// <see cref="NotSupportedException"/> naming itself.</summary>
public static class PlayerLoop
{
    private static NotSupportedException Unsupported(string member) => new($"PlayerLoop.{member} is not supported: the engine runs its own frame steps and has no editable player loop");

    public static PlayerLoopSystem GetDefaultPlayerLoop() => throw Unsupported(nameof(GetDefaultPlayerLoop));
    public static PlayerLoopSystem GetCurrentPlayerLoop() => throw Unsupported(nameof(GetCurrentPlayerLoop));
    public static void SetPlayerLoop(PlayerLoopSystem loop) => throw Unsupported(nameof(SetPlayerLoop));
}
