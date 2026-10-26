using System.Reflection;
using Disunity.Hosting;

// Loads the original games from their scene plans and plays them headless.
// Arguments: the native library, then `game=plan.json=scripts.dll` for each game.
DisunityHost.ConfigureNativeLibrary(args[0]);
var games = args.Skip(1)
    .Select(arg => arg.Split('=', 3))
    .ToDictionary(parts => parts[0], parts => new Game(File.ReadAllText(parts[1]), Assembly.LoadFrom(parts[2])));
LoaderRules.Run();
PongScene.Run(games["pong"]);
BreakoutScene.Run(games["breakout"]);
SnakeScene.Run(games["snake"]);

internal sealed record Game(string Plan, Assembly Scripts)
{
    /// <summary>Package fonts: the synthetic test font stands in for Unity's Arial.</summary>
    public SceneLoadOptions Options => new(Scripts, name => name == "Arial"
        ? SyntheticFont.Bytes()
        : throw new InvalidOperationException($"no font {name}"));
}

internal static class Check
{
    public static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    /// <summary>Presses or releases a key just after the current frame, so the next step reads it.</summary>
    public static void Key(UnityEngine.KeyCode key, bool pressed) =>
        DisunityHost.PushKey(UnityEngine.Time.time + 0.001, key, pressed);

    public static void Steps(int count, float delta = 0.02f)
    {
        for (var i = 0; i < count; i++) DisunityHost.Step(delta);
    }

    /// <summary>Every live object of a game script type, found as the game would find it.</summary>
    public static UnityEngine.Object[] Find(Type type) => (UnityEngine.Object[])typeof(UnityEngine.Object)
        .GetMethod(nameof(UnityEngine.Object.FindObjectsByType))!
        .MakeGenericMethod(type)
        .Invoke(null, [UnityEngine.FindObjectsSortMode.None])!;
}
