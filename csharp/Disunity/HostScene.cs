using System.Text.RegularExpressions;
using UnityEngine;
using UnityEngine.SceneManagement;

namespace Disunity.Hosting;

/// <summary>The scene the loaded plan describes: its name and the plan's prefab templates, which belong to no scene.</summary>
internal sealed record LoadedSceneInfo(string Name, IReadOnlySet<GameObject> Templates)
{
    /// <summary>A player's levels are named level0, level1 and so on, after their build index; any other plan is build index 0.</summary>
    public int BuildIndex => Regex.Match(Name, @"^level(\d+)$") is { Success: true } match && int.TryParse(match.Groups[1].Value, out var index) ? index : 0;
}

public static partial class DisunityHost
{
    /// <summary>The scene the last <see cref="LoadScene"/> built, or null before one loads and after the world unloads.</summary>
    internal static LoadedSceneInfo? CurrentScene { get; private set; }

    internal static Scene SceneOf(GameObject value) =>
        CurrentScene is { } info && !value.IsDestroyed && !info.Templates.Contains(value) ? new Scene { Handle = Scene.LoadedHandle } : default;

    internal static List<GameObject> SceneRoots() =>
        CurrentScene is { } info ? ChildrenOf(0).Where(value => !info.Templates.Contains(value)).ToList() : new List<GameObject>();
}
