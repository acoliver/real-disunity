using UnityEngine;

namespace Disunity.Hosting;

/// <summary>
/// What <see cref="Application"/> reports about the running game. A built player's
/// environment comes from its data directory, its <c>PlayerSettings</c> and the Unity version
/// that built it; other games run with <see cref="Default"/>.
/// </summary>
/// <param name="DataPath">The game's data directory, with forward slashes.</param>
/// <param name="PersistentDataPath">Where the game keeps files between runs, created on use.
/// Player preferences are saved there. Empty keeps nothing between runs.</param>
/// <param name="Platform">The platform the game was built for.</param>
public sealed record PlayerEnvironment(
    string DataPath,
    string PersistentDataPath,
    string CompanyName,
    string ProductName,
    string Version,
    string UnityVersion,
    RuntimePlatform Platform)
{
    /// <summary>The Unity release the engine implements.</summary>
    public const string EngineUnityVersion = "6000.3.15f1";

    public static PlayerEnvironment Default { get; } = new(
        AppContext.BaseDirectory.Replace('\\', '/').TrimEnd('/'), "", "DefaultCompany", "Disunity", "1.0",
        EngineUnityVersion, OperatingSystem.IsWindows() ? RuntimePlatform.WindowsPlayer
            : OperatingSystem.IsMacOS() ? RuntimePlatform.OSXPlayer : RuntimePlatform.LinuxPlayer);

    public string StreamingAssetsPath => DataPath + "/StreamingAssets";
}

public static partial class DisunityHost
{
    /// <summary>The environment <see cref="Application"/> reports.</summary>
    public static PlayerEnvironment GameEnvironment { get; private set; } = PlayerEnvironment.Default;

    /// <summary>Sets the environment <see cref="Application"/> reports and loads the player
    /// preferences saved in its persistent data path.</summary>
    public static void ConfigurePlayer(PlayerEnvironment environment)
    {
        ArgumentNullException.ThrowIfNull(environment);
        GameEnvironment = environment;
        PlayerPrefs.Open(environment.PersistentDataPath);
        Application.ResetLifetime();
    }
}
